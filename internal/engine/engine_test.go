package engine

import (
	"bytes"
	"io"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/cool0707/xljs/internal/i18n"
	"github.com/cool0707/xljs/internal/ui"
)

func TestMain(m *testing.M) {
	// Script tests assert Japanese messages regardless of the CI locale.
	i18n.SetLanguage("ja")
	os.Exit(m.Run())
}

func scriptsDir(t *testing.T) string {
	t.Helper()
	d, err := filepath.Abs(filepath.Join("..", "..", "testdata", "scripts"))
	if err != nil {
		t.Fatal(err)
	}
	return d
}

type result struct {
	code   int
	err    error
	stdout string
	stderr string
}

// syncBuffer is a buffer that mock windows (logging from their own
// goroutine) and the script can write to at the same time.
type syncBuffer struct {
	mu sync.Mutex
	b  bytes.Buffer
}

func (s *syncBuffer) Write(p []byte) (int, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.b.Write(p)
}

func (s *syncBuffer) String() string {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.b.String()
}

func runScript(t *testing.T, script string, answers string, args ...string) result {
	t.Helper()
	var out, errOut syncBuffer
	m, err := ui.NewMock(answers, &errOut)
	if err != nil {
		t.Fatal(err)
	}
	t.Chdir(t.TempDir())
	t.Setenv("XLJS_CONFIG_DIR", t.TempDir())
	code, err := Run(Options{ScriptPath: script, Args: args, UI: m, Stdout: &out, Stderr: &errOut, Version: "test"})
	return result{code, err, out.String(), errOut.String()}
}

// TestScripts runs every testdata/scripts/*.test.{js,ts} with the mock UI
// in an empty temporary directory. Each script asserts its own behaviour.
func TestScripts(t *testing.T) {
	dir := scriptsDir(t)
	var scripts []string
	for _, pat := range []string{"*.test.js", "*.test.ts"} {
		m, _ := filepath.Glob(filepath.Join(dir, pat))
		scripts = append(scripts, m...)
	}
	if len(scripts) == 0 {
		t.Fatal("no test scripts found")
	}
	for _, s := range scripts {
		name := filepath.Base(s)
		t.Run(name, func(t *testing.T) {
			answers := strings.TrimSuffix(s, filepath.Ext(s)) + ".answers.json"
			if _, err := os.Stat(answers); err != nil {
				answers = ""
			}
			r := runScript(t, s, answers)
			if r.err != nil || r.code != 0 {
				t.Fatalf("exit %d: %v\nstdout:\n%s\nstderr:\n%s", r.code, r.err, r.stdout, r.stderr)
			}
			if !strings.Contains(r.stdout, " ok") {
				t.Errorf("missing completion marker in stdout:\n%s", r.stdout)
			}
			if name == "app.test.js" {
				for _, want := range []string{
					`window.open("アプリのテスト", "https://app.xljs/ui/index.html", 640x480)`,
					`window.open("詳細", "https://app.xljs/ui/sub%20page.html", 300x200)`,
					`window.closed("詳細")`,
				} {
					if !strings.Contains(r.stderr, want) {
						t.Errorf("mock log does not contain %q:\n%s", want, r.stderr)
					}
				}
			}
			if name == "bridge.test.js" {
				for _, want := range []string{
					`window.init({"progress":0,"message":"準備中","when":{"$date":`,
					`window.$("#title").text("見出し")`,
					`window.$("#title").addClass("big")`,
					`window.$("#title").text() -> null`,
					`window.evaluate("(a) => a + 1") -> null`,
					`app: invoke add(1, 2) -> 3`,
					`-> {"year":2026,"file":"a.xlsx","dir":"directory","size":3}`,
					`app: invoke fail() -> error: RangeError: だめです`,
					`app: invoke missing() -> error: Error: 画面から呼ばれた関数 "missing" がありません`,
					`app: invoke notCallable() -> error: Error: 画面から呼ばれた関数 "notCallable"`,
					`app: invoke dynamic(21) -> 42`,
					`app: invoke whoami() -> true`,
					`window.state.progress = 100`,
					`window.emit("done", {"ok":true})`,
					`window.invoke("render", [1,2]) -> 2`,
					"error: Error: ハンドラのエラー",
				} {
					if !strings.Contains(r.stderr, want) {
						t.Errorf("mock log does not contain %q:\n%s", want, r.stderr)
					}
				}
				for _, unwanted := range []string{"expected", "window.state.progress = 0", "window.state.after", "timeout"} {
					if strings.Contains(r.stderr, unwanted) {
						t.Errorf("mock log contains %q:\n%s", unwanted, r.stderr)
					}
				}
				for _, want := range []string{"hello 画面\n", "state month 2026-04\n", "render -> 2\n"} {
					if !strings.Contains(r.stdout, want) {
						t.Errorf("stdout does not contain %q:\n%s", want, r.stdout)
					}
				}
			}
			if name == "form.test.js" {
				for _, want := range []string{
					"form error: count: 10 以下の値を入力してください",
					"form error: day: 日付（YYYY-MM-DD）を入力してください",
					"form error: code: 英大文字 3 文字",
					"form error: inputs: ファイルが見つかりません",
					"form error: tax: 商品別は税込のみ",
					`progress("実行", total=3) start`,
				} {
					if !strings.Contains(r.stderr, want) {
						t.Errorf("mock log does not contain %q:\n%s", want, r.stderr)
					}
				}
			}
			if name == "table.test.js" {
				for _, want := range []string{
					`table("集計結果", 3 rows, ["顧客名" "件数" "合計" "日付" "備考"])`,
					`table("列指定", 3 rows, ["顧客名" "金額"])`,
					`table("配列", 2 rows, ["A" "B"])`,
					`table("値", 2 rows, ["値"])`,
				} {
					if !strings.Contains(r.stderr, want) {
						t.Errorf("mock log does not contain %q:\n%s", want, r.stderr)
					}
				}
			}
			if name == "ui.test.js" {
				for _, want := range []string{"showItemInFolder([", "a.xlsx", "b.csv", `openPath(`, `"https://example.com/"`} {
					if !strings.Contains(r.stderr, want) {
						t.Errorf("mock log does not contain %q:\n%s", want, r.stderr)
					}
				}
			}
		})
	}
}

