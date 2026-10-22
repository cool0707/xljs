//go:build !windows

package i18n

import "os"

func systemJapanese() bool {
	for _, k := range []string{"LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"} {
		if v := os.Getenv(k); v != "" {
			return isJapaneseTag(v)
		}
	}
	return false
}
