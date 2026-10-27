package vba

import (
	"unicode/utf8"

	"golang.org/x/text/encoding"
	"golang.org/x/text/encoding/charmap"
	"golang.org/x/text/encoding/japanese"
	"golang.org/x/text/encoding/korean"
	"golang.org/x/text/encoding/simplifiedchinese"
	"golang.org/x/text/encoding/traditionalchinese"
)

var codePages = map[int]encoding.Encoding{
	874:   charmap.Windows874,
	932:   japanese.ShiftJIS,
	936:   simplifiedchinese.GBK,
	949:   korean.EUCKR,
	950:   traditionalchinese.Big5,
	1250:  charmap.Windows1250,
	1251:  charmap.Windows1251,
	1252:  charmap.Windows1252,
	1253:  charmap.Windows1253,
	1254:  charmap.Windows1254,
	1255:  charmap.Windows1255,
	1256:  charmap.Windows1256,
	1257:  charmap.Windows1257,
	1258:  charmap.Windows1258,
	10000: charmap.Macintosh,
}

// decoder returns a function converting text in the code page to UTF-8
// (Windows-1252 for unknown code pages).
func decoder(cp int) func([]byte) string {
	if cp == 65001 {
		return func(b []byte) string { return string(b) }
	}
	enc, ok := codePages[cp]
	if !ok {
		enc = charmap.Windows1252
	}
	return func(b []byte) string {
		s, err := enc.NewDecoder().Bytes(b)
		if err != nil || !utf8.Valid(s) {
			return string(b)
		}
		return string(s)
	}
}
