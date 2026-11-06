//! The look of tables (ListObjects) on paper: the table style's fills, fonts
//! and borders laid over the cells (book.toPdf / toHtml). Custom styles come
//! from styles.xml (tableStyles + dxfs), the built-in ones (TableStyleLight1 …
//! TableStyleDark11) from an approximation of Excel's presets by family.

use std::collections::HashMap;

use umya_spreadsheet::Color;
use umya_spreadsheet::Worksheet;

use super::xlsx_chart::{Node, parse};

/// A border side of a differential format: style and color (None = automatic).
#[derive(Debug, Clone, PartialEq)]
pub struct Side {
    pub style: String,
    pub color: Option<Color>,
}

/// The parts of a differential format (dxf) that show on paper.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Dxf {
    pub fill: Option<Color>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub color: Option<Color>,
    /// left, right, top, bottom, vertical (inside), horizontal (inside).
    pub sides: [Option<Side>; 6],
}

/// A rectangle of cells (1-based, inclusive): c1, r1, c2, r2.
type Area = (u32, u32, u32, u32);

/// A table style: its elements (type, band size, format).
pub type StyleDef = Vec<(String, u32, Dxf)>;

/// The custom table styles of a workbook by name.
pub fn custom_styles(styles_xml: &[u8]) -> HashMap<String, StyleDef> {
    let mut out = HashMap::new();
    let Some(root) = parse(styles_xml) else {
        return out;
    };
    let dxfs: Vec<Dxf> = root.child("dxfs").map(|d| d.all("dxf").map(dxf).collect()).unwrap_or_default();
    let Some(ts) = root.child("tableStyles") else {
        return out;
    };
    for s in ts.all("tableStyle") {
        let Some(name) = s.attr("name") else { continue };
        let els = s
            .all("tableStyleElement")
            .filter_map(|e| {
                let d = dxfs.get(e.attr("dxfId")?.parse::<usize>().ok()?)?.clone();
                let size = e.attr("size").and_then(|v| v.parse().ok()).unwrap_or(1);
                Some((e.attr("type")?.to_string(), size, d))
            })
            .collect();
        out.insert(name.to_string(), els);
    }
    out
}

fn color(n: &Node) -> Option<Color> {
    let mut c = Color::default();
    if n.attr("auto").is_some_and(|v| v == "1" || v == "true") {
        return None;
    }
    if let Some(t) = n.attr("theme").and_then(|v| v.parse().ok()) {
        c.set_theme_index(t);
    } else if let Some(rgb) = n.attr("rgb") {
        let rgb = if rgb.len() == 6 { format!("FF{rgb}") } else { rgb.to_string() };
        if rgb.len() != 8 || !rgb.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        c.set_argb_str(rgb);
    } else {
        let i = n.attr("indexed").and_then(|v| v.parse().ok())?;
        if i == 64 {
            return None; // the system foreground
        }
        c.set_indexed(i);
    }
    if let Some(t) = n.attr("tint").and_then(|v| v.parse().ok()) {
        c.set_tint(t);
    }
    Some(c)
}

fn dxf(n: &Node) -> Dxf {
    let mut d = Dxf::default();
    let on = |e: Option<&Node>| e.map(|e| !matches!(e.attr("val"), Some("0" | "false")));
    if let Some(f) = n.child("font") {
        d.bold = on(f.child("b"));
        d.italic = on(f.child("i"));
        d.color = f.child("color").and_then(color);
    }
    if let Some(p) = n.child("fill").and_then(|f| f.child("patternFill")) {
        let t = p.attr("patternType").unwrap_or("solid");
        if t != "none" {
            // A dxf's solid fill is its background color.
            d.fill = p.child("bgColor").and_then(color).or_else(|| p.child("fgColor").and_then(color));
        }
    }
    if let Some(b) = n.child("border") {
        for (i, name) in ["left", "right", "top", "bottom", "vertical", "horizontal"].into_iter().enumerate() {
            if let Some(s) = b.child(name)
                && let Some(st) = s.attr("style")
            {
                d.sides[i] = Some(Side { style: st.to_string(), color: s.child("color").and_then(color) });
            }
        }
    }
    d
}

// ---- built-in styles ----

