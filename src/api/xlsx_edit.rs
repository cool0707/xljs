//! Copying sheets and inserting and deleting rows and columns, as Excel
//! does: what refers to the cells moves with them (gridcraft's reference
//! adjustment, through umya-spreadsheet's `Workbook::edit_lines`).

use gridcraft_formula::adjust::{Axis, Edit, adjust_if_changed};
use gridcraft_formula::Expr;
use gridcraft_xlsx_formulas::fmla;
use umya_spreadsheet::CellFormulaValues;
use umya_spreadsheet::helper::rewrite::LineEdit;

use super::xlsx::{Book, column_name};
use crate::{Error, Result, err, t};

const MAX_ROWS: u32 = 1_048_576;
const MAX_COLS: u32 = 16_384;

/// Rows or columns to insert or delete.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Lines {
    pub columns: bool,
    /// The first one (1-based).
    pub at: u32,
    pub count: u32,
    pub insert: bool,
}

impl Lines {
    fn max(&self) -> u32 {
        if self.columns { MAX_COLS } else { MAX_ROWS }
    }

    fn edit(&self) -> Edit {
        let axis = if self.columns { Axis::Cols } else { Axis::Rows };
        let (at, count) = (self.at - 1, self.count);
        if self.insert { Edit::Insert { axis, at, count } } else { Edit::Delete { axis, at, count } }
    }

    fn line_edit(&self) -> LineEdit {
        let e = match (self.columns, self.insert) {
            (false, true) => LineEdit::insert_rows(self.at, self.count),
            (false, false) => LineEdit::remove_rows(self.at, self.count),
            (true, true) => LineEdit::insert_columns(self.at, self.count),
            (true, false) => LineEdit::remove_columns(self.at, self.count),
        };
        e.with_table_column(&t!("xlsx.tableColumn"))
    }

    /// The rows (columns) `lo..=hi` cut apart: lines inserted inside them,
    /// or some of them (not all) deleted.
    fn cuts(&self, lo: u32, hi: u32) -> bool {
        let end = self.at + self.count - 1;
        if self.insert {
            self.at > lo && self.at <= hi
        } else {
            self.at <= hi && end >= lo && !(self.at <= lo && end >= hi)
        }
    }

    /// The lines of an area (columns or rows, 1-based).
    fn span(&self, area: &str) -> Option<(u32, u32)> {
        let (a, b) = area.split_once(':').unwrap_or((area, area));
        let p = |s: &str| -> Option<(u32, u32)> {
            let s = s.replace('$', "");
            let l = s.chars().take_while(char::is_ascii_alphabetic).count();
            let col = if l == 0 { 0 } else { super::xlsx::column_number(&s[..l]).ok()? };
            let row = s[l..].parse::<u32>().unwrap_or(0);
            Some((col, row))
        };
        let (a, b) = (p(a)?, p(b)?);
        let (lo, hi) = if self.columns { (a.0, b.0) } else { (a.1, b.1) };
        if lo == 0 {
            // A whole column (row) when the edit is on rows (columns).
            return Some((1, self.max()));
        }
        Some((lo.min(hi), lo.max(hi)))
    }

    /// "3" for a row, "C" for a column.
    fn name(&self, n: u32) -> String {
        if self.columns { column_name(i64::from(n)).unwrap_or_default() } else { n.to_string() }
    }
}

/// A formula of the file with the references to `sheet` moved by `edit`;
/// None when it has none that move (or it can not be read).
fn adjusted(host: Option<&str>, text: &str, sheet: &str, edit: &Edit) -> Option<String> {
    let body = text.trim();
    let body = body.strip_prefix('=').unwrap_or(body);
    if body.is_empty() {
        return None;
    }
    let shown = fmla::from_file(body);
    let e = gridcraft_formula::parse(&shown).ok()?;
    // A host no sheet has: references without a sheet are left alone.
    let new = adjust_if_changed(&e, host.unwrap_or("\u{0}"), sheet, edit)?;
    Some(fmla::text_to_file(&gridcraft_formula::print(&new)))
}

