package engine

import (
	"bytes"
	"encoding/base64"
	"errors"
	"io"
	"io/fs"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/cool0707/xljs/internal/ui"
	"github.com/dop251/goja"
	"github.com/dop251/goja/ast"
	"github.com/dop251/goja/parser"
)

// fakeDebugUI plays the debugger window (debug-page.js) with a second goja
// runtime standing in for WebView2: it loads the same scripts, calls Go
// through WindowOptions.Handle and receives the events, and presses Run as
// soon as the page is ready and Close when the run has ended. Other windows
// go to the wrapped UI.
type fakeDebugUI struct {
	ui.UI
	t    *testing.T
	page *debugPage
}

// check fails the test unless the debugger page ran without breaking.
func (f *fakeDebugUI) check() {
	f.t.Helper()
	if f.page == nil {
		f.t.Fatal("the debugger window was not opened")
	}
	f.page.mu.Lock()
	defer f.page.mu.Unlock()
	if f.page.err != nil {
		f.t.Errorf("debugger page: %v", f.page.err)
	}
}

func (f *fakeDebugUI) OpenWindow(o ui.WindowOptions) (ui.Window, error) {
	if o.URL != DebugPage {
		return f.UI.OpenWindow(o)
	}
	if !o.Debug || !o.DevTools || o.Handle == nil {
		f.t.Errorf("debugger window options: %+v", o)
	}
	vm := goja.New()
	p := &debugPage{o: o, vm: vm, l: newLoop(vm)}
	p.l.install()
	p.l.addPending() // until the window closes
	f.page = p
	go p.run()
	return p, nil
}

type debugPage struct {
	o  ui.WindowOptions
	vm *goja.Runtime
	l  *loop

	mu     sync.Mutex
	closed bool
	listen goja.Callable
	// Error is set when the page itself broke (not the script).
	err error
}

func (p *debugPage) Post(msg string) {
	p.mu.Lock()
	defer p.mu.Unlock()
	if p.closed {
		return
	}
	p.l.addPending()
	p.l.post(func() error {
		if p.listen == nil {
			return errors.New("event before listen")
		}
		_, err := p.listen(goja.Undefined(), p.vm.ToValue(msg))
		return err
	})
}

func (p *debugPage) Close() {
	p.mu.Lock()
	if p.closed {
		p.mu.Unlock()
		return
	}
	p.closed = true
	p.l.post(func() error { return nil }) // the page's pending
	p.mu.Unlock()
	p.o.OnClosed()
}

func (p *debugPage) run() {
	vm := p.vm
	vm.Set("btoa", func(s string) string {
		b := make([]byte, 0, len(s))
		for _, r := range s {
			b = append(b, byte(r))
		}
		return base64.StdEncoding.EncodeToString(b)
	})
	vm.Set("atob", func(s string) string {
		b, _ := base64.StdEncoding.DecodeString(s)
		r := make([]rune, len(b))
		for i, c := range b {
			r[i] = rune(c)
		}
		return string(r)
	})
	transport := vm.NewObject()
	transport.Set("call", func(text string) string {
		ch := make(chan []byte, 1)
		p.o.Handle([]byte(text), func(b []byte) { ch <- b })
		return string(<-ch)
	})
	transport.Set("listen", func(fn goja.Callable) { p.listen = fn })
	var session *goja.Object
	hooks := vm.NewObject()
	hooks.Set("status", func(state string) {
		if state == "done" {
			// The user presses Close.
			go p.o.OnMessage(debugClose, nil)
		}
	})
	fail := func(err error) {
		var ex *goja.Exception
		if session != nil && errors.As(err, &ex) {
			if fatal, ok := goja.AssertFunction(session.Get("fatal")); ok {
				if _, err := fatal(session, ex.Value()); err == nil {
					return
				}
			}
		}
		p.mu.Lock()
		p.err = err
		p.mu.Unlock()
		p.o.OnMessage(debugClose, nil)
	}
	vm.SetPromiseRejectionTracker(func(pr *goja.Promise, op goja.PromiseRejectionOperation) {
		if session == nil {
			return
		}
		if op == goja.PromiseRejectionReject {
			fn, _ := goja.AssertFunction(session.Get("rejected"))
			fn(session, vm.ToValue(pr), pr.Result())
		} else {
			fn, _ := goja.AssertFunction(session.Get("handled"))
			fn(session, vm.ToValue(pr))
		}
	})

	for _, name := range []string{"debug/prelude.js", "debug/bundle.js", "debug.js"} {
		// Named and positioned like in the browser, which leaves the source
		// map to the developer tools.
		src, err := fs.ReadFile(p.o.Assets, ui.BuiltinPrefix+name)
		if err == nil {
			var ast *ast.Program
			ast, err = parser.ParseFile(nil, ui.AssetOrigin+ui.BuiltinPrefix+name, string(src), 0, parser.WithDisableSourceMaps)
			if err == nil {
				var prog *goja.Program
				if prog, err = goja.CompileAST(ast, false); err == nil {
					_, err = vm.RunProgram(prog)
				}
			}
		}
		if err != nil {
			fail(err)
			return
		}
	}
	start, _ := goja.AssertFunction(vm.Get("xljsDebug").ToObject(vm).Get("start"))
	v, err := start(goja.Undefined(), transport, hooks)
	if err != nil {
		fail(err)
		return
	}
	session = v.ToObject(vm)
	run, _ := goja.AssertFunction(session.Get("run"))
	if _, err := run(session, vm.ToValue(false)); err != nil {
		fail(err)
	}
	// Uncaught errors (in timers and event handlers) are reported like the
	// page's "error" event does, and the page keeps running.
	for {
		err := p.l.run()
		if err == nil {
			return
		}
		fail(err)
	}
}

