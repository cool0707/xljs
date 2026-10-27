// Package vba reads the VBA source code of Excel macro workbooks: the
// VBA project (vbaProject.bin) of .xlsm/.xlsb/.xlam/.xltm files, and the
// _VBA_PROJECT_CUR storage of the old binary .xls/.xla files.
//
// The project is an OLE compound file ([MS-CFB]); its "dir" stream lists the
// modules and every module stream holds the source compressed as described
// in [MS-OVBA] 2.4.1, after the compiled code (MODULEOFFSET). The source is
// stored in the project's code page (932 for Japanese Excel).
package vba

import (
	"archive/zip"
	"bytes"
	"encoding/binary"
	"errors"
	"fmt"
	"io"
	"os"
	"strings"
	"unicode/utf16"

	"github.com/cool0707/xljs/internal/i18n"
	"github.com/richardlehane/mscfb"
)

// Module types.
const (
	Standard = "standard" // standard module (Module1)
	Class    = "class"    // class module
	Document = "document" // ThisWorkbook and the sheets' modules
	Form     = "form"     // user form (its code only; the layout is not read)
)

// Module is one VBA module.
type Module struct {
	Name string
	Type string
	// Code is the source with LF line endings, including the Attribute
	// lines at the top.
	Code string
	// Sheet is the name of the sheet a document module belongs to, when it
	// is known ("" for ThisWorkbook and for .xls/.xlsb files).
	Sheet string
}

// Project is the VBA project of a workbook.
type Project struct {
	Name     string
	CodePage int
	Modules  []Module
}

var (
	cfbSignature = []byte{0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1}
	zipSignature = []byte("PK\x03\x04")
)

// ReadFile reads the VBA project of the workbook at path. It returns nil
// (and no error) when the workbook has no macros.
func ReadFile(path string) (*Project, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	p, err := Read(data)
	if err != nil {
		return nil, fmt.Errorf("%s: %w", path, err)
	}
	return p, nil
}

// Read reads the VBA project of a workbook (or of a vbaProject.bin) held in
// memory. It returns nil (and no error) when there are no macros.
func Read(data []byte) (*Project, error) {
	switch {
	case bytes.HasPrefix(data, zipSignature):
		return readZip(data)
	case bytes.HasPrefix(data, cfbSignature):
		return readCFB(data, nil)
	}
	return nil, i18n.Err("vba.unsupported")
}

func readZip(data []byte) (*Project, error) {
	zr, err := zip.NewReader(bytes.NewReader(data), int64(len(data)))
	if err != nil {
		return nil, i18n.Err("vba.unsupported")
	}
	var bin *zip.File
	for _, f := range zr.File {
		if strings.EqualFold(pathBase(f.Name), "vbaProject.bin") {
			bin = f
			break
		}
	}
	if bin == nil {
		return nil, nil
	}
	b, err := readZipFile(bin)
	if err != nil {
		return nil, err
	}
	return readCFB(b, sheetCodeNames(zr))
}

func readZipFile(f *zip.File) ([]byte, error) {
	r, err := f.Open()
	if err != nil {
		return nil, err
	}
	defer r.Close()
	return io.ReadAll(r)
}

