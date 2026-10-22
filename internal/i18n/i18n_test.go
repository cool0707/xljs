package i18n

import (
	"errors"
	"io/fs"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
)

func TestTextJapanese(t *testing.T) {
	SetLanguage("ja")
	defer SetLanguage("auto")
	cases := map[string]string{
		"TypeError: Cannot read property 'foo' of undefined": "TypeError: undefined のプロパティ 'foo' は読み取れません",
		"ReferenceError: hoge is not defined":                "ReferenceError: hoge は定義されていません",
		"TypeError: obj.run is not a function":               "TypeError: obj.run は関数ではありません",
		`a.js:1:4: Could not resolve "./x.js"`:               `a.js:1:4: "./x.js" が見つかりません（import のパスを確認してください）`,
		`Unexpected ";"`:                                     `予期しない ";" があります`,
		"sheet 売上 does not exist":                            `シート "売上" はありません`,
		"plain message":                                      "plain message",
	}
	for in, want := range cases {
		if got := Text(in); got != want {
			t.Errorf("Text(%q) = %q, want %q", in, got, want)
		}
	}
}

func TestEnglishUnchanged(t *testing.T) {
	SetLanguage("en")
	defer SetLanguage("auto")
	in := "TypeError: Cannot read property 'foo' of undefined"
	if Text(in) != in {
		t.Fatal("English text must not be translated")
	}
	if T("ui.copy") != "Copy" {
		t.Fatal(T("ui.copy"))
	}
}

func TestLanguageSelection(t *testing.T) {
	defer SetLanguage("auto")
	for tag, want := range map[string]string{"ja": "ja", "ja-JP": "ja", "ja_JP.UTF-8": "ja", "EN-us": "en"} {
		if !SetLanguage(tag) || Lang() != want {
			t.Errorf("SetLanguage(%q) → %s, want %s", tag, Lang(), want)
		}
	}
	if SetLanguage("xx") {
		t.Error("unknown language must report false")
	}
	if T("no.such.key") != "no.such.key" {
		t.Error("unknown keys fall back to the key")
	}
	SetLanguage("ja")
	if got := T("cli.created", "a.exe"); got != "作成しました: a.exe" {
		t.Errorf("placeholder: %q", got)
	}
	if Current().Excel.Font != "游ゴシック" {
		t.Errorf("ja excel font = %q", Current().Excel.Font)
	}
}

func TestPathError(t *testing.T) {
	SetLanguage("ja")
	defer SetLanguage("auto")
	_, err := os.Open("/definitely/missing/file.xlsx")
	msg := Error(err)
	if !strings.Contains(msg, "file.xlsx") || !strings.Contains(msg, "開けません") || !strings.Contains(msg, "見つかりません") {
		t.Fatalf("got %q", msg)
	}
	if !strings.Contains(Error(errors.Join(err)), "見つかりません") {
		t.Fatal("wrapped path error")
	}
}

// TestLocaleFiles checks every locale against en.json: no unknown keys,
// the same placeholders, valid translation rules.
func TestLocaleFiles(t *testing.T) {
	en := locales["en"]
	ph := regexp.MustCompile(`\{\d+\}`)
	for lang, loc := range locales {
		for k, v := range loc.Messages {
			base, ok := en.Messages[k]
			if !ok {
				t.Errorf("%s.json: key %q is not in en.json", lang, k)
				continue
			}
			if strings.Join(sorted(ph.FindAllString(v, -1)), ",") != strings.Join(sorted(ph.FindAllString(base, -1)), ",") {
				t.Errorf("%s.json: %q has different placeholders than en.json", lang, k)
			}
		}
		for k := range en.Messages {
			if _, ok := loc.Messages[k]; !ok {
				t.Logf("%s.json: missing %q (English is used)", lang, k)
			}
		}
	}
}

// TestKeysUsedInSourceExist makes sure every message key referenced from
// Go or JavaScript code is defined in en.json.
func TestKeysUsedInSourceExist(t *testing.T) {
	root := filepath.Join("..", "..")
	re := regexp.MustCompile(`\b(?:i18n\.T|i18n\.Err|n\.throwT|T)\(\s*"([a-zA-Z]+\.[\w.]+)"`)
	found := 0
	filepath.WalkDir(root, func(p string, d fs.DirEntry, err error) error {
		if err != nil || d.IsDir() || !(strings.HasSuffix(p, ".go") || strings.HasSuffix(p, ".js")) || strings.Contains(p, "testdata") || strings.HasSuffix(p, "_test.go") {
			return nil
		}
		src, _ := os.ReadFile(p)
		for _, m := range re.FindAllStringSubmatch(string(src), -1) {
			found++
			if _, ok := locales["en"].Messages[m[1]]; !ok {
				t.Errorf("%s: key %q is not defined in locales/en.json", p, m[1])
			}
		}
		return nil
	})
	if found < 20 {
		t.Fatalf("only %d keys found; is the pattern still right?", found)
	}
}

func sorted(s []string) []string {
	out := append([]string(nil), s...)
	for i := range out {
		for j := i + 1; j < len(out); j++ {
			if out[j] < out[i] {
				out[i], out[j] = out[j], out[i]
			}
		}
	}
	return out
}
