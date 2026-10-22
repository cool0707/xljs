package api

import (
	"fmt"
	"math"
	"strconv"
	"strings"

	"github.com/cool0707/xljs/internal/i18n"
	"github.com/dop251/goja"
	"github.com/mattn/go-runewidth"
	"github.com/xuri/excelize/v2"
)

// styleSpec is a partial cell style coming from JavaScript. nil fields are
// left unchanged when the spec is merged into a cell's existing style.
type styleSpec struct {
	font      *fontSpec
	fill      *string // "" removes the fill
	align     *alignSpec
	numFmt    *string
	numFmtID  *int
	borders   map[string]*borderSide // per side for every cell
	outline   *borderSide            // outer edges of the range
	inside    *borderSide            // edges between cells of the range
	insideH   *borderSide
	insideV   *borderSide
	protected *bool
}

type fontSpec struct {
	name                        *string
	size                        *float64
	bold, italic, strike        *bool
	underline, color, vertAlign *string
}

type alignSpec struct {
	horizontal, vertical *string
	wrap, shrink         *bool
	indent, rotation     *int
}

type borderSide struct {
	style int // 0 removes the border
	color string
}

// borderNames are the Excel border styles; the index is excelize's style id.
var borderNames = []string{"none", "thin", "medium", "dashed", "dotted", "thick", "double",
	"hair", "mediumDashed", "dashDot", "mediumDashDot", "dashDotDot", "mediumDashDotDot", "slantDashDot"}

var borderStyles = func() map[string]int {
	m := map[string]int{}
	for i, name := range borderNames {
		m[strings.ToLower(name)] = i
	}
	return m
}()

var namedColors = map[string]string{
	"black": "000000", "white": "FFFFFF", "red": "FF0000", "green": "008000", "blue": "0000FF",
	"yellow": "FFFF00", "orange": "FFA500", "gray": "808080", "grey": "808080", "silver": "C0C0C0",
	"navy": "000080", "purple": "800080", "teal": "008080", "lime": "00FF00", "aqua": "00FFFF",
	"cyan": "00FFFF", "magenta": "FF00FF", "fuchsia": "FF00FF", "maroon": "800000", "olive": "808000",
	"lightgray": "D3D3D3", "lightgrey": "D3D3D3", "darkgray": "A9A9A9", "darkgrey": "A9A9A9",
}

// normColor converts "#RGB", "#RRGGBB", "RRGGBB", "#AARRGGBB" or a CSS-like
// color name to "RRGGBB".
func normColor(s string) (string, error) {
	c := strings.ToLower(strings.TrimSpace(s))
	if v, ok := namedColors[c]; ok {
		return v, nil
	}
	c = strings.TrimPrefix(c, "#")
	switch len(c) {
	case 3:
		c = string([]byte{c[0], c[0], c[1], c[1], c[2], c[2]})
	case 8:
		c = c[2:]
	}
	if len(c) != 6 {
		return "", i18n.Err("xlsx.invalidColor", s)
	}
	if _, err := strconv.ParseUint(c, 16, 32); err != nil {
		return "", i18n.Err("xlsx.invalidColor", s)
	}
	return strings.ToUpper(c), nil
}

// existingColor normalizes a color read back from a workbook (ARGB).
func existingColor(s string) string {
	if len(s) == 8 {
		return s[2:]
	}
	return s
}

// ---- parsing from JavaScript ----

type specReader struct {
	n   *natives
	obj *goja.Object
}

func (r specReader) has(key string) bool {
	v := r.obj.Get(key)
	return v != nil && !goja.IsUndefined(v)
}

func (r specReader) str(key string) *string {
	if !r.has(key) {
		return nil
	}
	v := r.obj.Get(key)
	s := ""
	if !goja.IsNull(v) {
		s = v.String()
	}
	return &s
}

func (r specReader) boolean(key string) *bool {
	if !r.has(key) {
		return nil
	}
	b := r.obj.Get(key).ToBoolean()
	return &b
}

func (r specReader) num(key string) *float64 {
	if !r.has(key) || goja.IsNull(r.obj.Get(key)) {
		return nil
	}
	f := r.obj.Get(key).ToFloat()
	return &f
}

