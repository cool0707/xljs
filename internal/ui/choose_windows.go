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
)

const (
	bsCommandLink    = 0x0E
	bsDefCommandLink = 0x0F
	bcmSetNote       = 0x1609
	bmSetImage       = 0x00F7
	imageIcon        = 1
	choiceIconSize   = 32   // logical pixels
	idChoice         = 1000 // + the index of the choice
)

type chooseState struct {
	choices []Choice
	result  int
	icons   []uintptr // created for the choices; destroyed after the dialog
}

var (
	curChoose      *chooseState
	chooseProcOnce sync.Once
	chooseProcPtr  uintptr
)

func chooseDlgProc(hwnd, msg, wparam, lparam uintptr) uintptr {
	st := curChoose
	switch msg {
	case wmInitDialog:
		procSetForegroundWindow.Call(hwnd)
		icons := exeIcons()
		procSendMessageW.Call(hwnd, wmSetIcon, iconBig, icons[0])
		procSendMessageW.Call(hwnd, wmSetIcon, iconSmall, icons[1])
		if st != nil {
			dpi, _, _ := procGetDpiForSystem.Call()
			if dpi == 0 {
				dpi = 96
			}
			for i, c := range st.choices {
				btn := dlgItem(hwnd, idChoice+i)
				if c.Note != "" {
					p := wstr(c.Note)
					procSendMessageW.Call(btn, bcmSetNote, 0, uintptr(unsafe.Pointer(p)))
					runtime.KeepAlive(p)
				}
				if len(c.Icon) > 0 {
					if icon := loadIconSize(c.Icon, choiceIconSize*dpi/96); icon != 0 {
						st.icons = append(st.icons, icon)
						procSendMessageW.Call(btn, bmSetImage, imageIcon, icon)
					}
				}
			}
		}
		procSetFocus.Call(dlgItem(hwnd, idChoice))
		if testAutoClose {
			procPostMessageW.Call(hwnd, wmCommand, idChoice, 0)
		}
		return 0
	case wmCommand:
		id := int(wparam & 0xFFFF)
		if st != nil && id >= idChoice && id < idChoice+len(st.choices) {
			st.result = id - idChoice
			procEndDialog.Call(hwnd, idOK)
			return 1
		}
		if id == idCancel {
			procEndDialog.Call(hwnd, idCancel)
			return 1
		}
	case wmClose:
		procEndDialog.Call(hwnd, idCancel)
		return 1
	}
	return 0
}

// textLines estimates the number of lines s takes in width dialog units
// (about 4 DLU per half-width character).
func textLines(s string, width int) int {
	lines := 0
	for _, para := range strings.Split(strings.ReplaceAll(s, "\r\n", "\n"), "\n") {
		l := (runewidth.StringWidth(para)*4 + width - 1) / width
		if l < 1 {
			l = 1
		}
		lines += l
	}
	return lines
}

// Choose shows the choices as command links (a large label with a note
// under it) above a Cancel button.
func (n *Native) Choose(title, message string, choices []Choice) int {
	chooseProcOnce.Do(func() { chooseProcPtr = syscall.NewCallback(chooseDlgProc) })

	const (
		cx     = 280
		margin = 7
		textW  = cx - 2*margin
		btnH   = 14
	)
	y := margin
	d := &dlgBuilder{}
	var items dlgBuilder // the controls, after the header that counts them
	count := 0
	add := func(style uint32, x, y, cx, cy int, id int, class uint16, text string) {
		items.item(style, int16(x), int16(y), int16(cx), int16(cy), uint16(id), class, text)
		count++
	}
	if message != "" {
		h := textLines(message, textW) * 9
		add(ssNoPrefix, margin, y, textW, h, 0xFFFF, 0x0082, message)
		y += h + 4
	}
	for i, c := range choices {
		// The label is in a larger font; the note starts after the arrow
		// (or the icon).
		h := 18
		if c.Note != "" {
			h += textLines(c.Note, textW-40) * 9
		}
		if len(c.Icon) > 0 && h < 26 {
			h = 26
		}
		style := uint32(wsTabStop | bsCommandLink)
		if i == 0 {
			style = wsTabStop | bsDefCommandLink
		}
		add(style, margin, y, textW, h, idChoice+i, 0x0080, c.Label)
		y += h + 2
	}
	y += 5
	add(wsTabStop, cx-margin-50, y, 50, btnH, idCancel, 0x0080, i18n.T("ui.cancel"))
	cy := y + btnH + margin

	d.u32(wsPopup | wsCaption | wsSysMenu | dsSetFont | dsModalFrame | dsCenter | dsSetForeground)
	d.u32(0)
	d.u16(uint16(count))
	d.u16(0)
	d.u16(0)
	d.u16(cx)
	d.u16(uint16(cy))
	d.u16(0) // menu
	d.u16(0) // class
	d.str(title)
	d.u16(9)
	d.str("Segoe UI")
	d.align4()
	d.b = append(d.b, items.b...) // items.b starts DWORD aligned

	words := make([]uint32, (len(d.b)+3)/4)
	copy(unsafe.Slice((*byte)(unsafe.Pointer(&words[0])), len(words)*4), d.b)

	st := &chooseState{choices: choices, result: -1}
	curChoose = st
	defer func() {
		curChoose = nil
		for _, icon := range st.icons {
			procDestroyIcon.Call(icon)
		}
	}()
	hinst, _, _ := procGetModuleHandleW.Call(0)
	procDialogBoxIndirectParamW.Call(hinst, uintptr(unsafe.Pointer(&words[0])), n.owner(), chooseProcPtr, 0)
	runtime.KeepAlive(words)
	return st.result
}
