package vba

// Test fixtures: a VBA project built the way Excel stores it (an OLE
// compound file with compressed streams), so that the reader can be tested
// without Excel. The files written with -update are checked with olevba
// (pip install oletools) too.

import (
	"bytes"
	"encoding/binary"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"testing"
	"unicode/utf16"

	"github.com/xuri/excelize/v2"
	"golang.org/x/text/encoding/japanese"
)

var update = flag.Bool("update", false, "rewrite testdata/scripts/macro.xlsm")

type fixtureModule struct {
	name, kind string // kind: Module, Document, Class, BaseClass
	code       string // CRLF line endings
}

func crlf(s string) string { return strings.ReplaceAll(s, "\n", "\r\n") }

func longModule() string {
	var b strings.Builder
	b.WriteString("Attribute VB_Name = \"集計\"\nOption Explicit\n\n' 長いモジュール（複数のチャンクに分かれる）\nSub 一覧を作る()\n    Dim i As Long\n")
	for i := 1; i <= 150; i++ {
		fmt.Fprintf(&b, "    Cells(%d, 1).Value = \"行 %d\": Cells(%d, 2).Value = %d * 2\n", i, i, i, i)
	}
	b.WriteString("End Sub\n")
	return b.String()
}

func fixtureModules() []fixtureModule {
	return []fixtureModule{
		{"ThisWorkbook", "Document", "Attribute VB_Name = \"ThisWorkbook\"\nAttribute VB_Base = \"0{00020819-0000-0000-C000-000000000046}\"\nAttribute VB_GlobalNameSpace = False\nAttribute VB_Creatable = False\nAttribute VB_PredeclaredId = True\nAttribute VB_Exposed = True\nAttribute VB_TemplateDerived = False\nAttribute VB_Customizable = True\nPrivate Sub Workbook_Open()\n    MsgBox \"ようこそ\"\nEnd Sub\n"},
		{"Sheet1", "Document", "Attribute VB_Name = \"Sheet1\"\nAttribute VB_Base = \"0{00020820-0000-0000-C000-000000000046}\"\nAttribute VB_PredeclaredId = True\nOption Explicit\n\nPrivate Sub Worksheet_Change(ByVal Target As Range)\n    Range(\"C1\").Value = WorksheetFunction.Sum(Range(\"B:B\"))\nEnd Sub\n"},
		{"Sheet2", "Document", "Attribute VB_Name = \"Sheet2\"\nAttribute VB_Base = \"0{00020820-0000-0000-C000-000000000046}\"\nOption Explicit\n"},
		{"Module1", "Module", "Attribute VB_Name = \"Module1\"\nOption Explicit\n\n' 売上を集計する\nSub 売上集計()\n    Dim ws As Worksheet\n    Set ws = Worksheets(\"売上\")\n    MsgBox \"合計: \" & WorksheetFunction.Sum(ws.Range(\"B2:B100\"))\nEnd Sub\n"},
		{"集計", "Module", longModule()},
		{"Class1", "Class", "Attribute VB_Name = \"Class1\"\nAttribute VB_PredeclaredId = False\nPublic 名前 As String\n"},
		{"UserForm1", "BaseClass", "Attribute VB_Name = \"UserForm1\"\nPrivate Sub CommandButton1_Click()\n    Unload Me\nEnd Sub\n"},
	}
}

func cp932(t testing.TB, s string) []byte {
	b, err := japanese.ShiftJIS.NewEncoder().Bytes([]byte(s))
	if err != nil {
		t.Fatal(err)
	}
	return b
}

func utf16Bytes(s string) []byte {
	u := utf16.Encode([]rune(s))
	b := make([]byte, 2*len(u))
	for i, c := range u {
		binary.LittleEndian.PutUint16(b[2*i:], c)
	}
	return b
}

// compress is the compression of [MS-OVBA] 2.4.1.3.1 (greedy matching).
func compress(src []byte) []byte {
	out := []byte{1}
	for start := 0; start < len(src); start += 4096 {
		chunk := src[start:min(start+4096, len(src))]
		var c []byte
		pos := 0
		for pos < len(chunk) {
			flagAt := len(c)
			c = append(c, 0)
			for bit := 0; bit < 8 && pos < len(chunk); bit++ {
				bits := copyTokenBits(pos)
				maxLen := 0xFFFF>>bits + 3
				bestLen, bestOff := 0, 0
				for cand := max(0, pos-(1<<bits)); cand < pos; cand++ {
					l := 0
					for pos+l < len(chunk) && l < maxLen && chunk[cand+l] == chunk[pos+l] {
						l++
					}
					if l > bestLen {
						bestLen, bestOff = l, pos-cand
					}
				}
				if bestLen >= 3 {
					tok := uint16((bestOff-1)<<(16-bits) | (bestLen - 3))
					c = binary.LittleEndian.AppendUint16(c, tok)
					c[flagAt] |= 1 << bit
					pos += bestLen
				} else {
					c = append(c, chunk[pos])
					pos++
				}
			}
		}
		out = binary.LittleEndian.AppendUint16(out, uint16(len(c)+2-3)|0xB000)
		out = append(out, c...)
	}
	return out
}

