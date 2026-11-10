//! What a workbook holds besides its values, as text for an AI chat (with
//! the VBA code, `xljs vba`): the formulas, the input rules (data
//! validation), the defined names, the conditional formats and the charts
//! of an Office Open XML workbook (.xlsx/.xlsm/.xltx/.xltm/.xlam), and the
//! columns of its tables with many rows (what their values are like, for
//! making input rules).
//!
//! Formulas filled down or across (the same in R1C1 form) are written once
//! with the ranges they cover, and so are rules with the same condition
//! and format over neighbouring ranges (Excel splits them when cells are
//! copied).

mod chart;
mod columns;
mod formula;
mod rules;

use std::collections::{HashMap, HashSet};
use std::io::{Cursor, Read};

use formula::Rect;
use quick_xml::Reader;
use quick_xml::events::Event;
use crate::xml::{self, Node};

use crate::{Error, Result, err, t};

/// What to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Include {
    pub vba: bool,
    pub formulas: bool,
    pub validation: bool,
    pub names: bool,
    pub conditional: bool,
    pub charts: bool,
    pub columns: bool,
}

impl Include {
    /// The names of the parts (the CLI's --include, the form's values).
    pub const KEYS: [&str; 7] = ["vba", "formulas", "validation", "names", "conditional", "charts", "columns"];

    pub const NONE: Include = Include { vba: false, formulas: false, validation: false, names: false, conditional: false, charts: false, columns: false };
    pub const VBA: Include = Include { vba: true, ..Include::NONE };
    pub const ALL: Include = Include { vba: true, formulas: true, validation: true, names: true, conditional: true, charts: true, columns: true };

    /// The parts named (Include::KEYS); an unknown name is an error.
    pub fn parse<S: AsRef<str>>(keys: &[S]) -> Result<Include> {
        let mut i = Include::NONE;
        for k in keys {
            match k.as_ref().trim() {
                "vba" => i.vba = true,
                "formulas" => i.formulas = true,
                "validation" => i.validation = true,
                "names" => i.names = true,
                "conditional" => i.conditional = true,
                "charts" => i.charts = true,
                "columns" => i.columns = true,
                "all" => i = Include::ALL,
                "" => {}
                other => return Err(err!("book.badInclude", other, Include::KEYS.join(", "))),
            }
        }
        Ok(i)
    }

    pub fn keys(&self) -> Vec<&'static str> {
        let on = [self.vba, self.formulas, self.validation, self.names, self.conditional, self.charts, self.columns];
        Include::KEYS.iter().zip(on).filter(|(_, on)| *on).map(|(k, _)| *k).collect()
    }

    /// Whether anything but the VBA code is asked for.
    pub fn sheets(&self) -> bool {
        self.formulas || self.validation || self.names || self.conditional || self.charts || self.columns
    }

    pub fn is_empty(&self) -> bool {
        !self.vba && !self.sheets()
    }
}

/// The formulas of a sheet with the same shape: the ranges they fill and
/// the formula of the top-left cell of the first one.
#[derive(Debug, Clone)]
struct FormulaGroup {
    rects: Vec<Rect>,
    formula: String,
    /// An array formula (its range) or a data table.
    array: bool,
}

/// A rule over ranges (an input rule or a conditional format), with the
/// cell its formulas are relative to.
#[derive(Debug, Clone)]
struct Rule {
    rects: Vec<Rect>,
    node: Node,
}

#[derive(Debug, Default)]
struct Sheet {
    name: String,
    hidden: bool,
    formulas: Vec<FormulaGroup>,
    validations: Vec<Rule>,
    conditional: Vec<Rule>,
    charts: Vec<chart::Chart>,
    // For the columns of the tables (read only when they are asked for):
    /// The cells with a value or a formula, by row and column.
    cells: Vec<columns::Cell>,
    /// The last row read when the sheet has more cells than are read.
    cut: Option<u32>,
    /// The rows frozen at the top.
    frozen: Option<u32>,
    /// The range of the sheet's filter.
    filter: Option<Rect>,
    tables: Vec<columns::Table>,
}

