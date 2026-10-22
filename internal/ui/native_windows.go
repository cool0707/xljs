//go:build windows

package ui

import (
	"encoding/binary"
	"fmt"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"syscall"
	"time"
	"unicode/utf16"
	"unsafe"

	"github.com/mattn/go-runewidth"
	"golang.org/x/sys/windows"
)

// Native implements UI with Win32 dialogs. All methods must be called from
// the thread that created it (the locked main thread).
type Native struct{}

var comOnce sync.Once
var comErr error

// NewNative initialises COM on the calling thread and returns the native UI.
// The caller must have locked the goroutine to its OS thread.
func NewNative() (UI, error) {
	comOnce.Do(func() {
		hr, _, _ := procCoInitializeEx.Call(0, windows.COINIT_APARTMENTTHREADED|windows.COINIT_DISABLE_OLE1DDE)
		// S_FALSE (already initialised) and RPC_E_CHANGED_MODE are fine.
		if failed(hr) && uint32(hr) != 0x80010106 {
			comErr = hrError("CoInitializeEx", hr)
		}
	})
	if comErr != nil {
		return nil, comErr
	}
	return &Native{}, nil
}

// NativeAvailable reports whether NewNative can succeed.
func NativeAvailable() bool { return true }

func (n *Native) Name() string { return "native" }

const (
	mbOK              = 0x0
	mbOKCancel        = 0x1
	mbIconError       = 0x10
	mbIconQuestion    = 0x20
	mbIconInformation = 0x40
	mbSetForeground   = 0x10000
	mbTopmost         = 0x40000
	idOK              = 1
)

func messageBox(title, message string, flags uintptr) uintptr {
	t, m := wstr(title), wstr(message)
	r, _, _ := procMessageBoxW.Call(0, uintptr(unsafe.Pointer(m)), uintptr(unsafe.Pointer(t)),
		flags|mbSetForeground|mbTopmost)
	runtime.KeepAlive(t)
	runtime.KeepAlive(m)
	return r
}

func (n *Native) Alert(title, message string) {
	messageBox(title, message, mbOK|mbIconInformation)
}

func (n *Native) Confirm(title, message string) bool {
	return messageBox(title, message, mbOKCancel|mbIconQuestion) == idOK
}

func (n *Native) ShowError(title, message string) {
	messageBox(title, message, mbOK|mbIconError)
}

// ---- File dialogs (IFileOpenDialog / IFileSaveDialog) ----

var (
	clsidFileOpenDialog = mustGUID("{DC1C5A9C-E88A-4DDE-A5A1-60F82A20AEF7}")
	clsidFileSaveDialog = mustGUID("{C0B4E2F3-BA21-4773-8DBA-335EC946EB8B}")
	iidIFileOpenDialog  = mustGUID("{D57C7288-D4AD-4768-BE02-9D969532D960}")
	iidIFileSaveDialog  = mustGUID("{84BCCD23-5FDE-4CDB-AEA4-AF64B83D78AB}")
	iidIShellItem       = mustGUID("{43826D1E-E718-42EE-BC55-A1E261C37BFE}")
)

// vtable indices
const (
	fdShow                = 3
	fdSetFileTypes        = 4
	fdSetOptions          = 9
	fdGetOptions          = 10
	fdSetFolder           = 12
	fdSetFileName         = 15
	fdSetTitle            = 17
	fdGetResult           = 20
	fdSetDefaultExtension = 22
	fodGetResults         = 27

	siGetDisplayName = 5
	siaGetCount      = 7
	siaGetItemAt     = 8
)

const (
	fosOverwritePrompt  = 0x2
	fosNoChangeDir      = 0x8
	fosPickFolders      = 0x20
	fosForceFileSystem  = 0x40
	fosAllowMultiselect = 0x200
	fosPathMustExist    = 0x800
	fosFileMustExist    = 0x1000
	sigdnFileSysPath    = 0x80058000
)

type filterSpec struct {
	name *uint16
	spec *uint16
}

func shellItemPath(item *comObject) (string, error) {
	var p *uint16
	hr := item.call(siGetDisplayName, sigdnFileSysPath, uintptr(unsafe.Pointer(&p)))
	if failed(hr) {
		return "", hrError("IShellItem.GetDisplayName", hr)
	}
	defer windows.CoTaskMemFree(unsafe.Pointer(p))
	return windows.UTF16PtrToString(p), nil
}