// cfbNode is a storage (children) or a stream (data) of a compound file.
type cfbNode struct {
	name     string
	data     []byte
	children []*cfbNode
	storage  bool
}

func storage(name string, children ...*cfbNode) *cfbNode {
	return &cfbNode{name: name, storage: true, children: children}
}

func streamNode(name string, data []byte) *cfbNode { return &cfbNode{name: name, data: data} }

// writeCFB writes a version 3 compound file ([MS-CFB]): 512-byte sectors,
// streams under 4096 bytes in the mini stream.
func writeCFB(root *cfbNode) []byte {
	const (
		secSize  = 512
		free     = 0xFFFFFFFF
		endChain = 0xFFFFFFFE
		fatSect  = 0xFFFFFFFD
		noStream = 0xFFFFFFFF
	)
	type entry struct {
		node               *cfbNode
		left, right, child uint32
		start              uint32
		size               uint64
	}
	var entries []*entry
	var add func(n *cfbNode) uint32
	// The children of a storage form a binary search tree ordered by name
	// length, then by upper-cased name.
	var tree func(kids []*cfbNode) uint32
	tree = func(kids []*cfbNode) uint32 {
		if len(kids) == 0 {
			return noStream
		}
		mid := len(kids) / 2
		id := add(kids[mid])
		e := entries[id]
		e.left = tree(kids[:mid])
		e.right = tree(kids[mid+1:])
		return id
	}
	add = func(n *cfbNode) uint32 {
		id := uint32(len(entries))
		e := &entry{node: n, left: noStream, right: noStream, child: noStream}
		entries = append(entries, e)
		if n.storage {
			kids := append([]*cfbNode{}, n.children...)
			sort.Slice(kids, func(i, j int) bool {
				a, b := utf16.Encode([]rune(kids[i].name)), utf16.Encode([]rune(kids[j].name))
				if len(a) != len(b) {
					return len(a) < len(b)
				}
				return strings.ToUpper(kids[i].name) < strings.ToUpper(kids[j].name)
			})
			e.child = tree(kids)
		}
		return id
	}
	add(root)

	var sectors [][]byte // regular sectors, in order
	var fat []uint32
	alloc := func(data []byte) uint32 {
		if len(data) == 0 {
			return endChain
		}
		first := uint32(len(sectors))
		for off := 0; off < len(data); off += secSize {
			s := make([]byte, secSize)
			copy(s, data[off:])
			sectors = append(sectors, s)
			fat = append(fat, uint32(len(sectors)))
		}
		fat[len(fat)-1] = endChain
		return first
	}
	var mini []byte
	var miniFAT []uint32
	for _, e := range entries[1:] {
		if e.node.storage {
			continue
		}
		e.size = uint64(len(e.node.data))
		if len(e.node.data) >= 4096 {
			e.start = alloc(e.node.data)
			continue
		}
		if len(e.node.data) == 0 {
			e.start = endChain
			continue
		}
		e.start = uint32(len(mini) / 64)
		for off := 0; off < len(e.node.data); off += 64 {
			blk := make([]byte, 64)
			copy(blk, e.node.data[off:])
			mini = append(mini, blk...)
			miniFAT = append(miniFAT, uint32(len(mini)/64))
		}
		miniFAT[len(miniFAT)-1] = endChain
	}
	entries[0].start = alloc(mini)
	entries[0].size = uint64(len(mini))
	var mf []byte
	for _, v := range miniFAT {
		mf = binary.LittleEndian.AppendUint32(mf, v)
	}
	for len(mf)%secSize != 0 {
		mf = binary.LittleEndian.AppendUint32(mf, free)
	}
	miniFATStart := alloc(mf)
	if len(mf) == 0 {
		miniFATStart = endChain
	}

	dir := make([]byte, 0, len(entries)*128)
	for _, e := range entries {
		b := make([]byte, 128)
		name := utf16Bytes(e.node.name)
		copy(b, name)
		binary.LittleEndian.PutUint16(b[64:], uint16(len(name)+2))
		switch {
		case e == entries[0]:
			b[66] = 5
		case e.node.storage:
			b[66] = 1
		default:
			b[66] = 2
		}
		b[67] = 1 // black
		binary.LittleEndian.PutUint32(b[68:], e.left)
		binary.LittleEndian.PutUint32(b[72:], e.right)
		binary.LittleEndian.PutUint32(b[76:], e.child)
		if !e.node.storage || e == entries[0] {
			binary.LittleEndian.PutUint32(b[116:], e.start)
			binary.LittleEndian.PutUint64(b[120:], e.size)
		}
		dir = append(dir, b...)
	}
	for len(dir)%secSize != 0 {
		unused := make([]byte, 128)
		binary.LittleEndian.PutUint32(unused[68:], noStream)
		binary.LittleEndian.PutUint32(unused[72:], noStream)
		binary.LittleEndian.PutUint32(unused[76:], noStream)
		dir = append(dir, unused...)
	}
	dirStart := alloc(dir)

	nFAT := 1
	for (len(sectors)+nFAT)*1 > nFAT*(secSize/4) {
		nFAT++
	}
	fatStart := uint32(len(sectors))
	for i := 0; i < nFAT; i++ {
		fat = append(fat, fatSect)
	}
	var fb []byte
	for _, v := range fat {
		fb = binary.LittleEndian.AppendUint32(fb, v)
	}
	for len(fb) < nFAT*secSize {
		fb = binary.LittleEndian.AppendUint32(fb, free)
	}
	for off := 0; off < len(fb); off += secSize {
		sectors = append(sectors, fb[off:off+secSize])
	}

	h := make([]byte, 512)
	copy(h, cfbSignature)
	binary.LittleEndian.PutUint16(h[24:], 0x003E)
	binary.LittleEndian.PutUint16(h[26:], 0x0003)
	binary.LittleEndian.PutUint16(h[28:], 0xFFFE)
	binary.LittleEndian.PutUint16(h[30:], 9)
	binary.LittleEndian.PutUint16(h[32:], 6)
	binary.LittleEndian.PutUint32(h[44:], uint32(nFAT))
	binary.LittleEndian.PutUint32(h[48:], dirStart)
	binary.LittleEndian.PutUint32(h[56:], 4096)
	binary.LittleEndian.PutUint32(h[60:], miniFATStart)
	binary.LittleEndian.PutUint32(h[64:], uint32(len(mf)/secSize))
	binary.LittleEndian.PutUint32(h[68:], endChain)
	for i := 0; i < 109; i++ {
		v := uint32(free)
		if i < nFAT {
			v = fatStart + uint32(i)
		}
		binary.LittleEndian.PutUint32(h[76+4*i:], v)
	}
	out := bytes.NewBuffer(h)
	for _, s := range sectors {
		out.Write(s)
	}
	return out.Bytes()
}

