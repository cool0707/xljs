package meta

import (
	"io/fs"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"

	"github.com/cool0707/xljs/internal/i18n"
)

func TestParse(t *testing.T) {
	src := "\xEF\xBB\xBF#!/usr/bin/env xljs\n" +
		"// Sales tool\n" +
		"\n" +
		"// ==xljs==\n" +
		"// @mode    app            // console（既定）| app\n" +
		"// @html    ui\\index.html\n" +
		"// @assets  ui/ img\n" +
		"// @assets  data/list.csv\n" +
		"// @title   売上集計ツール // comment\n" +
		"// @size    960x640\n" +
		"// @lang    ja\n" +
		"// @author  someone\n" +
		"// free text\n" +
		"// ==/xljs==\n" +
		"// @mode console\n" +
		"console.log(1);\n"
	m, warns, err := Parse("a.js", []byte(src))
	if err != nil {
		t.Fatal(err)
	}
	want := &Meta{Mode: "app", HTML: "ui/index.html", Assets: []string{"ui", "img", "data/list.csv"},
		Title: "売上集計ツール", Width: 960, Height: 640, Lang: "ja"}
	if !reflect.DeepEqual(m, want) {
		t.Fatalf("got %+v\nwant %+v", m, want)
	}
	if len(warns) != 1 || !strings.Contains(warns[0], "@author") || !strings.Contains(warns[0], "a.js:12") {
		t.Fatalf("warnings = %q", warns)
	}
	if !m.IsApp() {
		t.Error("IsApp")
	}
}

func TestParseNoBlock(t *testing.T) {
	for _, src := range []string{"", "console.log(1)\n// ==xljs==\n// @mode app\n// ==/xljs==\n", "/* x */\n// ==xljs==\n// @mode app\n"} {
		m, warns, err := Parse("a.js", []byte(src))
		if err != nil || len(warns) != 0 || m.IsApp() {
			t.Errorf("%q: %+v %v %v", src, m, warns, err)
		}
	}
}

func TestParseErrors(t *testing.T) {
	i18n.SetLanguage("en")
	defer i18n.SetLanguage("auto")
	for _, c := range []struct{ src, want string }{
		{"// ==xljs==\n// @mode gui\n// ==/xljs==\n", "a.js:2"},
		{"// ==xljs==\n// @size big\n// ==/xljs==\n", "@size"},
		{"// ==xljs==\n// @html ../x.html\n// ==/xljs==\n", "../x.html"},
		{"// ==xljs==\n// @assets C:/x\n// ==/xljs==\n", "C:/x"},
		{"// ==xljs==\n// @mode app\n", "==/xljs=="},
	} {
		_, _, err := Parse("a.js", []byte(c.src))
		if err == nil || !strings.Contains(err.Error(), c.want) {
			t.Errorf("%q: err = %v, want it to mention %q", c.src, err, c.want)
		}
	}
}

func TestAssetFS(t *testing.T) {
	dir := t.TempDir()
	for _, f := range []string{"main.js", "ui/index.html", "ui/css/a.css", "uix/b.txt", "page.html"} {
		p := filepath.Join(dir, filepath.FromSlash(f))
		os.MkdirAll(filepath.Dir(p), 0o755)
		os.WriteFile(p, []byte(f), 0o644)
	}
	m := &Meta{HTML: "page.html", Assets: []string{"ui"}}
	if got := m.AssetRoots(); !reflect.DeepEqual(got, []string{"ui", "page.html"}) {
		t.Fatalf("roots = %q", got)
	}
	fsys := m.AssetFS(dir)
	for _, f := range []string{"ui/index.html", "ui/css/a.css", "page.html"} {
		if b, err := fs.ReadFile(fsys, f); err != nil || string(b) != f {
			t.Errorf("%s: %q %v", f, b, err)
		}
	}
	for _, f := range []string{"main.js", "uix/b.txt", "../x", "/etc/passwd"} {
		if _, err := fs.ReadFile(fsys, f); err == nil {
			t.Errorf("%s should not be visible", f)
		}
	}
	if (&Meta{HTML: "ui/index.html", Assets: []string{"ui"}}).AssetRoots()[0] != "ui" {
		t.Error("html inside an asset folder should not be listed twice")
	}
}
