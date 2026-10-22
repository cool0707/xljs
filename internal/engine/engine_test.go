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

func runScript(t *testing.T, script string, answers string, args ...string) result {
	t.Helper()
	var out, errOut bytes.Buffer
	m, err := ui.NewMock(answers, &errOut)
	if err != nil {
		t.Fatal(err)
	}
	t.Chdir(t.TempDir())
	// Remembered form values go to the user's config folder.
	cfg := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", cfg)
	t.Setenv("APPDATA", cfg)
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
			if name == "form.test.js" {
				for _, want := range []string{
					`form("売上集計") -> {"code":"ab"`,
					`form invalid: src: 存在するフォルダを指定してください`,
					`form invalid: n: 10 以下を入力してください`,
					`form invalid: code: 形式が正しくありません`,
					"form invalid: files: .xlsx のファイルを指定してください",
					`form invalid: tax: 商品別は税込のみ`,
					`form("form.test") -> null`,
				} {
					if !strings.Contains(r.stderr, want) {
						t.Errorf("mock log does not contain %q:\n%s", want, r.stderr)
					}
				}
				if !strings.Contains(r.stdout, "処理 太郎") {
					t.Errorf("ctx.log output missing:\n%s", r.stdout)
				}
			}
			if name == "table.test.js" {
				for _, want := range []string{`table("集計結果", 3 rows)`, `table("選択") -> [0 2]`, `table("配列", 2 rows)`} {
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
