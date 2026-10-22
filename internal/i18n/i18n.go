// Package i18n provides localized messages. Each language is one JSON file
// in locales/ (embedded at build time); en.json is the fallback for every
// key. See locales/README.md for how to add a language.
package i18n

import (
	"embed"
	"encoding/json"
	"fmt"
	"os"
	"path"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"sync/atomic"
)

//go:embed locales/*.json
var localeFS embed.FS

// Locale is one language file.
type Locale struct {
	Lang string `json:"lang"`
	Name string `json:"name"`
	// Match lists language tags (or prefixes such as "ja") that select this
	// locale, compared case-insensitively ("ja-JP", "ja_JP.UTF-8" → "ja").
	Match []string `json:"match"`
	// Excel holds defaults for workbooks created by scripts.
	Excel struct {
		Font string `json:"font"`
	} `json:"excel"`
	Messages map[string]string `json:"messages"`
	// Translations are [regexp, replacement] pairs that translate English
	// messages produced by libraries (JavaScript engine, bundler, excelize).
	Translations [][2]string `json:"translations"`

	rules []*regexp.Regexp
}

const fallback = "en"

var (
	locales = map[string]*Locale{}
	current atomic.Pointer[Locale]
)

func init() {
	entries, err := localeFS.ReadDir("locales")
	if err != nil {
		panic(err)
	}
	for _, e := range entries {
		if path.Ext(e.Name()) != ".json" {
			continue
		}
		data, err := localeFS.ReadFile("locales/" + e.Name())
		if err != nil {
			panic(err)
		}
		loc, err := parseLocale(data)
		if err != nil {
			panic(fmt.Sprintf("i18n: locales/%s: %v", e.Name(), err))
		}
		locales[loc.Lang] = loc
	}
	if locales[fallback] == nil {
		panic("i18n: locales/en.json is missing")
	}
	SetLanguage(os.Getenv("XLJS_LANG"))
}

func parseLocale(data []byte) (*Locale, error) {
	var loc Locale
	if err := json.Unmarshal(data, &loc); err != nil {
		return nil, err
	}
	if loc.Lang == "" {
		return nil, fmt.Errorf(`"lang" is empty`)
	}
	for _, t := range loc.Translations {
		re, err := regexp.Compile(t[0])
		if err != nil {
			return nil, fmt.Errorf("translation %q: %w", t[0], err)
		}
		loc.rules = append(loc.rules, re)
	}
	return &loc, nil
}

func normTag(s string) string {
	s = strings.ToLower(strings.TrimSpace(s))
	if i := strings.IndexAny(s, ".@"); i >= 0 { // ja_JP.UTF-8
		s = s[:i]
	}
	return strings.ReplaceAll(s, "_", "-")
}

// find returns the locale for a language tag, trying the full tag first
// and then its primary language ("ja-jp" → "ja").
func find(tag string) *Locale {
	tag = normTag(tag)
	if tag == "" {
		return nil
	}
	for _, try := range []string{tag, strings.SplitN(tag, "-", 2)[0]} {
		if loc := locales[try]; loc != nil {
			return loc
		}
		for _, loc := range locales {
			for _, m := range loc.Match {
				if normTag(m) == try {
					return loc
				}
			}
		}
	}
	return nil
}

// SetLanguage selects a language by tag ("ja", "en-US", ...) or "auto"/""
// for the system language. It returns false (and keeps the system/fallback
// language) when no locale matches.
func SetLanguage(tag string) bool {
	if tag != "" && !strings.EqualFold(tag, "auto") {
		if loc := find(tag); loc != nil {
			current.Store(loc)
			return true
		}
	}
	loc := find(systemLanguage())
	if loc == nil {
		loc = locales[fallback]
	}
	current.Store(loc)
	return tag == "" || strings.EqualFold(tag, "auto")
}

// Current returns the active locale.
func Current() *Locale { return current.Load() }

// Lang returns the active language code, e.g. "ja".
func Lang() string { return Current().Lang }

// Available returns the codes of all embedded languages, sorted.
func Available() []string {
	out := make([]string, 0, len(locales))
	for k := range locales {
		out = append(out, k)
	}
	sort.Strings(out)
	return out
}

// Message returns the raw message for key in the active language, falling
// back to English and finally to the key itself.
func Message(key string) string {
	if m, ok := Current().Messages[key]; ok {
		return m
	}
	if m, ok := locales[fallback].Messages[key]; ok {
		return m
	}
	return key
}

var placeholder = regexp.MustCompile(`\{(\d+)\}`)

// T returns the message for key with {0}, {1}, ... replaced by args.
func T(key string, args ...any) string {
	return Format(Message(key), args...)
}

// Format replaces {0}, {1}, ... in msg with args.
func Format(msg string, args ...any) string {
	if len(args) == 0 {
		return msg
	}
	return placeholder.ReplaceAllStringFunc(msg, func(m string) string {
		i, _ := strconv.Atoi(m[1 : len(m)-1])
		if i < len(args) {
			return fmt.Sprint(args[i])
		}
		return m
	})
}

// Err returns an error with the localized message for key.
func Err(key string, args ...any) error {
	return &LocalizedError{Key: key, Msg: T(key, args...)}
}

// LocalizedError is an error carrying a message key (useful for tests and callers
// that want to recognize specific errors).
type LocalizedError struct {
	Key string
	Msg string
}

func (e *LocalizedError) Error() string { return e.Msg }

// Messages returns all messages of the active language merged over the
// English ones (used by the JavaScript side).
func Messages() map[string]string {
	out := make(map[string]string, len(locales[fallback].Messages))
	for k, v := range locales[fallback].Messages {
		out[k] = v
	}
	for k, v := range Current().Messages {
		out[k] = v
	}
	return out
}