func (r specReader) integer(key string) *int {
	f := r.num(key)
	if f == nil {
		return nil
	}
	i := int(math.Round(*f))
	return &i
}

func (n *natives) child(v goja.Value) (specReader, bool) {
	if isNullish(v) {
		return specReader{}, false
	}
	obj, ok := v.(*goja.Object)
	if !ok {
		return specReader{}, false
	}
	return specReader{n, obj}, true
}

// borderSideOf parses "thin", true, null/"none", or {style, color}.
func (n *natives) borderSideOf(v goja.Value) (*borderSide, error) {
	if v == nil || goja.IsUndefined(v) {
		return nil, nil
	}
	if goja.IsNull(v) {
		return &borderSide{}, nil
	}
	parseStyle := func(s string) (int, error) {
		st, ok := borderStyles[strings.ToLower(strings.TrimSpace(s))]
		if !ok {
			return 0, i18n.Err("xlsx.invalidBorder", s)
		}
		return st, nil
	}
	switch x := v.Export().(type) {
	case bool:
		if x {
			return &borderSide{style: 1, color: "000000"}, nil
		}
		return &borderSide{}, nil
	case string:
		st, err := parseStyle(x)
		return &borderSide{style: st, color: "000000"}, err
	}
	r, ok := n.child(v)
	if !ok {
		return nil, i18n.Err("xlsx.invalidBorder", v.String())
	}
	side := &borderSide{style: 1, color: "000000"}
	if s := r.str("style"); s != nil {
		st, err := parseStyle(*s)
		if err != nil {
			return nil, err
		}
		side.style = st
	}
	if c := r.str("color"); c != nil && *c != "" {
		col, err := normColor(*c)
		if err != nil {
			return nil, err
		}
		side.color = col
	}
	return side, nil
}

func (n *natives) parseStyle(v goja.Value) (*styleSpec, error) {
	r, ok := n.child(v)
	if !ok {
		return &styleSpec{}, nil
	}
	sp := &styleSpec{}
	if fr, ok := n.child(r.obj.Get("font")); ok {
		f := &fontSpec{name: fr.str("name"), size: fr.num("size"), bold: fr.boolean("bold"),
			italic: fr.boolean("italic"), strike: fr.boolean("strike"), vertAlign: fr.str("vertAlign")}
		if fr.has("underline") {
			u := ""
			switch x := fr.obj.Get("underline").Export().(type) {
			case bool:
				if x {
					u = "single"
				}
			case string:
				u = x
				if u == "none" {
					u = ""
				}
			}
			f.underline = &u
		}
		if c := fr.str("color"); c != nil {
			col := ""
			if *c != "" {
				var err error
				if col, err = normColor(*c); err != nil {
					return nil, err
				}
			}
			f.color = &col
		}
		sp.font = f
	}
	if r.has("fill") {
		fv := r.obj.Get("fill")
		var s string
		if fr, ok := n.child(fv); ok {
			if c := fr.str("color"); c != nil {
				s = *c
			}
		} else if !goja.IsNull(fv) {
			s = fv.String()
		}
		if s != "" && !strings.EqualFold(s, "none") {
			col, err := normColor(s)
			if err != nil {
				return nil, err
			}
			s = col
		} else {
			s = ""
		}
		sp.fill = &s
	}
	if ar, ok := n.child(r.obj.Get("alignment")); ok {
		a := &alignSpec{horizontal: ar.str("horizontal"), vertical: ar.str("vertical"),
			wrap: ar.boolean("wrap"), shrink: ar.boolean("shrink"), indent: ar.integer("indent"), rotation: ar.integer("rotation")}
		if a.vertical != nil && *a.vertical == "middle" {
			c := "center"
			a.vertical = &c
		}
		sp.align = a
	}
	if r.has("numberFormat") {
		nv := r.obj.Get("numberFormat")
		if i, ok := nv.Export().(int64); ok {
			id := int(i)
			sp.numFmtID = &id
		} else {
			s := ""
			if !goja.IsNull(nv) {
				s = nv.String()
			}
			sp.numFmt = &s
		}
	}
	if r.has("border") {
		bv := r.obj.Get("border")
		br, isObj := n.child(bv)
		_, isStr := bv.Export().(string)
		if !isObj || isStr || br.has("style") || br.has("color") {
			// shorthand: every side of every cell
			side, err := n.borderSideOf(bv)
			if err != nil {
				return nil, err
			}
			sp.borders = map[string]*borderSide{"left": side, "right": side, "top": side, "bottom": side}
		} else {
			sp.borders = map[string]*borderSide{}
			if br.has("all") {
				side, err := n.borderSideOf(br.obj.Get("all"))
				if err != nil {
					return nil, err
				}
				for _, k := range []string{"left", "right", "top", "bottom"} {
					sp.borders[k] = side
				}
			}
			for _, k := range []string{"left", "right", "top", "bottom", "diagonalUp", "diagonalDown"} {
				if br.has(k) {
					side, err := n.borderSideOf(br.obj.Get(k))
					if err != nil {
						return nil, err
					}
					sp.borders[k] = side
				}
			}
			var err error
			if sp.outline, err = n.borderSideOf(br.obj.Get("outline")); err != nil {
				return nil, err
			}
			if sp.inside, err = n.borderSideOf(br.obj.Get("inside")); err != nil {
				return nil, err
			}
			if sp.insideH, err = n.borderSideOf(br.obj.Get("horizontal")); err != nil {
				return nil, err
			}
			if sp.insideV, err = n.borderSideOf(br.obj.Get("vertical")); err != nil {
				return nil, err
			}
		}
	}
	if pr, ok := n.child(r.obj.Get("protection")); ok {
		sp.protected = pr.boolean("locked")
	}
	return sp, nil
}