fn theme(i: u32, tint: f64) -> Option<Color> {
    let mut c = Color::default();
    c.set_theme_index(i);
    if tint != 0.0 {
        c.set_tint(tint);
    }
    Some(c)
}

fn side(style: &str, c: Option<Color>) -> Option<Side> {
    Some(Side { style: style.into(), color: c })
}

fn el(t: &str, d: Dxf) -> (String, u32, Dxf) {
    (t.to_string(), 1, d)
}

/// Excel's built-in table styles, approximated by family: Light 1-7, 8-14,
/// 15-21, Medium 1-7, 8-14, 15-21, 22-28, Dark 1-7, 8-11. Within a family
/// the first uses the text color and the next six the accents 1-6.
pub fn builtin(name: &str) -> Option<StyleDef> {
    let (family, n) = if let Some(v) = name.strip_prefix("TableStyleLight") {
        ("L", v.parse::<u32>().ok()?)
    } else if let Some(v) = name.strip_prefix("TableStyleMedium") {
        ("M", v.parse::<u32>().ok()?)
    } else {
        ("D", name.strip_prefix("TableStyleDark")?.parse::<u32>().ok()?)
    };
    let i = (n - 1) % 7; // 0 = text color, 1-6 = accents
    let group = (n - 1) / 7;
    // The base color, its light and lighter tints.
    let base = if i == 0 { 1 } else { 3 + i };
    let c = |tint: f64| {
        if i == 0 && tint > 0.0 { theme(0, -(1.0 - tint) * 0.5) } else { theme(base, tint) }
    };
    let white = || theme(0, 0.0);
    let bold = |d: Dxf| Dxf { bold: Some(true), ..d };
    let header_solid = || Dxf { fill: c(0.0), color: white(), bold: Some(true), ..Default::default() };
    let total_double = || Dxf { bold: Some(true), sides: [None, None, side("double", c(0.0)), None, None, None], ..Default::default() };
    let stripe = |tint: f64| Dxf { fill: c(tint), ..Default::default() };
    let mut out: StyleDef = Vec::new();
    match (family, group) {
        ("L", 0) => {
            out.push(el(
                "wholeTable",
                Dxf {
                    color: if i == 0 { theme(1, 0.0) } else { theme(base, -0.25) },
                    sides: [None, None, side("thin", c(0.0)), side("thin", c(0.0)), None, None],
                    ..Default::default()
                },
            ));
            out.push(el("firstRowStripe", stripe(0.8)));
            out.push(el("firstColumnStripe", stripe(0.8)));
            out.push(el("firstColumn", bold(Dxf::default())));
            out.push(el("lastColumn", bold(Dxf::default())));
            out.push(el("headerRow", Dxf { bold: Some(true), sides: [None, None, None, side("thin", c(0.0)), None, None], ..Default::default() }));
            out.push(el("totalRow", total_double()));
        }
        ("L", 1) => {
            out.push(el("wholeTable", Dxf { sides: [side("thin", c(0.0)), side("thin", c(0.0)), side("thin", c(0.0)), side("thin", c(0.0)), None, None], ..Default::default() }));
            out.push(el("firstRowStripe", Dxf { sides: [None, None, side("thin", c(0.0)), side("thin", c(0.0)), None, None], ..Default::default() }));
            out.push(el("firstColumnStripe", Dxf { sides: [side("thin", c(0.0)), side("thin", c(0.0)), None, None, None, None], ..Default::default() }));
            out.push(el("firstColumn", bold(Dxf::default())));
            out.push(el("lastColumn", bold(Dxf::default())));
            out.push(el("headerRow", header_solid()));
            out.push(el("totalRow", total_double()));
        }
        ("L", _) => {
            let t = || side("thin", c(0.0));
            out.push(el("wholeTable", Dxf { sides: [t(), t(), t(), t(), t(), t()], ..Default::default() }));
            out.push(el("firstRowStripe", stripe(0.8)));
            out.push(el("firstColumnStripe", stripe(0.8)));
            out.push(el("firstColumn", bold(Dxf::default())));
            out.push(el("lastColumn", bold(Dxf::default())));
            out.push(el("headerRow", Dxf { bold: Some(true), sides: [None, None, None, side("medium", c(0.0)), None, None], ..Default::default() }));
            out.push(el("totalRow", total_double()));
        }
        ("M", 0) => {
            let t = || side("thin", c(0.4));
            out.push(el("wholeTable", Dxf { sides: [t(), t(), t(), t(), None, t()], ..Default::default() }));
            out.push(el("firstRowStripe", stripe(0.8)));
            out.push(el("firstColumnStripe", stripe(0.8)));
            out.push(el("firstColumn", bold(Dxf::default())));
            out.push(el("lastColumn", bold(Dxf::default())));
            out.push(el("headerRow", Dxf { sides: [side("thin", c(0.0)), side("thin", c(0.0)), side("thin", c(0.0)), side("thin", c(0.0)), None, None], ..header_solid() }));
            out.push(el("totalRow", total_double()));
        }
        ("M", 1) => {
            let w = || side("thin", white());
            out.push(el("wholeTable", Dxf { fill: c(0.8), sides: [None, None, None, None, w(), w()], ..Default::default() }));
            out.push(el("firstRowStripe", stripe(0.6)));
            out.push(el("firstColumnStripe", stripe(0.6)));
            out.push(el("firstColumn", header_solid()));
            out.push(el("lastColumn", header_solid()));
            out.push(el("headerRow", Dxf { sides: [None, None, None, side("thick", white()), None, None], ..header_solid() }));
            out.push(el("totalRow", Dxf { sides: [None, None, side("thick", white()), None, None, None], ..header_solid() }));
        }
        ("M", 2) => {
            let t = || side("thin", theme(1, 0.0));
            out.push(el("wholeTable", Dxf { sides: [t(), t(), t(), t(), None, t()], ..Default::default() }));
            out.push(el("firstRowStripe", Dxf { fill: theme(0, -0.15), ..Default::default() }));
            out.push(el("firstColumnStripe", Dxf { fill: theme(0, -0.15), ..Default::default() }));
            out.push(el("firstColumn", header_solid()));
            out.push(el("lastColumn", header_solid()));
            out.push(el("headerRow", Dxf { sides: [None, None, None, side("thick", theme(1, 0.0)), None, None], ..header_solid() }));
            out.push(el("totalRow", Dxf { bold: Some(true), sides: [None, None, side("double", theme(1, 0.0)), None, None, None], ..Default::default() }));
        }
        ("M", _) => {
            let t = || side("thin", c(0.4));
            out.push(el("wholeTable", Dxf { fill: c(0.8), sides: [t(), t(), t(), t(), t(), t()], ..Default::default() }));
            out.push(el("firstRowStripe", stripe(0.6)));
            out.push(el("firstColumnStripe", stripe(0.6)));
            out.push(el("firstColumn", bold(Dxf::default())));
            out.push(el("lastColumn", bold(Dxf::default())));
            out.push(el("headerRow", bold(Dxf::default())));
            out.push(el("totalRow", total_double()));
        }
        ("D", 0) => {
            let dark = |t: f64| {
                if i == 0 { theme(1, if t < -0.3 { 0.15 } else { 0.35 }) } else { theme(base, t) }
            };
            out.push(el("wholeTable", Dxf { fill: dark(-0.25), color: white(), ..Default::default() }));
            out.push(el("firstRowStripe", Dxf { fill: dark(-0.5), ..Default::default() }));
            out.push(el("firstColumnStripe", Dxf { fill: dark(-0.5), ..Default::default() }));
            out.push(el("firstColumn", Dxf { bold: Some(true), fill: dark(-0.5), sides: [None, side("medium", white()), None, None, None, None], ..Default::default() }));
            out.push(el("lastColumn", Dxf { bold: Some(true), fill: dark(-0.5), sides: [side("medium", white()), None, None, None, None, None], ..Default::default() }));
            out.push(el("headerRow", Dxf { bold: Some(true), fill: theme(1, 0.0), sides: [None, None, None, side("medium", white()), None, None], ..Default::default() }));
            out.push(el("totalRow", Dxf { bold: Some(true), fill: dark(-0.5), sides: [None, None, side("double", white()), None, None, None], ..Default::default() }));
        }
        ("D", _) => {
            let accent = [1, 4, 6, 8][(i as usize).min(3)];
            out.push(el("wholeTable", Dxf { fill: theme(0, -0.15), ..Default::default() }));
            out.push(el("firstRowStripe", Dxf { fill: theme(accent, 0.4), ..Default::default() }));
            out.push(el("firstColumnStripe", Dxf { fill: theme(accent, 0.4), ..Default::default() }));
            out.push(el("firstColumn", bold(Dxf::default())));
            out.push(el("lastColumn", bold(Dxf::default())));
            out.push(el("headerRow", Dxf { bold: Some(true), color: white(), fill: theme(1, 0.0), ..Default::default() }));
            out.push(el("totalRow", Dxf { bold: Some(true), sides: [None, None, side("double", theme(1, 0.0)), None, None, None], ..Default::default() }));
        }
        _ => return None,
    }
    Some(out)
}