/// Inserts or deletes rows or columns of a sheet.
pub(crate) fn edit_lines(b: &mut Book, sheet: &str, lines: Lines) -> Result<()> {
    let max = lines.max();
    let what = if lines.columns { "column" } else { "row" };
    if lines.count == 0 {
        return Ok(());
    }
    if lines.at < 1 || lines.at > max {
        return Err(Error::msg(format!("the {what} number must be greater than or equal to 1 and less than or equal to {max}, got {}", lines.at)));
    }
    if !lines.insert && u64::from(lines.at) + u64::from(lines.count) - 1 > u64::from(max) {
        return Err(err!("xlsx.edit.beyond", lines.name(max)));
    }
    let count = if lines.insert { lines.count.min(max - lines.at + 1) } else { lines.count };
    let lines = Lines { count, ..lines };
    check(b, sheet, &lines)?;
    b.calc_reset();
    let edit = lines.edit();
    let name = b.sheet(sheet)?.name().to_string();
    b.wb
        .edit_lines(&name, &lines.line_edit(), &mut |host, text| adjusted(host, text, &name, &edit))
        .map_err(|e| Error::msg(e.to_string()))?;
    if lines.insert {
        copy_format(b, &name, &lines)?;
    }
    b.parts_changed();
    // Excel works the results out again when it opens the file.
    b.wb.calculation_properties_mut().set_full_calc_on_load(true);
    Ok(())
}

/// What Excel refuses to cut apart: array formulas, tables (their header
/// row) and pivot tables; and cells pushed off the sheet.
fn check(b: &Book, sheet: &str, lines: &Lines) -> Result<()> {
    let ws = b.sheet(sheet)?;
    let at = |c: &umya_spreadsheet::Cell| if lines.columns { c.coordinate().col_num() } else { c.coordinate().row_num() };
    if lines.insert {
        let limit = lines.max() - lines.count;
        if ws.cells().iter().any(|c| at(c) > limit && (!c.value().is_empty() || c.formula_obj().is_some())) {
            return Err(err!("xlsx.edit.pushedOff"));
        }
    }
    for c in ws.cells() {
        let Some(f) = c.formula_obj() else { continue };
        if *f.formula_type() != CellFormulaValues::Array || f.dynamic_array() || !f.reference().contains(':') {
            continue;
        }
        if let Some((lo, hi)) = lines.span(f.reference())
            && lines.cuts(lo, hi)
        {
            return Err(err!("xlsx.edit.array", f.reference()));
        }
    }
    for t in ws.tables() {
        let (a, z) = t.area();
        let (lo, hi) = if lines.columns { (a.col_num(), z.col_num()) } else { (a.row_num(), z.row_num()) };
        // The header row deleted, the table left.
        if !lines.columns && !lines.insert && lines.cuts(lo, hi) && lines.at <= lo {
            let name = if t.display_name().is_empty() { t.name() } else { t.display_name() };
            return Err(err!("xlsx.edit.tableHeader", name));
        }
    }
    for area in ws.pivot_table_areas() {
        if let Some((lo, hi)) = lines.span(&area)
            && lines.cuts(lo, hi)
        {
            return Err(err!("xlsx.edit.pivot", area));
        }
    }
    Ok(())
}

