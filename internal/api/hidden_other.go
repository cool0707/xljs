//go:build !windows

package api

import "io/fs"

// hasHiddenAttribute: other systems mark hidden files by the leading "."
// only (see isHidden).
func hasHiddenAttribute(fs.DirEntry) bool { return false }