// ---- applying ----

// sidesFor returns the border sides for the cell at (col,row) inside the
// range [c1..c2]x[r1..r2].
func (sp *styleSpec) sidesFor(col, row, c1, r1, c2, r2 int) map[string]*borderSide {
	if sp.borders == nil && sp.outline == nil && sp.inside == nil && sp.insideH == nil && sp.insideV == nil {
		return nil
	}
	m := map[string]*borderSide{}
	for k, v := range sp.borders {
		m[k] = v
	}
	set := func(side string, b *borderSide) {
		if b != nil {
			m[side] = b
		}
	}
	h, v := sp.insideH, sp.insideV
	if h == nil {
		h = sp.inside
	}
	if v == nil {
		v = sp.inside
	}
	if row > r1 {
		set("top", h)
	}
	if row < r2 {
		set("bottom", h)
	}
	if col > c1 {
		set("left", v)
	}
	if col < c2 {
		set("right", v)
	}
	if row == r1 {
		set("top", sp.outline)
	}
	if row == r2 {
		set("bottom", sp.outline)
	}
	if col == c1 {
		set("left", sp.outline)
	}
	if col == c2 {
		set("right", sp.outline)
	}
	return m
}

func sidesKey(m map[string]*borderSide) string {
	if m == nil {
		return ""
	}
	var b strings.Builder
	for _, k := range []string{"left", "right", "top", "bottom", "diagonalUp", "diagonalDown"} {
		if s := m[k]; s != nil {
			fmt.Fprintf(&b, "%s:%d:%s;", k, s.style, s.color)
		}
	}
	return b.String()
}