func pathBase(name string) string {
	if i := strings.LastIndexAny(name, `/\`); i >= 0 {
		return name[i+1:]
	}
	return name
}

// stream is one stream of a compound file.
type stream struct {
	path []string // storages, outermost first
	name string
	data []byte
}

func readCFB(data []byte, sheets map[string]string) (*Project, error) {
	doc, err := mscfb.New(bytes.NewReader(data))
	if err != nil {
		return nil, i18n.Err("vba.corrupt", err.Error())
	}
	var streams []*stream
	for f, err := doc.Next(); err == nil; f, err = doc.Next() {
		if f.FileInfo().IsDir() {
			continue
		}
		if f.Name == "EncryptedPackage" {
			return nil, i18n.Err("vba.encrypted")
		}
		// Only the streams of a VBA project are needed; skip the workbook
		// itself and the forms' layouts.
		if f.Name != "PROJECT" && !(len(f.Path) > 0 && strings.EqualFold(f.Path[len(f.Path)-1], "VBA")) {
			continue
		}
		b := make([]byte, f.Size)
		if _, err := io.ReadFull(f, b); err != nil {
			return nil, i18n.Err("vba.corrupt", err.Error())
		}
		streams = append(streams, &stream{path: f.Path, name: f.Name, data: b})
	}
	// The project is the storage holding VBA/dir: the root of a
	// vbaProject.bin, _VBA_PROJECT_CUR in an .xls.
	var dir *stream
	for _, s := range streams {
		if strings.EqualFold(s.name, "dir") {
			dir = s
			break
		}
	}
	if dir == nil {
		return nil, nil
	}
	root := dir.path[:len(dir.path)-1]
	find := func(path []string, name string) *stream {
		for _, s := range streams {
			if strings.EqualFold(s.name, name) && samePath(s.path, path) {
				return s
			}
		}
		return nil
	}

	d, err := decompress(dir.data)
	if err != nil {
		return nil, i18n.Err("vba.corrupt", "dir: "+err.Error())
	}
	info, err := parseDir(d)
	if err != nil {
		return nil, i18n.Err("vba.corrupt", "dir: "+err.Error())
	}
	dec := decoder(info.codePage)
	p := &Project{Name: dec(info.name), CodePage: info.codePage}

	var types map[string]string
	if s := find(root, "PROJECT"); s != nil {
		types = projectTypes(dec(s.data))
	}
	vbaPath := append(append([]string{}, root...), dir.path[len(dir.path)-1])
	for _, m := range info.modules {
		name := m.nameUnicode
		if name == "" {
			name = dec(m.name)
		}
		streamName := m.streamUnicode
		if streamName == "" {
			streamName = dec(m.stream)
		}
		s := find(vbaPath, streamName)
		if s == nil {
			return nil, i18n.Err("vba.moduleMissing", name)
		}
		if int(m.offset) > len(s.data) {
			return nil, i18n.Err("vba.corrupt", name)
		}
		src, err := decompress(s.data[m.offset:])
		if err != nil {
			return nil, i18n.Err("vba.corrupt", name+": "+err.Error())
		}
		mod := Module{Name: name, Code: normalizeNewlines(dec(src))}
		mod.Type = lookupFold(types, name)
		if mod.Type == "" {
			mod.Type = Class
			if m.procedural {
				mod.Type = Standard
			}
		}
		if mod.Type == Document {
			mod.Sheet = lookupFold(sheets, name)
		}
		p.Modules = append(p.Modules, mod)
	}
	return p, nil
}

func samePath(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if !strings.EqualFold(a[i], b[i]) {
			return false
		}
	}
	return true
}

func lookupFold(m map[string]string, key string) string {
	if v, ok := m[key]; ok {
		return v
	}
	for k, v := range m {
		if strings.EqualFold(k, key) {
			return v
		}
	}
	return ""
}

func normalizeNewlines(s string) string {
	s = strings.ReplaceAll(s, "\r\n", "\n")
	return strings.ReplaceAll(s, "\r", "\n")
}

// dirInfo is what the "dir" stream says about the project.
type dirInfo struct {
	name     []byte
	codePage int
	modules  []dirModule
}

type dirModule struct {
	name, stream               []byte
	nameUnicode, streamUnicode string
	offset                     uint32
	procedural                 bool
}

// parseDir reads the records of the decompressed "dir" stream ([MS-OVBA]
// 2.3.4.2). Every record is an id, a size and that many bytes, except
// PROJECTVERSION whose size field (always 4) is followed by 6 bytes.
func parseDir(d []byte) (*dirInfo, error) {
	info := &dirInfo{codePage: 1252}
	var cur *dirModule
	pos := 0
	for pos+6 <= len(d) {
		id := binary.LittleEndian.Uint16(d[pos:])
		size := int(binary.LittleEndian.Uint32(d[pos+2:]))
		pos += 6
		if id == 0x0009 { // PROJECTVERSION
			size = 6
		}
		if size < 0 || pos+size > len(d) {
			return nil, errors.New("truncated record")
		}
		v := d[pos : pos+size]
		pos += size
		switch id {
		case 0x0003: // PROJECTCODEPAGE
			if len(v) >= 2 {
				info.codePage = int(binary.LittleEndian.Uint16(v))
			}
		case 0x0004: // PROJECTNAME
			info.name = v
		case 0x0019: // MODULENAME starts a module
			if cur != nil {
				info.modules = append(info.modules, *cur)
			}
			cur = &dirModule{name: v}
		case 0x0047: // MODULENAMEUNICODE
			if cur != nil {
				cur.nameUnicode = utf16le(v)
			}
		case 0x001A: // MODULESTREAMNAME
			if cur != nil {
				cur.stream = v
			}
		case 0x0032: // MODULESTREAMNAME, Unicode part
			if cur != nil {
				cur.streamUnicode = utf16le(v)
			}
		case 0x0031: // MODULEOFFSET
			if cur != nil && len(v) >= 4 {
				cur.offset = binary.LittleEndian.Uint32(v)
			}
		case 0x0021: // MODULETYPE: procedural
			if cur != nil {
				cur.procedural = true
			}
		case 0x002B: // module terminator
			if cur != nil {
				info.modules = append(info.modules, *cur)
				cur = nil
			}
		case 0x0010: // end of the dir stream
			pos = len(d)
		}
	}
	if cur != nil {
		info.modules = append(info.modules, *cur)
	}
	return info, nil
}

func utf16le(b []byte) string {
	u := make([]uint16, len(b)/2)
	for i := range u {
		u[i] = binary.LittleEndian.Uint16(b[2*i:])
	}
	return string(utf16.Decode(u))
}

// projectTypes reads the module types from the PROJECT stream ([MS-OVBA]
// 2.3.1): Module=, Document=name/&H..., Class= and BaseClass= (forms).
func projectTypes(text string) map[string]string {
	types := map[string]string{}
	for _, line := range strings.Split(normalizeNewlines(text), "\n") {
		if strings.HasPrefix(line, "[") {
			break
		}
		key, value, ok := strings.Cut(line, "=")
		if !ok {
			continue
		}
		var t string
		switch strings.TrimSpace(key) {
		case "Module":
			t = Standard
		case "Class":
			t = Class
		case "BaseClass":
			t = Form
		case "Document":
			t = Document
			value, _, _ = strings.Cut(value, "/")
		default:
			continue
		}
		types[strings.TrimSpace(value)] = t
	}
	return types
}
