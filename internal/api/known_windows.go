//go:build windows

package api

import (
	"strings"

	"golang.org/x/sys/windows"
)

func knownFolder(name string) string {
	var id *windows.KNOWNFOLDERID
	switch strings.ToLower(name) {
	case "desktop":
		id = windows.FOLDERID_Desktop
	case "documents":
		id = windows.FOLDERID_Documents
	case "downloads":
		id = windows.FOLDERID_Downloads
	case "music":
		id = windows.FOLDERID_Music
	case "pictures":
		id = windows.FOLDERID_Pictures
	case "videos":
		id = windows.FOLDERID_Videos
	default:
		return ""
	}
	p, err := windows.KnownFolderPath(id, 0)
	if err != nil {
		return ""
	}
	return p
}
