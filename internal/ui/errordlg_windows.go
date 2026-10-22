//go:build windows

package ui

import (
	"runtime"
	"strings"
	"sync"
	"syscall"
	"unsafe"

	"github.com/cool0707/xljs/internal/i18n"
	"github.com/mattn/go-runewidth"
	"golang.org/x/sys/windows"
)

var (
	procOpenClipboard    = moduser32.NewProc("OpenClipboard")
	procEmptyClipboard   = moduser32.NewProc("EmptyClipboard")
	procSetClipboardData = moduser32.NewProc("SetClipboardData")
	procCloseClipboard   = moduser32.NewProc("CloseClipboard")
	procLoadIconW        = moduser32.NewProc("LoadIconW")
	procMessageBeep      = moduser32.NewProc("MessageBeep")
	procGlobalAlloc      = modkernel.NewProc("GlobalAlloc")
	procGlobalLock       = modkernel.NewProc("GlobalLock")
	procGlobalUnlock     = modkernel.NewProc("GlobalUnlock")
	procGlobalFree       = modkernel.NewProc("GlobalFree")
	procRtlMoveMemory    = modkernel.NewProc("RtlMoveMemory")
	procPostMessageW     = moduser32.NewProc("PostMessageW")
)

// testAutoClose makes dialogs press their buttons by themselves (tests only).
var testAutoClose bool

const (
	wsVScroll     = 0x00200000
	wsExTopmost   = 0x00000008
	esMultiline   = 0x0004
	esAutoVScroll = 0x0040
	esReadOnly    = 0x0800
	ssIcon        = 0x0003
	stmSetIcon    = 0x0170
	emSetLimit    = 0x00C5
	cfUnicodeText = 13
	gmemMoveable  = 0x0002
	idiError      = 32513
	idCopy        = 101
	idText        = 102
	idIcon        = 103
)

// setClipboardText puts s on the clipboard as Unicode text.
func setClipboardText(hwnd uintptr, s string) bool {
	u, err := windows.UTF16FromString(s)
	if err != nil {
		return false
	}
	if r, _, _ := procOpenClipboard.Call(hwnd); r == 0 {
		return false
	}
	defer procCloseClipboard.Call()
	procEmptyClipboard.Call()
	size := uintptr(len(u) * 2)
	h, _, _ := procGlobalAlloc.Call(gmemMoveable, size)
	if h == 0 {
		return false
	}
	p, _, _ := procGlobalLock.Call(h)
	if p == 0 {
		procGlobalFree.Call(h)
		return false
	}
	procRtlMoveMemory.Call(p, uintptr(unsafe.Pointer(&u[0])), size)
	runtime.KeepAlive(u)
	procGlobalUnlock.Call(h)
	if r, _, _ := procSetClipboardData.Call(cfUnicodeText, h); r == 0 {
		procGlobalFree.Call(h)
		return false
	}
	return true // the clipboard owns the memory now
}

type errorState struct {
	text string // with CRLF line breaks
}

var (
	curError      *errorState
	errorProcOnce sync.Once
	errorProcPtr  uintptr
)

func errorDlgProc(hwnd, msg, wparam, lparam uintptr) uintptr {
	st := curError
	switch msg {
	case wmInitDialog:
		procSetForegroundWindow.Call(hwnd)
		if icon, _, _ := procLoadIconW.Call(0, idiError); icon != 0 {
			procSendMessageW.Call(dlgItem(hwnd, idIcon), stmSetIcon, icon, 0)
		}
		edit := dlgItem(hwnd, idText)
		procSendMessageW.Call(edit, emSetLimit, 0, 0)
		if st != nil {
			p := wstr(st.text)
			procSetDlgItemTextW.Call(hwnd, idText, uintptr(unsafe.Pointer(p)))
			runtime.KeepAlive(p)
		}
		procSetFocus.Call(dlgItem(hwnd, idOK))
		if testAutoClose {
			procPostMessageW.Call(hwnd, wmCommand, idCopy, 0)
			procPostMessageW.Call(hwnd, wmCommand, idOK, 0)
		}
		return 0
	case wmCommand:
		switch wparam & 0xFFFF {
		case idCopy:
			if st != nil && setClipboardText(hwnd, st.text) {
				p := wstr(i18n.T("ui.copied"))
				procSetDlgItemTextW.Call(hwnd, idCopy, uintptr(unsafe.Pointer(p)))
				runtime.KeepAlive(p)
			}
			return 1
		case idOK, idCancel:
			procEndDialog.Call(hwnd, idOK)
			return 1
		}
	case wmClose:
		procEndDialog.Call(hwnd, idOK)
		return 1
	}
	return 0
}

// errorDialog shows message in a read-only, selectable text box with Copy
// and OK buttons. It returns false if the dialog could not be created.
func errorDialog(title, message string) bool {
	errorProcOnce.Do(func() { errorProcPtr = syscall.NewCallback(errorDlgProc) })

	text := strings.ReplaceAll(strings.ReplaceAll(message, "\r\n", "\n"), "\n", "\r\n")
	text = strings.ReplaceAll(text, "\t", "    ")

	const (
		cx     = 340
		margin = 7
		iconW  = 21
		textX  = margin + iconW + 6
		textW  = cx - textX - margin
		btnH   = 14
	)
	// Estimate the height the text needs (about 4 DLU per half-width char,
	// 9 DLU per line), within limits; longer text scrolls.
	lines := 0
	for _, l := range strings.Split(text, "\r\n") {
		w := runewidth.StringWidth(l)
		n := (w*4 + textW - 12) / (textW - 12)
		if n < 1 {
			n = 1
		}
		lines += n
	}
	textH := lines*9 + 6
	if textH < 34 {
		textH = 34
	}
	if textH > 190 {
		textH = 190
	}
	btnY := margin + textH + 7
	cy := btnY + btnH + margin

	copyLabel := i18n.T("ui.copy")
	copyW := int16(60)

	d := &dlgBuilder{}
	d.u32(wsPopup | wsCaption | wsSysMenu | dsSetFont | dsModalFrame | dsCenter | dsSetForeground)
	d.u32(wsExTopmost)
	d.u16(4)
	d.u16(0)
	d.u16(0)
	d.u16(cx)
	d.u16(uint16(cy))
	d.u16(0)
	d.u16(0)
	d.str(title)
	d.u16(9)
	d.str("Segoe UI")
	d.item(ssIcon, margin, margin, iconW, 20, idIcon, 0x0082, "")
	d.item(wsBorder|wsVScroll|wsTabStop|esMultiline|esReadOnly|esAutoVScroll, textX, margin, textW, int16(textH), idText, 0x0081, "")
	d.item(wsTabStop, int16(cx-margin-50-4)-copyW, int16(btnY), copyW, btnH, idCopy, 0x0080, copyLabel)
	d.item(wsTabStop|bsDefPushButton, cx-margin-50, int16(btnY), 50, btnH, idOK, 0x0080, "OK")

	words := make([]uint32, (len(d.b)+3)/4)
	copy(unsafe.Slice((*byte)(unsafe.Pointer(&words[0])), len(words)*4), d.b)

	curError = &errorState{text: text}
	defer func() { curError = nil }()
	procMessageBeep.Call(mbIconError)
	hinst, _, _ := procGetModuleHandleW.Call(0)
	r, _, _ := procDialogBoxIndirectParamW.Call(hinst, uintptr(unsafe.Pointer(&words[0])), 0, errorProcPtr, 0)
	runtime.KeepAlive(words)
	return r != 0 && r != ^uintptr(0)
}
