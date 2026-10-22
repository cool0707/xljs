package api

import (
	"bytes"
	"errors"
	"fmt"
	"math"
	"strconv"
	"strings"
	"time"

	"github.com/dop251/goja"
	"github.com/xuri/excelize/v2"
)

// book wraps an open workbook.
type book struct {
	f        *excelize.File
	path     string
	date1904 bool
	// isDate caches whether a style id formats numbers as dates.
	isDate map[int]bool
	// fresh is true for a workbook from create() whose default sheet has
	// not been used yet; the first addSheet renames it.
	fresh         bool
	formulas      bool
	styleDate     int
	styleDateTime int
	closed        bool
}

func newBook(f *excelize.File, path string) *book {
	b := &book{f: f, path: path, isDate: map[int]bool{}}
	if p, err := f.GetWorkbookProps(); err == nil && p.Date1904 != nil {
		b.date1904 = *p.Date1904
	}
	return b
}

func (n *natives) book(v goja.Value) *book {
	b, ok := v.Export().(*book)
	if !ok || b == nil {
		n.throwf("invalid workbook handle")
	}
	if b.closed {
		n.throwf("workbook is closed")
	}
	return b
}

// Built-in number format ids that display dates or times, including the
// Japanese locale specific ones.
func builtinDateFmt(id int) bool {
	switch {
	case id >= 14 && id <= 22, id >= 27 && id <= 36, id >= 45 && id <= 47, id >= 50 && id <= 58:
		return true
	}
	return false
}

// dateFormatCode reports whether a custom number format displays a date or
// time. Literal text, escapes and bracketed sections are ignored; elapsed
// time formats ([h]:mm) are treated as plain numbers.
func dateFormatCode(code string) bool {
	if i := strings.Index(code, ";"); i >= 0 {
		code = code[:i]
	}
	lower := strings.ToLower(code)
	if strings.Contains(lower, "[h") || strings.Contains(lower, "[m") || strings.Contains(lower, "[s") {
		return false
	}
	var b strings.Builder
	inQuote := false
	for i := 0; i < len(lower); i++ {
		c := lower[i]
		switch {
		case inQuote:
			if c == '"' {
				inQuote = false
			}
		case c == '"':
			inQuote = true
		case c == '\\' || c == '_' || c == '*':
			i++
		case c == '[':
			if j := strings.IndexByte(lower[i:], ']'); j >= 0 {
				i += j
			}
		default:
			b.WriteByte(c)
		}
	}
	s := strings.ReplaceAll(b.String(), "general", "")
	return strings.ContainsAny(s, "ymdhs")
}

func (b *book) styleIsDate(id int) bool {
	if id == 0 {
		return false
	}
	if v, ok := b.isDate[id]; ok {
		return v
	}
	v := false
	if st, err := b.f.GetStyle(id); err == nil && st != nil {
		if st.CustomNumFmt != nil && *st.CustomNumFmt != "" {
			v = dateFormatCode(*st.CustomNumFmt)
		} else {
			v = builtinDateFmt(st.NumFmt)
		}
	}
	b.isDate[id] = v
	return v
}

type xlsxConv struct {
	n        *natives
	dateCtor goja.Constructor
}

func (n *natives) newConv() *xlsxConv {
	ctor, _ := goja.AssertConstructor(n.vm.Get("Date"))
	return &xlsxConv{n: n, dateCtor: ctor}
}

// jsDate builds a JavaScript Date whose local wall-clock time equals the
// wall-clock time t (Excel dates carry no time zone).
func (c *xlsxConv) jsDate(t time.Time) goja.Value {
	local := time.Date(t.Year(), t.Month(), t.Day(), t.Hour(), t.Minute(), t.Second(), t.Nanosecond(), time.Local)
	ms := float64(local.UnixNano()) / 1e6
	ms = math.Round(ms)
	obj, err := c.dateCtor(nil, c.n.vm.ToValue(ms))
	if err != nil {
		return c.n.vm.ToValue(t.Format(time.RFC3339))
	}
	return obj
}