#[derive(Debug)]
struct Name {
    name: String,
    /// The index of the sheet it belongs to (None: the workbook).
    sheet: Option<usize>,
    hidden: bool,
    formula: String,
    comment: String,
}

/// What was read of a workbook.
#[derive(Debug, Default)]
pub struct Workbook {
    sheets: Vec<Sheet>,
    names: Vec<Name>,
    dxfs: Vec<Node>,
    theme: rules::Theme,
    date1904: bool,
    /// The shared strings (read with the cells).
    strings: Vec<String>,
    /// The number format of each cell style: (id, code).
    formats: Vec<(u32, String)>,
}

/// Reads the workbook at path (what inc asks for).
pub fn read_file(path: &str, inc: &Include) -> Result<Workbook> {
    let data = std::fs::read(path).map_err(|e| Error::path("open", path, e))?;
    read(&data, inc)
}

/// Reads a workbook held in memory; its cells only when the columns of
/// its tables are asked for. Binary workbooks (.xls, .xlsb) and encrypted
/// ones cannot be read.
pub fn read(data: &[u8], inc: &Include) -> Result<Workbook> {
    let values = inc.columns;
    if data.starts_with(&[0xD0, 0xCF, 0x11, 0xE0]) {
        return Err(err!("book.oldFormat"));
    }
    let zr = zip::ZipArchive::new(Cursor::new(data)).map_err(|_| err!("book.notWorkbook"))?;
    let mut pkg = Package { names: zr.file_names().map(|n| (n.to_lowercase(), n.to_string())).collect(), zr };
    let Some(wb) = pkg.xml("xl/workbook.xml") else {
        if pkg.open("xl/workbook.bin").is_some() {
            return Err(err!("book.binary"));
        }
        return Err(err!("book.notWorkbook"));
    };
    let rels = pkg.rels("xl/workbook.xml");
    let target = |id: &str| rels.iter().find(|r| r.0 == id).map(|r| r.2.clone());
    let mut out = Workbook {
        date1904: wb.child("workbookPr").is_some_and(|p| p.flag("date1904", false)),
        ..Default::default()
    };
    if let Some(s) = pkg.xml("xl/styles.xml") {
        out.dxfs = s.child("dxfs").map(|d| d.children("dxf").cloned().collect()).unwrap_or_default();
        if values {
            out.formats = number_formats(&s);
        }
    }
    if values {
        let part = rels.iter().find(|r| r.1.ends_with("/sharedStrings")).map(|r| r.2.clone()).unwrap_or_else(|| "xl/sharedStrings.xml".into());
        if let Some(sst) = pkg.xml(&part) {
            out.strings = sst.children("si").map(columns::rich_text).collect();
        }
    }
    let theme_path = rels.iter().find(|r| r.1.ends_with("/theme")).map(|r| r.2.clone()).unwrap_or_else(|| "xl/theme/theme1.xml".into());
    if let Some(th) = pkg.xml(&theme_path) {
        out.theme = rules::theme(&th);
    }
    if let Some(sheets) = wb.child("sheets") {
        for s in sheets.children("sheet") {
            let mut sh = Sheet {
                name: s.attr("name").unwrap_or("").to_string(),
                hidden: matches!(s.attr("state"), Some("hidden" | "veryHidden")),
                ..Default::default()
            };
            if let Some(part) = s.attr("id").and_then(target) {
                if let Some(data) = pkg.open(&part) {
                    read_sheet(&data, &mut sh, values);
                }
                sh.charts = pkg.charts(&part);
                if values {
                    sh.tables = pkg.tables(&part);
                }
            }
            out.sheets.push(sh);
        }
    }
    if let Some(dn) = wb.child("definedNames") {
        for n in dn.children("definedName") {
            out.names.push(Name {
                name: n.attr("name").unwrap_or("").to_string(),
                sheet: n.attr("localSheetId").and_then(|v| v.parse().ok()),
                hidden: n.flag("hidden", false),
                formula: n.all_text(),
                comment: n.attr("comment").unwrap_or("").to_string(),
            });
        }
    }
    Ok(out)
}

