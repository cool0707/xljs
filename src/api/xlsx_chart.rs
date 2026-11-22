//! Excel's own charts in a workbook: where each chart is on its sheet, its
//! kind, series and axes, with the values of the series read from the
//! cells they refer to (or, when those cannot be read, from the chart's
//! cache). The script turns them into a ChartSpec (ExcelChart in
//! prelude.js).
//!
//! The chart parts are read from the xlsx file itself (kept when the
//! workbook is opened): umya-spreadsheet's model of a chart leaves out
//! kinds it does not know (stock, surface, the newer chartEx kinds) and
//! keeps one chart group of each kind.

use std::collections::HashMap;
use std::io::Cursor;

use serde_json::{Map, Value as J, json};
use umya_spreadsheet::CellRawValue;
use umya_spreadsheet::drawingml::anchor::anchor_cells;
use umya_spreadsheet::drawingml::chart::{AutoColors, DataSource, paint, ref_areas};
use umya_spreadsheet::drawingml::package::Package;

use super::xlsx::{Book, date_format_code, format_code, from_serial, parse_ref, style_is_date};

/// The parts of an xlsx file that describe its drawings (charts, shapes,
/// pictures) and how sheets reach them. Empty when the file has no drawings.
#[derive(Default, Debug, Clone)]
pub struct ChartParts {
    pub(super) files: HashMap<String, Vec<u8>>,
}

impl ChartParts {
    pub fn collect(data: &[u8]) -> ChartParts {
        let mut files = HashMap::new();
        let Ok(mut zr) = zip::ZipArchive::new(Cursor::new(data)) else { return ChartParts { files } };
        // Charts, shapes and pictures all live in drawings.
        if !zr.file_names().any(|n| n.starts_with("xl/drawings/")) {
            return ChartParts { files };
        }
        for i in 0..zr.len() {
            let Ok(f) = zr.by_index(i) else { continue };
            let name = f.name().to_string();
            let keep = name == "xl/workbook.xml"
                || name == "xl/_rels/workbook.xml.rels"
                || name.starts_with("xl/worksheets/_rels/")
                || name.starts_with("xl/drawings/")
                || name.starts_with("xl/charts/")
                || name.starts_with("xl/media/")
                || name.starts_with("xl/theme/");
            if keep && let Some(b) = crate::zippart::read_file(f) {
                files.insert(name, b);
            }
        }
        ChartParts { files }
    }
}

/// The parts as XML (well-formed only), their relationships, the sheets'
/// parts and the theme's colors.
impl Package for ChartParts {
    fn xml(&self, part: &str) -> Option<Node> {
        parse(self.files.get(part)?)
    }
}

pub(super) use crate::xml::{Node, parse};

// ---- values ----

/// A date as the script receives it: { "$date": [y, m (1-12), d, h, mi, s, ms] }.
fn date_json(t: (i32, u32, u32, u32, u32, u32, u32)) -> J {
    json!({ "$date": [t.0, t.1, t.2, t.3, t.4, t.5, t.6] })
}

fn cell_json(b: &Book, c: Option<&umya_spreadsheet::Cell>) -> J {
    let Some(c) = c else { return J::Null };
    match c.raw_value() {
        CellRawValue::Empty => J::Null,
        CellRawValue::Bool(v) => J::Bool(*v),
        CellRawValue::Numeric(f) => {
            if style_is_date(c.style().number_format())
                && let Some(t) = from_serial(*f, b.date1904)
            {
                return date_json(t);
            }
            json!(f)
        }
        CellRawValue::String(s) | CellRawValue::Lazy(s) => {
            if s.is_empty() { J::Null } else { J::String(s.to_string()) }
        }
        other => {
            let s = other.to_string();
            if s.is_empty() { J::Null } else { J::String(s) }
        }
    }
}

/// One area of a reference: (sheet, first column, first row, last column, last row).
type Area = (String, u32, u32, u32, u32);

/// The areas of a series reference ("'売上 2026'!$B$2:$B$13", "(Sheet1!$A$1,Sheet1!$A$3)").
fn areas(f: &str) -> Option<Vec<Area>> {
    let parts = ref_areas(f);
    if parts.is_empty() {
        return None;
    }
    parts
        .iter()
        .map(|p| {
            let (sheet, range) = p.rsplit_once('!')?;
            let sheet = sheet.trim();
            let sheet = match sheet.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
                Some(s) => s.replace("''", "'"),
                None => sheet.to_string(),
            };
            let range = range.replace('$', "");
            let (a, z) = range.split_once(':').unwrap_or((&range, &range));
            let (c1, r1) = parse_ref(a).ok()?;
            let (c2, r2) = parse_ref(z).ok()?;
            Some((sheet, c1.min(c2), r1.min(r2), c1.max(c2), r1.max(r2)))
        })
        .collect()
}

