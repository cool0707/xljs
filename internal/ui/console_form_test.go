package ui

import (
	"bytes"
	"reflect"
	"strings"
	"testing"

	"github.com/cool0707/xljs/internal/i18n"
)

func TestConsoleForm(t *testing.T) {
	i18n.SetLanguage("ja")
	defer i18n.SetLanguage("auto")
	var out bytes.Buffer
	in := strings.Join([]string{
		"",               // name: keep "太郎"
		"9",              // mode: out of range
		"商品別",            // mode: by label
		"y",              // tax
		`"C:\data"`,      // folder: quotes removed
		"a.xlsx; b.xlsx", // files
		"",
	}, "\n")
	c := &Console{In: strings.NewReader(in), Out: &out}
	v, ok := c.Form(FormOptions{
		Title: "設定",
		Error: "全体のエラー",
		Fields: []FormField{
			{Name: "name", Label: "名前", Type: "text", Value: "太郎", Required: true, Error: "短すぎます"},
			{Name: "mode", Label: "単位", Type: "select", Value: "顧客別",
				Options: []FormOption{{Value: "顧客別", Label: "顧客別"}, {Value: "商品別", Label: "商品別"}}},
			{Name: "tax", Label: "税込", Type: "checkbox", Value: false},
			{Name: "src", Label: "フォルダ", Type: "folder"},
			{Name: "in", Label: "入力", Type: "files", Accept: []string{".xlsx"}},
		},
	})
	if !ok {
		t.Fatalf("cancelled:\n%s", out.String())
	}
	want := map[string]any{"name": "太郎", "mode": "商品別", "tax": true, "src": `C:\data`, "in": []any{"a.xlsx", "b.xlsx"}}
	if !reflect.DeepEqual(v, want) {
		t.Errorf("values = %#v", v)
	}
	for _, s := range []string{"[設定]", "! 全体のエラー", "! 名前: 短すぎます", "名前 * [太郎]: ", " *1) 顧客別", "1 から 2 までの番号", "税込 [y/N]", "(*.xlsx)"} {
		if !strings.Contains(out.String(), s) {
			t.Errorf("output does not contain %q:\n%s", s, out.String())
		}
	}

	// End of input cancels.
	c = &Console{In: strings.NewReader(""), Out: &out}
	if _, ok := c.Form(FormOptions{Fields: []FormField{{Name: "a", Type: "text"}}}); ok {
		t.Error("EOF must cancel")
	}
}

func TestConsoleTable(t *testing.T) {
	var out bytes.Buffer
	c := &Console{In: strings.NewReader("4\n3-2\n1, 3-4\n"), Out: &out}
	o := TableOptions{Title: "表", Columns: []string{"名前", "数"}, Rows: [][]string{{"あい", "1"}, {"b", "22"}, {"c\nd", ""}, {"e"}}, Select: SelectMultiple}
	sel, ok := c.Table(o)
	if !ok || !reflect.DeepEqual(sel, []int{3}) {
		t.Fatalf("selection = %v %v", sel, ok)
	}
	sel, ok = c.Table(o) // "3-2" is invalid, then "1, 3-4"
	if !ok || !reflect.DeepEqual(sel, []int{0, 2, 3}) {
		t.Fatalf("selection = %v %v\n%s", sel, ok, out.String())
	}
	table := renderTable(o.Columns, o.Rows)
	want := "┌───┬──────┬────┐\n" +
		"│ # │ 名前 │ 数 │\n" +
		"├───┼──────┼────┤\n" +
		"│ 1 │ あい │ 1  │\n" +
		"│ 2 │ b    │ 22 │\n" +
		"│ 3 │ c d  │    │\n" +
		"│ 4 │ e    │    │\n" +
		"└───┴──────┴────┘\n"
	if table != want {
		t.Errorf("table:\n%s\nwant:\n%s", table, want)
	}

	// Empty input cancels; display only does not ask.
	c = &Console{In: strings.NewReader("\n"), Out: &out}
	if _, ok := c.Table(o); ok {
		t.Error("empty input must cancel")
	}
	o.Select = SelectNone
	if _, ok := c.Table(o); !ok {
		t.Error("display only")
	}
}

func TestParseSelection(t *testing.T) {
	for in, want := range map[string][]int{"1": {0}, "2,1": {1, 0}, "1-3": {0, 1, 2}, "1 2、2": {0, 1}} {
		if got, ok := parseSelection(in, 5); !ok || !reflect.DeepEqual(got, want) {
			t.Errorf("%q = %v %v", in, got, ok)
		}
	}
	for _, in := range []string{"0", "6", "a", "3-1", "1-9", ","} {
		if got, ok := parseSelection(in, 5); ok {
			t.Errorf("%q accepted: %v", in, got)
		}
	}
}