func writeScript(t *testing.T, name, src string) string {
	t.Helper()
	p := filepath.Join(t.TempDir(), name)
	if err := os.WriteFile(p, []byte(src), 0o644); err != nil {
		t.Fatal(err)
	}
	return p
}

func TestEntryExports(t *testing.T) {
	// Exports of the entry (also re-exported ones) can be called from
	// windows, so app.handle refuses their names; the bundle stays runnable.
	dir := t.TempDir()
	if err := os.WriteFile(filepath.Join(dir, "lib.js"), []byte("export function fromLib() {}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	src := `export * from "./lib.js";
export async function f() {}
const z = 1;
export { z as "a b", z as default };
export let later = 0;
for (const name of ["f", "a b", "default", "later", "fromLib"]) {
  try { app.handle(name, () => 0); console.log("no error", name); } catch (e) {}
}
app.handle("other", () => 0);
console.log("done");
`
	p := filepath.Join(dir, "main.js")
	if err := os.WriteFile(p, []byte(src), 0o644); err != nil {
		t.Fatal(err)
	}
	r := runScript(t, p, "")
	if r.code != 0 || r.err != nil || r.stdout != "done\n" {
		t.Fatalf("exit %d, %v, stdout %q, stderr %q", r.code, r.err, r.stdout, r.stderr)
	}
}

func TestErrorStackPointsToSource(t *testing.T) {
	r := runScript(t, filepath.Join(scriptsDir(t), "_errors.js"), "")
	if r.code != 1 || r.err == nil {
		t.Fatalf("want failure, got code %d err %v", r.code, r.err)
	}
	msg := r.err.Error()
	for _, want := range []string{"RangeError: boom from lib", "_errlib.js:2:", "_errors.js:3:"} {
		if !strings.Contains(msg, want) {
			t.Errorf("error %q does not contain %q", msg, want)
		}
	}
	if strings.Contains(msg, "prelude") {
		t.Errorf("internal frames leaked: %s", msg)
	}
}

func TestExitCodes(t *testing.T) {
	cases := []struct {
		name, src string
		code      int
		errPart   string
	}{
		{"exit", `try { exit(3) } catch { console.log("caught") }`, 3, ""},
		{"exit-in-timer", `setTimeout(() => xljs.exit(4), 1); await sleep(100); console.log("no")`, 4, ""},
		{"throw-after-await", `await sleep(1); throw new Error("late")`, 1, "Error: late"},
		{"throw-value", `throw "plain"`, 1, "plain"},
		{"unhandled", `Promise.reject(new Error("lost")); await sleep(1)`, 1, "処理されなかった Promise"},
		{"pending", `await new Promise(() => {})`, 1, "完了しませんでした"},
		{"syntax", `const = 1`, 1, "x.js:1:"},
		{"missing-import", `import "./nope.js"`, 1, "nope.js"},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			p := writeScript(t, "x.js", c.src)
			r := runScript(t, p, "")
			if r.code != c.code {
				t.Fatalf("code = %d, want %d (err %v, stdout %q)", r.code, c.code, r.err, r.stdout)
			}
			if c.errPart != "" && (r.err == nil || !strings.Contains(r.err.Error(), c.errPart)) {
				t.Fatalf("err = %v, want it to contain %q", r.err, c.errPart)
			}
			if strings.Contains(r.stdout, "caught") || strings.Contains(r.stdout, "no") {
				t.Fatalf("code after exit ran: %q", r.stdout)
			}
		})
	}
}

func TestArgsAndConsole(t *testing.T) {
	p := writeScript(t, "a.js", `console.log(xljs.args.join("|")); console.error("to stderr"); console.log("%s=%d", "n", 42)`)
	r := runScript(t, p, "", "x", "y z")
	if r.code != 0 {
		t.Fatalf("code %d: %v", r.code, r.err)
	}
	if r.stdout != "x|y z\nn=42\n" {
		t.Fatalf("stdout = %q", r.stdout)
	}
	if !strings.Contains(r.stderr, "to stderr") {
		t.Fatalf("stderr = %q", r.stderr)
	}
}

func TestMessagesFollowLanguage(t *testing.T) {
	src := `const o = null; console.log(o.name)`
	cases := []struct{ lang, want string }{
		{"ja", "のプロパティ 'name' は読み取れません"},
		{"en", "Cannot read property 'name' of"},
	}
	defer i18n.SetLanguage("ja")
	for _, c := range cases {
		i18n.SetLanguage(c.lang)
		r := runScript(t, writeScript(t, "x.js", src), "")
		if r.err == nil || !strings.Contains(r.err.Error(), c.want) {
			t.Errorf("%s: err = %v, want %q", c.lang, r.err, c.want)
		}
		r = runScript(t, writeScript(t, "y.js", `fs.readText("nope/missing.csv")`), "")
		wantFS := map[string]string{"ja": "見つかりません", "en": "missing.csv"}[c.lang]
		if r.err == nil || !strings.Contains(r.err.Error(), wantFS) {
			t.Errorf("%s: fs err = %v, want %q", c.lang, r.err, wantFS)
		}
		r = runScript(t, writeScript(t, "z.js", `import "./nope.js"`), "")
		wantImp := map[string]string{"ja": "が見つかりません", "en": "Could not resolve"}[c.lang]
		if r.err == nil || !strings.Contains(r.err.Error(), wantImp) {
			t.Errorf("%s: import err = %v, want %q", c.lang, r.err, wantImp)
		}
	}
}

func TestDefaultFontFollowsLanguage(t *testing.T) {
	defer i18n.SetLanguage("ja")
	for lang, font := range map[string]string{"ja": "游ゴシック", "en": "Calibri"} {
		i18n.SetLanguage(lang)
		src := `const f = workbooks.create().defaultFont.name; if (f !== "` + font + `") throw new Error(f)`
		if r := runScript(t, writeScript(t, "f.js", src), ""); r.code != 0 {
			t.Errorf("%s: %v", lang, r.err)
		}
	}
}

func TestAppMode(t *testing.T) {
	src := "// ==xljs==\n// @mode app\n// @unknown 1\n// ==/xljs==\nconsole.log('ran')\n"
	p := writeScript(t, "app.js", src)

	// The console UI cannot show windows: the script must not run.
	var out, errOut bytes.Buffer
	code, err := Run(Options{ScriptPath: p, UI: ui.NewConsole(), Stdout: &out, Stderr: &errOut})
	if code != 1 || err == nil || !strings.Contains(err.Error(), "ネイティブ UI") || out.Len() != 0 {
		t.Fatalf("console: code %d, err %v, stdout %q", code, err, out.String())
	}
	if !strings.Contains(errOut.String(), "@unknown") {
		t.Errorf("no warning for an unknown key: %q", errOut.String())
	}

	// Bad metadata stops the script before it runs.
	bad := writeScript(t, "bad.js", "// ==xljs==\n// @mode window\n// ==/xljs==\nconsole.log('ran')\n")
	if r := runScript(t, bad, ""); r.code != 1 || r.err == nil || !strings.Contains(r.err.Error(), "bad.js:2") {
		t.Fatalf("bad meta: %+v", r)
	}

}

// slowUI is a mock whose windows stay open until closed by the script or
// until after (when non-zero) has passed, as if the user closed them.
type slowUI struct {
	*ui.Mock
	after  time.Duration
	mu     sync.Mutex
	events []string
}

type slowWindow struct {
	u    *slowUI
	o    ui.WindowOptions
	once sync.Once
}

func (u *slowUI) log(e string) {
	u.mu.Lock()
	u.events = append(u.events, e)
	u.mu.Unlock()
}

func (u *slowUI) OpenWindow(o ui.WindowOptions) (ui.Window, error) {
	w := &slowWindow{u: u, o: o}
	u.log("open " + o.Title)
	if u.after > 0 {
		time.AfterFunc(u.after, func() { w.closeAs("user") })
	}
	return w, nil
}

func (w *slowWindow) Post(string) {}
func (w *slowWindow) Close()      { w.closeAs("script") }
func (w *slowWindow) closeAs(by string) {
	w.once.Do(func() {
		w.u.log("closed by " + by)
		w.o.OnClosed()
	})
}

// windowUI shows forms and tables as (slowUI) windows.
type windowUI struct{ *slowUI }

func (windowUI) SupportsWindows() bool { return true }

func TestAppWindowKeepsRunning(t *testing.T) {
	m, _ := ui.NewMockFromJSON("", io.Discard)
	app := "// ==xljs==\n// @mode app\n// ==/xljs==\n"

	// The script finishes at once, but the process waits for the window.
	u := &slowUI{Mock: m, after: 150 * time.Millisecond}
	var out bytes.Buffer
	start := time.Now()
	p := writeScript(t, "a.js", app+"app.main.closed.then(() => console.log('closed')); console.log('end of script')")
	code, err := Run(Options{ScriptPath: p, UI: u, Stdout: &out, Stderr: io.Discard})
	if code != 0 || err != nil {
		t.Fatalf("code %d: %v", code, err)
	}
	if time.Since(start) < 150*time.Millisecond || out.String() != "end of script\nclosed\n" {
		t.Fatalf("returned after %v with output %q", time.Since(start), out.String())
	}

	// exit() closes windows that are still open.
	u = &slowUI{Mock: m}
	p = writeScript(t, "b.js", app+"app.open({ title: 'sub' }); setTimeout(() => exit(5), 10)")
	code, err = Run(Options{ScriptPath: p, UI: u, Stdout: io.Discard, Stderr: io.Discard})
	if code != 5 || err != nil {
		t.Fatalf("exit: %d %v", code, err)
	}
	if got := strings.Join(u.events, ","); got != "open b,open sub,closed by script,closed by script" {
		t.Fatalf("events = %s", got)
	}

	// exit() after awaiting a window that the user closed (the promise is
	// resolved from Go, outside of any call into the script).
	for _, src := range []string{
		"await app.open({ title: 'sub' }).closed; exit(7); console.log('after')",
		"app.open({ title: 'sub' }).closed.then(() => { exit(7); console.log('after') })",
		"const r = await ui.table([1], { selectable: true }); if (!r.length) exit(7); console.log('after')",
	} {
		u = &slowUI{Mock: m, after: 20 * time.Millisecond}
		out.Reset()
		code, err = Run(Options{ScriptPath: writeScript(t, "d.js", src), UI: windowUI{u}, Stdout: &out, Stderr: io.Discard})
		if code != 7 || err != nil || out.Len() != 0 {
			t.Errorf("%s: exit %d, %v, stdout %q", src, code, err, out.String())
		}
	}

	// Through ui.Run the script runs on its own thread and the UI calls
	// (including the window callbacks) are served by the caller.
	u = &slowUI{Mock: m, after: 20 * time.Millisecond}
	ui.Run(u, func(su ui.UI) {
		code, err = Run(Options{ScriptPath: writeScript(t, "c.js", app+"await app.main.closed"), UI: su, Stdout: io.Discard, Stderr: io.Discard})
	})
	if code != 0 || err != nil {
		t.Fatalf("ui.Run: %d %v", code, err)
	}
}
