package ui

import (
	"io/fs"
	"path"
	"regexp"
	"testing"
)

// TestFontAwesome checks that the bundled Font Awesome stylesheet finds
// every font it refers to.
func TestFontAwesome(t *testing.T) {
	const css = "fontawesome/css/all.min.css"
	src, err := fs.ReadFile(BuiltinFS, css)
	if err != nil {
		t.Fatal(err)
	}
	urls := regexp.MustCompile(`url\(([^)]+)\)`).FindAllSubmatch(src, -1)
	if len(urls) == 0 {
		t.Fatal("no url() in " + css)
	}
	for _, m := range urls {
		name := path.Join(path.Dir(css), string(m[1]))
		if _, err := fs.Stat(BuiltinFS, name); err != nil {
			t.Errorf("%s refers to %s: %v", css, m[1], err)
		}
	}
}