/// The parts of the workbook's zip file.
struct Package<'a> {
    zr: zip::ZipArchive<Cursor<&'a [u8]>>,
    /// The names of the parts by their lower case (Excel matches them so).
    names: HashMap<String, String>,
}

impl Package<'_> {
    fn open(&mut self, name: &str) -> Option<Vec<u8>> {
        let real = self.names.get(&name.to_lowercase())?;
        let mut f = self.zr.by_name(real).ok()?;
        let mut b = Vec::new();
        f.read_to_end(&mut b).ok()?;
        Some(b)
    }

    fn xml(&mut self, name: &str) -> Option<Node> {
        self.open(name).and_then(|x| xml::parse_partial(&x))
    }

    /// The relationships of a part (internal ones): (id, type, part).
    fn rels(&mut self, part: &str) -> Vec<(String, String, String)> {
        let (dir, file) = part.rsplit_once('/').unwrap_or(("", part));
        let Some(root) = self.xml(&format!("{dir}/_rels/{file}.rels")) else { return Vec::new() };
        root.children("Relationship")
            .filter(|r| r.attr("TargetMode") != Some("External"))
            .filter_map(|r| {
                let t = r.attr("Target")?;
                let path = match t.strip_prefix('/') {
                    Some(t) => t.to_string(),
                    None => crate::meta::clean_path(&format!("{dir}/{t}")),
                };
                Some((r.attr("Id")?.to_string(), r.attr("Type").unwrap_or("").to_string(), path))
            })
            .collect()
    }

    /// The Excel tables of a worksheet.
    fn tables(&mut self, sheet: &str) -> Vec<columns::Table> {
        let mut out = Vec::new();
        for (_, kind, part) in self.rels(sheet) {
            if kind.ends_with("/table")
                && let Some(t) = self.xml(&part).as_ref().and_then(columns::table)
            {
                out.push(t);
            }
        }
        out
    }

    /// The charts in the drawings of a worksheet or a chart sheet, in their
    /// order.
    fn charts(&mut self, sheet: &str) -> Vec<chart::Chart> {
        let mut out = Vec::new();
        for (_, kind, drawing) in self.rels(sheet) {
            if !kind.ends_with("/drawing") {
                continue;
            }
            let Some(d) = self.xml(&drawing) else { continue };
            let rels = self.rels(&drawing);
            for anchor in d.children.iter().filter(|c| c.name.ends_with("Anchor")) {
                let mut frames = Vec::new();
                anchor.find_all("graphicFrame", &mut frames);
                for frame in frames {
                    let mut refs = Vec::new();
                    frame.find_all("chart", &mut refs);
                    let Some(id) = refs.first().and_then(|c| c.attr("id")) else { continue };
                    let Some((_, kind, part)) = rels.iter().find(|r| r.0 == id) else { continue };
                    let Some(root) = self.xml(part) else { continue };
                    let mut names = Vec::new();
                    frame.find_all("cNvPr", &mut names);
                    let (range, sized) = anchor_range(anchor);
                    out.push(chart::Chart {
                        name: names.first().and_then(|n| n.attr("name")).unwrap_or("").to_string(),
                        range,
                        sized,
                        root,
                        ex: kind.ends_with("/chartEx"),
                    });
                }
            }
        }
        out
    }
}

/// The number format of each cell style (cellXfs): its id and code.
fn number_formats(styles: &Node) -> Vec<(u32, String)> {
    let custom: HashMap<u32, String> = styles
        .child("numFmts")
        .map(|n| n.children("numFmt").filter_map(|f| Some((f.attr("numFmtId")?.parse().ok()?, f.attr("formatCode").unwrap_or("").to_string()))).collect())
        .unwrap_or_default();
    let Some(xfs) = styles.child("cellXfs") else { return Vec::new() };
    xfs.children("xf")
        .map(|x| {
            let id: u32 = x.attr("numFmtId").and_then(|v| v.parse().ok()).unwrap_or(0);
            let code = custom.get(&id).cloned().or_else(|| crate::api::xlsx::builtin_code(id).map(str::to_string)).unwrap_or_else(|| "General".into());
            (id, code)
        })
        .collect()
}