// value converts a raw cell value to a typed JavaScript value.
func (c *xlsxConv) value(b *book, sheet, ref, raw string) goja.Value {
	vm := c.n.vm
	if raw == "" {
		return goja.Null()
	}
	typ, err := b.f.GetCellType(sheet, ref)
	if err != nil {
		return vm.ToValue(raw)
	}
	switch typ {
	case excelize.CellTypeBool:
		return vm.ToValue(raw == "1" || strings.EqualFold(raw, "true"))
	case excelize.CellTypeUnset, excelize.CellTypeNumber:
		f, err := strconv.ParseFloat(raw, 64)
		if err != nil {
			return vm.ToValue(raw)
		}
		if style, err := b.f.GetCellStyle(sheet, ref); err == nil && b.styleIsDate(style) {
			// Round to the millisecond to absorb floating point noise.
			if t, err := excelize.ExcelDateToTime(f, b.date1904); err == nil {
				return c.jsDate(t.Round(time.Millisecond))
			}
		}
		return vm.ToValue(f)
	case excelize.CellTypeDate:
		for _, layout := range []string{time.RFC3339Nano, "2006-01-02T15:04:05", "2006-01-02"} {
			if t, err := time.Parse(layout, raw); err == nil {
				return c.jsDate(t)
			}
		}
		return vm.ToValue(raw)
	}
	return vm.ToValue(raw)
}

func (b *book) dateStyles() (int, int, error) {
	if b.styleDate == 0 {
		fmtD := "yyyy/m/d"
		id, err := b.f.NewStyle(&excelize.Style{CustomNumFmt: &fmtD})
		if err != nil {
			return 0, 0, err
		}
		b.styleDate = id
		fmtDT := "yyyy/m/d h:mm"
		id, err = b.f.NewStyle(&excelize.Style{CustomNumFmt: &fmtDT})
		if err != nil {
			return 0, 0, err
		}
		b.styleDateTime = id
	}
	return b.styleDate, b.styleDateTime, nil
}

// setValue writes a JavaScript value into a cell.
func (b *book) setValue(sheet, ref string, v goja.Value) error {
	if b.fresh && sheet == "Sheet1" {
		b.fresh = false
	}
	if isNullish(v) {
		return b.f.SetCellValue(sheet, ref, nil)
	}
	switch x := v.Export().(type) {
	case time.Time:
		local := x.Local()
		wall := time.Date(local.Year(), local.Month(), local.Day(), local.Hour(), local.Minute(), local.Second(), local.Nanosecond(), time.UTC)
		prev, _ := b.f.GetCellStyle(sheet, ref)
		if err := b.f.SetCellValue(sheet, ref, wall); err != nil {
			return err
		}
		if prev == 0 || !b.styleIsDate(prev) {
			sd, sdt, err := b.dateStyles()
			if err != nil {
				return err
			}
			id := sdt
			if local.Hour() == 0 && local.Minute() == 0 && local.Second() == 0 && local.Nanosecond() == 0 {
				id = sd
			}
			return b.f.SetCellStyle(sheet, ref, ref, id)
		}
		return nil
	case int64:
		return b.f.SetCellValue(sheet, ref, x)
	case float64:
		if math.IsNaN(x) || math.IsInf(x, 0) {
			return b.f.SetCellValue(sheet, ref, nil)
		}
		return b.f.SetCellValue(sheet, ref, x)
	case bool:
		return b.f.SetCellValue(sheet, ref, x)
	case string:
		return b.f.SetCellStr(sheet, ref, x)
	}
	return b.f.SetCellStr(sheet, ref, v.String())
}

