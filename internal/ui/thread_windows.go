//go:build windows

package ui

import (
	"sync"
	"syscall"
	"time"
	"unsafe"
)

var (
	procRegisterClassExW = moduser32.NewProc("RegisterClassExW")
	procCreateWindowExW  = moduser32.NewProc("CreateWindowExW")
	procDefWindowProcW   = moduser32.NewProc("DefWindowProcW")
	procDestroyWindow    = moduser32.NewProc("DestroyWindow")
	procGetMessageW      = moduser32.NewProc("GetMessageW")
	procTranslateMessage = moduser32.NewProc("TranslateMessage")
	procDispatchMessageW = moduser32.NewProc("DispatchMessageW")
)

const (
	wmApp       = 0x8000
	wmRunQueued = wmApp + 1
	hwndMessage = ^uintptr(2) // (HWND)-3
)

type wndClassEx struct {
	size       uint32
	style      uint32
	wndProc    uintptr
	clsExtra   int32
	wndExtra   int32
	instance   uintptr
	icon       uintptr
	cursor     uintptr
	background uintptr
	menuName   *uint16
	className  *uint16
	iconSm     uintptr
}

type winMsg struct {
	hwnd    uintptr
	message uint32
	wParam  uintptr
	lParam  uintptr
	time    uint32
	pt      struct{ x, y int32 }
	private uint32
}

// winThread is the UI thread of the native UI: the main thread running the
// Win32 message loop. Queued functions are run from a message-only window,
// so they also run while a modal dialog has its own loop.
type winThread struct {
	q    queue
	hwnd uintptr
}

var (
	dispatchOnce  sync.Once
	dispatchClass *uint16
	dispatchQueue *winThread
)

func newThread(u UI) uiThread {
	if _, ok := u.(*Native); !ok {
		return newChanThread()
	}
	dispatchOnce.Do(func() {
		dispatchClass = wstr("xljsDispatch")
		hinst, _, _ := procGetModuleHandleW.Call(0)
		wc := wndClassEx{wndProc: syscall.NewCallback(dispatchProc), instance: hinst, className: dispatchClass}
		wc.size = uint32(unsafe.Sizeof(wc))
		procRegisterClassExW.Call(uintptr(unsafe.Pointer(&wc)))
	})
	t := &winThread{}
	hinst, _, _ := procGetModuleHandleW.Call(0)
	t.hwnd, _, _ = procCreateWindowExW.Call(0, uintptr(unsafe.Pointer(dispatchClass)), 0, 0,
		0, 0, 0, 0, hwndMessage, 0, hinst, 0)
	if t.hwnd == 0 {
		return newChanThread()
	}
	dispatchQueue = t
	return t
}

func dispatchProc(hwnd uintptr, msg uint32, wParam, lParam uintptr) uintptr {
	if msg == wmRunQueued && dispatchQueue != nil && dispatchQueue.hwnd == hwnd {
		dispatchQueue.q.drain()
		return 0
	}
	r, _, _ := procDefWindowProcW.Call(hwnd, uintptr(msg), wParam, lParam)
	return r
}

func (t *winThread) post(fn func()) {
	t.q.push(fn)
	procPostMessageW.Call(t.hwnd, wmRunQueued, 0, 0)
}

func (t *winThread) serve(done <-chan struct{}) {
	var m winMsg
	for {
		select {
		case <-done:
			t.q.drain()
			procDestroyWindow.Call(t.hwnd)
			dispatchQueue = nil
			return
		default:
		}
		r, _, _ := procGetMessageW.Call(uintptr(unsafe.Pointer(&m)), 0, 0, 0)
		if int32(r) <= 0 {
			// A stray WM_QUIT (or an error): keep serving until the script ends.
			continue
		}
		procTranslateMessage.Call(uintptr(unsafe.Pointer(&m)))
		procDispatchMessageW.Call(uintptr(unsafe.Pointer(&m)))
	}
}

// Calls into the progress dialog are cross-thread round trips; scripts
// check for cancellation once per item, so reuse the answer briefly.
func (t *winThread) cancelPoll() time.Duration { return 50 * time.Millisecond }
