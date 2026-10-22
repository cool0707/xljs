// Package i18n selects Japanese or English for messages shown to the user
// and translates common error messages from the JavaScript engine, the
// bundler, excelize and the operating system into Japanese.
package i18n

import (
	"fmt"
	"os"
	"strings"
	"sync/atomic"
)

var japanese atomic.Bool

func init() {
	japanese.Store(detect())
}

func detect() bool {
	if v := os.Getenv("XLJS_LANG"); v != "" && !strings.EqualFold(v, "auto") {
		return isJapaneseTag(v)
	}
	return systemJapanese()
}

func isJapaneseTag(s string) bool {
	s = strings.ToLower(strings.TrimSpace(s))
	return s == "ja" || strings.HasPrefix(s, "ja_") || strings.HasPrefix(s, "ja-") || strings.HasPrefix(s, "ja.") || s == "japanese"
}

// SetLanguage selects "ja", "en" or "auto" (system language).
func SetLanguage(lang string) {
	if lang == "" || strings.EqualFold(lang, "auto") {
		japanese.Store(systemJapanese())
		return
	}
	japanese.Store(isJapaneseTag(lang))
}

// Japanese reports whether messages are shown in Japanese.
func Japanese() bool { return japanese.Load() }

// Lang returns "ja" or "en".
func Lang() string {
	if Japanese() {
		return "ja"
	}
	return "en"
}

// S returns ja or en depending on the current language.
func S(ja, en string) string {
	if Japanese() {
		return ja
	}
	return en
}

// F formats the message for the current language.
func F(ja, en string, args ...any) string {
	return fmt.Sprintf(S(ja, en), args...)
}

// Errorf creates an error with a localized message.
func Errorf(ja, en string, args ...any) error {
	return fmt.Errorf(S(ja, en), args...)
}