// merged returns base with the spec applied.
func (b *book) merged(base *excelize.Style, sp *styleSpec, sides map[string]*borderSide, replace bool) *excelize.Style {
	st := &excelize.Style{}
	if base != nil && !replace {
		*st = *base
		if base.Font != nil {
			f := *base.Font
			f.Color = existingColor(f.Color)
			st.Font = &f
		}
		if base.Alignment != nil {
			a := *base.Alignment
			st.Alignment = &a
		}
		st.Fill.Color = nil
		for _, c := range base.Fill.Color {
			st.Fill.Color = append(st.Fill.Color, existingColor(c))
		}
		st.Border = nil
		for _, bd := range base.Border {
			bd.Color = existingColor(bd.Color)
			st.Border = append(st.Border, bd)
		}
	}
	if f := sp.font; f != nil {
		if st.Font == nil {
			st.Font = &excelize.Font{Family: b.defaultFont(), Size: b.defaultFontSize()}
		}
		if st.Font.Family == "" {
			st.Font.Family = b.defaultFont()
		}
		if st.Font.Size == 0 {
			st.Font.Size = b.defaultFontSize()
		}
		if f.name != nil && *f.name != "" {
			st.Font.Family = *f.name
		}
		if f.size != nil {
			st.Font.Size = *f.size
		}
		if f.bold != nil {
			st.Font.Bold = *f.bold
		}
		if f.italic != nil {
			st.Font.Italic = *f.italic
		}
		if f.strike != nil {
			st.Font.Strike = *f.strike
		}
		if f.underline != nil {
			st.Font.Underline = *f.underline
		}
		if f.vertAlign != nil {
			st.Font.VertAlign = *f.vertAlign
		}
		if f.color != nil {
			st.Font.Color = *f.color
			st.Font.ColorTheme = nil
			st.Font.ColorIndexed = 0
			st.Font.ColorTint = 0
		}
	}
	if sp.fill != nil {
		if *sp.fill == "" {
			st.Fill = excelize.Fill{}
		} else {
			st.Fill = excelize.Fill{Type: "pattern", Pattern: 1, Color: []string{*sp.fill}}
		}
	}
	if a := sp.align; a != nil {
		if st.Alignment == nil {
			st.Alignment = &excelize.Alignment{}
		}
		if a.horizontal != nil {
			st.Alignment.Horizontal = *a.horizontal
		}
		if a.vertical != nil {
			st.Alignment.Vertical = *a.vertical
		}
		if a.wrap != nil {
			st.Alignment.WrapText = *a.wrap
		}
		if a.shrink != nil {
			st.Alignment.ShrinkToFit = *a.shrink
		}
		if a.indent != nil {
			st.Alignment.Indent = *a.indent
		}
		if a.rotation != nil {
			st.Alignment.TextRotation = *a.rotation
		}
	}
	if sp.numFmt != nil {
		if *sp.numFmt == "" || strings.EqualFold(*sp.numFmt, "General") {
			st.NumFmt, st.CustomNumFmt = 0, nil
		} else {
			s := *sp.numFmt
			st.NumFmt, st.CustomNumFmt = 0, &s
		}
	}
	if sp.numFmtID != nil {
		st.NumFmt, st.CustomNumFmt = *sp.numFmtID, nil
	}
	if sides != nil {
		cur := map[string]excelize.Border{}
		for _, bd := range st.Border {
			cur[bd.Type] = bd
		}
		for side, s := range sides {
			if s.style == 0 {
				delete(cur, side)
			} else {
				cur[side] = excelize.Border{Type: side, Style: s.style, Color: s.color}
			}
		}
		st.Border = nil
		for _, k := range []string{"left", "right", "top", "bottom", "diagonalUp", "diagonalDown"} {
			if bd, ok := cur[k]; ok {
				st.Border = append(st.Border, bd)
			}
		}
	}
	if sp.protected != nil {
		st.Protection = &excelize.Protection{Locked: *sp.protected}
	}
	return st
}

func (b *book) defaultFont() string {
	if f, err := b.f.GetDefaultFont(); err == nil && f != "" {
		return f
	}
	return "Calibri"
}

func (b *book) defaultFontSize() float64 {
	if st, err := b.f.GetStyle(0); err == nil && st.Font != nil && st.Font.Size > 0 {
		return st.Font.Size
	}
	return 11
}

// parseRange accepts "A1", "A1:C3" (any corner order).
func parseRange(ref string) (c1, r1, c2, r2 int, err error) {
	ref = strings.ToUpper(strings.ReplaceAll(strings.TrimSpace(ref), "$", ""))
	parts := strings.Split(ref, ":")
	if len(parts) > 2 || parts[0] == "" {
		return 0, 0, 0, 0, i18n.Err("xlsx.invalidRange", ref)
	}
	if c1, r1, err = excelize.CellNameToCoordinates(parts[0]); err != nil {
		return 0, 0, 0, 0, i18n.Err("xlsx.invalidRange", ref)
	}
	c2, r2 = c1, r1
	if len(parts) == 2 {
		if c2, r2, err = excelize.CellNameToCoordinates(parts[1]); err != nil {
			return 0, 0, 0, 0, i18n.Err("xlsx.invalidRange", ref)
		}
	}
	if c2 < c1 {
		c1, c2 = c2, c1
	}
	if r2 < r1 {
		r1, r2 = r2, r1
	}
	return c1, r1, c2, r2, nil
}

