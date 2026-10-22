package api

import (
	"encoding/csv"
	"io"
	"strings"
	"unicode/utf8"

	"github.com/dop251/goja"
)

func delimRune(s string, def rune) rune {
	if s == "" {
		return def
	}
	r, _ := utf8.DecodeRuneInString(s)
	return r
}

// parseCSV parses text into records. Rows may have different lengths.
func parseCSV(text string, delim rune, comment rune) ([][]string, error) {
	r := csv.NewReader(strings.NewReader(text))
	r.Comma = delim
	r.Comment = comment
	r.FieldsPerRecord = -1
	r.LazyQuotes = true
	var out [][]string
	for {
		rec, err := r.Read()
		if err == io.EOF {
			break
		}
		if err != nil {
			return out, err
		}
		out = append(out, rec)
	}
	return out, nil
}

// stringifyCSV writes records. Line breaks inside fields are kept as-is
// (Excel stores in-cell line breaks as LF), only record separators use eol.
func stringifyCSV(rows [][]string, delim rune, crlf, quoteAll bool) (string, error) {
	var b strings.Builder
	eol := "\n"
	if crlf {
		eol = "\r\n"
	}
	for _, row := range rows {
		for i, f := range row {
			if i > 0 {
				b.WriteRune(delim)
			}
			if quoteAll || needsQuotes(f, delim) {
				b.WriteByte('"')
				b.WriteString(strings.ReplaceAll(f, `"`, `""`))
				b.WriteByte('"')
			} else {
				b.WriteString(f)
			}
		}
		b.WriteString(eol)
	}
	return b.String(), nil
}

func needsQuotes(f string, delim rune) bool {
	if f == "" {
		return false
	}
	if strings.ContainsRune(f, delim) || strings.ContainsAny(f, "\"\r\n") {
		return true
	}
	r, _ := utf8.DecodeRuneInString(f)
	return r == ' ' || r == '\t'
}

func (n *natives) installCSV() {
	vm := n.vm
	// csvParse(text, delimiter, comment) -> string[][]
	n.set("csvParse", func(c goja.FunctionCall) goja.Value {
		text := strings.TrimPrefix(n.str(c, 0), "\uFEFF")
		var comment rune
		if s := n.str(c, 2); s != "" {
			comment = delimRune(s, 0)
		}
		rows, err := parseCSV(text, delimRune(n.str(c, 1), ','), comment)
		if err != nil {
			n.throw(err)
		}
		out := make([]goja.Value, len(rows))
		for i, r := range rows {
			out[i] = n.strArray(r)
		}
		return n.array(out)
	})
	// csvStringify(rows: string[][], delimiter, crlf, quoteAll) -> string
	n.set("csvStringify", func(c goja.FunctionCall) goja.Value {
		var rows [][]string
		for _, r := range n.elems(c.Argument(0)) {
			es := n.elems(r)
			row := make([]string, len(es))
			for i, e := range es {
				if !isNullish(e) {
					row[i] = e.String()
				}
			}
			rows = append(rows, row)
		}
		s, err := stringifyCSV(rows, delimRune(n.str(c, 1), ','), c.Argument(2).ToBoolean(), c.Argument(3).ToBoolean())
		if err != nil {
			n.throw(err)
		}
		return vm.ToValue(s)
	})
}