/// The cells a drawing's anchor covers ("L2:S16"), or where it starts
/// ("L2", false) without its end; "" for an absolute anchor.
fn anchor_range(a: &Node) -> (String, bool) {
    match xml::anchor_cells(a) {
        Some(((c1, r1), Some((c2, r2)))) => (Rect { r1, c1, r2, c2 }.display(), true),
        Some(((c, r), None)) => (Rect::cell(c, r).display(), false),
        None => (String::new(), false),
    }
}

/// A cell's formula element as read.
struct CellFormula {
    col: u32,
    row: u32,
    f: Node,
}

/// Reads the formulas and the rules of a worksheet (and its cells with
/// values), streaming through its cells.
fn read_sheet(data: &[u8], sh: &mut Sheet, values: bool) {
    let mut r = Reader::from_reader(data);
    let mut buf = Vec::new();
    let (mut row, mut col) = (0u32, 0u32);
    let mut cells = Vec::new();
    let mut parts = Vec::new();
    loop {
        let ev = r.read_event_into(&mut buf);
        let (e, empty) = match ev {
            Ok(Event::Start(ref e)) => (e.clone().into_owned(), false),
            Ok(Event::Empty(ref e)) => (e.clone().into_owned(), true),
            Ok(Event::Eof) | Err(_) => break,
            _ => {
                buf.clear();
                continue;
            }
        };
        buf.clear();
        match e.local_name().as_ref() {
            b"row" => {
                let n = xml::element(&e);
                row = n.attr("r").and_then(|v| v.parse().ok()).unwrap_or(row + 1);
                col = 0;
            }
            b"c" => {
                let n = xml::element(&e);
                match n.attr("r").and_then(formula::parse_cell) {
                    Some((c, rr)) => (col, row) = (c, rr),
                    None => col += 1,
                }
                if empty {
                    continue;
                }
                let c = xml::subtree(&mut r, n);
                if let Some(f) = c.child("f") {
                    cells.push(CellFormula { col, row, f: f.clone() });
                }
                if values && sh.cut.is_none() {
                    if sh.cells.len() >= columns::MAX_CELLS {
                        sh.cut = Some(row.saturating_sub(1));
                    } else if let Some(v) = columns::cell(&c, col, row) {
                        sh.cells.push(v);
                    }
                }
            }
            b"pane" if sh.frozen.is_none() => {
                let n = xml::element(&e);
                let y = n.attr("ySplit").and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
                if matches!(n.attr("state"), Some("frozen" | "frozenSplit")) && y >= 1.0 {
                    sh.frozen = Some(y as u32);
                }
            }
            b"autoFilter" if sh.filter.is_none() => {
                sh.filter = xml::element(&e).attr("ref").and_then(formula::parse_range);
            }
            b"dataValidations" | b"conditionalFormatting" | b"extLst" if !empty => {
                let n = xml::element(&e);
                parts.push(xml::subtree(&mut r, n));
            }
            _ => {}
        }
    }
    sh.formulas = formula_groups(cells);
    sh.cells.sort_by_key(|c| (c.row, c.col));
    // The rules of Excel 2010 (x14) in extLst are read like the others; a
    // conditional format that only adds to one of those (a data bar's
    // details) names it by id and is left out.
    let mut ids = HashSet::new();
    let mut found = Vec::new();
    for p in &parts {
        if p.name != "extLst" {
            p.find_all("id", &mut found);
        }
    }
    for n in found {
        ids.insert(n.all_text().trim().to_string());
    }
    let sqref = |n: &Node| -> Vec<Rect> {
        let s = n.attr("sqref").map(str::to_string).or_else(|| n.child("sqref").map(|s| s.all_text())).unwrap_or_default();
        formula::parse_sqref(&s)
    };
    let mut cf: Vec<(i64, Rule)> = Vec::new();
    for p in &parts {
        let mut dvs = Vec::new();
        let mut cfs = Vec::new();
        if p.name == "conditionalFormatting" {
            cfs.push(p);
        } else {
            p.find_all("dataValidation", &mut dvs);
            p.find_all("conditionalFormatting", &mut cfs);
        }
        for d in dvs {
            sh.validations.push(Rule { rects: sqref(d), node: d.clone() });
        }
        for c in cfs {
            let rects = sqref(c);
            for rule in c.children("cfRule") {
                if p.name == "extLst" && rule.attr("id").is_some_and(|id| ids.contains(id)) {
                    continue;
                }
                let priority = rule.attr("priority").and_then(|v| v.parse().ok()).unwrap_or(i64::MAX);
                cf.push((priority, Rule { rects: rects.clone(), node: rule.clone() }));
            }
        }
    }
    cf.sort_by_key(|(p, _)| *p);
    sh.conditional = cf.into_iter().map(|(_, r)| r).collect();
}

