package i18n

import (
	"errors"
	"os"
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
	if S("日本語", "English") != "English" {
		t.Fatal("S should pick English")
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
	wrapped := Error(errors.Join(err))
	if !strings.Contains(wrapped, "見つかりません") {
		t.Fatalf("got %q", wrapped)
	}
}
