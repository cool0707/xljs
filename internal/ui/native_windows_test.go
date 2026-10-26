//go:build windows

package ui

import (
	"io/fs"
	"runtime"
	"strings"
	"testing"
	"testing/fstest"
	"time"
	"unsafe"
)

var (
	procGetClipboardData = moduser32.NewProc("GetClipboardData")
	procGlobalSize       = modkernel.NewProc("GlobalSize")
)

func clipboardText(t *testing.T) (string, bool) {
	if r, _, _ := procOpenClipboard.Call(0); r == 0 {
		return "", false
	}
	defer procCloseClipboard.Call()
	h, _, _ := procGetClipboardData.Call(cfUnicodeText)
	if h == 0 {
		return "", false
	}
	size, _, _ := procGlobalSize.Call(h)
	p, _, _ := procGlobalLock.Call(h)
	if p == 0 || size < 2 {
		return "", false
	}
	defer procGlobalUnlock.Call(h)
	buf := make([]uint16, size/2)
	procRtlMoveMemory.Call(uintptr(unsafe.Pointer(&buf[0])), p, size)
	n := 0
	for n < len(buf) && buf[n] != 0 {
		n++
	}
	return string(utf16Decode(buf[:n])), true
}

func utf16Decode(u []uint16) []rune {
	var out []rune
	for i := 0; i < len(u); i++ {
		r := rune(u[i])
		if r >= 0xD800 && r < 0xDC00 && i+1 < len(u) {
			r = (r-0xD800)<<10 + (rune(u[i+1]) - 0xDC00) + 0x10000
			i++
		}
		out = append(out, r)
	}
	return out
}

func withAutoClose(t *testing.T) {
	runtime.LockOSThread()
	testAutoClose = true
	t.Cleanup(func() {
		testAutoClose = false
		runtime.UnlockOSThread()
	})
}

// The dialogs are built from in-memory templates; these tests make sure
// Windows accepts them and that the buttons work.
func TestErrorDialog(t *testing.T) {
	withAutoClose(t)
	msg := "TypeError: エラー\n\tat x.js:1:2"
	if !errorDialog("テスト", msg) {
		t.Fatal("error dialog could not be created")
	}
	if got, ok := clipboardText(t); ok {
		if got != "TypeError: エラー\r\n    at x.js:1:2" {
			t.Errorf("clipboard = %q", got)
		}
	} else {
		t.Log("clipboard not accessible; skipped content check")
	}
}

func TestPromptDialog(t *testing.T) {
	withAutoClose(t)
	n := &Native{}
	got, ok := n.Prompt("テスト", "名前を入力してください", "既定値")
	if !ok || got != "既定値" {
		t.Fatalf("Prompt = %q, %v", got, ok)
	}
}

func TestProgressDialog(t *testing.T) {
	runtime.LockOSThread()
	defer runtime.UnlockOSThread()
	u, err := NewNative()
	if err != nil {
		t.Fatal(err)
	}
	p := u.Progress(ProgressOptions{Title: "テスト", Total: 3, Cancellable: true})
	if _, ok := p.(*nativeProgress); !ok {
		t.Fatalf("expected the native progress dialog, got %T", p)
	}
	for i := 0; i <= 3; i++ {
		p.Update(i, 3, "item")
	}
	if p.Cancelled() {
		t.Error("unexpectedly cancelled")
	}
	p.Close()
	p.Close()
}

// TestWebViewWindow opens a WebView2 window through Run (script thread +
// UI thread), loads a page from the assets and closes the window once the
// page has reported back.
func TestWebViewWindow(t *testing.T) {
	if WebView2Version() == "" {
		t.Skip("WebView2 runtime is not installed")
	}
	runtime.LockOSThread()
	defer runtime.UnlockOSThread()
	u, err := NewNative()
	if err != nil {
		t.Fatal(err)
	}
	assets := fstest.MapFS{
		"ui/index.html": {Data: []byte(`<!doctype html><meta charset="utf-8"><title>t</title>
<script src="app.js"></script>`)},
		"ui/app.js": {Data: []byte(`chrome.webview.postMessage("ready " + location.href + " " + document.title)`)},
	}
	got := make(chan string, 1)
	var msg string
	Run(u, func(su UI) {
		closed := make(chan struct{})
		var w Window
		w, err = su.OpenWindow(WindowOptions{
			Title:  "テスト",
			Width:  400,
			Height: 300,
			URL:    AssetOrigin + "ui/index.html",
			Assets: assets,
			OnMessage: func(m string, _ []string) {
				select {
				case got <- m:
				default:
				}
			},
			OnClosed: func() { close(closed) },
		})
		if err != nil {
			return
		}
		select {
		case msg = <-got:
		case <-time.After(30 * time.Second):
		}
		w.Close()
		w.Close()
		<-closed
	})
	if err != nil {
		t.Fatal(err)
	}
	if want := "ready https://app.xljs/ui/index.html t"; msg != want {
		t.Fatalf("message = %q, want %q", msg, want)
	}
}