/// The cells of the areas in order: down a column, or along a row. Of an
/// area more than one cell wide and tall (categories with levels), the
/// innermost level: the last column (or row).
fn area_cells(a: &[Area]) -> Vec<(String, u32, u32)> {
    let mut out = Vec::new();
    for (s, c1, r1, c2, r2) in a {
        if c1 == c2 || r2 - r1 >= c2 - c1 {
            for r in *r1..=*r2 {
                out.push((s.clone(), *c2, r));
            }
        } else {
            for c in *c1..=*c2 {
                out.push((s.clone(), c, *r2));
            }
        }
    }
    out
}

/// The data of c:cat, c:val, c:xVal, c:yVal, c:bubbleSize or c:tx:
/// { values, format, ref, header } (header: the text of the cell above a
/// column of values, or left of a row). Text among values (not labels:
/// c:val, c:yVal, c:bubbleSize) is no value, as Excel draws nothing for it.
fn source(b: &Book, n: &Node) -> J {
    let labels = !matches!(n.name.as_str(), "val" | "yVal" | "bubbleSize");
    let Some(src) = DataSource::read(n) else { return J::Null };
    let numeric = src.numeric();
    // The cache (and literals): values by point index.
    let cached: Vec<J> = src
        .points
        .iter()
        .map(|p| {
            let Some(p) = p else { return J::Null };
            match p.text.trim().parse::<f64>() {
                Ok(f) if numeric => {
                    if date_format_code(&p.format)
                        && let Some(t) = from_serial(f, b.date1904)
                    {
                        date_json(t)
                    } else {
                        json!(f)
                    }
                }
                _ => J::String(p.text.clone()),
            }
        })
        .collect();
    let mut out = Map::new();
    let mut format = src.format;
    let mut values = cached;
    if !src.formula.is_empty()
        && let Some(a) = areas(&src.formula)
    {
        let cells = area_cells(&a);
        if !cells.is_empty() && cells.iter().all(|(s, _, _)| b.wb.sheet_by_name(s).is_ok()) {
            values = cells
                .iter()
                .map(|(s, c, r)| {
                    let ws = b.wb.sheet_by_name(s).unwrap();
                    let v = cell_json(b, ws.cell((*c, *r)));
                    if !labels && v.is_string() { J::Null } else { v }
                })
                .collect();
            let (s, c, r) = &cells[0];
            if let Some(cell) = b.wb.sheet_by_name(s).unwrap().cell((*c, *r)) {
                let code = format_code(cell.style().number_format());
                if !code.is_empty() && code != "General" {
                    format = code;
                }
            }
            // The header: the cell before the first one (a column: above it; a row: left of it).
            if let [(s, c1, r1, c2, r2)] = a.as_slice() {
                let ws = b.wb.sheet_by_name(s).unwrap();
                let before = if c1 == c2 || r2 - r1 >= c2 - c1 {
                    if *r1 > 1 { ws.cell((*c2, r1 - 1)) } else { None }
                } else if *c1 > 1 {
                    ws.cell((c1 - 1, *r2))
                } else {
                    None
                };
                if let J::String(h) = cell_json(b, before) {
                    out.insert("header".into(), J::String(h));
                }
            }
        }
        out.insert("ref".into(), J::String(src.formula));
    }
    if format == "General" {
        format.clear();
    }
    out.insert("values".into(), J::Array(values));
    out.insert("format".into(), J::String(format));
    J::Object(out)
}

/// The text of a series name (c:tx) or title (c:tx with c:rich or c:strRef).
fn text_of(b: &Book, tx: &Node) -> Option<String> {
    if tx.find("rich").is_some() {
        return Some(tx.rich_text());
    }
    if let Some(v) = tx.child("v") {
        return Some(v.text.clone());
    }
    let s = source(b, tx);
    let parts: Vec<String> = s["values"]
        .as_array()?
        .iter()
        .filter_map(|v| match v {
            J::String(s) => Some(s.clone()),
            J::Number(n) => Some(n.to_string()),
            _ => None,
        })
        .collect();
    Some(parts.join(" "))
}

// ---- charts ----

const GROUPS: &[&str] = &[
    "barChart", "bar3DChart", "lineChart", "line3DChart", "pieChart", "pie3DChart", "ofPieChart", "doughnutChart",
    "areaChart", "area3DChart", "scatterChart", "bubbleChart", "radarChart", "stockChart", "surfaceChart", "surface3DChart",
];

