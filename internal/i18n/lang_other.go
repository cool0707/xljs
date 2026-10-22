//go:build !windows

package i18n

import "os"

// systemLanguage returns the language tag from the POSIX locale variables.
func systemLanguage() string {
	for _, k := range []string{"LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"} {
		if v := os.Getenv(k); v != "" && v != "C" && v != "POSIX" {
			return v
		}
	}
	return ""
}