func cellName(col, row int) string {
	s, _ := excelize.CoordinatesToCellName(col, row)
	return s
}

// applyStyle merges sp into every cell of the range.
func (b *book) applyStyle(sheet, ref string, sp *styleSpec, replace bool) error {
	c1, r1, c2, r2, err := parseRange(ref)
	if err != nil {
		return err
	}
	if b.fresh && sheet == "Sheet1" {
		b.fresh = false
	}
	type key struct {
		old   int
		sides string
	}
	cache := map[key]int{}
	baseCache := map[int]*excelize.Style{}
	for row := r1; row <= r2; row++ {
		runStart, runID := 0, -1
		flush := func(end int) error {
			if runID < 0 {
				return nil
			}
			return b.f.SetCellStyle(sheet, cellName(runStart, row), cellName(end, row), runID)
		}
		for col := c1; col <= c2; col++ {
			old, err := b.f.GetCellStyle(sheet, cellName(col, row))
			if err != nil {
				return err
			}
			sides := sp.sidesFor(col, row, c1, r1, c2, r2)
			k := key{old, sidesKey(sides)}
			id, ok := cache[k]
			if !ok {
				base, ok := baseCache[old]
				if !ok {
					base, _ = b.f.GetStyle(old)
					baseCache[old] = base
				}
				if id, err = b.f.NewStyle(b.merged(base, sp, sides, replace)); err != nil {
					return err
				}
				cache[k] = id
			}
			if id != runID {
				if err := flush(col - 1); err != nil {
					return err
				}
				runStart, runID = col, id
			}
		}
		if err := flush(c2); err != nil {
			return err
		}
	}
	if err := b.syncNeighbors(sheet, sp, c1, r1, c2, r2); err != nil {
		return err
	}
	// Styles cached for date detection may be stale now.
	b.isDate = map[int]bool{}
	return nil
}

// syncNeighbors copies borders set on the edges of the range to the
// touching side of already formatted neighbor cells, so the two cells do
// not disagree about the line between them (as Excel's own UI does).
func (b *book) syncNeighbors(sheet string, sp *styleSpec, c1, r1, c2, r2 int) error {
	type edge struct {
		side, opposite string
		dc, dr         int
	}
	edges := []edge{{"top", "bottom", 0, -1}, {"bottom", "top", 0, 1}, {"left", "right", -1, 0}, {"right", "left", 1, 0}}
	for _, e := range edges {
		var cells [][2]int
		switch e.side {
		case "top":
			for c := c1; c <= c2; c++ {
				cells = append(cells, [2]int{c, r1})
			}
		case "bottom":
			for c := c1; c <= c2; c++ {
				cells = append(cells, [2]int{c, r2})
			}
		case "left":
			for r := r1; r <= r2; r++ {
				cells = append(cells, [2]int{c1, r})
			}
		case "right":
			for r := r1; r <= r2; r++ {
				cells = append(cells, [2]int{c2, r})
			}
		}
		for _, cell := range cells {
			sides := sp.sidesFor(cell[0], cell[1], c1, r1, c2, r2)
			side := sides[e.side]
			nc, nr := cell[0]+e.dc, cell[1]+e.dr
			if side == nil || nc < 1 || nr < 1 {
				continue
			}
			ref := cellName(nc, nr)
			old, err := b.f.GetCellStyle(sheet, ref)
			if err != nil || old == 0 {
				continue
			}
			base, err := b.f.GetStyle(old)
			if err != nil {
				continue
			}
			id, err := b.f.NewStyle(b.merged(base, &styleSpec{}, map[string]*borderSide{e.opposite: side}, false))
			if err != nil {
				return err
			}
			if err := b.f.SetCellStyle(sheet, ref, ref, id); err != nil {
				return err
			}
		}
	}
	return nil
}