fn chart_json(b: &Book, root: &Node, theme: &HashMap<String, String>, auto: &AutoColors) -> Map<String, J> {
    let mut out = Map::new();
    let Some(chart) = root.child("chart") else { return out };
    if let Some(t) = chart.child("title") {
        let text = t.child("tx").and_then(|tx| text_of(b, tx));
        out.insert("title".into(), text.map(J::String).unwrap_or(J::Bool(true)));
    }
    out.insert("autoTitleDeleted".into(), J::Bool(chart.val_flag("autoTitleDeleted").unwrap_or(false)));
    out.insert("legend".into(), chart.child("legend").map(|l| J::String(l.val("legendPos").unwrap_or("r").to_string())).unwrap_or(J::Null));
    let Some(plot) = chart.child("plotArea") else { return out };
    out.insert("dataTable".into(), J::Bool(plot.child("dTable").is_some()));
    let mut groups = Vec::new();
    for g in plot.children.iter().filter(|c| GROUPS.contains(&c.name.as_str())) {
        let line = matches!(g.name.as_str(), "lineChart" | "line3DChart" | "scatterChart" | "radarChart" | "stockChart");
        let mut series = Vec::new();
        for s in g.all("ser") {
            let mut o = Map::new();
            o.insert("name".into(), s.child("tx").and_then(|tx| text_of(b, tx)).map(J::String).unwrap_or(J::Null));
            // Lines take their color from the line; markers-only scatter from the marker.
            let marker_none = s.path(&["marker", "symbol"]).and_then(|m| m.attr("val")) == Some("none");
            let (mut color, no_fill) = paint(s.child("spPr"), line, theme);
            if color.is_none() && line {
                color = paint(s.path(&["marker", "spPr"]), false, theme).0;
            }
            // No color of its own: the automatic color of its index (a line
            // without its line still has its markers in that color).
            if color.is_none() && (!no_fill || line) {
                let idx = s.val("idx").and_then(|v| v.parse::<usize>().ok()).unwrap_or(series.len());
                color = auto.nth(idx);
            }
            if let Some(c) = color {
                o.insert("color".into(), J::String(c));
            }
            o.insert("noFill".into(), J::Bool(no_fill));
            o.insert("marker".into(), J::Bool(!marker_none));
            o.insert("smooth".into(), J::Bool(s.val_flag("smooth").unwrap_or(false)));
            o.insert("trendline".into(), J::Bool(s.child("trendline").is_some()));
            o.insert("errBars".into(), J::Bool(s.child("errBars").is_some()));
            // Colors of single points (pies).
            let points: Vec<J> = s
                .all("dPt")
                .filter_map(|p| {
                    let idx = p.val("idx")?.parse::<u32>().ok()?;
                    let (c, _) = paint(p.child("spPr"), false, theme);
                    Some(json!({ "idx": idx, "color": c }))
                })
                .collect();
            if !points.is_empty() {
                o.insert("points".into(), J::Array(points));
            }
            for (k, name) in [("cat", "cat"), ("val", "val"), ("xVal", "xVal"), ("yVal", "yVal"), ("bubbleSize", "bubbleSize")] {
                if let Some(n) = s.child(name) {
                    o.insert(k.into(), source(b, n));
                }
            }
            series.push(J::Object(o));
        }
        let mut o = Map::new();
        o.insert("kind".into(), J::String(g.name.clone()));
        for k in ["barDir", "grouping", "scatterStyle", "holeSize", "radarStyle"] {
            if let Some(v) = g.val(k) {
                o.insert(k.into(), J::String(v.to_string()));
            }
        }
        o.insert("varyColors".into(), J::Bool(g.val_flag("varyColors").unwrap_or(false)));
        let marker = g.val_flag("marker");
        if let Some(m) = marker {
            o.insert("marker".into(), J::Bool(m));
        }
        o.insert("axes".into(), J::Array(g.all("axId").filter_map(|a| a.attr("val")).map(|v| J::String(v.into())).collect()));
        o.insert("series".into(), J::Array(series));
        groups.push(J::Object(o));
    }
    out.insert("groups".into(), J::Array(groups));
    // The automatic colors of points (pies whose slices have no color of their own).
    out.insert("autoColors".into(), J::Array((0..64).filter_map(|i| auto.nth(i)).map(J::String).collect()));
    let mut axes = Vec::new();
    for a in plot.children.iter().filter(|c| matches!(c.name.as_str(), "catAx" | "dateAx" | "valAx" | "serAx")) {
        let mut o = Map::new();
        o.insert("kind".into(), J::String(a.name.clone()));
        o.insert("id".into(), J::String(a.val("axId").unwrap_or("").into()));
        o.insert("pos".into(), J::String(a.val("axPos").unwrap_or("").into()));
        o.insert("deleted".into(), J::Bool(a.val_flag("delete").unwrap_or(false)));
        if let Some(t) = a.child("title") {
            o.insert("title".into(), t.child("tx").and_then(|tx| text_of(b, tx)).map(J::String).unwrap_or(J::Null));
        }
        if let Some(nf) = a.child("numFmt") {
            o.insert("format".into(), J::String(nf.attr("formatCode").unwrap_or("").into()));
            o.insert("sourceLinked".into(), J::Bool(matches!(nf.attr("sourceLinked"), Some("1" | "true"))));
        }
        if let Some(sc) = a.child("scaling") {
            o.insert("reversed".into(), J::Bool(sc.val("orientation") == Some("maxMin")));
            for k in ["min", "max", "logBase"] {
                if let Some(v) = sc.val(k).and_then(|v| v.parse::<f64>().ok()) {
                    o.insert(k.into(), json!(v));
                }
            }
        }
        axes.push(J::Object(o));
    }
    out.insert("axes".into(), J::Array(axes));
    out
}

