//go:build windows

package api

import (
	"io/fs"
	"syscall"
)

// hasHiddenAttribute reports whether the entry has the hidden or system
// attribute (desktop.ini, Thumbs.db, $Recycle.Bin, ...).
func hasHiddenAttribute(d fs.DirEntry) bool {
	info, err := d.Info()
	if err != nil {
		return false
	}
	if a, ok := info.Sys().(*syscall.Win32FileAttributeData); ok {
		return a.FileAttributes&(syscall.FILE_ATTRIBUTE_HIDDEN|syscall.FILE_ATTRIBUTE_SYSTEM) != 0
	}
	return false
}
