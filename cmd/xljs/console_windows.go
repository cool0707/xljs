//go:build windows

package main

import (
	"os"
	"unsafe"

	"golang.org/x/sys/windows"
)

var (
	kernel32                  = windows.NewLazySystemDLL("kernel32.dll")
	procGetConsoleWindow      = kernel32.NewProc("GetConsoleWindow")
	procGetConsoleProcessList = kernel32.NewProc("GetConsoleProcessList")
	procAttachConsole         = kernel32.NewProc("AttachConsole")
)

const attachParentProcess = ^uintptr(0) // (DWORD)-1

func validHandle(h windows.Handle) bool {
	return h != 0 && h != windows.InvalidHandle
}

// setupConsole works out whether output can be seen. A GUI-subsystem build
// (xljsw.exe) started from a terminal attaches to that terminal.
func setupConsole() *consoleState {
	st := &consoleState{}
	if hwnd, _, _ := procGetConsoleWindow.Call(); hwnd != 0 {
		st.has = true
		var pids [4]uint32
		n, _, _ := procGetConsoleProcessList.Call(uintptr(unsafe.Pointer(&pids[0])), 4)
		st.own = n == 1
		return st
	}
	outH, _ := windows.GetStdHandle(windows.STD_OUTPUT_HANDLE)
	errH, _ := windows.GetStdHandle(windows.STD_ERROR_HANDLE)
	redirected := validHandle(outH) || validHandle(errH)
	if r, _, _ := procAttachConsole.Call(attachParentProcess); r != 0 {
		if !validHandle(outH) {
			if f, err := os.OpenFile("CONOUT$", os.O_RDWR, 0); err == nil {
				os.Stdout = f
				windows.SetStdHandle(windows.STD_OUTPUT_HANDLE, windows.Handle(f.Fd()))
			}
		}
		if !validHandle(errH) {
			if f, err := os.OpenFile("CONOUT$", os.O_RDWR, 0); err == nil {
				os.Stderr = f
				windows.SetStdHandle(windows.STD_ERROR_HANDLE, windows.Handle(f.Fd()))
			}
		}
		st.has = true
		return st
	}
	st.has = redirected
	return st
}
