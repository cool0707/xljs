//go:build windows

package engine

import (
	"runtime"
	"strings"
	"testing"

	"github.com/cool0707/xljs/internal/ui"
)

// TestDebuggerWindow runs scripts in the real debugger window (WebView2):
// natives through the synchronous bridge, timers and promises of Go
// (workbooks, sleep) and the end of the run.
func TestDebuggerWindow(t *testing.T) {
	if ui.WebView2Version() == "" {
		t.Skip("WebView2 runtime is not installed")
	}
	runtime.LockOSThread()
	defer runtime.UnlockOSThread()
	u, err := ui.NewNative()
	if err != nil {
		t.Fatal(err)
	}
	cases := []struct {
		name, src, stdout, err string
		code                   int
	}{
		{"ok", `const b = workbooks.create();
const s = b.addSheet("集計");
s.cell("A1").value = new Date(2026, 3, 1);
await sleep(10);
console.log(s.cell("A1").value.getMonth(), xljs.ui, typeof document);
exit(4);`, "3 native object\n", "", 4},
		{"error", `function f() { throw new RangeError("だめ"); }
setTimeout(f, 1);`, "", "RangeError: だめ", 1},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			p := writeScript(t, "main.js", c.src)
			var out, errOut syncBuffer
			var code int
			var err error
			ui.Run(u, func(su ui.UI) {
				code, err = Run(Options{ScriptPath: p, UI: su, Stdout: &out, Stderr: &errOut, Version: "test",
					Debug: true, DebugScript: true, debugAutoRun: true})
			})
			if code != c.code {
				t.Errorf("exit code %d, want %d (err %v)\nstderr:\n%s", code, c.code, err, errOut.String())
			}
			msg := ""
			if err != nil {
				msg = err.Error()
			}
			if (c.err == "") != (msg == "") || !strings.Contains(msg, c.err) {
				t.Errorf("error %q, want %q", msg, c.err)
			}
			if !strings.Contains(msg, "main.js:1:") && c.err != "" {
				t.Errorf("error %q does not point to main.js", msg)
			}
			if out.String() != c.stdout {
				t.Errorf("stdout %q, want %q", out.String(), c.stdout)
			}
		})
	}
}