func (n *Native) fileDialog(save, folder bool, o PickerOptions) ([]string, error) {
	clsid, iid := &clsidFileOpenDialog, &iidIFileOpenDialog
	if save {
		clsid, iid = &clsidFileSaveDialog, &iidIFileSaveDialog
	}
	dlg, err := coCreate(clsid, iid)
	if err != nil {
		return nil, err
	}
	defer dlg.release()

	var opts uint32
	dlg.call(fdGetOptions, uintptr(unsafe.Pointer(&opts)))
	opts |= fosForceFileSystem | fosNoChangeDir
	switch {
	case folder:
		opts |= fosPickFolders | fosPathMustExist
	case save:
		opts |= fosOverwritePrompt | fosPathMustExist
	default:
		opts |= fosFileMustExist | fosPathMustExist
		if o.Multiple {
			opts |= fosAllowMultiselect
		}
	}
	dlg.call(fdSetOptions, uintptr(opts))

	var keep []any
	if o.Title != "" {
		t := wstr(o.Title)
		keep = append(keep, t)
		dlg.call(fdSetTitle, uintptr(unsafe.Pointer(t)))
	}
	if !folder && len(o.Filters) > 0 {
		specs := make([]filterSpec, 0, len(o.Filters))
		for _, f := range o.Filters {
			name := f.Description
			pat := strings.Join(f.Patterns, ";")
			if name == "" {
				name = pat
			}
			specs = append(specs, filterSpec{wstr(name), wstr(pat)})
		}
		keep = append(keep, specs)
		dlg.call(fdSetFileTypes, uintptr(len(specs)), uintptr(unsafe.Pointer(&specs[0])))
		if save && len(o.Filters[0].Patterns) > 0 {
			ext := strings.TrimPrefix(o.Filters[0].Patterns[0], "*.")
			if ext != "" && !strings.ContainsAny(ext, "*?") {
				e := wstr(ext)
				keep = append(keep, e)
				dlg.call(fdSetDefaultExtension, uintptr(unsafe.Pointer(e)))
			}
		}
	}
	if o.StartIn != "" {
		if abs, err := filepath.Abs(o.StartIn); err == nil {
			var item *comObject
			p := wstr(abs)
			hr, _, _ := procSHCreateItemFromParsingName.Call(uintptr(unsafe.Pointer(p)), 0,
				uintptr(unsafe.Pointer(&iidIShellItem)), uintptr(unsafe.Pointer(&item)))
			runtime.KeepAlive(p)
			if !failed(hr) && item != nil {
				dlg.call(fdSetFolder, uintptr(unsafe.Pointer(item)))
				item.release()
			}
		}
	}
	if save && o.SuggestedName != "" {
		s := wstr(o.SuggestedName)
		keep = append(keep, s)
		dlg.call(fdSetFileName, uintptr(unsafe.Pointer(s)))
	}

	hr := dlg.call(fdShow, 0)
	runtime.KeepAlive(keep)
	if uint32(hr) == hrErrorCancelled {
		return nil, nil
	}
	if failed(hr) {
		return nil, hrError("IFileDialog.Show", hr)
	}

	if !save && !folder && o.Multiple {
		var arr *comObject
		if hr := dlg.call(fodGetResults, uintptr(unsafe.Pointer(&arr))); failed(hr) {
			return nil, hrError("GetResults", hr)
		}
		defer arr.release()
		var count uint32
		arr.call(siaGetCount, uintptr(unsafe.Pointer(&count)))
		out := make([]string, 0, count)
		for i := uint32(0); i < count; i++ {
			var item *comObject
			if hr := arr.call(siaGetItemAt, uintptr(i), uintptr(unsafe.Pointer(&item))); failed(hr) {
				continue
			}
			if p, err := shellItemPath(item); err == nil {
				out = append(out, p)
			}
			item.release()
		}
		return out, nil
	}
	var item *comObject
	if hr := dlg.call(fdGetResult, uintptr(unsafe.Pointer(&item))); failed(hr) {
		return nil, hrError("GetResult", hr)
	}
	defer item.release()
	p, err := shellItemPath(item)
	if err != nil {
		return nil, err
	}
	return []string{p}, nil
}

func (n *Native) OpenFiles(o PickerOptions) []string {
	paths, err := n.fileDialog(false, false, o)
	if err != nil {
		n.ShowError(o.Title, err.Error())
	}
	return paths
}

func (n *Native) SaveFile(o PickerOptions) (string, bool) {
	paths, err := n.fileDialog(true, false, o)
	if err != nil {
		n.ShowError(o.Title, err.Error())
	}
	if len(paths) == 0 {
		return "", false
	}
	return paths[0], true
}

func (n *Native) PickDirectory(o PickerOptions) (string, bool) {
	paths, err := n.fileDialog(false, true, o)
	if err != nil {
		n.ShowError(o.Title, err.Error())
	}
	if len(paths) == 0 {
		return "", false
	}
	return paths[0], true
}

