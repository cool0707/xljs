//go:build windows

package ui

import (
	"runtime"
	"testing"
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
