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

func TestConsoleFormTablesAndTabs(t *testing.T) {
	i18n.SetLanguage("ja")
	defer i18n.SetLanguage("auto")
	var out bytes.Buffer
	in := strings.Join([]string{
		"",    // a: keep
		"5",   // pick: out of range
		"2-3", // pick
		"",    // one: keep row 1
		"-",   // none: clear
	}, "\n")
	c := &Console{In: strings.NewReader(in), Out: &out}
	rows := [][]string{{"A社", "2"}, {"B社", "1"}, {"C社", "5"}}
	v, ok := c.Form(FormOptions{Fields: []FormField{
		{Name: "a", Label: "共通", Type: "text", Value: "x"},
		{Name: "info", Label: "一覧", Type: "table", Columns: []string{"名前"}, Rows: [][]string{{"表示だけ"}}, Tab: "基本"},
		{Name: "pick", Label: "対象", Type: "table", Columns: []string{"顧客", "件数"}, Rows: rows, Select: SelectMultiple, Value: []any{}, Tab: "基本"},
		{Name: "one", Label: "一つ", Type: "table", Columns: []string{"顧客", "件数"}, Rows: rows, Select: SelectSingle, Value: []any{float64(0)}, Tab: "詳細"},
		{Name: "none", Label: "なし", Type: "table", Columns: []string{"顧客", "件数"}, Rows: rows, Select: SelectMultiple, Value: []any{float64(1)}, Tab: "詳細"},
	}})
	if !ok {
		t.Fatalf("cancelled:\n%s", out.String())
	}
	want := map[string]any{"a": "x", "pick": []any{1, 2}, "one": []any{0}, "none": []any{}}
	if !reflect.DeepEqual(v, want) {
		t.Errorf("values = %#v", v)
	}
	for _, s := range []string{"== 基本 ==", "== 詳細 ==", "表示だけ", "│ 3 │ C社", "Enter で 1 のまま", "Enter で選択なし", "1 から 3 までの番号"} {
		if !strings.Contains(out.String(), s) {
			t.Errorf("output does not contain %q:\n%s", s, out.String())
		}
	}
}

func TestConsoleFormButtons(t *testing.T) {
	i18n.SetLanguage("ja")
	defer i18n.SetLanguage("auto")
	var out bytes.Buffer
	c := &Console{In: strings.NewReader("\n\n\n2\n\n4\n読込\n"), Out: &out}
	o := FormOptions{Fields: []FormField{{Name: "a", Value: "x"}}, Submit: "OK", Buttons: []string{"読込", "確認"}}
	for _, want := range []any{nil, 1, 0} {
		v, ok := c.Form(o)
		if !ok || v["$button"] != want || v["a"] != "x" {
			t.Fatalf("values = %v %v, want button %v\n%s", v, ok, want, out.String())
		}
	}
	if !strings.Contains(out.String(), "実行する操作") || !strings.Contains(out.String(), " *3) OK") {
		t.Errorf("output:\n%s", out.String())
	}
}

func TestConsoleFormEditableTable(t *testing.T) {
	i18n.SetLanguage("ja")
	defer i18n.SetLanguage("auto")
	var out bytes.Buffer
	// Row 9 is out of range; row 2: 金額 "200", 区分 by number, 済 "y".
	c := &Console{In: strings.NewReader("9\n2\n200\n1\ny\n\n"), Out: &out}
	o := FormOptions{Fields: []FormField{{
		Name: "t", Type: "table", Columns: []string{"名前", "金額", "区分", "済"},
		Rows: [][]string{{"a", "1", "売上", "false"}, {"b", "x"}},
		Editable: []FormCellColumn{
			{Index: 1, Type: "number"},
			{Index: 2, Type: "select", Options: []FormOption{{Value: "売上", Label: "売上"}, {Value: "返品", Label: "返品"}}},
			{Index: 3, Type: "checkbox"},
		},
		CellErrors: []FormCellError{{Row: 1, Col: 1, Message: "数値を入力してください"}, {Row: 1, Col: -1, Message: "確認"}},
	}}}
	v, ok := c.Form(o)
	if !ok {
		t.Fatal("cancelled")
	}
	if got, want := v["$cells"], map[string]any{"t": []any{[]any{1, 1, "200"}, []any{1, 2, "売上"}, []any{1, 3, true}}}; !reflect.DeepEqual(got, want) {
		t.Errorf("cells = %#v\n%s", got, out.String())
	}
	if _, has := v["t"]; has {
		t.Errorf("a table without select has no value: %v", v)
	}
	for _, want := range []string{"! 2 行目 金額: 数値を入力してください", "! 2 行目: 確認", "1 から 2 までの番号"} {
		if !strings.Contains(out.String(), want) {
			t.Errorf("output does not contain %q:\n%s", want, out.String())
		}
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
