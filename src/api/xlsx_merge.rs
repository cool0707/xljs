//! Merged cells as scripts see them: any cell of a merged area stands for
//! the area's top-left cell, which is the one with the value, the formula,
//! the note and the format Excel shows. Reading a covered cell reads the
//! top-left one, writing it writes there.
//!
//! This is a rule of the script API only. The workbook keeps the cells as
//! the file has them, and the formula engine, printing and saving see those:
//! `=B1` of a merged A1:C1 is empty, as in Excel.

use std::collections::HashMap;

use umya_spreadsheet::Worksheet;

use super::xlsx::{Book, parse_ref};
use crate::Result;

/// A merged area: (first column, first row, last column, last row).
pub(crate) type Rect = (u32, u32, u32, u32);

const MAX_ROW: u32 = 1_048_576;
const MAX_COL: u32 = 16_384;

fn rect(m: &umya_spreadsheet::Range) -> Option<Rect> {
    let (c1, r1) = (m.coordinate_start_col()?.num(), m.coordinate_start_row()?.num());
    let (c2, r2) = (m.coordinate_end_col()?.num(), m.coordinate_end_row()?.num());
    let r = (c1.min(c2), r1.min(r2), c1.max(c2), r1.max(r2));
    // A "merge" of one cell is not one.
    (r.0 != r.2 || r.1 != r.3).then_some(r)
}

/// The merged areas of the sheet.
pub(crate) fn rects(ws: &Worksheet) -> Vec<Rect> {
    ws.merge_cells().iter().filter_map(rect).collect()
}

/// The merged area the cell is in.
pub(crate) fn area(ws: &Worksheet, col: u32, row: u32) -> Option<Rect> {
    ws.merge_cells().iter().filter_map(rect).find(|&(c1, r1, c2, r2)| (c1..=c2).contains(&col) && (r1..=r2).contains(&row))
}

/// The cell that stands for (col, row): the top-left cell of its merged
/// area, or the cell itself.
pub(crate) fn anchor(ws: &Worksheet, col: u32, row: u32) -> (u32, u32) {
    area(ws, col, row).map_or((col, row), |(c1, r1, ..)| (c1, r1))
}

/// The cell that stands for the cell `r` ("B3") of the sheet.
pub(crate) fn anchor_of(b: &Book, sheet: &str, r: &str) -> Result<(u32, u32)> {
    let (col, row) = parse_ref(r)?;
    Ok(anchor(b.sheet(sheet)?, col, row))
}

/// The name ("A1") of the cell that stands for the cell `r`.
pub(crate) fn anchor_name(b: &Book, sheet: &str, r: &str) -> Result<String> {
    let (col, row) = anchor_of(b, sheet, r)?;
    super::xlsx::cell_name(col, row)
}

/// The merged areas of a sheet, for looking up many cells.
pub(crate) struct Merges {
    pub rects: Vec<Rect>,
    /// The areas by the rows they are in (those of at most TALL rows).
    by_row: HashMap<u32, Vec<usize>>,
    /// The others (whole columns, long vertical merges).
    tall: Vec<usize>,
}

const TALL: u32 = 64;

impl Merges {
    pub(crate) fn new(ws: &Worksheet) -> Merges {
        let rects = rects(ws);
        let (mut by_row, mut tall): (HashMap<u32, Vec<usize>>, Vec<usize>) = Default::default();
        for (i, &(_, r1, _, r2)) in rects.iter().enumerate() {
            if r2 - r1 >= TALL {
                tall.push(i);
                continue;
            }
            for r in r1..=r2 {
                by_row.entry(r).or_default().push(i);
            }
        }
        Merges { rects, by_row, tall }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.rects.is_empty()
    }

    /// The merged area the cell is in.
    pub(crate) fn area(&self, col: u32, row: u32) -> Option<Rect> {
        let inside = |i: &&usize| {
            let (c1, r1, c2, r2) = self.rects[**i];
            (c1..=c2).contains(&col) && (r1..=r2).contains(&row)
        };
        let near = self.by_row.get(&row).into_iter().flatten();
        near.chain(self.tall.iter()).find(inside).map(|i| self.rects[*i])
    }

    /// Whether the cell is in a merged area without being its top-left cell:
    /// Excel shows nothing of what such a cell holds.
    pub(crate) fn covered(&self, col: u32, row: u32) -> bool {
        self.area(col, row).is_some_and(|(c1, r1, ..)| (c1, r1) != (col, row))
    }
}

/// How far a merged area reaches for the last row and column of a sheet: to
/// its last row and column, but an area up to the edge of the sheet (whole
/// rows or columns merged) reaches no further than its first.
pub(crate) fn reach((c1, r1, c2, r2): Rect) -> (u32, u32) {
    (if c2 >= MAX_COL { c1 } else { c2 }, if r2 >= MAX_ROW { r1 } else { r2 })
}

/// The area to format for the cells c1..c2 × r1..r2: the whole merged area
/// when they are all in one (a cell of a merged area is formatted as the
/// area, so its borders are those of the area), else the cells themselves.
pub(crate) fn format_area(ws: &Worksheet, (c1, r1, c2, r2): Rect) -> Rect {
    match area(ws, c1, r1) {
        Some(m) if m.2 >= c2 && m.3 >= r2 => m,
        _ => (c1, r1, c2, r2),
    }
}