/// Gives the lines inserted the format of the one before them (above, or
/// to the left), as Excel does; of the one after them when they are first.
fn copy_format(b: &mut Book, sheet: &str, lines: &Lines) -> Result<()> {
    let from = if lines.at > 1 { lines.at - 1 } else { lines.at + lines.count };
    if from > lines.max() {
        return Ok(());
    }
    let ws = b.layout_mut(sheet)?;
    let styles: Vec<(u32, umya_spreadsheet::Style)> = ws
        .cells()
        .iter()
        .filter_map(|c| {
            let (col, row) = (c.coordinate().col_num(), c.coordinate().row_num());
            let (line, other) = if lines.columns { (col, row) } else { (row, col) };
            (line == from && *c.style() != umya_spreadsheet::Style::default()).then(|| (other, c.style().clone()))
        })
        .collect();
    let new = lines.at..lines.at + lines.count;
    for n in new.clone() {
        for (other, style) in &styles {
            let at = if lines.columns { (n, *other) } else { (*other, n) };
            ws.cell_mut(at).set_style(style.clone());
        }
    }
    if lines.columns {
        if let Some(src) = ws.column_dimension_by_number(from).cloned() {
            for n in new {
                let mut c = src.clone();
                c.set_col_num(n);
                c.set_hidden(false);
                *ws.column_dimension_by_number_mut(n) = c;
            }
        }
    } else if let Some(src) = ws.row_dimension(from) {
        let (height, custom, style) = (src.height(), src.custom_height(), src.style().clone());
        for n in new {
            let r = ws.row_dimension_mut(n);
            if custom {
                r.set_height(height);
                r.set_custom_height(true);
            }
            if style != umya_spreadsheet::Style::default() {
                r.set_style(style.clone());
            }
        }
    }
    Ok(())
}

/// Copies a sheet, as Excel does, to `name` (default: "名前 (2)") right
/// after it. Returns the copy's name.
pub(crate) fn copy_sheet(b: &mut Book, sheet: &str, name: Option<&str>, position: Option<usize>) -> Result<String> {
    let sheets = b.wb.sheet_collection();
    let Some(index) = sheets.iter().position(|s| s.name() == sheet) else {
        return Err(Error::msg(format!("sheet {sheet} does not exist")));
    };
    let taken = |n: &str| sheets.iter().any(|s| s.name().eq_ignore_ascii_case(n));
    let name = match name {
        Some(n) => {
            if taken(n) {
                return Err(err!("xlsx.sheetExists", n));
            }
            super::xlsx::check_sheet_name(n)?;
            n.to_string()
        }
        None => (2..)
            .map(|i| {
                let suffix = format!(" ({i})");
                let base: String = sheet.chars().take(31 - suffix.chars().count()).collect();
                format!("{base}{suffix}")
            })
            .find(|n| !taken(n))
            .unwrap_or_default(),
    };
    let position = position.unwrap_or(index + 1);
    let tables: Vec<String> = sheets[index].tables().iter().map(|t| t.name().to_string()).collect();
    b.calc_reset();
    let copy = b.wb.copy_sheet(index, &name, position).map_err(|e| Error::msg(e.to_string()))?;
    // Formulas of the copy that use its tables by name use the copy's.
    let renamed: Vec<(String, String)> = tables.into_iter().zip(copy.tables().iter().map(|t| t.name().to_string())).collect();
    if !renamed.is_empty() {
        copy.rewrite_formulas(&mut |_, text| rename_tables(text, &renamed));
    }
    if b.fresh && sheet == "Sheet1" {
        b.fresh = false;
    }
    b.parts_changed();
    Ok(name)
}

/// A formula of the file with the tables `renamed` (old, new) renamed.
fn rename_tables(text: &str, renamed: &[(String, String)]) -> Option<String> {
    if !text.contains('[') {
        return None;
    }
    let e = gridcraft_formula::parse(&fmla::from_file(text)).ok()?;
    let mut changed = false;
    let new = e.map(&mut |x| match x {
        Expr::Struct(mut s) => {
            if let Some((_, n)) = renamed.iter().find(|(o, _)| o.eq_ignore_ascii_case(&s.table)) {
                s.table = n.clone();
                changed = true;
            }
            Expr::Struct(s)
        }
        other => other,
    });
    changed.then(|| fmla::text_to_file(&gridcraft_formula::print(&new)))
}

#[cfg(test)]
#[path = "xlsx_edit_tests.rs"]
mod tests;
