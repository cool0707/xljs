package ui

import (
	"fmt"
	"os"
	"strconv"
	"strings"

	"github.com/cool0707/xljs/internal/i18n"
	"github.com/mattn/go-runewidth"
	"golang.org/x/term"
)

// SupportsWindows is false: forms and tables are asked on the console.
func (c *Console) SupportsWindows() bool { return false }

// Form asks for each field in turn; Enter keeps the current value.
func (c *Console) Form(o FormOptions) (map[string]any, bool) {
	if o.Title != "" {
		fmt.Fprintf(c.Out, "[%s]\n", o.Title)
	}
	if o.Description != "" {
		fmt.Fprintln(c.Out, o.Description)
	}
	if o.Error != "" {
		fmt.Fprintln(c.Out, "! "+o.Error)
	}
	values := map[string]any{}
	tab := ""
	for _, f := range o.Fields {
		if f.Tab != tab {
			tab = f.Tab
			fmt.Fprintf(c.Out, "== %s ==\n", tab)
		}
		if f.Type == "table" && f.Select == SelectNone {
			fmt.Fprintln(c.Out, fieldLabel(f))
			fmt.Fprint(c.Out, renderTable(f.Columns, f.Rows))
			continue
		}
		if f.Error != "" {
			fmt.Fprintf(c.Out, "! %s: %s\n", fieldLabel(f), f.Error)
		}
		v, ok := c.askField(f)
		if !ok {
			return nil, false
		}
		values[f.Name] = v
	}
	if len(o.Buttons) > 0 {
		b, ok := c.askButton(o)
		if !ok {
			return nil, false
		}
		values["$button"] = b
	}
	return values, true
}

// askButton asks which of the form's buttons to press: the index of a
// button, or nil for the submit button (listed last, the default).
func (c *Console) askButton(o FormOptions) (any, bool) {
	opts := make([]FormOption, 0, len(o.Buttons)+1)
	for i, b := range o.Buttons {
		opts = append(opts, FormOption{Value: strconv.Itoa(i), Label: b})
	}
	cur := "0"
	if o.Submit != "" {
		opts = append(opts, FormOption{Value: "", Label: o.Submit})
		cur = ""
	}
	v, ok := c.askChoice(i18n.T("ui.formButton"), opts, cur)
	if !ok {
		return nil, false
	}
	if n, err := strconv.Atoi(v.(string)); err == nil {
		return n, true
	}
	return nil, true
}

func fieldLabel(f FormField) string {
	if f.Label != "" {
		return f.Label
	}
	return f.Name
}

func valueString(v any) string {
	switch x := v.(type) {
	case nil:
		return ""
	case string:
		return x
	case []any:
		parts := make([]string, 0, len(x))
		for _, e := range x {
			parts = append(parts, valueString(e))
		}
		return strings.Join(parts, ";")
	}
	return fmt.Sprint(v)
}

func (c *Console) askField(f FormField) (any, bool) {
	label := fieldLabel(f)
	if f.Required {
		label += " *"
	}
	if f.Help != "" {
		fmt.Fprintln(c.Out, "  "+f.Help)
	}
	cur := valueString(f.Value)
	switch f.Type {
	case "checkbox":
		on, _ := f.Value.(bool)
		hint := "[y/N]"
		if on {
			hint = "[Y/n]"
		}
		fmt.Fprintf(c.Out, "%s %s: ", label, hint)
		line, ok := c.readLine()
		if !ok {
			fmt.Fprintln(c.Out)
			return nil, false
		}
		switch strings.ToLower(strings.TrimSpace(line)) {
		case "":
			return on, true
		case "y", "yes", "はい", "ok", "1", "true":
			return true, true
		}
		return false, true
	case "select", "radio":
		return c.askChoice(label, f.Options, cur)
	case "table":
		return c.askRows(label, f)
	case "password":
		return c.askPassword(label, cur)
	case "files":
		label += filterHint(acceptFilters(f.Accept)) + i18n.T("ui.multipleHint")
		line, ok := c.Prompt("", label, cur)
		if !ok {
			return nil, false
		}
		var out []any
		for _, p := range strings.Split(line, ";") {
			if p = strings.Trim(strings.TrimSpace(p), `"`); p != "" {
				out = append(out, p)
			}
		}
		return out, true
	case "file", "saveFile":
		label += filterHint(acceptFilters(f.Accept))
	}
	line, ok := c.Prompt("", label, cur)
	if !ok {
		return nil, false
	}
	switch f.Type {
	case "file", "saveFile", "folder":
		line = strings.Trim(strings.TrimSpace(line), `"`)
	}
	return line, true
}

// askRows shows a table field and asks for the numbers of the rows to
// select; the value is their 0-based indices.
func (c *Console) askRows(label string, f FormField) (any, bool) {
	fmt.Fprintln(c.Out, label)
	fmt.Fprint(c.Out, renderTable(f.Columns, f.Rows))
	var cur []any
	var nums []string
	if list, ok := f.Value.([]any); ok {
		for _, v := range list {
			if n, ok := v.(float64); ok {
				cur = append(cur, int(n))
				nums = append(nums, strconv.Itoa(int(n)+1))
			}
		}
	}
	prompt := i18n.T("ui.tablePick")
	if len(nums) > 0 {
		prompt = i18n.T("ui.tableKeep", strings.Join(nums, ","))
	}
	for {
		line, ok := c.Prompt("", prompt, "")
		if !ok {
			return nil, false
		}
		switch line = strings.TrimSpace(line); line {
		case "":
			return cur, true
		case "-":
			return []any{}, true
		}
		sel, ok := parseSelection(line, len(f.Rows))
		if !ok {
			fmt.Fprintln(c.Out, "! "+i18n.T("ui.badChoice", len(f.Rows)))
			continue
		}
		if f.Select == SelectSingle && len(sel) > 1 {
			sel = sel[:1]
		}
		out := make([]any, len(sel))
		for i, n := range sel {
			out[i] = n
		}
		return out, true
	}
}