func TestDebuggerErrorStack(t *testing.T) {
	// As in TestErrorStackPointsToSource: the page's frames are mapped back.
	r := runScriptIn(t, true, filepath.Join(scriptsDir(t), "_errors.js"), "")
	if r.code != 1 || r.err == nil {
		t.Fatalf("want failure, got code %d err %v", r.code, r.err)
	}
	msg := r.err.Error()
	for _, want := range []string{"RangeError: boom from lib", "at fail (_errlib.js:2:", "_errors.js:3:"} {
		if !strings.Contains(msg, want) {
			t.Errorf("error %q does not contain %q", msg, want)
		}
	}
	for _, unwanted := range []string{"bundle.js", "debug.js", "prelude", ui.AssetOrigin} {
		if strings.Contains(msg, unwanted) {
			t.Errorf("error %q contains %q", msg, unwanted)
		}
	}
}

func TestDebuggerPage(t *testing.T) {
	cases := []struct {
		name, src string
		code      int
		err       string
		stdout    string
	}{
		{"exit", `console.log("a"); setTimeout(() => console.log("never"), 50); exit(3); console.log("b");`, 3, "", "a\n"},
		{"throw", `function f() { throw new RangeError("だめ"); }
setTimeout(() => f(), 1);`, 1, "RangeError: だめ", ""},
		{"rejected", `await sleep(1); throw new TypeError("x");`, 1, "TypeError: x", ""},
		{"unhandled", `Promise.reject(new Error("y")); await sleep(5);`, 1, "処理されなかった Promise の reject: Error: y", ""},
		{"pending", `await new Promise(() => {});`, 1, "完了しませんでした", ""},
		{"values", `
const b = workbooks.create();
const s = b.addSheet("S");
s.cell("A1").value = new Date(2026, 3, 1, 9, 30);
s.cell("A2").value = -0;
s.cell("A3").value = "日本語";
const v = s.cell("A1").value;
console.log(v instanceof Date, v.getFullYear(), v.getHours(), s.cell("A3").value);
const bytes = b.toArrayBuffer();
console.log(bytes instanceof ArrayBuffer, bytes.byteLength > 100);
const b2 = workbooks.open(new Uint8Array(bytes));
console.log(b2.sheetNames.join(","));
try { fs.readText("nope.txt"); } catch (e) { console.log(e.name, e.message.length > 0); }
console.log(new Blob(["x"]) instanceof Blob, typeof Intl.NumberFormat);
`, 0, "", "true 2026 9 日本語\ntrue true\nS\nError true\ntrue function\n"},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			p := writeScript(t, "main.js", c.src)
			r := runScriptIn(t, true, p, "")
			if r.code != c.code {
				t.Errorf("exit code %d, want %d (err %v)\nstderr:\n%s", r.code, c.code, r.err, r.stderr)
			}
			msg := ""
			if r.err != nil {
				msg = r.err.Error()
			}
			if (c.err == "") != (msg == "") || !strings.Contains(msg, c.err) {
				t.Errorf("error %q, want %q", msg, c.err)
			}
			if c.stdout != "" && r.stdout != c.stdout {
				t.Errorf("stdout %q, want %q", r.stdout, c.stdout)
			}
			if strings.Contains(msg, ui.AssetOrigin) {
				t.Errorf("error keeps the page URL: %s", msg)
			}
		})
	}
}

func TestDebuggerAppWindow(t *testing.T) {
	// As in TestAppWindowKeepsRunning: the run ends when the windows have
	// closed, and exit() closes them.
	m, _ := ui.NewMockFromJSON("", io.Discard)
	app := "// ==xljs==\n// @mode app\n// ==/xljs==\n"
	run := func(u ui.UI, src string) (int, error, string) {
		var out bytes.Buffer
		f := &fakeDebugUI{UI: u, t: t}
		code, err := Run(Options{ScriptPath: writeScript(t, "a.js", src), UI: f, Stdout: &out, Stderr: io.Discard,
			Debug: true, DebugScript: true})
		f.check()
		return code, err, out.String()
	}

	u := &slowUI{Mock: m, after: 100 * time.Millisecond}
	start := time.Now()
	code, err, out := run(u, app+"app.main.closed.then(() => console.log('closed')); console.log('end of script')")
	if code != 0 || err != nil || time.Since(start) < 100*time.Millisecond || out != "end of script\nclosed\n" {
		t.Fatalf("code %d, err %v, after %v, output %q", code, err, time.Since(start), out)
	}

	u = &slowUI{Mock: m}
	if code, err, _ := run(u, app+"app.open({ title: 'sub' }); setTimeout(() => exit(5), 10)"); code != 5 || err != nil {
		t.Fatalf("exit: %d %v", code, err)
	}
	if got := strings.Join(u.events, ","); got != "open a,open sub,closed by script,closed by script" {
		t.Fatalf("events = %s", got)
	}

	for _, src := range []string{
		"await app.open({ title: 'sub' }).closed; exit(7); console.log('after')",
		"app.open({ title: 'sub' }).closed.then(() => { exit(7); console.log('after') })",
	} {
		u = &slowUI{Mock: m, after: 20 * time.Millisecond}
		if code, err, out := run(u, src); code != 7 || err != nil || out != "" {
			t.Errorf("%s: exit %d, %v, stdout %q", src, code, err, out)
		}
	}
}