// ---- Prompt (in-memory dialog template) ----

const (
	wsPopup         = 0x80000000
	wsChild         = 0x40000000
	wsVisible       = 0x10000000
	wsCaption       = 0x00C00000
	wsSysMenu       = 0x00080000
	wsBorder        = 0x00800000
	wsTabStop       = 0x00010000
	dsSetFont       = 0x40
	dsModalFrame    = 0x80
	dsSetForeground = 0x200
	dsCenter        = 0x800
	ssNoPrefix      = 0x80
	esAutoHScroll   = 0x80
	bsDefPushButton = 0x1

	wmInitDialog = 0x0110
	wmCommand    = 0x0111
	wmClose      = 0x0010
	emSetSel     = 0x00B1

	idCancel = 2
	idEdit   = 100
)

type dlgBuilder struct{ b []byte }

func (d *dlgBuilder) u16(v uint16) { d.b = binary.LittleEndian.AppendUint16(d.b, v) }
func (d *dlgBuilder) u32(v uint32) { d.b = binary.LittleEndian.AppendUint32(d.b, v) }
func (d *dlgBuilder) str(s string) {
	for _, c := range utf16.Encode([]rune(s)) {
		d.u16(c)
	}
	d.u16(0)
}
func (d *dlgBuilder) align4() {
	for len(d.b)%4 != 0 {
		d.b = append(d.b, 0)
	}
}
func (d *dlgBuilder) item(style uint32, x, y, cx, cy int16, id uint16, class uint16, text string) {
	d.align4()
	d.u32(style | wsChild | wsVisible)
	d.u32(0)
	d.u16(uint16(x))
	d.u16(uint16(y))
	d.u16(uint16(cx))
	d.u16(uint16(cy))
	d.u16(id)
	d.u16(0xFFFF)
	d.u16(class)
	d.str(text)
	d.u16(0)
}

type promptState struct {
	def    string
	result string
	ok     bool
}

var (
	curPrompt   *promptState
	dlgProcOnce sync.Once
	dlgProcPtr  uintptr
)

func dlgItem(hwnd uintptr, id int) uintptr {
	h, _, _ := procGetDlgItem.Call(hwnd, uintptr(id))
	return h
}

func promptDlgProc(hwnd, msg, wparam, lparam uintptr) uintptr {
	st := curPrompt
	switch msg {
	case wmInitDialog:
		procSetForegroundWindow.Call(hwnd)
		if st != nil {
			p := wstr(st.def)
			procSetDlgItemTextW.Call(hwnd, idEdit, uintptr(unsafe.Pointer(p)))
			runtime.KeepAlive(p)
		}
		edit := dlgItem(hwnd, idEdit)
		procSendMessageW.Call(edit, emSetSel, 0, ^uintptr(0))
		procSetFocus.Call(edit)
		return 0
	case wmCommand:
		switch wparam & 0xFFFF {
		case idOK:
			edit := dlgItem(hwnd, idEdit)
			n, _, _ := procGetWindowTextLengthW.Call(edit)
			buf := make([]uint16, n+1)
			procGetDlgItemTextW.Call(hwnd, idEdit, uintptr(unsafe.Pointer(&buf[0])), n+1)
			if st != nil {
				st.result = windows.UTF16ToString(buf)
				st.ok = true
			}
			procEndDialog.Call(hwnd, idOK)
			return 1
		case idCancel:
			procEndDialog.Call(hwnd, idCancel)
			return 1
		}
	case wmClose:
		procEndDialog.Call(hwnd, idCancel)
		return 1
	}
	return 0
}