// autoFit sets column widths from the displayed text of the used cells.
func (b *book) autoFit(sheet string, c1, c2 int, minW, maxW float64) error {
	rows, err := b.f.GetRows(sheet)
	if err != nil {
		return err
	}
	scale := b.defaultFontSize() / 11
	widths := map[int]float64{}
	for _, row := range rows {
		for i, text := range row {
			col := i + 1
			if col < c1 || col > c2 || text == "" {
				continue
			}
			w := 0
			for _, line := range strings.Split(text, "\n") {
				if lw := runewidth.StringWidth(line); lw > w {
					w = lw
				}
			}
			if fw := float64(w)*1.1*scale + 2; fw > widths[col] {
				widths[col] = fw
			}
		}
	}
	for col := c1; col <= c2; col++ {
		w, ok := widths[col]
		if !ok {
			continue
		}
		w = math.Max(minW, math.Min(maxW, math.Round(w*10)/10))
		name, _ := excelize.ColumnNumberToName(col)
		if err := b.f.SetColWidth(sheet, name, name, w); err != nil {
			return err
		}
	}
	return nil
}

func (n *natives) installXLSXStyle() {
	vm := n.vm
	// xlsxStyle(book, sheet, range, spec, replace)
	n.set("xlsxStyle", func(c goja.FunctionCall) goja.Value {
		b := n.book(c.Argument(0))
		sp, err := n.parseStyle(c.Argument(3))
		if err != nil {
			n.throw(err)
		}
		if err := b.applyStyle(n.str(c, 1), n.str(c, 2), sp, c.Argument(4).ToBoolean()); err != nil {
			n.throw(err)
		}
		return goja.Undefined()
	})
	// xlsxGetStyle(book, sheet, cell) -> the cell's style in the same shape
	// that style() accepts (only non-default values are included).
	n.set("xlsxGetStyle", func(c goja.FunctionCall) goja.Value {
		b := n.book(c.Argument(0))
		id, err := b.f.GetCellStyle(n.str(c, 1), strings.ToUpper(n.str(c, 2)))
		if err != nil {
			n.throw(err)
		}
		st, err := b.f.GetStyle(id)
		if err != nil {
			n.throw(err)
		}
		return n.styleObject(st)
	})
	n.set("xlsxMerge", func(c goja.FunctionCall) goja.Value {
		b := n.book(c.Argument(0))
		c1, r1, c2, r2, err := parseRange(n.str(c, 2))
		if err != nil {
			n.throw(err)
		}
		var e error
		if c.Argument(3).ToBoolean() {
			e = b.f.UnmergeCell(n.str(c, 1), cellName(c1, r1), cellName(c2, r2))
		} else {
			e = b.f.MergeCell(n.str(c, 1), cellName(c1, r1), cellName(c2, r2))
		}
		if e != nil {
			n.throw(e)
		}
		return goja.Undefined()
	})
	// xlsxFreeze(book, sheet, topLeftCell) — "" removes the panes.
	n.set("xlsxFreeze", func(c goja.FunctionCall) goja.Value {
		b := n.book(c.Argument(0))
		sheet, ref := n.str(c, 1), strings.ToUpper(n.str(c, 2))
		if ref == "" || ref == "A1" {
			if err := b.f.SetPanes(sheet, &excelize.Panes{}); err != nil {
				n.throw(err)
			}
			return goja.Undefined()
		}
		col, row, err := excelize.CellNameToCoordinates(ref)
		if err != nil {
			n.throw(i18n.Err("xlsx.invalidRange", ref))
		}
		pane := "bottomRight"
		switch {
		case col == 1:
			pane = "bottomLeft"
		case row == 1:
			pane = "topRight"
		}
		err = b.f.SetPanes(sheet, &excelize.Panes{Freeze: true, XSplit: col - 1, YSplit: row - 1, TopLeftCell: ref, ActivePane: pane,
			Selection: []excelize.Selection{{SQRef: ref, ActiveCell: ref, Pane: pane}}})
		if err != nil {
			n.throw(err)
		}
		return goja.Undefined()
	})
	n.set("xlsxAutoFilter", func(c goja.FunctionCall) goja.Value {
		b := n.book(c.Argument(0))
		c1, r1, c2, r2, err := parseRange(n.str(c, 2))
		if err != nil {
			n.throw(err)
		}
		if err := b.f.AutoFilter(n.str(c, 1), cellName(c1, r1)+":"+cellName(c2, r2), nil); err != nil {
			n.throw(err)
		}
		return goja.Undefined()
	})
	n.set("xlsxSetRowHeight", func(c goja.FunctionCall) goja.Value {
		b := n.book(c.Argument(0))
		if err := b.f.SetRowHeight(n.str(c, 1), int(c.Argument(2).ToInteger()), c.Argument(3).ToFloat()); err != nil {
			n.throw(err)
		}
		return goja.Undefined()
	})
	n.set("xlsxGetColWidth", func(c goja.FunctionCall) goja.Value {
		b := n.book(c.Argument(0))
		w, err := b.f.GetColWidth(n.str(c, 1), strings.ToUpper(n.str(c, 2)))
		if err != nil {
			n.throw(err)
		}
		return vm.ToValue(w)
	})
	// xlsxAutoFit(book, sheet, firstCol, lastCol, min, max)
	n.set("xlsxAutoFit", func(c goja.FunctionCall) goja.Value {
		b := n.book(c.Argument(0))
		c1, c2 := int(c.Argument(2).ToInteger()), int(c.Argument(3).ToInteger())
		if err := b.autoFit(n.str(c, 1), c1, c2, c.Argument(4).ToFloat(), c.Argument(5).ToFloat()); err != nil {
			n.throw(err)
		}
		return goja.Undefined()
	})
	n.set("xlsxDefaultFont", func(c goja.FunctionCall) goja.Value {
		b := n.book(c.Argument(0))
		return n.object(map[string]any{"name": b.defaultFont(), "size": b.defaultFontSize()})
	})
}

