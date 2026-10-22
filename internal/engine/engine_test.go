package engine

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"

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
		wantFS := map[string]string{"ja": "見つかりません", "en": "no such file"}[c.lang]
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