func (n *natives) installXLSX() {
	vm := n.vm

	n.set("xlsxOpen", func(c goja.FunctionCall) goja.Value {
		path := n.str(c, 0)
		f, err := excelize.OpenFile(path)
		if err != nil {
			n.throwf("%s: %v", path, err)
		}
		return n.track(newBook(f, path))
	})
	n.set("xlsxOpenBytes", func(c goja.FunctionCall) goja.Value {
		f, err := excelize.OpenReader(bytes.NewReader(n.bytes(c.Argument(0))))
		if err != nil {
			n.throw(err)
		}
		return n.track(newBook(f, n.str(c, 1)))
	})
	n.set("xlsxCreate", func(c goja.FunctionCall) goja.Value {
		b := newBook(excelize.NewFile(), "")
		b.fresh = true
		return n.track(b)
	})
	n.set("xlsxPath", func(c goja.FunctionCall) goja.Value {
		b := n.book(c.Argument(0))
		if b.path == "" {
			return goja.Null()
		}
		return vm.ToValue(b.path)
	})
	n.set("xlsxSheetNames", func(c goja.FunctionCall) goja.Value {
		return n.strArray(n.book(c.Argument(0)).f.GetSheetList())
	})
	n.set("xlsxAddSheet", func(c goja.FunctionCall) goja.Value {
		b := n.book(c.Argument(0))
		name := n.str(c, 1)
		if idx, _ := b.f.GetSheetIndex(name); idx >= 0 {
			n.throwf("シート %q は既に存在します", name)
		}
		if b.fresh {
			b.fresh = false
			if err := b.f.SetSheetName("Sheet1", name); err != nil {
				n.throw(err)
			}
			return goja.Undefined()
		}
		if _, err := b.f.NewSheet(name); err != nil {
			n.throw(err)
		}
		return goja.Undefined()
	})
	n.set("xlsxRemoveSheet", func(c goja.FunctionCall) goja.Value {
		b := n.book(c.Argument(0))
		if err := b.f.DeleteSheet(n.str(c, 1)); err != nil {
			n.throw(err)
		}
		return goja.Undefined()
	})
	n.set("xlsxRenameSheet", func(c goja.FunctionCall) goja.Value {
		b := n.book(c.Argument(0))
		from, to := n.str(c, 1), n.str(c, 2)
		if err := b.f.SetSheetName(from, to); err != nil {
			n.throw(err)
		}
		if b.fresh && from == "Sheet1" {
			b.fresh = false
		}
		return goja.Undefined()
	})
	n.set("xlsxHasSheet", func(c goja.FunctionCall) goja.Value {
		idx, _ := n.book(c.Argument(0)).f.GetSheetIndex(n.str(c, 1))
		return vm.ToValue(idx >= 0)
	})
	n.set("xlsxGet", func(c goja.FunctionCall) goja.Value {
		b := n.book(c.Argument(0))
		sheet, ref := n.str(c, 1), strings.ToUpper(n.str(c, 2))
		raw, err := b.f.GetCellValue(sheet, ref, excelize.Options{RawCellValue: true})
		if err != nil {
			n.throw(err)
		}
		return n.newConv().value(b, sheet, ref, raw)
	})
	n.set("xlsxGetText", func(c goja.FunctionCall) goja.Value {
		b := n.book(c.Argument(0))
		s, err := b.f.GetCellValue(n.str(c, 1), strings.ToUpper(n.str(c, 2)))
		if err != nil {
			n.throw(err)
		}
		return vm.ToValue(s)
	})
	n.set("xlsxSet", func(c goja.FunctionCall) goja.Value {
		b := n.book(c.Argument(0))
		if err := b.setValue(n.str(c, 1), strings.ToUpper(n.str(c, 2)), c.Argument(3)); err != nil {
			n.throw(err)
		}
		return goja.Undefined()
	})
	n.set("xlsxGetFormula", func(c goja.FunctionCall) goja.Value {
		b := n.book(c.Argument(0))
		f, err := b.f.GetCellFormula(n.str(c, 1), strings.ToUpper(n.str(c, 2)))
		if err != nil {
			n.throw(err)
		}
		if f == "" {
			return goja.Null()
		}
		if !strings.HasPrefix(f, "=") {
			f = "=" + f
		}
		return vm.ToValue(f)
	})
	n.set("xlsxSetFormula", func(c goja.FunctionCall) goja.Value {
		b := n.book(c.Argument(0))
		sheet := n.str(c, 1)
		if b.fresh && sheet == "Sheet1" {
			b.fresh = false
		}
		formula := strings.TrimPrefix(n.str(c, 3), "=")
		if err := b.f.SetCellFormula(sheet, strings.ToUpper(n.str(c, 2)), formula); err != nil {
			n.throw(err)
		}
		b.formulas = true
		return goja.Undefined()
	})
	n.set("xlsxCalc", func(c goja.FunctionCall) goja.Value {
		b := n.book(c.Argument(0))
		s, err := b.f.CalcCellValue(n.str(c, 1), strings.ToUpper(n.str(c, 2)))
		if err != nil {
			n.throw(err)
		}
		return vm.ToValue(s)
	})
	// xlsxReadAll(book, sheet, text) -> any[][]
	n.set("xlsxReadAll", func(c goja.FunctionCall) goja.Value {
		b := n.book(c.Argument(0))
		sheet := n.str(c, 1)
		text := c.Argument(2).ToBoolean()
		var opts []excelize.Options
		if !text {
			opts = append(opts, excelize.Options{RawCellValue: true})
		}
		rows, err := b.f.GetRows(sheet, opts...)
		if err != nil {
			n.throw(err)
		}
		conv := n.newConv()
		out := make([]goja.Value, len(rows))
		for r, row := range rows {
			vals := make([]goja.Value, len(row))
			for col, raw := range row {
				if text {
					vals[col] = vm.ToValue(raw)
					continue
				}
				ref, _ := excelize.CoordinatesToCellName(col+1, r+1)
				vals[col] = conv.value(b, sheet, ref, raw)
			}
			out[r] = n.array(vals)
		}
		return n.array(out)
	})
	// xlsxWriteRows(book, sheet, startRef, rows: any[][])
	n.set("xlsxWriteRows", func(c goja.FunctionCall) goja.Value {
		b := n.book(c.Argument(0))
		sheet := n.str(c, 1)
		col0, row0, err := excelize.CellNameToCoordinates(strings.ToUpper(n.optStr(c.Argument(2), "A1")))
		if err != nil {
			n.throw(err)
		}
		for r, rowv := range n.elems(c.Argument(3)) {
			for col, v := range n.elems(rowv) {
				ref, err := excelize.CoordinatesToCellName(col0+col, row0+r)
				if err != nil {
					n.throw(err)
				}
				if isNullish(v) {
					continue
				}
				if err := b.setValue(sheet, ref, v); err != nil {
					n.throw(fmt.Errorf("%s!%s: %w", sheet, ref, err))
				}
			}
		}
		return goja.Undefined()
	})
	n.set("xlsxRowCount", func(c goja.FunctionCall) goja.Value {
		b := n.book(c.Argument(0))
		rows, err := b.f.GetRows(n.str(c, 1), excelize.Options{RawCellValue: true})
		if err != nil {
			n.throw(err)
		}
		return vm.ToValue(len(rows))
	})
	n.set("xlsxDimension", func(c goja.FunctionCall) goja.Value {
		b := n.book(c.Argument(0))
		rows, err := b.f.GetRows(n.str(c, 1), excelize.Options{RawCellValue: true})
		if err != nil {
			n.throw(err)
		}
		maxCol := 0
		for _, r := range rows {
			if len(r) > maxCol {
				maxCol = len(r)
			}
		}
		if len(rows) == 0 || maxCol == 0 {
			return vm.ToValue("")
		}
		end, _ := excelize.CoordinatesToCellName(maxCol, len(rows))
		return vm.ToValue("A1:" + end)
	})
	n.set("xlsxSetColWidth", func(c goja.FunctionCall) goja.Value {
		b := n.book(c.Argument(0))
		if err := b.f.SetColWidth(n.str(c, 1), strings.ToUpper(n.str(c, 2)), strings.ToUpper(n.str(c, 3)), c.Argument(4).ToFloat()); err != nil {
			n.throw(err)
		}
		return goja.Undefined()
	})
	n.set("xlsxSave", func(c goja.FunctionCall) goja.Value {
		b := n.book(c.Argument(0))
		path := n.str(c, 1)
		if path == "" {
			path = b.path
		}
		if path == "" {
			n.throw(errors.New("保存先が指定されていません"))
		}
		b.prepareSave()
		if err := b.f.SaveAs(path); err != nil {
			n.throwf("%s: %v", path, err)
		}
		b.path = path
		return goja.Undefined()
	})
	n.set("xlsxToBytes", func(c goja.FunctionCall) goja.Value {
		b := n.book(c.Argument(0))
		b.prepareSave()
		buf, err := b.f.WriteToBuffer()
		if err != nil {
			n.throw(err)
		}
		return vm.ToValue(vm.NewArrayBuffer(buf.Bytes()))
	})
	n.set("xlsxClose", func(c goja.FunctionCall) goja.Value {
		b, ok := c.Argument(0).Export().(*book)
		if ok && b != nil && !b.closed {
			b.closed = true
			b.f.Close()
		}
		return goja.Undefined()
	})
	n.set("xlsxCellName", func(c goja.FunctionCall) goja.Value {
		s, err := excelize.CoordinatesToCellName(int(c.Argument(0).ToInteger()), int(c.Argument(1).ToInteger()))
		if err != nil {
			n.throw(err)
		}
		return vm.ToValue(s)
	})
	n.set("xlsxParseRef", func(c goja.FunctionCall) goja.Value {
		col, row, err := excelize.CellNameToCoordinates(strings.ToUpper(n.str(c, 0)))
		if err != nil {
			n.throw(err)
		}
		return vm.NewArray(col, row)
	})
	n.set("xlsxColumnName", func(c goja.FunctionCall) goja.Value {
		s, err := excelize.ColumnNumberToName(int(c.Argument(0).ToInteger()))
		if err != nil {
			n.throw(err)
		}
		return vm.ToValue(s)
	})
}

func (n *natives) track(b *book) goja.Value {
	n.env.OnCleanup(func() {
		if !b.closed {
			b.closed = true
			b.f.Close()
		}
	})
	return n.vm.ToValue(b)
}

func (b *book) prepareSave() {
	if b.formulas {
		t := true
		_ = b.f.SetCalcProps(&excelize.CalcPropsOptions{FullCalcOnLoad: &t})
	}
}