/// Groups the formulas of a sheet by shape.
fn formula_groups(cells: Vec<CellFormula>) -> Vec<FormulaGroup> {
    struct G {
        first: (u32, u32),
        formula: String,
        cells: Vec<(u32, u32)>,
        array: Option<Rect>,
    }
    let mut groups: Vec<G> = Vec::new();
    let mut by_shape: HashMap<String, usize> = HashMap::new();
    // Shared formulas: the group of each index.
    let mut shared: HashMap<String, usize> = HashMap::new();
    for c in cells {
        let at = (c.col, c.row);
        let text = c.f.all_text();
        match c.f.attr("t").unwrap_or("normal") {
            "array" | "dataTable" => {
                let rect = c.f.attr("ref").and_then(formula::parse_range).unwrap_or(Rect::cell(c.col, c.row));
                let formula = if c.f.attr("t") == Some("dataTable") {
                    // {=TABLE(row input, column input)}
                    let (r1, r2) = (c.f.attr("r1").unwrap_or(""), c.f.attr("r2").unwrap_or(""));
                    if c.f.flag("dt2D", false) {
                        format!("TABLE({r1},{r2})")
                    } else if c.f.flag("dtr", false) {
                        format!("TABLE({r1},)")
                    } else {
                        format!("TABLE(,{r1})")
                    }
                } else {
                    text
                };
                groups.push(G { first: at, formula, cells: Vec::new(), array: Some(rect) });
            }
            t => {
                let si = (t == "shared").then(|| c.f.attr("si").unwrap_or("").to_string());
                let i = if text.is_empty() {
                    // A cell of a shared formula: the shape of its first cell.
                    match si.as_ref().and_then(|si| shared.get(si)) {
                        Some(&i) => i,
                        None => continue,
                    }
                } else {
                    let key = formula::shape(&text, c.col, c.row);
                    let i = *by_shape.entry(key).or_insert_with(|| {
                        groups.push(G { first: at, formula: text.clone(), cells: Vec::new(), array: None });
                        groups.len() - 1
                    });
                    if let Some(si) = si {
                        shared.insert(si, i);
                    }
                    i
                };
                groups[i].cells.push(at);
            }
        }
    }
    let mut out: Vec<FormulaGroup> = groups
        .into_iter()
        .map(|g| match g.array {
            Some(r) => FormulaGroup { rects: vec![r], formula: g.formula, array: true },
            None => {
                let rects = formula::cells_to_rects(g.cells);
                let formula = formula::shift(&g.formula, g.first, (rects[0].c1, rects[0].r1));
                FormulaGroup { rects, formula, array: false }
            }
        })
        .collect();
    out.sort_by_key(|g| g.rects[0]);
    out
}

