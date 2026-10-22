//go:build !windows

package api

import (
	"os"
	"path/filepath"
	"strings"
)

func knownFolder(name string) string {
	home, err := os.UserHomeDir()
	if err != nil {
		return ""
	}
	switch strings.ToLower(name) {
	case "desktop":
		return filepath.Join(home, "Desktop")
	case "documents":
		return filepath.Join(home, "Documents")
	case "downloads":
		return filepath.Join(home, "Downloads")
	case "music":
		return filepath.Join(home, "Music")
	case "pictures":
		return filepath.Join(home, "Pictures")
	case "videos":
		return filepath.Join(home, "Videos")
	}
	return ""
}
