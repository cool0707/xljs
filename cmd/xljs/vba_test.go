package main

import (
	"errors"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/cool0707/xljs/internal/i18n"
	"github.com/cool0707/xljs/internal/ui"
	"github.com/xuri/excelize/v2"
)

func TestDropVBA(t *testing.T) {
	dir := t.TempDir()
	data, err := os.ReadFile(filepath.Join("..", "..", "testdata", "scripts", "macro.xlsm"))
	if err != nil {
		t.Fatal(err)
	}
	a, b := filepath.Join(dir, "売上.xlsm"), filepath.Join(dir, "b.xlsm")
	os.WriteFile(a, data, 0o644)
	os.WriteFile(b, data, 0o644)
	if !isWorkbook(a) || !isWorkbook("x.XLS") || isWorkbook("x.ts") {
		t.Error("isWorkbook")
	}
	u, err := ui.NewMock("", os.Stderr)
	if err != nil {
		t.Fatal(err)
	}
	con := &consoleState{has: true}
	if code := dropVBA(con, u, []string{a, b}); code != 0 {
		t.Fatalf("exit %d", code)
	}
	text, err := os.ReadFile(filepath.Join(dir, "売上_VBA.txt"))
	if err != nil {
		t.Fatal(err)
	}
	if n := strings.Count(string(text), "\n# "); !strings.HasPrefix(string(text), "# ") || n != 1 {
		t.Errorf("want two workbooks:\n%s", text)
	}
	if !strings.Contains(string(text), "Sub 売上集計()") {
		t.Errorf("no code:\n%s", text)
	}

	plain := filepath.Join(dir, "plain.xlsx")
	f := excelize.NewFile()
	if err := f.SaveAs(plain); err != nil {
		t.Fatal(err)
	}
	var le *i18n.LocalizedError
	if _, _, err := vbaText([]string{a, plain}); !errors.As(err, &le) || le.Key != "cli.vbaNone" {
		t.Errorf("no macros: %v", err)
	}
	_, _, err = vbaText([]string{filepath.Join(dir, "missing.xlsm")})
	if err == nil {
		t.Error("missing: no error")
	}
}