// vbaStorages returns the VBA storage and the PROJECT stream of a project
// with the fixture modules.
func vbaStorages(t testing.TB) (*cfbNode, *cfbNode) {
	rec := func(b []byte, id uint16, data []byte) []byte {
		b = binary.LittleEndian.AppendUint16(b, id)
		b = binary.LittleEndian.AppendUint32(b, uint32(len(data)))
		return append(b, data...)
	}
	u16 := func(v uint16) []byte { return binary.LittleEndian.AppendUint16(nil, v) }
	u32 := func(v uint32) []byte { return binary.LittleEndian.AppendUint32(nil, v) }

	mods := fixtureModules()
	var d []byte
	d = rec(d, 0x0001, u32(1))
	d = rec(d, 0x0002, u32(0x409))
	d = rec(d, 0x0014, u32(0x409))
	d = rec(d, 0x0003, u16(932))
	d = rec(d, 0x0004, []byte("VBAProject"))
	d = rec(d, 0x0005, nil)
	d = rec(d, 0x0040, nil)
	d = rec(d, 0x0006, nil)
	d = rec(d, 0x003D, nil)
	d = rec(d, 0x0007, u32(0))
	d = rec(d, 0x0008, u32(0))
	d = binary.LittleEndian.AppendUint16(d, 0x0009) // PROJECTVERSION
	d = binary.LittleEndian.AppendUint32(d, 4)
	d = binary.LittleEndian.AppendUint32(d, 1)
	d = binary.LittleEndian.AppendUint16(d, 2)
	d = rec(d, 0x000C, nil)
	d = rec(d, 0x003C, nil)
	// A registered reference (stdole), skipped by the reader.
	d = rec(d, 0x0016, []byte("stdole"))
	d = rec(d, 0x003E, utf16Bytes("stdole"))
	libid := []byte(`*\G{00020430-0000-0000-C000-000000000046}#2.0#0#C:\Windows\System32\stdole2.tlb#OLE Automation`)
	d = rec(d, 0x000D, append(append(u32(uint32(len(libid))), libid...), 0, 0, 0, 0, 0, 0))
	d = rec(d, 0x000F, u16(uint16(len(mods))))
	d = rec(d, 0x0013, u16(0xFFFF))

	vba := storage("VBA", streamNode("_VBA_PROJECT", []byte{0xCC, 0x61, 0xFF, 0xFF, 0x00, 0x00, 0x00}))
	project := "ID=\"{5A1B2C3D-0000-0000-0000-000000000000}\"\n"
	for i, m := range mods {
		d = rec(d, 0x0019, cp932(t, m.name))
		d = rec(d, 0x0047, utf16Bytes(m.name))
		d = rec(d, 0x001A, cp932(t, m.name))
		d = rec(d, 0x0032, utf16Bytes(m.name))
		d = rec(d, 0x001C, nil)
		d = rec(d, 0x0048, nil)
		// Some compiled code before the source.
		cache := bytes.Repeat([]byte{0xAA}, 20+i)
		d = rec(d, 0x0031, u32(uint32(len(cache))))
		d = rec(d, 0x001E, u32(0))
		d = rec(d, 0x002C, u16(0xFFFF))
		if m.kind == "Module" {
			d = rec(d, 0x0021, nil)
		} else {
			d = rec(d, 0x0022, nil)
		}
		d = rec(d, 0x002B, nil)
		vba.children = append(vba.children, streamNode(m.name, append(cache, compress(cp932(t, crlf(m.code)))...)))
		if m.kind == "Document" {
			project += "Document=" + m.name + "/&H00000000\n"
		} else {
			project += m.kind + "=" + m.name + "\n"
		}
	}
	d = rec(d, 0x0010, nil)
	vba.children = append(vba.children, streamNode("dir", compress(d)))
	project += "Name=\"VBAProject\"\nHelpContextID=\"0\"\nVersionCompatible32=\"393222000\"\nCMG=\"0000\"\nDPB=\"0000\"\nGC=\"0000\"\n\n[Host Extender Info]\n&H00000001={3832D640-CF90-11CF-8E43-00A0C911005A};VBE;&H00000000\n"
	return vba, streamNode("PROJECT", cp932(t, crlf(project)))
}

