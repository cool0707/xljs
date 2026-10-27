package vba

import (
	"bytes"
	"errors"
	"strings"
	"testing"

	"github.com/cool0707/xljs/internal/i18n"
	"github.com/xuri/excelize/v2"
)

func checkProject(t *testing.T, p *Project, sheets bool) {
	t.Helper()
	if p == nil {
		t.Fatal("no project")
	}
	if p.Name != "VBAProject" || p.CodePage != 932 {
		t.Errorf("project %q, code page %d", p.Name, p.CodePage)
	}
	want := fixtureModules()
	if len(p.Modules) != len(want) {
		t.Fatalf("%d modules, want %d", len(p.Modules), len(want))
	}
	types := map[string]string{"Module": Standard, "Document": Document, "Class": Class, "BaseClass": Form}
	for i, m := range p.Modules {
		w := want[i]
		if m.Name != w.name || m.Type != types[w.kind] {
			t.Errorf("module %d: %q %s, want %q %s", i, m.Name, m.Type, w.name, types[w.kind])
		}
		if m.Code != w.code {
			t.Errorf("module %s: code\n%q\nwant\n%q", m.Name, m.Code, w.code)
		}
	}
	if sheets {
		if s := p.Modules[1].Sheet; s != "売上" {
			t.Errorf("Sheet1 is sheet %q", s)
		}
		if s := p.Modules[2].Sheet; s != "設定" {
			t.Errorf("Sheet2 is sheet %q", s)
		}
	}
	if s := p.Modules[0].Sheet; s != "" {
		t.Errorf("ThisWorkbook is sheet %q", s)
	}
}

func TestReadXLSM(t *testing.T) {
	p, err := Read(fixtureXLSM(t))
	must(t, err)
	checkProject(t, p, true)
}
func TestReadXLS(t *testing.T) {
	p, err := Read(fixtureXLS(t))
	must(t, err)
	checkProject(t, p, false)
}
func TestReadBin(t *testing.T) {
	p, err := Read(vbaProjectBin(t))
	must(t, err)
	checkProject(t, p, false)
}

func must(t *testing.T, err error) {
	t.Helper()
	if err != nil {
		t.Fatal(err)
	}
}

func TestNoMacros(t *testing.T) {
	f := excelize.NewFile()
	var buf bytes.Buffer
	must(t, f.Write(&buf))
	p, err := Read(buf.Bytes())
	if p != nil || err != nil {
		t.Errorf("xlsx: %v, %v", p, err)
	}
	// An .xls without a VBA project.
	p, err = Read(writeCFB(storage("Root Entry", streamNode("Workbook", make([]byte, 100)))))
	if p != nil || err != nil {
		t.Errorf("xls: %v, %v", p, err)
	}
}

func TestErrors(t *testing.T) {
	key := func(err error) string {
		var le *i18n.LocalizedError
		if errors.As(err, &le) {
			return le.Key
		}
		return ""
	}
	if _, err := Read([]byte("a,b\n1,2\n")); key(err) != "vba.unsupported" {
		t.Errorf("csv: %v", err)
	}
	if _, err := Read(writeCFB(storage("Root Entry", streamNode("EncryptedPackage", make([]byte, 10))))); key(err) != "vba.encrypted" {
		t.Errorf("encrypted: %v", err)
	}
	if _, err := ReadFile("does-not-exist.xlsm"); err == nil {
		t.Error("missing file: no error")
	}
}

func TestDecompress(t *testing.T) {
	// The example of [MS-OVBA] 3.2.3.
	in := []byte{0x01, 0x2F, 0xB0, 0x00, 0x23, 0x61, 0x61, 0x61, 0x62, 0x63, 0x64, 0x65, 0x82, 0x66, 0x00, 0x70, 0x61, 0x67, 0x68, 0x69, 0x6A, 0x01, 0x38, 0x08, 0x61, 0x6B, 0x6C, 0x00, 0x30, 0x6D, 0x6E, 0x6F, 0x70, 0x06, 0x71, 0x02, 0x70, 0x04, 0x10, 0x72, 0x73, 0x74, 0x75, 0x76, 0x10, 0x77, 0x78, 0x79, 0x7A, 0x00, 0x3C}
	out, err := decompress(in)
	must(t, err)
	if want := "#aaabcdefaaaaghijaaaaaklaaamnopqaaaaaaaaaaaarstuvwxyzaaa"; string(out) != want {
		t.Errorf("got %q, want %q", out, want)
	}
	// An uncompressed chunk.
	raw := bytes.Repeat([]byte("0123456789abcdef"), 256)
	out, err = decompress(append([]byte{0x01, 0xFF, 0x3F}, raw...))
	must(t, err)
	if !bytes.Equal(out, raw) {
		t.Error("uncompressed chunk differs")
	}
	// Round trip through several chunks.
	src := []byte(strings.Repeat("Cells(i, 1).Value = \"あいう\" & i\r\n", 500))
	out, err = decompress(compress(src))
	must(t, err)
	if !bytes.Equal(out, src) {
		t.Error("round trip differs")
	}
	for _, bad := range [][]byte{nil, {0x02}, {0x01, 0x03, 0xB0, 0x01, 0x00, 0x10}} {
		if _, err := decompress(bad); err == nil {
			t.Errorf("%x: no error", bad)
		}
	}
}

func TestText(t *testing.T) {
	i18n.SetLanguage("ja")
	defer i18n.SetLanguage("en")
	p, err := Read(fixtureXLSM(t))
	must(t, err)
	text := Text("売上.xlsm", p)
	for _, want := range []string{
		"# 売上.xlsm の VBA コード\n",
		"VBA プロジェクト「VBAProject」のコードです（モジュール 7 個）",
		"## ThisWorkbook（ブックのモジュール）\n\n```vba\nPrivate Sub Workbook_Open()\n    MsgBox \"ようこそ\"\nEnd Sub\n```\n",
		"## Sheet1（シート「売上」のモジュール）\n\n```vba\nOption Explicit\n\nPrivate Sub Worksheet_Change(",
		"## Module1（標準モジュール）",
		"## 集計（標準モジュール）",
		"## Class1（クラスモジュール）",
		"## UserForm1（ユーザーフォーム。",
		"コードのないモジュール: Sheet2\n",
	} {
		if !strings.Contains(text, want) {
			t.Errorf("missing %q in\n%s", want, text)
		}
	}
	if strings.Contains(text, "Attribute VB_") {
		t.Errorf("Attribute lines left:\n%s", text)
	}
	// Backticks in the code get a longer fence.
	p = &Project{Name: "P", Modules: []Module{{Name: "M", Type: Standard, Code: "s = \"```\"\n"}}}
	if text := Text("a.xlsm", p); !strings.Contains(text, "````vba\ns = \"```\"\n````\n") {
		t.Errorf("fence:\n%s", text)
	}
}
