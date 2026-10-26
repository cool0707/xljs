// Package meta reads the metadata block at the top of an entry script and
// gives access to the files (assets) a script ships with.
//
//	// ==xljs==
//	// @mode    app            // console (default) | app
//	// @html    ui/index.html  // first page, relative to the script
//	// @assets  ui/            // folders bundled into the exe (repeatable)
//	// @title   売上集計ツール
//	// @size    960x640
//	// @icon    ui/app.ico     // window icon (.ico or .png), bundled too
//	// @lang    ja
//	// ==/xljs==
//
// Only line comments before the first line of code are examined.
package meta

import (
	"bufio"
	"bytes"
	"os"
	"path"
	"regexp"
	"strconv"
	"strings"

	"github.com/cool0707/xljs/internal/i18n"
)

// Mode values.
const (
	ModeConsole = "console"
	ModeApp     = "app"
)

// Meta is the parsed metadata block. Paths are slash-separated and relative
// to the script directory.
type Meta struct {
	Mode   string   `json:"mode,omitempty"`
	HTML   string   `json:"html,omitempty"`
	Assets []string `json:"assets,omitempty"`
	Title  string   `json:"title,omitempty"`
	Width  int      `json:"width,omitempty"`
	Height int      `json:"height,omitempty"`
	Icon   string   `json:"icon,omitempty"`
	Lang   string   `json:"lang,omitempty"`
}

// IsApp reports whether the script runs in app mode (opens a main window).
func (m *Meta) IsApp() bool { return m != nil && m.Mode == ModeApp }

var (
	keyLine     = regexp.MustCompile(`^@([A-Za-z][\w-]*)(?:\s+(.*))?$`)
	trailing    = regexp.MustCompile(`\s+//.*$`)
	sizePattern = regexp.MustCompile(`^(\d+)\s*[xX×]\s*(\d+)$`)
)

// ParseFile reads the metadata block of the script at path. A script
// without a block yields an empty Meta. Warnings (unknown keys) are
// returned separately; malformed values are errors.
func ParseFile(file string) (*Meta, []string, error) {
	src, err := os.ReadFile(file)
	if err != nil {
		return nil, nil, err
	}
	return Parse(file, src)
}

// Parse reads the metadata block from src; name is used in messages.
func Parse(name string, src []byte) (*Meta, []string, error) {
	m := &Meta{}
	var warnings []string
	src = bytes.TrimPrefix(src, []byte("\xEF\xBB\xBF"))
	sc := bufio.NewScanner(bytes.NewReader(src))
	sc.Buffer(make([]byte, 64*1024), 1024*1024)
	lineNo := 0
	in, done := false, false
	at := func() string { return name + ":" + strconv.Itoa(lineNo) }
	for sc.Scan() {
		lineNo++
		line := strings.TrimSpace(sc.Text())
		if lineNo == 1 && strings.HasPrefix(line, "#!") {
			continue
		}
		if line == "" {
			continue
		}
		if !strings.HasPrefix(line, "//") {
			break // first line of code
		}
		body := strings.TrimSpace(strings.TrimPrefix(line, "//"))
		switch {
		case !in && body == "==xljs==":
			in = true
			continue
		case in && body == "==/xljs==":
			in, done = false, true
		}
		if done {
			break
		}
		if !in {
			continue
		}
		mt := keyLine.FindStringSubmatch(body)
		if mt == nil {
			continue // free text inside the block
		}
		key := strings.ToLower(mt[1])
		val := strings.TrimSpace(trailing.ReplaceAllString(mt[2], ""))
		switch key {
		case "mode":
			v := strings.ToLower(val)
			if v != ModeConsole && v != ModeApp {
				return nil, warnings, i18n.Err("meta.badMode", at(), val)
			}
			m.Mode = v
		case "html":
			p, err := relPath(val, at())
			if err != nil {
				return nil, warnings, err
			}
			m.HTML = p
		case "assets":
			for _, f := range strings.Fields(val) {
				p, err := relPath(f, at())
				if err != nil {
					return nil, warnings, err
				}
				m.Assets = append(m.Assets, p)
			}
		case "title":
			m.Title = val
		case "size":
			s := sizePattern.FindStringSubmatch(val)
			if s == nil {
				return nil, warnings, i18n.Err("meta.badSize", at(), val)
			}
			m.Width, _ = strconv.Atoi(s[1])
			m.Height, _ = strconv.Atoi(s[2])
		case "icon":
			p, err := relPath(val, at())
			if err != nil {
				return nil, warnings, err
			}
			if ext := strings.ToLower(path.Ext(p)); ext != ".ico" && ext != ".png" {
				return nil, warnings, i18n.Err("meta.badIcon", at(), val)
			}
			m.Icon = p
		case "lang":
			m.Lang = val
		default:
			warnings = append(warnings, i18n.T("meta.unknownKey", at(), "@"+mt[1]))
		}
	}
	if err := sc.Err(); err != nil {
		return nil, warnings, err
	}
	if in {
		return nil, warnings, i18n.Err("meta.unterminated", name)
	}
	return m, warnings, nil
}

// relPath validates a path given in the block: relative, inside the script
// folder. It returns the cleaned slash-separated form.
func relPath(p, at string) (string, error) {
	if p == "" {
		return "", i18n.Err("meta.badPath", at, p)
	}
	s := path.Clean(strings.ReplaceAll(p, `\`, "/"))
	if path.IsAbs(s) || s == ".." || strings.HasPrefix(s, "../") || strings.Contains(s, ":") {
		return "", i18n.Err("meta.badPath", at, p)
	}
	return s, nil
}