/// Rules with the same condition and format (the same shapes of their
/// formulas, each relative to its own first cell) as one, over all their
/// ranges; the first of them gives the order. describe(rule, fmt) turns
/// a rule into text.
fn merge_rules(rules: &[Rule], describe: &dyn Fn(&Node, rules::Fmt) -> String) -> Vec<(Vec<Rect>, String)> {
    let mut groups: Vec<(Vec<Rect>, usize)> = Vec::new();
    let mut by_key: HashMap<String, usize> = HashMap::new();
    for (i, r) in rules.iter().enumerate() {
        let Some(first) = r.rects.first() else { continue };
        let base = (first.c1, first.r1);
        let key = describe(&r.node, &|f: &str| formula::shape(f, base.0, base.1));
        match by_key.get(&key) {
            Some(&g) => groups[g].0.extend(r.rects.iter().copied()),
            None => {
                by_key.insert(key, groups.len());
                groups.push((r.rects.clone(), i));
            }
        }
    }
    groups
        .into_iter()
        .map(|(rects, i)| {
            let r = &rules[i];
            let from = (r.rects[0].c1, r.rects[0].r1);
            let rects = formula::merge_rects(rects);
            let to = (rects[0].c1, rects[0].r1);
            let text = describe(&r.node, &|f: &str| code(&format!("={}", formula::display(&formula::shift(f, from, to)))));
            (rects, text)
        })
        .collect()
}

/// Ranges shown in a line; more are counted.
const MAX_RANGES: usize = 12;
/// Lines of formulas or rules per sheet; more are counted.
const MAX_LINES: usize = 2000;

fn ranges(rects: &[Rect]) -> String {
    let mut s: Vec<String> = rects.iter().take(MAX_RANGES).map(Rect::display).collect();
    if rects.len() > MAX_RANGES {
        s.push(t!("book.moreRanges", rects.len() - MAX_RANGES));
    }
    s.join(", ")
}

/// Inline code (with more backticks around text that has some), on one
/// line.
pub(crate) fn code(s: &str) -> String {
    let s = s.replace("\r\n", " ").replace(['\r', '\n'], " ");
    let (mut best, mut n) = (0, 0);
    for ch in s.chars() {
        n = if ch == '`' { n + 1 } else { 0 };
        best = best.max(n);
    }
    let fence = "`".repeat(best + 1);
    if best > 0 { format!("{fence} {s} {fence}") } else { format!("{fence}{s}{fence}") }
}

fn sheet_heading(s: &Sheet) -> String {
    if s.hidden { format!("## {}（{}）\n\n", s.name, t!("book.hidden")) } else { format!("## {}\n\n", s.name) }
}

/// Lists lines under a heading per sheet; None when no sheet has any.
fn per_sheet(wb: &Workbook, lines: &dyn Fn(&Sheet) -> Vec<String>) -> Option<String> {
    let mut b = String::new();
    for s in &wb.sheets {
        let mut l = lines(s);
        if l.is_empty() {
            continue;
        }
        b.push_str(&sheet_heading(s));
        if l.len() > MAX_LINES {
            let more = l.len() - MAX_LINES;
            l.truncate(MAX_LINES);
            l.push(t!("book.moreLines", more));
        }
        for x in l {
            b.push_str(&format!("- {x}\n"));
        }
        b.push('\n');
    }
    (!b.is_empty()).then_some(b)
}