// TestBuiltinPages loads the form and table pages in WebView2: each page
// says "ready", renders the "init" message and answers "shown".
func TestBuiltinPages(t *testing.T) {
	if WebView2Version() == "" {
		t.Skip("WebView2 runtime is not installed")
	}
	runtime.LockOSThread()
	defer runtime.UnlockOSThread()
	u, err := NewNative()
	if err != nil {
		t.Fatal(err)
	}
	inits := map[string]string{
		"form.html": `{"type":"init","texts":{"ok":"OK"},"fields":[{"name":"a","label":"A","type":"text","value":"x","options":[],"accept":[]},` +
			`{"name":"b","label":"B","type":"files","value":[],"options":[],"accept":[".xlsx"]}]}`,
		"table.html": `{"type":"init","total":1,"select":"single","texts":{},"columns":[{"label":"A","width":0}]}`,
	}
	for page, init := range inits {
		var got []string
		Run(u, func(su UI) {
			msgs := make(chan string, 10)
			closed := make(chan struct{})
			var w Window
			w, err = su.OpenWindow(WindowOptions{
				Title:     page,
				URL:       AssetOrigin + BuiltinPrefix + page,
				Assets:    builtinOnly{},
				OnMessage: func(m string, _ []string) { msgs <- m },
				OnClosed:  func() { close(closed) },
			})
			if err != nil {
				return
			}
			timeout := time.After(30 * time.Second)
		loop:
			for {
				select {
				case m := <-msgs:
					got = append(got, m)
					if strings.Contains(m, `"ready"`) {
						w.Post(init)
					}
					if strings.Contains(m, `"shown"`) {
						break loop
					}
				case <-timeout:
					break loop
				}
			}
			w.Close()
			<-closed
		})
		if err != nil {
			t.Fatal(err)
		}
		if want := `{"type":"ready"},{"type":"shown"}`; strings.Join(got, ",") != want {
			t.Errorf("%s: messages %q, want %s", page, got, want)
		}
	}
}

// TestPageRuntime checks the injected page runtime (web/xljs.js) and the
// navigation handlers: the page says ready, shows the state it gets with
// init and answers eval requests; a popup and a navigation away from the
// app are blocked.
func TestPageRuntime(t *testing.T) {
	if WebView2Version() == "" {
		t.Skip("WebView2 runtime is not installed")
	}
	runtime.LockOSThread()
	defer runtime.UnlockOSThread()
	u, err := NewNative()
	if err != nil {
		t.Fatal(err)
	}
	assets := fstest.MapFS{
		"index.html": {Data: []byte(`<!doctype html><meta charset="utf-8"><title>t</title><span id="m" data-bind="msg"></span>`)},
	}
	requests := []string{
		`{"xljs":"eval","id":1,"code":"document.getElementById('m').textContent","fn":false,"args":[]}`,
		`{"xljs":"eval","id":2,"code":"() => { window.open('about:blank'); location.href = 'file:///C:/'; return 1; }","fn":true,"args":[]}`,
		`{"xljs":"eval","id":3,"code":"location.href","fn":false,"args":[]}`,
	}
	var got []string
	Run(u, func(su UI) {
		msgs := make(chan string, 10)
		closed := make(chan struct{})
		var w Window
		w, err = su.OpenWindow(WindowOptions{
			Title:     "runtime",
			URL:       AssetOrigin + "index.html",
			Assets:    assets,
			OnMessage: func(m string, _ []string) { msgs <- m },
			OnClosed:  func() { close(closed) },
		})
		if err != nil {
			return
		}
		timeout := time.After(30 * time.Second)
	loop:
		for len(got) < len(requests) {
			select {
			case m := <-msgs:
				switch {
				case m == `{"xljs":"ready"}`:
					w.Post(`{"xljs":"init","state":{"msg":"こんにちは"},"handlers":[]}`)
					w.Post(requests[0])
				case strings.Contains(m, `"result"`) || strings.Contains(m, `"error"`):
					got = append(got, m)
					if len(got) < len(requests) {
						time.Sleep(300 * time.Millisecond) // let a navigation start
						w.Post(requests[len(got)])
					}
				}
			case <-timeout:
				break loop
			}
		}
		w.Close()
		<-closed
	})
	if err != nil {
		t.Fatal(err)
	}
	want := []string{
		`{"xljs":"result","id":1,"value":"こんにちは"}`,
		`{"xljs":"result","id":2,"value":1}`,
		`{"xljs":"result","id":3,"value":"https://app.xljs/index.html"}`,
	}
	if strings.Join(got, "\n") != strings.Join(want, "\n") {
		t.Errorf("messages:\n%s\nwant:\n%s", strings.Join(got, "\n"), strings.Join(want, "\n"))
	}
}

// builtinOnly serves BuiltinFS under BuiltinPrefix.
type builtinOnly struct{}

func (builtinOnly) Open(name string) (fs.File, error) {
	if rest, ok := strings.CutPrefix(name, BuiltinPrefix); ok {
		return BuiltinFS.Open(rest)
	}
	return nil, fs.ErrNotExist
}