// ---- laying a style over the cells ----

/// What a table style gives one cell. Sides: top, right, bottom, left.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Overlay {
    pub fill: Option<Color>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub color: Option<Color>,
    pub sides: [Option<Side>; 4],
}

impl Overlay {
    /// Lays an element's format over the cell (c, r) of the region (c1, r1, c2, r2).
    fn apply(&mut self, d: &Dxf, c: u32, r: u32, reg: (u32, u32, u32, u32)) {
        if d.fill.is_some() {
            self.fill = d.fill.clone();
        }
        if d.bold.is_some() {
            self.bold = d.bold;
        }
        if d.italic.is_some() {
            self.italic = d.italic;
        }
        if d.color.is_some() {
            self.color = d.color.clone();
        }
        let [l, rt, t, b, v, h] = &d.sides;
        let pick = |edge: bool, outer: &Option<Side>, inner: &Option<Side>| {
            if edge { outer.clone() } else { inner.clone() }
        };
        let sides = [pick(r == reg.1, t, h), pick(c == reg.2, rt, v), pick(r == reg.3, b, h), pick(c == reg.0, l, v)];
        for (i, s) in sides.into_iter().enumerate() {
            if s.is_some() {
                self.sides[i] = s;
            }
        }
    }
}

/// The table areas of a sheet (1-based c1, r1, c2, r2).
pub fn areas(ws: &Worksheet) -> Vec<(u32, u32, u32, u32)> {
    ws.tables()
        .iter()
        .filter_map(|t| {
            let (a, b) = t.area();
            let (c1, r1, c2, r2) = (a.col_num(), a.row_num(), b.col_num(), b.row_num());
            (c1 >= 1 && r1 >= 1).then(|| (c1.min(c2), r1.min(r2), c1.max(c2), r1.max(r2)))
        })
        .collect()
}