/// The kind of a chartEx chart (waterfall, funnel, treemap, ...).
fn chart_ex_kind(root: &Node) -> String {
    root.find("series").and_then(|s| s.attr("layoutId")).unwrap_or("chartEx").to_string()
}

/// "E2:L18" for the cells an anchor covers.
fn anchor_range(a: &Node) -> J {
    match anchor_cells(a) {
        Some((f, Some(t))) => J::String(format!("{}:{}", super::xlsx::cell(f.0, f.1), super::xlsx::cell(t.0, t.1))),
        Some((f, None)) => J::String(super::xlsx::cell(f.0, f.1)),
        None => J::Null,
    }
}

/// Calls f with the anchor, the graphic frame, the relationship type and
/// the part of each chart on a sheet, in the order of its drawing.
fn each_chart(parts: &ChartParts, sheet: &str, mut f: impl FnMut(&Node, &Node, &str, &str)) {
    if parts.files.is_empty() {
        return;
    }
    let Some(sheet_part) = parts.sheet_part(sheet) else { return };
    let mut drawings: Vec<String> =
        parts.rels(&sheet_part).into_values().filter(|(t, _)| t.ends_with("/drawing")).map(|(_, p)| p).collect();
    drawings.sort();
    for d in drawings {
        let Some(drawing) = parts.xml(&d) else { continue };
        let rels = parts.rels(&d);
        for anchor in drawing.children.iter().filter(|c| c.name.ends_with("Anchor")) {
            let Some(frame) = anchor.find("graphicFrame") else { continue };
            let Some(data) = frame.find("graphicData") else { continue };
            let Some(id) = data.children.iter().find(|c| c.name == "chart").and_then(|c| c.attr("id")) else { continue };
            let Some((kind, part)) = rels.get(id) else { continue };
            f(anchor, frame, kind, part);
        }
    }
}

/// The charts on a sheet, in the order of its drawing.
pub fn sheet_charts(b: &Book, sheet: &str) -> Vec<J> {
    let mut out = Vec::new();
    let Some(parts) = &b.charts else { return out };
    let theme = parts.theme_colors();
    each_chart(parts, sheet, |anchor, frame, kind, part| {
        let Some(root) = parts.xml(part) else { return };
        let mut o = if kind.ends_with("/chartEx") {
            let mut m = Map::new();
            m.insert("chartEx".into(), J::String(chart_ex_kind(&root)));
            m
        } else {
            chart_json(b, &root, &theme, &AutoColors::of_chart(parts, part, &theme))
        };
        let name = frame.path(&["nvGraphicFramePr", "cNvPr"]).and_then(|c| c.attr("name")).unwrap_or("");
        o.insert("name".into(), J::String(name.to_string()));
        o.insert("range".into(), anchor_range(anchor));
        out.push(J::Object(o));
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a sheet's charts are read as, from a workbook saved without
    /// changes: the same as from the file it was opened from (the charts
    /// were dropped on save before).
    #[test]
    fn charts_survive_a_save() {
        let _g = crate::testutil::lock_lang("ja");
        for (file, sheets) in [
            ("testdata/xlsx/charts.xlsx", &["売上", "散布", "工程"][..]),
            ("testdata/xlsx/print.xlsx", &[][..]),
            ("testdata/book/rules.xlsx", &[][..]),
        ] {
            let data = std::fs::read(crate::testutil::repo_path(file)).unwrap();
            let b = super::super::xlsx::open_bytes(&data, String::new()).unwrap();
            let saved = super::super::xlsx::open_bytes(&b.to_bytes().unwrap(), String::new()).unwrap();
            let names: Vec<String> = if sheets.is_empty() {
                b.wb.sheet_collection().iter().map(|s| s.name().to_string()).collect()
            } else {
                sheets.iter().map(|s| s.to_string()).collect()
            };
            let mut count = 0;
            for sheet in &names {
                let before = sheet_charts(&b, sheet);
                count += before.len();
                assert_eq!(sheet_charts(&saved, sheet), before, "{file} {sheet}");
            }
            assert!(count > 0, "{file}: no charts");
        }
    }
}
