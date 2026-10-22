package ui

import (
	"bytes"
	"reflect"
	"strings"
	"testing"

	"github.com/cool0707/xljs/internal/i18n"
)

func TestConsoleForm(t *testing.T) {
	i18n.SetLanguage("en")
	defer i18n.SetLanguage("auto")
	spec := FormSpec{Title: "売上", Description: "説明", Fields: []FormField{
		{Name: "name", Label: "名前", Value: "既定"},
		{Name: "mode", Label: "単位", Type: "select", Options: []string{"顧客別", "商品別"}, Value: "顧客別"},
		{Name: "tax", Type: "checkbox"},
		{Name: "files", Type: "files"},
		{Name: "out", Type: "saveFile", Value: "a.xlsx"},
	}}
	// name: default, mode: invalid then 2, tax: y, files: two paths, out: quoted path
	var out bytes.Buffer
	c := &Console{In: strings.NewReader("\n9\n2\ny\na.csv; \"b c.csv\"\n\"C:\\x\\y.xlsx\"\n"), Out: &out}
	got, ok := c.Form(spec)
	if !ok {
		t.Fatal("cancelled")
	}
	want := map[string]any{"name": "既定", "mode": "商品別", "tax": true, "files": []string{"a.csv", "b c.csv"}, "out": `C:\x\y.xlsx`}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("got %#v", got)
	}
	if !strings.Contains(out.String(), "[売上]") || !strings.Contains(out.String(), "2) 商品別") || !strings.Contains(out.String(), "from 1 to 2") {
		t.Errorf("output:\n%s", out.String())
	}

	// A retry asks only the fields with errors.
	spec.Fields[0].Error = "too short"
	out.Reset()
	c = &Console{In: strings.NewReader("新しい名前\n"), Out: &out}
	got, ok = c.Form(spec)
	if !ok || got["name"] != "新しい名前" || got["mode"] != "顧客別" || strings.Contains(out.String(), "[売上]") || !strings.Contains(out.String(), "too short") {
		t.Fatalf("retry: %v %#v\n%s", ok, got, out.String())
	}

	// End of input cancels.
	c = &Console{In: strings.NewReader(""), Out: &out}
	if _, ok := c.Form(spec); ok {
		t.Fatal("expected cancel at end of input")
	}
}

func TestConsoleTable(t *testing.T) {
	var out bytes.Buffer
	c := &Console{In: strings.NewReader("x\n0, 2-3\n"), Out: &out}
	sel, ok := c.Table(TableSpec{Title: "表", Text: "┌─┐", Rows: 5, Selectable: "multiple"})
	if !ok || !reflect.DeepEqual(sel, []int{0, 2, 3}) {
		t.Fatalf("%v %v", sel, ok)
	}
	if !strings.Contains(out.String(), "[表]\n┌─┐") {
		t.Errorf("output:\n%s", out.String())
	}
	c = &Console{In: strings.NewReader("\n"), Out: &out}
	if _, ok := c.Table(TableSpec{Rows: 2, Selectable: "single"}); ok {
		t.Fatal("empty input should cancel")
	}
	c = &Console{In: strings.NewReader(""), Out: &out}
	if _, ok := c.Table(TableSpec{Rows: 2}); !ok {
		t.Fatal("showing only needs no input")
	}
}

func TestParseSelection(t *testing.T) {
	for _, c := range []struct {
		in    string
		rows  int
		multi bool
		want  []int
	}{
		{"1", 3, false, []int{1}},
		{"0,2", 3, false, nil},
		{"0、2 1", 3, true, []int{0, 2, 1}},
		{"1-2,1", 4, true, []int{1, 2}},
		{"*", 3, true, []int{0, 1, 2}},
		{"3", 3, true, nil},
		{"2-1", 3, true, nil},
		{"a", 3, true, nil},
	} {
		got, ok := parseSelection(c.in, c.rows, c.multi)
		if (c.want == nil) != !ok || (ok && !reflect.DeepEqual(got, c.want)) {
			t.Errorf("%q: %v %v, want %v", c.in, got, ok, c.want)
		}
	}
}