// vbaProjectBin is the vbaProject.bin of the fixture.
func vbaProjectBin(t testing.TB) []byte {
	vba, project := vbaStorages(t)
	return writeCFB(storage("Root Entry", vba, project))
}

// fixtureXLS is an .xls holding the fixture project.
func fixtureXLS(t testing.TB) []byte {
	vba, project := vbaStorages(t)
	return writeCFB(storage("Root Entry",
		streamNode("Workbook", bytes.Repeat([]byte{0}, 4200)),
		storage("_VBA_PROJECT_CUR", vba, project)))
}

// fixtureXLSM is an .xlsm with the sheets 売上 (Sheet1) and 設定 (Sheet2)
// and the fixture project.
func fixtureXLSM(t testing.TB) []byte {
	f := excelize.NewFile()
	defer f.Close()
	if err := f.SetSheetName("Sheet1", "売上"); err != nil {
		t.Fatal(err)
	}
	if _, err := f.NewSheet("設定"); err != nil {
		t.Fatal(err)
	}
	f.SetCellValue("売上", "A1", "品名")
	f.SetCellValue("売上", "B1", "金額")
	f.SetCellValue("売上", "A2", "りんご")
	f.SetCellValue("売上", "B2", 120)
	for sheet, code := range map[string]string{"売上": "Sheet1", "設定": "Sheet2"} {
		if err := f.SetSheetProps(sheet, &excelize.SheetPropsOptions{CodeName: &code}); err != nil {
			t.Fatal(err)
		}
	}
	wb := "ThisWorkbook"
	if err := f.SetWorkbookProps(&excelize.WorkbookPropsOptions{CodeName: &wb}); err != nil {
		t.Fatal(err)
	}
	if err := f.AddVBAProject(vbaProjectBin(t)); err != nil {
		t.Fatal(err)
	}
	f.Path = "macro.xlsm" // the content type of the workbook part follows the extension
	var buf bytes.Buffer
	if err := f.Write(&buf); err != nil {
		t.Fatal(err)
	}
	return buf.Bytes()
}

// TestWriteFixture writes testdata/scripts/macro.xlsm (used by the script
// tests) with -update.
func TestWriteFixture(t *testing.T) {
	path := filepath.Join("..", "..", "testdata", "scripts", "macro.xlsm")
	if *update {
		if err := os.WriteFile(path, fixtureXLSM(t), 0o644); err != nil {
			t.Fatal(err)
		}
		if dir := os.Getenv("VBA_FIXTURE_DIR"); dir != "" {
			os.WriteFile(filepath.Join(dir, "macro.xls"), fixtureXLS(t), 0o644)
			os.WriteFile(filepath.Join(dir, "vbaProject.bin"), vbaProjectBin(t), 0o644)
		}
	}
	p, err := ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	checkProject(t, p, true)
}
