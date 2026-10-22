//go:build windows

package ui

import (
	"encoding/json"
	"runtime"
	"testing"
	"testing/fstest"
	"time"
	"unsafe"

	"github.com/cool0707/xljs/internal/webui"
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
		"ui/app.js": {Data: []byte(`chrome.webview.postMessage("ready " + location.href + " " + document.title + " " + typeof xljs.invoke)`)},
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
	if want := "ready https://app.xljs/ui/index.html t function"; msg != want {
		t.Fatalf("message = %q, want %q", msg, want)
	}
}

// TestBuiltinPage loads the built-in form page and checks that it asks the
// script for its definition through the injected bridge (window.xljs).
func TestBuiltinPage(t *testing.T) {
	if WebView2Version() == "" {
		t.Skip("WebView2 runtime is not installed")
	}
	runtime.LockOSThread()
	defer runtime.UnlockOSThread()
	u, err := NewNative()
	if err != nil {
		t.Fatal(err)
	}
	got := make(chan string, 4)
	var first map[string]any
	Run(u, func(su UI) {
		closed := make(chan struct{})
		var w Window
		w, err = su.OpenWindow(WindowOptions{
			Title:     "フォーム",
			URL:       AssetOrigin + "form.html",
			Assets:    webui.FS,
			OnMessage: func(m string, _ []string) { got <- m },
			OnClosed:  func() { close(closed) },
		})
		if err != nil {
			return
		}
		select {
		case m := <-got:
			json.Unmarshal([]byte(m), &first)
		case <-time.After(30 * time.Second):
		}
		w.Close()
		<-closed
	})
	if err != nil {
		t.Fatal(err)
	}
	if first["type"] != "invoke" || first["name"] != "init" {
		t.Fatalf("first message = %v", first)
	}
}