func (n *Native) Prompt(title, message, def string) (string, bool) {
	dlgProcOnce.Do(func() { dlgProcPtr = syscall.NewCallback(promptDlgProc) })

	const cx = 240
	const margin = 7
	textW := cx - 2*margin
	// Estimate the number of text lines (about 4 DLU per half-width char).
	lines := 0
	for _, para := range strings.Split(strings.ReplaceAll(message, "\r\n", "\n"), "\n") {
		w := runewidth.StringWidth(para)
		l := (w*4 + textW - 1) / textW
		if l < 1 {
			l = 1
		}
		lines += l
	}
	if lines > 20 {
		lines = 20
	}
	textH := lines * 9
	editY := margin + textH + 4
	btnY := editY + 14 + 7
	cy := btnY + 14 + margin

	d := &dlgBuilder{}
	d.u32(wsPopup | wsCaption | wsSysMenu | dsSetFont | dsModalFrame | dsCenter | dsSetForeground)
	d.u32(0)
	d.u16(4) // item count
	d.u16(0)
	d.u16(0)
	d.u16(cx)
	d.u16(uint16(cy))
	d.u16(0) // menu
	d.u16(0) // class
	d.str(title)
	d.u16(9)
	d.str("Segoe UI") // CJK text falls back through font linking
	d.item(ssNoPrefix, margin, margin, int16(textW), int16(textH), 0xFFFF, 0x0082, message)
	d.item(wsBorder|wsTabStop|esAutoHScroll, margin, int16(editY), int16(textW), 14, idEdit, 0x0081, "")
	d.item(wsTabStop|bsDefPushButton, cx-margin-50-4-50, int16(btnY), 50, 14, idOK, 0x0080, "OK")
	d.item(wsTabStop, cx-margin-50, int16(btnY), 50, 14, idCancel, 0x0080, "キャンセル")

	// The template must be DWORD aligned.
	words := make([]uint32, (len(d.b)+3)/4)
	copy(unsafe.Slice((*byte)(unsafe.Pointer(&words[0])), len(words)*4), d.b)

	st := &promptState{def: def}
	curPrompt = st
	defer func() { curPrompt = nil }()
	hinst, _, _ := procGetModuleHandleW.Call(0)
	procDialogBoxIndirectParamW.Call(hinst, uintptr(unsafe.Pointer(&words[0])), 0, dlgProcPtr, 0)
	runtime.KeepAlive(words)
	return st.result, st.ok
}

// ---- Progress (IProgressDialog) ----

var (
	clsidProgressDialog = mustGUID("{F8383852-FCD3-11D1-A6B9-006097DF5BD4}")
	iidIProgressDialog  = mustGUID("{EBBC7C04-315E-11D2-B62F-006097DF5BD4}")
)

const (
	pdStart        = 3
	pdStop         = 4
	pdSetTitle     = 5
	pdHasCancelled = 7
	pdSetProgress  = 8
	pdSetLine      = 10
	pdTimer        = 12

	progdlgAutoTime = 0x2
	progdlgNoTime   = 0x4
	progdlgMarquee  = 0x20
	progdlgNoCancel = 0x40
	pdtimerReset    = 0x1
)

type nativeProgress struct {
	dlg    *comObject
	total  int
	last   time.Time
	closed bool
}

func (n *Native) Progress(o ProgressOptions) Progress {
	activeProgress.Add(1)
	dlg, err := coCreate(&clsidProgressDialog, &iidIProgressDialog)
	if err != nil {
		// Fall back to a console indicator so scripts keep working.
		activeProgress.Add(-1)
		return NewConsole().Progress(o)
	}
	p := &nativeProgress{dlg: dlg, total: o.Total}
	t := wstr(o.Title)
	dlg.call(pdSetTitle, uintptr(unsafe.Pointer(t)))
	runtime.KeepAlive(t)
	var flags uintptr
	if o.Total <= 0 {
		flags |= progdlgMarquee | progdlgNoTime
	} else {
		flags |= progdlgAutoTime
	}
	if !o.Cancellable {
		flags |= progdlgNoCancel
	}
	dlg.call(pdStart, 0, 0, flags, 0)
	dlg.call(pdTimer, pdtimerReset, 0)
	p.setLines(0, o.Message)
	return p
}

func (p *nativeProgress) setLine(n int, s string) {
	u := wstr(s)
	p.dlg.call(pdSetLine, uintptr(n), uintptr(unsafe.Pointer(u)), 1, 0)
	runtime.KeepAlive(u)
}

func (p *nativeProgress) setLines(done int, message string) {
	p.setLine(1, message)
	if p.total > 0 {
		p.setLine(2, fmt.Sprintf("%d / %d", done, p.total))
		p.dlg.call(pdSetProgress, uintptr(uint32(done)), uintptr(uint32(p.total)))
	} else if done > 0 {
		p.setLine(2, fmt.Sprintf("%d", done))
	}
}

func (p *nativeProgress) Update(done, total int, message string) {
	if p.closed {
		return
	}
	if total > 0 {
		p.total = total
	}
	now := time.Now()
	if now.Sub(p.last) < 50*time.Millisecond && done < p.total {
		return
	}
	p.last = now
	p.setLines(done, message)
}

func (p *nativeProgress) Cancelled() bool {
	if p.closed {
		return false
	}
	return p.dlg.call(pdHasCancelled) != 0 || Interrupted()
}

func (p *nativeProgress) Close() {
	if p.closed {
		return
	}
	p.closed = true
	p.dlg.call(pdStop)
	p.dlg.release()
	activeProgress.Add(-1)
}
