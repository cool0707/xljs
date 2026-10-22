//go:build windows

package i18n

import (
	"unsafe"

	"golang.org/x/sys/windows"
)

var (
	kernel32                     = windows.NewLazySystemDLL("kernel32.dll")
	procGetUserDefaultUILanguage = kernel32.NewProc("GetUserDefaultUILanguage")
	procLCIDToLocaleName         = kernel32.NewProc("LCIDToLocaleName")
)

// systemLanguage returns the Windows display language as a tag ("ja-JP").
func systemLanguage() string {
	lid, _, _ := procGetUserDefaultUILanguage.Call()
	var buf [85]uint16 // LOCALE_NAME_MAX_LENGTH
	n, _, _ := procLCIDToLocaleName.Call(lid, uintptr(unsafe.Pointer(&buf[0])), uintptr(len(buf)), 0)
	if n == 0 {
		return ""
	}
	return windows.UTF16ToString(buf[:])
}
