//go:build windows

package ui

import (
	"fmt"
	"syscall"
	"unsafe"

	"golang.org/x/sys/windows"
)

var (
	modole32   = windows.NewLazySystemDLL("ole32.dll")
	modshell32 = windows.NewLazySystemDLL("shell32.dll")
	moduser32  = windows.NewLazySystemDLL("user32.dll")
	modkernel  = windows.NewLazySystemDLL("kernel32.dll")

	procCoCreateInstance            = modole32.NewProc("CoCreateInstance")
	procCoInitializeEx              = modole32.NewProc("CoInitializeEx")
	procSHCreateItemFromParsingName = modshell32.NewProc("SHCreateItemFromParsingName")
	procMessageBoxW                 = moduser32.NewProc("MessageBoxW")
	procDialogBoxIndirectParamW     = moduser32.NewProc("DialogBoxIndirectParamW")
	procEndDialog                   = moduser32.NewProc("EndDialog")
	procSetDlgItemTextW             = moduser32.NewProc("SetDlgItemTextW")
	procGetDlgItemTextW             = moduser32.NewProc("GetDlgItemTextW")
	procGetDlgItem                  = moduser32.NewProc("GetDlgItem")
	procGetWindowTextLengthW        = moduser32.NewProc("GetWindowTextLengthW")
	procSendMessageW                = moduser32.NewProc("SendMessageW")
	procSetFocus                    = moduser32.NewProc("SetFocus")
	procSetForegroundWindow         = moduser32.NewProc("SetForegroundWindow")
	procGetModuleHandleW            = modkernel.NewProc("GetModuleHandleW")
)

const (
	clsctxInprocServer = 0x1
	hrErrorCancelled   = 0x800704C7
)

func mustGUID(s string) windows.GUID {
	g, err := windows.GUIDFromString(s)
	if err != nil {
		panic(err)
	}
	return g
}

// comObject is a raw COM interface pointer.
type comObject struct {
	vtbl unsafe.Pointer
}

// call invokes the vtable method at index idx.
func (o *comObject) call(idx int, args ...uintptr) uintptr {
	fn := *(*uintptr)(unsafe.Add(o.vtbl, uintptr(idx)*unsafe.Sizeof(uintptr(0))))
	all := make([]uintptr, 0, len(args)+1)
	all = append(all, uintptr(unsafe.Pointer(o)))
	all = append(all, args...)
	r, _, _ := syscall.SyscallN(fn, all...)
	return r
}

func (o *comObject) release() {
	if o != nil {
		o.call(2)
	}
}

func failed(hr uintptr) bool { return int32(uint32(hr)) < 0 }

func hrError(op string, hr uintptr) error {
	return fmt.Errorf("%s failed: HRESULT 0x%08X", op, uint32(hr))
}

func coCreate(clsid, iid *windows.GUID) (*comObject, error) {
	var p *comObject
	hr, _, _ := procCoCreateInstance.Call(
		uintptr(unsafe.Pointer(clsid)), 0, clsctxInprocServer,
		uintptr(unsafe.Pointer(iid)), uintptr(unsafe.Pointer(&p)))
	if failed(hr) {
		return nil, hrError("CoCreateInstance", hr)
	}
	return p, nil
}

func wstr(s string) *uint16 {
	p, err := windows.UTF16PtrFromString(s)
	if err != nil {
		// String contains NUL; truncate at it.
		for i, r := range s {
			if r == 0 {
				p, _ = windows.UTF16PtrFromString(s[:i])
				break
			}
		}
	}
	return p
}
