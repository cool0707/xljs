package ui

import (
	"fmt"
	"os"
	"strconv"
	"strings"

	"github.com/cool0707/xljs/internal/i18n"
	"golang.org/x/term"
)

// Form asks for each field on the console. On a retry (some fields have an
// Error) only the invalid fields are asked again.
func (c *Console) Form(f FormSpec) (map[string]any, bool) {
	retry := false
	for _, fl := range f.Fields {
		retry = retry || fl.Error != ""
	}
	if !retry {
		if f.Title != "" {
			fmt.Fprintf(c.Out, "[%s]\n", f.Title)
		}
		if f.Description != "" {
			fmt.Fprintln(c.Out, f.Description)
		}
	}
	vals := map[string]any{}
	for _, fl := range f.Fields {
		if retry && fl.Error == "" {
			vals[fl.Name] = fl.Value
			continue
		}
		if fl.Error != "" {
			fmt.Fprintf(c.Out, "  %s: %s\n", fieldLabel(fl), fl.Error)
		}
		v, ok := c.askField(fl)
		if !ok {
			fmt.Fprintln(c.Out)
			return nil, false
		}
		vals[fl.Name] = v
	}
	return vals, true
}

func fieldLabel(fl FormField) string {
	if fl.Label != "" {
		return fl.Label
	}
	return fl.Name
}

func valueText(v any) string {
	switch x := v.(type) {
	case nil:
		return ""
	case string:
		return x
	case []string:
		return strings.Join(x, "; ")
	case []any:
		parts := make([]string, len(x))
		for i, p := range x {
			parts[i] = fmt.Sprint(p)
		}
		return strings.Join(parts, "; ")
	}
	return fmt.Sprint(v)
}

func (c *Console) askField(fl FormField) (any, bool) {
	label := fieldLabel(fl)
	if fl.Required {
		label += " *"
	}
	switch fl.Type {
	case "checkbox":
		def, _ := fl.Value.(bool)
		hint := "y/N"
		if def {
			hint = "Y/n"
		}
		fmt.Fprintf(c.Out, "%s [%s]: ", label, hint)
		line, ok := c.readLine()
		if !ok {
			return nil, false
		}
		switch strings.ToLower(strings.TrimSpace(line)) {
		case "":
			return def, true
		case "y", "yes", "はい", "1", "true", "on":
			return true, true
		}
		return false, true
	case "select", "radio":
		for i, o := range fl.Options {
			fmt.Fprintf(c.Out, "  %d) %s\n", i+1, o)
		}
		for {
			line, ok := c.Prompt("", label, valueText(fl.Value))
			if !ok {
				return nil, false
			}
			line = strings.TrimSpace(line)
			if n, err := strconv.Atoi(line); err == nil && n >= 1 && n <= len(fl.Options) {
				return fl.Options[n-1], true
			}
			for _, o := range fl.Options {
				if o == line {
					return o, true
				}
			}
			if line == "" {
				return "", true
			}
			fmt.Fprintln(c.Out, i18n.T("ui.formChooseOption", len(fl.Options)))
		}
	case "password":
		fmt.Fprintf(c.Out, "%s: ", label)
		if f, ok := c.In.(*os.File); ok && term.IsTerminal(int(f.Fd())) {
			b, err := term.ReadPassword(int(f.Fd()))
			fmt.Fprintln(c.Out)
			if err != nil {
				return nil, false
			}
			if len(b) == 0 {
				return valueText(fl.Value), true
			}
			return string(b), true
		}
		line, ok := c.readLine()
		if !ok {
			return nil, false
		}
		return line, true
	case "files":
		line, ok := c.Prompt("", label+i18n.T("ui.multipleHint"), valueText(fl.Value))
		if !ok {
			return nil, false
		}
		var paths []string
		for _, p := range strings.Split(line, ";") {
			if p = strings.Trim(strings.TrimSpace(p), `"`); p != "" {
				paths = append(paths, p)
			}
		}
		return paths, true
	case "file", "folder", "saveFile":
		line, ok := c.Prompt("", label, valueText(fl.Value))
		if !ok {
			return nil, false
		}
		return strings.Trim(strings.TrimSpace(line), `"`), true
	}
	line, ok := c.Prompt("", label, valueText(fl.Value))
	if !ok {
		return nil, false
	}
	return line, true
}

// Table prints the table; for selectable tables it then asks for row
// numbers ("1,3-5"; empty cancels).
func (c *Console) Table(t TableSpec) ([]int, bool) {
	if t.Title != "" {
		fmt.Fprintf(c.Out, "[%s]\n", t.Title)
	}
	fmt.Fprintln(c.Out, t.Text)
	if t.Selectable == "" {
		return nil, true
	}
	multi := t.Selectable == "multiple"
	for {
		if multi {
			fmt.Fprint(c.Out, i18n.T("ui.tableSelectMultiple"))
		} else {
			fmt.Fprint(c.Out, i18n.T("ui.tableSelectSingle"))
		}
		line, ok := c.readLine()
		if !ok || strings.TrimSpace(line) == "" {
			if !ok {
				fmt.Fprintln(c.Out)
			}
			return nil, false
		}
		if sel, ok := parseSelection(line, t.Rows, multi); ok {
			return sel, true
		}
		fmt.Fprintln(c.Out, i18n.T("ui.tableBadSelection", t.Rows-1))
	}
}

// parseSelection reads row numbers such as "0, 2-4 7" or "*" (all).
func parseSelection(s string, rows int, multi bool) ([]int, bool) {
	var out []int
	seen := map[int]bool{}
	add := func(n int) bool {
		if n < 0 || n >= rows {
			return false
		}
		if !seen[n] {
			seen[n] = true
			out = append(out, n)
		}
		return true
	}
	for _, tok := range strings.FieldsFunc(s, func(r rune) bool { return r == ',' || r == ' ' || r == '、' || r == '\t' }) {
		if tok == "*" && multi {
			for i := 0; i < rows; i++ {
				add(i)
			}
			continue
		}
		a, b, isRange := strings.Cut(tok, "-")
		lo, err1 := strconv.Atoi(a)
		hi := lo
		var err2 error
		if isRange {
			hi, err2 = strconv.Atoi(b)
		}
		if err1 != nil || err2 != nil || hi < lo {
			return nil, false
		}
		for n := lo; n <= hi; n++ {
			if !add(n) {
				return nil, false
			}
		}
	}
	if len(out) == 0 || (!multi && len(out) != 1) {
		return nil, false
	}
	return out, true
}
