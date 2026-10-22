//go:build windows

package ui

import (
	"runtime"
	"unsafe"

	"github.com/cool0707/xljs/internal/i18n"
	"golang.org/x/sys/windows"
)

var (
	procILCreateFromPathW          = modshell32.NewProc("ILCreateFromPathW")
	procILFindLastID               = modshell32.NewProc("ILFindLastID")
	procILFree                     = modshell32.NewProc("ILFree")
	procSHOpenFolderAndSelectItems = modshell32.NewProc("SHOpenFolderAndSelectItems")
	procShellExecuteW              = modshell32.NewProc("ShellExecuteW")
)

func pidl(path string) uintptr {
	p := wstr(path)
	r, _, _ := procILCreateFromPathW.Call(uintptr(unsafe.Pointer(p)))
	runtime.KeepAlive(p)
	return r
}

// ShellShowInFolder opens Explorer with the files selected. Files in
// different folders open one Explorer window per folder.
func ShellShowInFolder(paths []string) error {
	if err := checkPaths(paths); err != nil {
		return err
	}
	// COM must be initialised on this thread (no-op if it already is).
	procCoInitializeEx.Call(0, windows.COINIT_APARTMENTTHREADED|windows.COINIT_DISABLE_OLE1DDE)
	order, groups, err := groupByDir(paths)
	if err != nil {
		return err
	}
	for _, dir := range order {
		folder := pidl(dir)
		if folder == 0 {
			return i18n.Err("shell.cannotOpenFolder", dir)
		}
		var items, children []uintptr
		for _, f := range groups[dir] {
			if it := pidl(f); it != 0 {
				items = append(items, it)
				child, _, _ := procILFindLastID.Call(it)
				children = append(children, child)
			}
		}
		var hr uintptr
		if len(children) > 0 {
			hr, _, _ = procSHOpenFolderAndSelectItems.Call(folder, uintptr(len(children)), uintptr(unsafe.Pointer(&children[0])), 0)
		} else {
			hr, _, _ = procSHOpenFolderAndSelectItems.Call(folder, 0, 0, 0)
		}
		runtime.KeepAlive(children)
		for _, it := range items {
			procILFree.Call(it)
		}
		procILFree.Call(folder)
		if failed(hr) {
			return hrError("SHOpenFolderAndSelectItems", hr)
		}
	}
	return nil
}

// ShellOpen opens a file, folder or URL with its default application.
func ShellOpen(target string) error {
	verb, t := wstr("open"), wstr(target)
	r, _, _ := procShellExecuteW.Call(0, uintptr(unsafe.Pointer(verb)), uintptr(unsafe.Pointer(t)), 0, 0, 1 /* SW_SHOWNORMAL */)
	runtime.KeepAlive(verb)
	runtime.KeepAlive(t)
	if r <= 32 {
		return i18n.Err("shell.cannotOpen", target, r)
	}
	return nil
}