/// The parts of the workbook named file asked for by inc (all but the VBA
/// code) as Markdown: a top-level heading per part, as vba::text writes
/// the code.
pub fn text(file: &str, wb: &Workbook, inc: &Include) -> String {
    let mut b = String::new();
    let mut part = |title: String, intro: Option<String>, body: Option<String>, none: String| {
        b.push_str(&format!("# {title}\n\n"));
        match body {
            Some(body) => {
                if let Some(i) = intro {
                    b.push_str(&format!("{i}\n\n"));
                }
                b.push_str(&body);
            }
            None => b.push_str(&format!("{none}\n\n")),
        }
    };
    if inc.formulas {
        let body = per_sheet(wb, &|s| {
            s.formulas
                .iter()
                .map(|g| {
                    let f = code(&format!("={}", formula::display(&g.formula)));
                    if g.array { format!("{}: {f}{}", ranges(&g.rects), t!("book.array")) } else { format!("{}: {f}", ranges(&g.rects)) }
                })
                .collect()
        });
        part(t!("book.formulasTitle", file), Some(t!("book.formulasIntro")), body, t!("book.formulasNone"));
    }
    if inc.validation {
        let body = per_sheet(wb, &|s| {
            merge_rules(&s.validations, &|n, fmt| rules::validation(n, fmt, wb.date1904))
                .into_iter()
                .map(|(r, d)| format!("{}: {d}", ranges(&r)))
                .collect()
        });
        part(t!("book.validationTitle", file), Some(t!("book.rulesIntro")), body, t!("book.validationNone"));
    }
    if inc.names {
        let mut body = String::new();
        for n in &wb.names {
            // The range of an AutoFilter.
            if n.name.eq_ignore_ascii_case("_xlnm._FilterDatabase") {
                continue;
            }
            let name = match n.name.strip_prefix("_xlnm.") {
                Some("Print_Area") => t!("book.printArea"),
                Some("Print_Titles") => t!("book.printTitles"),
                Some(other) => other.to_string(),
                None => n.name.clone(),
            };
            let mut notes = Vec::new();
            if let Some(s) = n.sheet {
                notes.push(t!("book.nameSheet", wb.sheets.get(s).map(|s| s.name.as_str()).unwrap_or("?")));
            }
            if n.hidden {
                notes.push(t!("book.hidden"));
            }
            let notes = if notes.is_empty() { String::new() } else { format!("（{}）", notes.join(&t!("vba.separator"))) };
            let comment = if n.comment.is_empty() { String::new() } else { format!(" — {}", n.comment.replace(['\r', '\n'], " ")) };
            body.push_str(&format!("- {name}{notes}: {}{comment}\n", code(&format!("={}", formula::display(&n.formula)))));
        }
        let body = (!body.is_empty()).then(|| body + "\n");
        part(t!("book.namesTitle", file), None, body, t!("book.namesNone"));
    }
    if inc.conditional {
        let body = per_sheet(wb, &|s| {
            merge_rules(&s.conditional, &|n, fmt| rules::conditional(n, fmt, &wb.dxfs, &wb.theme))
                .into_iter()
                .map(|(r, d)| format!("{}: {d}", ranges(&r)))
                .collect()
        });
        part(t!("book.conditionalTitle", file), Some(format!("{}{}", t!("book.rulesIntro"), t!("book.conditionalOrder"))), body, t!("book.conditionalNone"));
    }
    if inc.charts {
        let mut body = String::new();
        for s in wb.sheets.iter().filter(|s| !s.charts.is_empty()) {
            body.push_str(&sheet_heading(s));
            for (i, c) in s.charts.iter().enumerate() {
                let name = if c.name.is_empty() { t!("book.chart.unnamed", i + 1) } else { c.name.clone() };
                let at = match (c.range.as_str(), c.sized) {
                    ("", _) => String::new(),
                    (r, true) => format!("（{r}）"),
                    (r, false) => format!("（{}）", t!("book.chart.from", r)),
                };
                body.push_str(&format!("### {name}{at}\n\n"));
                for l in chart::describe(c) {
                    body.push_str(&format!("- {l}\n"));
                }
                body.push('\n');
            }
        }
        let body = (!body.is_empty()).then_some(body);
        part(t!("book.chartsTitle", file), None, body, t!("book.chartsNone"));
    }
    if inc.columns {
        let mut body = String::new();
        for s in &wb.sheets {
            if let Some(t) = columns::sheet_text(wb, s) {
                body.push_str(&sheet_heading(s));
                body.push_str(&t);
            }
        }
        let body = (!body.is_empty()).then_some(body);
        part(t!("book.columnsTitle", file), Some(t!("book.columnsIntro", columns::MIN_ROWS)), body, t!("book.columnsNone", columns::MIN_ROWS));
    }
    b
}

/// The text of the parts of a workbook that could not be read: the
/// headings with the error.
pub fn error_text(file: &str, inc: &Include, e: &Error) -> String {
    let msg = crate::i18n::error(e);
    let mut b = String::new();
    for (on, key) in [
        (inc.formulas, "book.formulasTitle"),
        (inc.validation, "book.validationTitle"),
        (inc.names, "book.namesTitle"),
        (inc.conditional, "book.conditionalTitle"),
        (inc.charts, "book.chartsTitle"),
        (inc.columns, "book.columnsTitle"),
    ] {
        if on {
            b.push_str(&format!("# {}\n\n{msg}\n\n", t!(key, file)));
        }
    }
    b
}

#[cfg(test)]
mod tests;