func acceptFilters(accept []string) []FileFilter {
	if len(accept) == 0 {
		return nil
	}
	f := FileFilter{}
	for _, a := range accept {
		f.Patterns = append(f.Patterns, "*"+a)
	}
	return []FileFilter{f}
}

// askChoice lists the options and accepts a number, a value or a label.
func (c *Console) askChoice(label string, opts []FormOption, cur string) (any, bool) {
	fmt.Fprintln(c.Out, label)
	def := ""
	for i, o := range opts {
		mark := " "
		if o.Value == cur {
			mark = "*"
			def = strconv.Itoa(i + 1)
		}
		fmt.Fprintf(c.Out, " %s%d) %s\n", mark, i+1, o.Label)
	}
	for {
		line, ok := c.Prompt("", i18n.T("ui.choose"), def)
		if !ok {
			return nil, false
		}
		line = strings.TrimSpace(line)
		if line == "" {
			return cur, true
		}
		if n, err := strconv.Atoi(line); err == nil && n >= 1 && n <= len(opts) {
			return opts[n-1].Value, true
		}
		for _, o := range opts {
			if line == o.Value || line == o.Label {
				return o.Value, true
			}
		}
		fmt.Fprintln(c.Out, "! "+i18n.T("ui.badChoice", len(opts)))
	}
}

func (c *Console) askPassword(label, cur string) (any, bool) {
	f, ok := c.In.(*os.File)
	if !ok || !term.IsTerminal(int(f.Fd())) {
		return c.Prompt("", label, "")
	}
	fmt.Fprintf(c.Out, "%s: ", label)
	b, err := term.ReadPassword(int(f.Fd()))
	fmt.Fprintln(c.Out)
	if err != nil {
		return nil, false
	}
	if len(b) == 0 {
		return cur, true
	}
	return string(b), true
}

// Table prints the rows with 1-based numbers and, when selecting, asks for
// the numbers of the rows ("1,3-5").
func (c *Console) Table(o TableOptions) ([]int, bool) {
	if o.Title != "" {
		fmt.Fprintf(c.Out, "[%s]\n", o.Title)
	}
	fmt.Fprint(c.Out, renderTable(o.Columns, o.Rows))
	if o.Select == SelectNone {
		return nil, true
	}
	for {
		line, ok := c.Prompt("", i18n.T("ui.tableSelect"), "")
		if !ok || strings.TrimSpace(line) == "" {
			return nil, false
		}
		sel, ok := parseSelection(line, len(o.Rows))
		if !ok {
			fmt.Fprintln(c.Out, "! "+i18n.T("ui.badChoice", len(o.Rows)))
			continue
		}
		if o.Select == SelectSingle && len(sel) > 1 {
			sel = sel[:1]
		}
		return sel, true
	}
}

// parseSelection parses "1, 3-5" into 0-based indices below n.
func parseSelection(s string, n int) ([]int, bool) {
	var out []int
	seen := map[int]bool{}
	add := func(i int) {
		if !seen[i] {
			seen[i] = true
			out = append(out, i)
		}
	}
	for _, part := range strings.FieldsFunc(s, func(r rune) bool { return r == ',' || r == ' ' || r == '、' }) {
		lo, hi, isRange := strings.Cut(part, "-")
		a, err := strconv.Atoi(lo)
		if err != nil || a < 1 || a > n {
			return nil, false
		}
		b := a
		if isRange {
			if b, err = strconv.Atoi(hi); err != nil || b < a || b > n {
				return nil, false
			}
		}
		for i := a; i <= b; i++ {
			add(i - 1)
		}
	}
	return out, len(out) > 0
}

// renderTable draws a box table like console.table, with a number column.
func renderTable(cols []string, rows [][]string) string {
	header := append([]string{"#"}, cols...)
	widths := make([]int, len(header))
	clean := func(s string) string { return strings.NewReplacer("\r\n", " ", "\n", " ", "\t", " ").Replace(s) }
	for i, h := range header {
		widths[i] = runewidth.StringWidth(h)
	}
	body := make([][]string, len(rows))
	for r, row := range rows {
		line := make([]string, len(header))
		line[0] = strconv.Itoa(r + 1)
		for i := range cols {
			if i < len(row) {
				line[i+1] = clean(row[i])
			}
		}
		for i, cell := range line {
			widths[i] = max(widths[i], runewidth.StringWidth(cell))
		}
		body[r] = line
	}
	var b strings.Builder
	rule := func(l, m, r string) {
		b.WriteString(l)
		for i, w := range widths {
			if i > 0 {
				b.WriteString(m)
			}
			b.WriteString(strings.Repeat("─", w+2))
		}
		b.WriteString(r + "\n")
	}
	line := func(cells []string) {
		b.WriteString("│")
		for i, cell := range cells {
			if i > 0 {
				b.WriteString("│")
			}
			b.WriteString(" " + cell + strings.Repeat(" ", widths[i]-runewidth.StringWidth(cell)) + " ")
		}
		b.WriteString("│\n")
	}
	rule("┌", "┬", "┐")
	line(header)
	rule("├", "┼", "┤")
	for _, r := range body {
		line(r)
	}
	rule("└", "┴", "┘")
	return b.String()
}
