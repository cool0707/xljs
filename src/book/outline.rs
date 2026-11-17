//! An outline of a workbook for an AI chat (the development mode's new
//! script wizard): its sheets, their size, where their tables are (the
//! rows of the header and of the data, the names of the columns and
//! their groups: layout.rs), the rows outside the tables (a title,
//! settings, totals), the first rows of the data, and the formulas (one
//! line for a formula filled down or across).

use super::columns::{self, Cell, Ctx, Grid, Val};
use super::formula::{self, Rect};
use super::layout::{self, Found};
use super::{Include, Sheet, Workbook, code, ranges};
use crate::{Error, Result, t};

/// Formulas listed per sheet (the ones of the same shape count as one).
const MAX_FORMULAS: usize = 40;
/// Columns shown of the rows (outside the tables, the ones with cells).
const MAX_COLUMNS: usize = 15;
/// Characters of a cell shown.
const MAX_CELL: usize = 40;
/// Rows shown outside the tables (above each, under the last), and of a
/// sheet without tables that has no more rows than this.
const MAX_OUTSIDE: usize = 20;
/// Columns of a table listed by name.
const MAX_NAMES: usize = 60;

fn cut(s: &str) -> String {
    let s = s.replace(['\r', '\n'], " ").replace('|', "\\|");
    let c: Vec<char> = s.chars().collect();
    if c.len() > MAX_CELL { format!("{}…", c[..MAX_CELL].iter().collect::<String>()) } else { s }
}

/// What the cell shows (a number in its format).
fn shown(cx: &Ctx, c: &Cell) -> String {
    match &c.val {
        Val::Blank => String::new(),
        Val::Num(f) => crate::api::numfmt::format_serial(*f, cx.format(c.style).1, cx.is_date(c.style), cx.wb.date1904),
        Val::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_string(),
        Val::Err(e) => e.to_string(),
        v => cx.text(v).unwrap_or("").to_string(),
    }
}

/// The outline of the workbook at path (Markdown). sample_rows is the
/// number of rows of data shown of each table; with 0 no cells are shown
/// but the headers of the tables.
pub fn outline(path: &str, sample_rows: usize) -> Result<String> {
    let data = std::fs::read(path).map_err(|e| Error::path("open", path, e))?;
    let name = std::path::Path::new(path).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    if data.starts_with(&[0xD0, 0xCF, 0x11, 0xE0]) {
        // .xls (or an encrypted workbook): only its macros can be read.
        return Ok(format!("### {name}\n\n{}\n", t!("wizard.oldFormat")));
    }
    let wb = super::read(&data, &Include { formulas: true, columns: true, ..Include::NONE })?;
    let mut out = format!("### {name}\n");
    for sh in &wb.sheets {
        out.push('\n');
        out.push_str(&sheet(&wb, sh, sample_rows as u32));
    }
    Ok(out)
}

/// The rows (None: rows left out) as a Markdown table of the columns
/// cols (the first MAX_COLUMNS of them), without the cells in skip.
fn grid(cx: &Ctx, g: &Grid, rows: &[Option<u32>], cols: &[u32], skip: &[Rect]) -> String {
    let shown_cols = &cols[..cols.len().min(MAX_COLUMNS)];
    let head: Vec<String> = shown_cols.iter().map(|c| formula::col_name(*c)).collect();
    let mut s = format!("| | {} |\n|---|{}\n", head.join(" | "), "---|".repeat(shown_cols.len()));
    for r in rows {
        let Some(r) = r else {
            s.push_str(&format!("| … | {} |\n", vec![""; shown_cols.len()].join(" | ")));
            continue;
        };
        let row = g.row(*r);
        let cells: Vec<String> = shown_cols
            .iter()
            .map(|col| match row.binary_search_by_key(col, |c| c.col) {
                Ok(i) if !skip.iter().any(|x| x.c1 <= *col && *col <= x.c2 && x.r1 <= *r && *r <= x.r2) => cut(&shown(cx, &row[i])),
                _ => String::new(),
            })
            .collect();
        s.push_str(&format!("| {r} | {} |\n", cells.join(" | ")));
    }
    if cols.len() > shown_cols.len() {
        s.push_str(&format!("{}\n", t!("wizard.moreColumns", cols.len() - shown_cols.len())));
    }
    s
}

/// The rows ra..=rb outside the tables (the first `max` of them with
/// cells) under a heading; "" without any.
fn outside(cx: &Ctx, g: &Grid, (ra, rb): (u32, u32), tables: &[Rect], max: usize, first: bool) -> String {
    // The cells with something to show (not a formula without a value).
    let free = |c: &Cell| !matches!(c.val, Val::Blank) && !tables.iter().any(|x| x.c1 <= c.col && c.col <= x.c2 && x.r1 <= c.row && c.row <= x.r2);
    let mut rows = Vec::new();
    let mut more = 0;
    let mut cols = std::collections::BTreeSet::new();
    for row in g.rows_in(ra, rb) {
        let mut cells = row.iter().filter(|c| free(c)).peekable();
        let Some(c) = cells.peek() else { continue };
        if rows.len() >= max {
            more += 1;
            continue;
        }
        rows.push(Some(c.row));
        cols.extend(cells.map(|c| c.col));
    }
    let cols: Vec<u32> = cols.into_iter().collect();
    let (Some(Some(a)), Some(Some(z))) = (rows.first().copied(), rows.last().copied()) else { return String::new() };
    let heading = if first {
        t!("wizard.firstRows", rows.len())
    } else if a == z {
        t!("wizard.outsideRow", a)
    } else {
        t!("wizard.outside", a, z)
    };
    let mut s = format!("\n{heading}\n\n{}", grid(cx, g, &rows, &cols, tables));
    if more > 0 {
        s.push_str(&format!("{}\n", t!("wizard.moreRows", more)));
    }
    s
}