// kv builds an object from key/value pairs in order, skipping zero values.
func (n *natives) kv(pairs ...any) *goja.Object {
	o := n.vm.NewObject()
	for i := 0; i+1 < len(pairs); i += 2 {
		switch v := pairs[i+1].(type) {
		case nil:
			continue
		case string:
			if v == "" || v == "#" {
				continue
			}
		case bool:
			if !v {
				continue
			}
		case int:
			if v == 0 {
				continue
			}
		case float64:
			if v == 0 {
				continue
			}
		case *goja.Object:
			if v == nil || len(v.Keys()) == 0 {
				continue
			}
		}
		o.Set(pairs[i].(string), pairs[i+1])
	}
	return o
}

func (n *natives) styleObject(st *excelize.Style) goja.Value {
	var font, border, align *goja.Object
	if f := st.Font; f != nil {
		color := ""
		if c := existingColor(f.Color); c != "" {
			color = "#" + c
		}
		font = n.kv("name", f.Family, "size", f.Size, "bold", f.Bold, "italic", f.Italic,
			"underline", f.Underline, "strike", f.Strike, "color", color, "vertAlign", f.VertAlign)
	}
	if len(st.Border) > 0 {
		var pairs []any
		for _, bd := range st.Border {
			name := "thin"
			if bd.Style >= 0 && bd.Style < len(borderNames) {
				name = borderNames[bd.Style]
			}
			pairs = append(pairs, bd.Type, n.kv("style", name, "color", "#"+existingColor(bd.Color)))
		}
		border = n.kv(pairs...)
	}
	if a := st.Alignment; a != nil {
		align = n.kv("horizontal", a.Horizontal, "vertical", a.Vertical, "wrap", a.WrapText,
			"shrink", a.ShrinkToFit, "indent", a.Indent, "rotation", a.TextRotation)
	}
	fill := ""
	if len(st.Fill.Color) > 0 && st.Fill.Pattern == 1 {
		fill = "#" + existingColor(st.Fill.Color[0])
	}
	var numFmt any
	if st.CustomNumFmt != nil {
		numFmt = *st.CustomNumFmt
	} else if st.NumFmt != 0 {
		numFmt = st.NumFmt
	}
	return n.kv("font", font, "fill", fill, "border", border, "alignment", align, "numberFormat", numFmt)
}
