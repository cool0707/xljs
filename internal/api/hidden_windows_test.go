//go:build windows

package api

import (
	"os"
	"path/filepath"
	"syscall"
	"testing"
)

func TestWalkSkipsHiddenAttribute(t *testing.T) {
	dir := t.TempDir()
	for _, name := range []string{"a.xlsx", "desktop.ini", "sys.dat", filepath.Join("hid", "b.xlsx")} {
		p := filepath.Join(dir, name)
		os.MkdirAll(filepath.Dir(p), 0o755)
		if err := os.WriteFile(p, []byte("x"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	setAttr := func(name string, attr uint32) {
		p, _ := syscall.UTF16PtrFromString(filepath.Join(dir, name))
		if err := syscall.SetFileAttributes(p, attr); err != nil {
			t.Fatal(err)
		}
	}
	setAttr("desktop.ini", syscall.FILE_ATTRIBUTE_HIDDEN|syscall.FILE_ATTRIBUTE_SYSTEM)
	setAttr("sys.dat", syscall.FILE_ATTRIBUTE_SYSTEM)
	setAttr("hid", syscall.FILE_ATTRIBUTE_HIDDEN|syscall.FILE_ATTRIBUTE_DIRECTORY)

	res, err := walk(dir, nil, nil, false, true, false)
	if err != nil {
		t.Fatal(err)
	}
	if len(res) != 1 || res[0]["rel"] != "a.xlsx" {
		t.Errorf("walk without hidden = %v, want only a.xlsx", res)
	}
	res, err = walk(dir, nil, nil, false, true, true)
	if err != nil {
		t.Fatal(err)
	}
	if len(res) != 4 {
		t.Errorf("walk with hidden = %d entries (%v), want 4", len(res), res)
	}
}