/// A table: where it is, its columns, and the rows of its header with the
/// first n rows of its data.
fn table(cx: &Ctx, g: &Grid, f: &Found, n: u32) -> String {
    let sep = t!("vba.separator");
    let title = if f.name.is_some() { columns::title(f) } else { t!("wizard.range", f.rect.display()) };
    let mut s = format!("\n{}\n", t!("wizard.table", title, columns::info(f, "wizard.totals").join(&sep)));
    let named: Vec<String> = f
        .names
        .iter()
        .enumerate()
        .filter(|(_, name)| !name.is_empty())
        .map(|(i, name)| t!("wizard.column", formula::col_name(f.c1 + i as u32), cut(name)))
        .collect();
    if !named.is_empty() || !f.groups.is_empty() {
        s.push('\n');
    }
    if !named.is_empty() {
        let mut list = named.iter().take(MAX_NAMES).cloned().collect::<Vec<_>>().join(&sep);
        if named.len() > MAX_NAMES {
            list.push_str(&t!("wizard.moreColumns", named.len() - MAX_NAMES));
        }
        s.push_str(&format!("- {}\n", t!("wizard.columns", list)));
    }
    if let Some(groups) = columns::groups(f) {
        s.push_str(&format!("- {groups}\n"));
    }
    if n == 0 {
        return s;
    }
    let mut rows: Vec<Option<u32>> = Vec::new();
    if let Some((a, z)) = f.header {
        rows.extend((a..=z).map(Some));
    }
    let above = f.totals.filter(|t| *t < f.data.0);
    rows.extend(above.map(Some));
    let shown = f.rows().min(n);
    rows.extend((f.data.0..f.data.0 + shown).map(Some));
    if f.rows() > shown {
        rows.push(None);
    }
    rows.extend(f.totals.filter(|t| *t > f.data.1).map(Some));
    if rows.is_empty() {
        return s;
    }
    let heading = match (f.header.is_some(), shown) {
        (true, 0) => t!("wizard.headerRows"),
        (true, _) => t!("wizard.tableRows", shown),
        (false, _) => t!("wizard.dataRows", shown),
    };
    s.push_str(&format!("\n{heading}\n\n{}", grid(cx, g, &rows, &(f.c1..=f.c2).collect::<Vec<_>>(), &[])));
    s
}

fn sheet(wb: &Workbook, sh: &Sheet, n: u32) -> String {
    let cx = Ctx { wb };
    let g = Grid::new(&sh.cells);
    let last_row = g.last_row();
    let last_col = sh.cells.iter().map(|c| c.col).max().unwrap_or(0);
    if last_row == 0 {
        return format!("{}\n", t!("wizard.sheetEmpty", sh.name));
    }
    let mut s = format!("{}\n", t!("wizard.sheetHead", sh.name, Rect::cell(last_col, last_row).display(), last_row, last_col));
    if let Some(r) = sh.cut {
        s.push_str(&format!("\n{}\n", t!("book.columnsCut", r)));
    }
    let found = layout::find(&cx, sh, &g);
    let rects: Vec<Rect> = found.iter().map(Found::taken).collect();
    if found.is_empty() {
        // A sheet without tables: all of a small one (a form, settings),
        // else its first rows.
        let max = if g.rows.len() <= MAX_OUTSIDE { MAX_OUTSIDE } else { n as usize };
        if n > 0 {
            s.push_str(&outside(&cx, &g, (1, last_row), &[], max, true));
        }
    }
    // The rows above each table that are not shown yet, then what is
    // left (under the tables, next to them).
    let mut done = 0;
    for (f, rect) in found.iter().zip(&rects) {
        if n > 0 && rect.r1 > done + 1 {
            s.push_str(&outside(&cx, &g, (done + 1, rect.r1 - 1), &rects, MAX_OUTSIDE, false));
        }
        done = done.max(rect.r1 - 1);
        s.push_str(&table(&cx, &g, f, n));
    }
    if n > 0 && !found.is_empty() && done < last_row {
        s.push_str(&outside(&cx, &g, (done + 1, last_row), &rects, MAX_OUTSIDE, false));
    }
    // The formulas, by shape.
    if !sh.formulas.is_empty() {
        let total: u64 = sh.formulas.iter().flat_map(|f| &f.rects).map(|r| (r.r2 - r.r1 + 1) as u64 * (r.c2 - r.c1 + 1) as u64).sum();
        s.push_str(&format!("\n{}\n\n", t!("wizard.formulas", total)));
        for f in sh.formulas.iter().take(MAX_FORMULAS) {
            let array = if f.array { t!("book.array") } else { String::new() };
            s.push_str(&format!("- {}: {}{array}\n", ranges(&f.rects), code(&format!("={}", formula::display(&f.formula)))));
        }
        if sh.formulas.len() > MAX_FORMULAS {
            s.push_str(&format!("{}\n", t!("wizard.moreFormulas", sh.formulas.len() - MAX_FORMULAS)));
        }
    }
    s
}
