package meta

import (
	"io/fs"
	"os"
	"strings"
)

// AssetRoots lists the files and folders a script ships with: the @assets
// entries plus the @html page.
func (m *Meta) AssetRoots() []string {
	if m == nil {
		return nil
	}
	roots := append([]string(nil), m.Assets...)
	if m.HTML != "" && !covered(roots, m.HTML) {
		roots = append(roots, m.HTML)
	}
	return roots
}

// AssetFS returns the script's assets read from the script folder dir.
// Only paths under AssetRoots are visible, so that running from source
// serves the same files as the packed exe.
func (m *Meta) AssetFS(dir string) fs.FS {
	return &subsetFS{base: os.DirFS(dir), roots: m.AssetRoots()}
}

type subsetFS struct {
	base  fs.FS
	roots []string
}

func (s *subsetFS) Open(name string) (fs.File, error) {
	if !fs.ValidPath(name) {
		return nil, &fs.PathError{Op: "open", Path: name, Err: fs.ErrInvalid}
	}
	if !covered(s.roots, name) {
		return nil, &fs.PathError{Op: "open", Path: name, Err: fs.ErrNotExist}
	}
	return s.base.Open(name)
}

// covered reports whether name is one of roots or inside one of them.
func covered(roots []string, name string) bool {
	for _, r := range roots {
		if r == "." || name == r || strings.HasPrefix(name, r+"/") {
			return true
		}
	}
	return false
}