/// What the tables' styles give the cells of a sheet. The styles are looked
/// up in custom (the workbook's own) and then among the built-in ones; a
/// table without tableStyleInfo has no style.
pub fn overlays(ws: &Worksheet, custom: &HashMap<String, StyleDef>) -> HashMap<(u32, u32), Overlay> {
    let mut out: HashMap<(u32, u32), Overlay> = HashMap::new();
    for t in ws.tables() {
        let (a, b) = t.area();
        let (c1, r1, c2, r2) = (a.col_num(), a.row_num(), b.col_num(), b.row_num());
        if c1 == 0 || r1 == 0 || c2 < c1 || r2 < r1 {
            continue;
        }
        let info = t.style_info();
        let Some(name) = info.map(|i| i.name().to_string()) else {
            continue;
        };
        let Some(def) = custom.get(&name).cloned().or_else(|| builtin(&name)) else {
            continue;
        };
        let (first_col, last_col, row_stripes, col_stripes) = match info {
            Some(i) => (i.is_show_first_col(), i.is_show_last_col(), i.is_show_row_stripes(), i.is_show_col_stripes()),
            None => (false, false, true, false),
        };
        let header = 1u32; // headerRowCount is not kept; tables nearly always have one
        let totals = t.totals_row_count();
        let body_r1 = r1 + header;
        let body_r2 = r2.saturating_sub(totals);
        let get = |ty: &str| def.iter().find(|e| e.0 == ty);
        // Elements from the lowest priority to the highest.
        let mut regions: Vec<(&Dxf, Area)> = Vec::new();
        if let Some(e) = get("wholeTable") {
            regions.push((&e.2, (c1, r1, c2, r2)));
        }
        if col_stripes {
            let s1 = get("firstColumnStripe").map(|e| e.1.max(1)).unwrap_or(1);
            let s2 = get("secondColumnStripe").map(|e| e.1.max(1)).unwrap_or(1);
            let mut c = c1;
            let mut first = true;
            while c <= c2 {
                let size = if first { s1 } else { s2 };
                let end = (c + size - 1).min(c2);
                if let Some(e) = get(if first { "firstColumnStripe" } else { "secondColumnStripe" }) {
                    regions.push((&e.2, (c, body_r1, end, body_r2)));
                }
                c = end + 1;
                first = !first;
            }
        }
        if row_stripes {
            let s1 = get("firstRowStripe").map(|e| e.1.max(1)).unwrap_or(1);
            let s2 = get("secondRowStripe").map(|e| e.1.max(1)).unwrap_or(1);
            let mut r = body_r1;
            let mut first = true;
            while r <= body_r2 {
                let size = if first { s1 } else { s2 };
                let end = (r + size - 1).min(body_r2);
                if let Some(e) = get(if first { "firstRowStripe" } else { "secondRowStripe" }) {
                    regions.push((&e.2, (c1, r, c2, end)));
                }
                r = end + 1;
                first = !first;
            }
        }
        if last_col && let Some(e) = get("lastColumn") {
            regions.push((&e.2, (c2, r1, c2, r2)));
        }
        if first_col && let Some(e) = get("firstColumn") {
            regions.push((&e.2, (c1, r1, c1, r2)));
        }
        if let Some(e) = get("headerRow") {
            regions.push((&e.2, (c1, r1, c2, r1)));
        }
        if totals > 0
            && let Some(e) = get("totalRow")
        {
            regions.push((&e.2, (c1, r2, c2, r2)));
        }
        for (ty, cell) in [("firstHeaderCell", (c1, r1)), ("lastHeaderCell", (c2, r1))] {
            if let Some(e) = get(ty) {
                regions.push((&e.2, (cell.0, cell.1, cell.0, cell.1)));
            }
        }
        if totals > 0 {
            for (ty, cell) in [("firstTotalCell", (c1, r2)), ("lastTotalCell", (c2, r2))] {
                if let Some(e) = get(ty) {
                    regions.push((&e.2, (cell.0, cell.1, cell.0, cell.1)));
                }
            }
        }
        for (d, reg) in regions {
            if reg.0 > reg.2 || reg.1 > reg.3 {
                continue;
            }
            for r in reg.1..=reg.3 {
                for c in reg.0..=reg.2 {
                    out.entry((c, r)).or_default().apply(d, c, r, reg);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_style() {
        let xml = br#"<styleSheet><dxfs count="2"><dxf><font><b val="0"/><color theme="1"/></font><fill><patternFill><bgColor theme="7" tint="0.8"/></patternFill></fill></dxf><dxf><border><left style="thin"><color theme="0" tint="-0.5"/></left><right style="thin"/><top style="thin"/><bottom style="thin"/><vertical style="hair"/><horizontal style="thin"/></border></dxf></dxfs><tableStyles count="1"><tableStyle name="mine" count="2"><tableStyleElement type="wholeTable" dxfId="1"/><tableStyleElement type="headerRow" dxfId="0"/></tableStyle></tableStyles></styleSheet>"#;
        let s = custom_styles(xml);
        let def = &s["mine"];
        assert_eq!(def.len(), 2);
        assert_eq!(def[0].0, "wholeTable");
        assert_eq!(def[0].2.sides[4].as_ref().unwrap().style, "hair");
        assert!(def[1].2.fill.is_some());
        assert_eq!(def[1].2.bold, Some(false));
    }

    #[test]
    fn regions() {
        let d = Dxf { sides: [side("thin", None), side("thin", None), side("thin", None), side("thin", None), side("hair", None), side("dotted", None)], ..Default::default() };
        let mut o = Overlay::default();
        o.apply(&d, 1, 1, (1, 1, 3, 3)); // top left corner
        assert_eq!(o.sides.iter().map(|s| s.as_ref().unwrap().style.as_str()).collect::<Vec<_>>(), ["thin", "hair", "dotted", "thin"]);
        assert!(builtin("TableStyleMedium2").is_some() && builtin("TableStyleLight21").is_some() && builtin("TableStyleDark11").is_some());
        assert!(builtin("PivotStyleLight16").is_none());
    }
}
