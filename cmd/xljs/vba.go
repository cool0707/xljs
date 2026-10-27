package main

import (
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/cool0707/xljs/internal/i18n"
	"github.com/cool0707/xljs/internal/ui"
	"github.com/cool0707/xljs/internal/vba"
)

// macroPatterns are the workbooks offered by the picker for extracting VBA.
var macroPatterns = []string{"*.xlsm", "*.xlsb", "*.xlam", "*.xltm", "*.xls"}

// isWorkbook reports whether path is an Excel workbook (with or without
// macros) rather than a script.
func isWorkbook(path string) bool {
	switch strings.ToLower(filepath.Ext(path)) {
	case ".xlsm", ".xlsb", ".xlam", ".xltm", ".xls", ".xla", ".xlt", ".xlsx", ".xltx":
		return true
	}
	return false
}

// vbaText reads the VBA code of the workbooks as one text (vba.Text of each)
// and counts the modules. Every workbook must have macros.
func vbaText(books []string) (string, int, error) {
	var parts []string
	modules := 0
	for _, b := range books {
		p, err := vba.ReadFile(b)
		if err != nil {
			return "", 0, err
		}
		if p == nil {
			return "", 0, i18n.Err("cli.vbaNone", filepath.Base(b))
		}
		parts = append(parts, vba.Text(filepath.Base(b), p))
		modules += len(p.Modules)
	}
	return strings.Join(parts, "\n"), modules, nil
}

// vbaOutput is the default text file for the workbook: 売上.xlsm → 売上_VBA.txt.
func vbaOutput(book string) string {
	return strings.TrimSuffix(book, filepath.Ext(book)) + "_VBA.txt"
}

// cmdVBA is xljs vba <book>... [-o out.txt]: the VBA code of the workbooks
// in one text file (next to the first workbook by default, - for stdout).
func cmdVBA(con *consoleState, args []string) int {
	fset := flag.NewFlagSet("vba", flag.ContinueOnError)
	out := fset.String("o", "", "")
	var books []string
	for len(args) > 0 {
		if err := fset.Parse(args); err != nil {
			return con.fail(nil, err)
		}
		args = fset.Args()
		if len(args) > 0 {
			books = append(books, args[0])
			args = args[1:]
		}
	}
	if len(books) == 0 {
		fmt.Fprintln(os.Stderr, i18n.T("cli.usageVBA"))
		return 2
	}
	text, _, err := vbaText(books)
	if err != nil {
		return con.fail(nil, err)
	}
	if *out == "-" {
		fmt.Print(text)
		return 0
	}
	if *out == "" {
		*out = vbaOutput(books[0])
	}
	if err := os.WriteFile(*out, []byte(text), 0o644); err != nil {
		return con.fail(nil, err)
	}
	fmt.Println(i18n.T("cli.created", *out))
	return 0
}

// dropVBA handles workbooks given instead of a script (dropped onto xljs.exe
// or chosen in its picker): it asks where to save their VBA code as one text
// file and offers to open it. Without the native UI the text is written
// next to the first workbook.
func dropVBA(con *consoleState, u ui.UI, books []string) int {
	text, modules, err := vbaText(books)
	if err != nil {
		return con.fail(u, err)
	}
	out := vbaOutput(books[0])
	native := u.Name() == "native"
	if native {
		abs, _ := filepath.Abs(out)
		p, ok := u.SaveFile(ui.PickerOptions{
			Title:         i18n.T("cli.vbaSaveTitle"),
			StartIn:       filepath.Dir(abs),
			SuggestedName: filepath.Base(abs),
			Filters:       []ui.FileFilter{{Description: i18n.T("cli.vbaText"), Patterns: []string{"*.txt"}}},
		})
		if !ok {
			return 0
		}
		out = p
	}
	if err := os.WriteFile(out, []byte(text), 0o644); err != nil {
		return con.fail(u, err)
	}
	if !native {
		fmt.Println(i18n.T("cli.created", out))
		return 0
	}
	if u.Confirm("xljs", i18n.T("cli.vbaSaved", modules, out)) {
		if err := u.OpenPath(out); err != nil {
			return con.fail(u, err)
		}
	}
	return 0
}
