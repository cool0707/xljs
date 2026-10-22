//go:build windows

package i18n

import "golang.org/x/sys/windows"

var procGetUserDefaultUILanguage = windows.NewLazySystemDLL("kernel32.dll").NewProc("GetUserDefaultUILanguage")

const langJapanese = 0x11 // primary language id of LANG_JAPANESE

func systemJapanese() bool {
	lid, _, _ := procGetUserDefaultUILanguage.Call()
	return lid&0x3ff == langJapanese
}
