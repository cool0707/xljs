package api

import (
	"testing"
)

func TestMatchPattern(t *testing.T) {
	cases := []struct {
		pat, rel string
		want     bool
	}{
		{"*.xlsx", "a.xlsx", true},
		{"*.xlsx", "sub/deep/A.XLSX", true},
		{"*.xlsx", "a.csv", false},
		{"~$*", "sub/~$a.xlsx", true},
		{"sub/*.xlsx", "sub/a.xlsx", true},
		{"sub/*.xlsx", "sub/x/a.xlsx", false},
		{"**/*.xlsx", "a.xlsx", true},
		{"**/*.xlsx", "x/y/a.xlsx", true},
		{"./sub/**", "sub/x/a", true},
		{"skip", "skip", true},
		{"skip", "a/skip", true},
	}
	for _, c := range cases {
		if got := matchPattern(c.pat, c.rel); got != c.want {
			t.Errorf("matchPattern(%q, %q) = %v, want %v", c.pat, c.rel, got, c.want)
		}
	}
}

func TestEncodingRoundTrip(t *testing.T) {
	for _, enc := range []string{"utf8", "utf8bom", "cp932", "shift_jis", "euc-jp", "utf16le"} {
		b, err := encode("日本語テキスト①", enc)
		if err != nil {
			t.Fatalf("%s: %v", enc, err)
		}
		s, err := decode(b, enc)
		if err != nil {
			t.Fatalf("%s: %v", enc, err)
		}
		if s != "日本語テキスト①" {
			t.Errorf("%s: got %q", enc, s)
		}
		if enc != "euc-jp" { // EUC-JP is not auto-detected
			s, _ = decode(b, "auto")
			if s != "日本語テキスト①" {
				t.Errorf("%s auto: got %q (detected %s)", enc, s, DetectEncoding(b))
			}
		}
	}
}

func TestDateFormatCode(t *testing.T) {
	cases := map[string]bool{
		"yyyy/m/d":           true,
		"h:mm:ss":            true,
		`yyyy"年"m"月"d"日"`:    true,
		"[$-ja-JP]ggge年m月d日": true,
		"0.00":               false,
		"#,##0":              false,
		"0.00E+00":           false,
		`"Day "0`:            false,
		"[h]:mm":             false,
		"General":            false,
		"[Red]0.0":           false,
	}
	for code, want := range cases {
		if got := dateFormatCode(code); got != want {
			t.Errorf("dateFormatCode(%q) = %v, want %v", code, got, want)
		}
	}
}
