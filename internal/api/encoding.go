package api

import (
	"bytes"
	"strings"
	"unicode/utf8"

	"github.com/cool0707/xljs/internal/i18n"
	"github.com/dop251/goja"
	"golang.org/x/text/encoding"
	"golang.org/x/text/encoding/japanese"
	"golang.org/x/text/encoding/unicode"
)

var bomUTF8 = []byte{0xEF, 0xBB, 0xBF}

func normEncoding(name string) string {
	s := strings.ToLower(strings.TrimSpace(name))
	s = strings.ReplaceAll(s, "_", "-")
	switch s {
	case "", "auto":
		return "auto"
	case "utf8", "utf-8":
		return "utf8"
	case "utf8bom", "utf-8bom", "utf8-bom", "utf-8-bom", "utf-8-sig", "utf8-sig":
		return "utf8bom"
	case "cp932", "shift-jis", "shiftjis", "sjis", "windows-31j", "ms932", "x-sjis", "ms-kanji", "csshiftjis":
		return "cp932"
	case "euc-jp", "eucjp":
		return "eucjp"
	case "iso-2022-jp", "jis":
		return "iso2022jp"
	case "utf16le", "utf-16le", "utf-16":
		return "utf16le"
	case "utf16be", "utf-16be":
		return "utf16be"
	}
	return s
}

func isUTF8BOM(name string) bool { return normEncoding(name) == "utf8bom" }

func textEncoding(norm string) (encoding.Encoding, error) {
	switch norm {
	case "cp932":
		return japanese.ShiftJIS, nil
	case "eucjp":
		return japanese.EUCJP, nil
	case "iso2022jp":
		return japanese.ISO2022JP, nil
	case "utf16le":
		return unicode.UTF16(unicode.LittleEndian, unicode.IgnoreBOM), nil
	case "utf16be":
		return unicode.UTF16(unicode.BigEndian, unicode.IgnoreBOM), nil
	}
	return nil, i18n.Err("data.unsupportedEncoding", norm)
}

// DetectEncoding guesses the encoding of b: a BOM wins, then valid UTF-8,
// otherwise CP932 (Shift_JIS).
func DetectEncoding(b []byte) string {
	switch {
	case bytes.HasPrefix(b, bomUTF8):
		return "utf8bom"
	case bytes.HasPrefix(b, []byte{0xFF, 0xFE}):
		return "utf16le"
	case bytes.HasPrefix(b, []byte{0xFE, 0xFF}):
		return "utf16be"
	case utf8.Valid(b):
		return "utf8"
	}
	return "cp932"
}

func decode(b []byte, name string) (string, error) {
	norm := normEncoding(name)
	if norm == "auto" {
		norm = DetectEncoding(b)
	}
	switch norm {
	case "utf8", "utf8bom":
		b = bytes.TrimPrefix(b, bomUTF8)
		if !utf8.Valid(b) {
			return string(bytes.ToValidUTF8(b, []byte("�"))), nil
		}
		return string(b), nil
	case "utf16le":
		b = bytes.TrimPrefix(b, []byte{0xFF, 0xFE})
	case "utf16be":
		b = bytes.TrimPrefix(b, []byte{0xFE, 0xFF})
	}
	enc, err := textEncoding(norm)
	if err != nil {
		return "", err
	}
	out, err := enc.NewDecoder().Bytes(b)
	if err != nil {
		return "", err
	}
	return string(out), nil
}

// encode converts s to the given encoding. Characters that cannot be
// represented are replaced (with "?" for CP932).
func encode(s, name string) ([]byte, error) {
	norm := normEncoding(name)
	switch norm {
	case "auto", "utf8":
		return []byte(s), nil
	case "utf8bom":
		return append(append([]byte{}, bomUTF8...), s...), nil
	}
	enc, err := textEncoding(norm)
	if err != nil {
		return nil, err
	}
	out, err := enc.NewEncoder().Bytes([]byte(s))
	if err != nil {
		out = encodeReplacing(enc, s)
	}
	switch norm {
	case "utf16le":
		out = append([]byte{0xFF, 0xFE}, out...)
	case "utf16be":
		out = append([]byte{0xFE, 0xFF}, out...)
	}
	return out, nil
}

// encodeReplacing encodes rune by rune, writing "?" for runes the encoding
// cannot represent (the slow path, only used when needed).
func encodeReplacing(enc encoding.Encoding, s string) []byte {
	e := enc.NewEncoder()
	out := make([]byte, 0, len(s))
	var buf [8]byte
	for _, r := range s {
		n := utf8.EncodeRune(buf[:], r)
		b, err := e.Bytes(buf[:n])
		if err != nil {
			out = append(out, '?')
			e.Reset()
			continue
		}
		out = append(out, b...)
	}
	return out
}

func (n *natives) installEncoding() {
	vm := n.vm
	n.set("decode", func(c goja.FunctionCall) goja.Value {
		s, err := decode(n.bytes(c.Argument(0)), n.optStr(c.Argument(1), "utf8"))
		if err != nil {
			n.throw(err)
		}
		return vm.ToValue(s)
	})
	n.set("encode", func(c goja.FunctionCall) goja.Value {
		b, err := encode(n.str(c, 0), n.optStr(c.Argument(1), "utf8"))
		if err != nil {
			n.throw(err)
		}
		return vm.ToValue(vm.NewArrayBuffer(b))
	})
	n.set("detectEncoding", func(c goja.FunctionCall) goja.Value {
		return vm.ToValue(DetectEncoding(n.bytes(c.Argument(0))))
	})
	n.set("normEncoding", func(c goja.FunctionCall) goja.Value {
		return vm.ToValue(normEncoding(n.str(c, 0)))
	})
}
