//! Where the tables of a sheet are: their columns, the rows of their
//! header and of their data. A sheet often starts with a title, settings
//! or totals, so the top rows say little about it.
//!
//! What tells where a table is, in this order:
//! - an Excel table (ListObject): its range and header as they are;
//! - the sheet's filter: its top row ends the header, the data is below;
//! - the same formula filled down: the rows it fills are data (the frozen
//!   panes, when they end right above, say where the header ends);
//! - the cells themselves: a block of cells around which the rows and
//!   columns are empty (as Excel's CurrentRegion), with the header at the
//!   frozen panes or the first row of text.
//!
//! The header may be several rows: the rows above its last one belong to
//! it when merged cells join them, when they are filled or boxed as a
//! header is, or hold a name over several columns (merged, or centered
//! across the selection): a group of columns. The row of a filter may be
//! empty (kept for its buttons), with the names in the row above.

use std::cmp::Reverse;
use std::collections::HashMap;

use super::columns::{Cell, Ctx, Grid, Key, MIN_ROWS, Table, kind, one_line};
use super::formula::{MAX_COL, Rect};
use super::Sheet;
use crate::xml::Node;

/// The empty cells kept of a sheet for their look.
pub(super) const MAX_BLANKS: usize = 1_000_000;
/// The rows a formula is filled down to tell a table by.
const MIN_RUN: u32 = 3;
/// The rows of a block of cells that may be a table.
const MIN_BLOCK: u32 = 3;
/// The rows searched for the header from the top of a block.
const HEADER_SEARCH: u32 = 30;
/// The rows of a header.
const MAX_HEADER: u32 = 6;

/// The look of a cell style (cellXfs): what tells a header from the data.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Xf {
    /// The fill: 0 for none, the same number for the same fill.
    pub fill: u32,
    pub top: bool,
    pub bottom: bool,
    pub left: bool,
    pub right: bool,
    /// Centered across the selection.
    pub across: bool,
    pub bold: bool,
}

impl Xf {
    /// Whether an empty cell of this style is kept (it may be part of a
    /// header).
    pub fn marked(&self) -> bool {
        self.fill != 0 || self.top || self.bottom || self.left || self.right || self.across
    }

    /// Lines around the cell: more than a line under a title.
    fn boxed(&self) -> bool {
        (self.left || self.right) && (self.top || self.bottom)
    }
}

/// The look of the cell styles of a workbook (styles.xml).
pub(super) fn xfs(styles: &Node) -> Vec<Xf> {
    let mut ids: HashMap<String, u32> = HashMap::new();
    let mut fill = |f: &Node| -> u32 {
        let Some(p) = f.children.first() else { return 0 };
        let key = if p.name == "patternFill" {
            match p.attr("patternType") {
                None | Some("none") => return 0,
                Some(t) => format!("{t} {:?}", p.child("fgColor").map(|c| &c.attrs)),
            }
        } else {
            format!("{} {:?}", p.name, p.attrs)
        };
        let next = ids.len() as u32 + 1;
        *ids.entry(key).or_insert(next)
    };
    let fills: Vec<u32> = styles.child("fills").map(|n| n.children("fill").map(&mut fill).collect()).unwrap_or_default();
    let side = |b: &Node, name: &str| b.child(name).and_then(|s| s.attr("style")).is_some_and(|s| s != "none");
    let borders: Vec<[bool; 4]> = styles
        .child("borders")
        .map(|n| n.children("border").map(|b| [side(b, "top"), side(b, "bottom"), side(b, "left") || side(b, "start"), side(b, "right") || side(b, "end")]).collect())
        .unwrap_or_default();
    let bold: Vec<bool> = styles.child("fonts").map(|n| n.children("font").map(|f| f.child("b").is_some_and(|b| b.flag("val", true))).collect()).unwrap_or_default();
    let Some(list) = styles.child("cellXfs") else { return Vec::new() };
    list.children("xf")
        .map(|x| {
            let id = |k: &str| x.attr(k).and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
            let b = borders.get(id("borderId")).copied().unwrap_or_default();
            Xf {
                fill: fills.get(id("fillId")).copied().unwrap_or(0),
                top: b[0],
                bottom: b[1],
                left: b[2],
                right: b[3],
                across: x.child("alignment").and_then(|a| a.attr("horizontal")) == Some("centerContinuous"),
                bold: bold.get(id("fontId")).copied().unwrap_or(false),
            }
        })
        .collect()
}

/// What told where a table is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum By {
    Table,
    Filter,
    Formulas,
    Frozen,
    Layout,
}

impl By {
    /// The message that names it (None for an Excel table: not a guess).
    pub fn key(self) -> Option<&'static str> {
        match self {
            By::Table => None,
            By::Filter => Some("book.tbl.byFilter"),
            By::Formulas => Some("book.tbl.byFormulas"),
            By::Frozen => Some("book.tbl.byFrozen"),
            By::Layout => Some("book.tbl.byLayout"),
        }
    }
}

/// A name over several columns of a header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Group {
    pub name: String,
    pub c1: u32,
    pub c2: u32,
}

/// A table found on a sheet.
#[derive(Debug, Clone)]
pub(super) struct Found {
    /// The name of an Excel table.
    pub name: Option<String>,
    /// Its cells: an Excel table's range, else from the header to the
    /// last row.
    pub rect: Rect,
    pub c1: u32,
    pub c2: u32,
    /// The rows of the header (first, last).
    pub header: Option<(u32, u32)>,
    /// The rows of the data (first, last); the last is the one before the
    /// first when there are none.
    pub data: (u32, u32),
    /// A row of totals left out of the data (after it, or between it and
    /// the header).
    pub totals: Option<u32>,
    pub by: By,
    /// The names of the columns c1..=c2: the texts of the header from the
    /// top, with " / " between them.
    pub names: Vec<String>,
    pub groups: Vec<Group>,
}

impl Found {
    /// The rows of data.
    pub fn rows(&self) -> u32 {
        (self.data.1 + 1).saturating_sub(self.data.0)
    }

    /// The cells it takes on the sheet, with the rows of its header above
    /// an Excel table.
    pub fn taken(&self) -> Rect {
        Rect { r1: self.header.map_or(self.rect.r1, |h| h.0.min(self.rect.r1)), ..self.rect }
    }
}

/// A text of a header and the cells it is over.
struct Label {
    text: String,
    c1: u32,
    c2: u32,
    r1: u32,
    r2: u32,
}

/// A row above a header.
enum Above {
    /// A row of the header.
    Header,
    /// Not one, but rows above it may be.
    Through,
    Stop,
}

fn inside(x: &Rect, c: u32, r: u32) -> bool {
    x.c1 <= c && c <= x.c2 && x.r1 <= r && r <= x.r2
}

fn overlaps(a: &Rect, b: &Rect) -> bool {
    a.c1 <= b.c2 && b.c1 <= a.c2 && a.r1 <= b.r2 && b.r1 <= a.r2
}

/// Whether a formula sums up rows (a row of totals).
fn aggregate(formula: &str) -> bool {
    const FUNCS: [&str; 14] =
        ["SUM(", "SUMIF(", "SUMIFS(", "SUMPRODUCT(", "SUBTOTAL(", "AGGREGATE(", "COUNT(", "COUNTA(", "COUNTIF(", "COUNTIFS(", "AVERAGE(", "AVERAGEIF(", "MAX(", "MIN("];
    let up = formula.to_uppercase();
    FUNCS.iter().any(|f| up.contains(f))
}

struct Look<'a> {
    cx: &'a Ctx<'a>,
    sh: &'a Sheet,
    g: &'a Grid<'a>,
    blanks: Grid<'a>,
}

impl Look<'_> {
    fn xf(&self, r: u32, c: u32) -> Xf {
        let style = self
            .g
            .at(r, c)
            .or_else(|| self.blanks.at(r, c))
            .map(|x| x.style)
            .or_else(|| self.sh.row_styles.binary_search_by_key(&r, |x| x.0).ok().map(|i| self.sh.row_styles[i].1));
        style.and_then(|s| self.cx.wb.xfs.get(s as usize)).copied().unwrap_or_default()
    }

    fn merge(&self, r: u32, c: u32) -> Option<&Rect> {
        self.sh.merges.iter().find(|m| inside(m, c, r))
    }

    /// The value of a cell (None for an empty one, a formula without a
    /// value too).
    fn key(&self, r: u32, c: u32) -> Option<Key> {
        self.g.at(r, c).and_then(|x| self.cx.key(x))
    }

    fn text(&self, r: u32, c: u32) -> String {
        self.g.at(r, c).map(|x| self.cx.header(x)).unwrap_or_default()
    }

    /// Whether the row has cells (values or formulas) in the columns.
    fn any(&self, r: u32, c1: u32, c2: u32) -> bool {
        !self.g.row_in(r, c1, c2).is_empty()
    }

    /// The cells of the row with a value.
    fn valued(&self, r: u32, c1: u32, c2: u32) -> Vec<&Cell> {
        self.g.row_in(r, c1, c2).iter().filter(|c| self.cx.key(c).is_some()).collect()
    }

    /// The text of a header over the cell: its own, that of the merged
    /// cells it is in, or that of a cell to its left centered across the
    /// selection up to it.
    fn label(&self, r: u32, c: u32) -> Option<Label> {
        if let Some(m) = self.merge(r, c) {
            let text = self.text(m.r1, m.c1);
            return (!text.is_empty()).then_some(Label { text, c1: m.c1, c2: m.c2, r1: m.r1, r2: m.r2 });
        }
        let mut start = c;
        if self.key(r, c).is_none() {
            loop {
                if !self.xf(r, start).across || start == 1 {
                    return None;
                }
                start -= 1;
                if self.merge(r, start).is_some() {
                    return None;
                }
                if self.key(r, start).is_some() {
                    break;
                }
            }
        }
        let text = self.text(r, start);
        if text.is_empty() {
            return None;
        }
        let mut end = start;
        if self.xf(r, start).across {
            while end < MAX_COL && self.key(r, end + 1).is_none() && self.xf(r, end + 1).across && self.merge(r, end + 1).is_none() {
                end += 1;
            }
        }
        (end >= c).then_some(Label { text, c1: start, c2: end, r1: r, r2: r })
    }

    /// The texts of a row of a header, from the left.
    fn labels(&self, r: u32, c1: u32, c2: u32) -> Vec<Label> {
        let mut out = Vec::new();
        let mut c = c1;
        while c <= c2 {
            match self.label(r, c) {
                Some(l) => {
                    c = l.c2.max(c) + 1;
                    out.push(l);
                }
                None => c += 1,
            }
        }
        out
    }

    /// Whether a row looks like a header by what it holds: text only (no
    /// formulas) in at least half of the columns (two of them when there
    /// are more).
    fn header_like(&self, r: u32, c1: u32, c2: u32) -> bool {
        let width = (c2 - c1 + 1) as usize;
        let need = width.div_ceil(2).max(if width > 1 { 2 } else { 1 });
        let mut n = 0;
        for c in self.g.row_in(r, c1, c2) {
            match self.cx.key(c) {
                None => {}
                Some(Key::Str(_)) if !c.formula => n += 1,
                Some(_) => return false,
            }
        }
        n >= need
    }

    /// Whether the row has a formula that sums up rows, not filled from
    /// the row above.
    fn sums(&self, r: u32, c1: u32, c2: u32) -> bool {
        self.sh.formulas.iter().filter(|f| !f.array).any(|f| {
            f.rects.iter().any(|x| {
                x.r1 <= r
                    && r <= x.r2
                    && x.c1 <= c2
                    && c1 <= x.c2
                    && aggregate(&f.formula)
                    && (x.c1.max(c1)..=x.c2.min(c2)).any(|c| r == 1 || !f.rects.iter().any(|y| inside(y, c, r - 1)))
            })
        })
    }

    /// Whether the row holds totals: a label (合計, Total) or a formula
    /// that sums up rows. With figures, a label alone is not enough (the
    /// name of a column may be one).
    fn totals(&self, r: u32, c1: u32, c2: u32, figures: bool) -> bool {
        const WORDS: [&str; 7] = ["合計", "総計", "小計", "計", "total", "grandtotal", "sum"];
        let cells = self.g.row_in(r, c1, c2);
        let label = cells.iter().any(|c| {
            if c.formula {
                return false;
            }
            let Some(Key::Str(s)) = self.cx.key(c) else { return false };
            let s: String = s.chars().filter(|c| !c.is_whitespace() && *c != '\u{3000}').collect::<String>().to_lowercase();
            WORDS.contains(&s.as_str())
        });
        let has_figures = || cells.iter().any(|c| c.formula || !matches!(self.cx.key(c), None | Some(Key::Str(_))));
        (label && (!figures || has_figures())) || self.sums(r, c1, c2)
    }

    /// Whether a cell is filled as a header: not as the data under it.
    fn header_fill(&self, r: u32, c: u32, data: u32) -> bool {
        let f = self.xf(r, c).fill;
        f != 0 && f != self.xf(data, c).fill
    }

    /// Whether the row `up` (right above the row `r` of a header ending
    /// at `last`) is one of the header.
    fn above(&self, up: u32, r: u32, last: u32, (c1, c2): (u32, u32), data: u32, spans_only: bool) -> Above {
        let joined = self.sh.merges.iter().any(|m| m.c1 <= c2 && c1 <= m.c2 && m.r1 <= up && r <= m.r2);
        let labels = self.labels(up, c1, c2);
        if labels.is_empty() {
            return if joined { Above::Through } else { Above::Stop };
        }
        if self.sums(up, c1, c2) {
            return Above::Stop;
        }
        // A title: one text over all the columns.
        let width = c2 - c1 + 1;
        if width >= 2 && labels.len() == 1 && labels[0].c1 <= c1 && c2 <= labels[0].c2 && labels[0].r2 < r {
            return Above::Stop;
        }
        // Joined to the header by merged cells, or the name of a group of
        // columns.
        if joined || labels.iter().any(|l| l.c1.max(c1) < l.c2.min(c2)) {
            return Above::Header;
        }
        if spans_only {
            return Above::Stop;
        }
        let valued = self.valued(up, c1, c2);
        let text_only = valued.iter().all(|c| !c.formula && matches!(self.cx.key(c), Some(Key::Str(_))));
        // The last row is empty (a row for the buttons of the filter):
        // the names are here.
        if r == last && self.valued(last, c1, c2).is_empty() && text_only {
            return Above::Header;
        }
        // A title at the left, filled or boxed along with the header.
        let alone = width >= 3 && valued.len() == 1 && valued[0].col == c1;
        // Filled as a header is.
        if !alone && valued.iter().all(|c| self.header_fill(up, c.col, data)) {
            return Above::Header;
        }
        // The header is not told by its fill: text in the same box.
        let filled = valued.iter().any(|c| self.header_fill(r, c.col, data));
        if !alone && !filled && text_only && valued.iter().all(|c| self.xf(up, c.col).boxed()) {
            return Above::Header;
        }
        // Nothing but the cells to tell by: rows of text.
        let plain = |row: u32| !self.g.row_in(row, c1, c2).iter().any(|c| self.header_fill(row, c.col, data) || self.xf(row, c.col).boxed());
        if plain(up) && plain(r) && self.header_like(up, c1, c2) && (self.header_like(r, c1, c2) || self.valued(r, c1, c2).is_empty()) {
            return Above::Header;
        }
        Above::Stop
    }

    /// The first row of the header that ends at the row `last`, not above
    /// the row `floor`. `data` is the first row of the data. With
    /// spans_only, only the names of groups of columns are looked for
    /// (the header of an Excel table is its own).
    fn header_top(&self, last: u32, cols: (u32, u32), data: u32, floor: u32, spans_only: bool) -> u32 {
        let (mut top, mut r) = (last, last);
        while r > floor.max(1) && last - r + 1 < MAX_HEADER {
            let up = r - 1;
            match self.above(up, r, last, cols, data, spans_only) {
                Above::Header => top = up,
                Above::Through => {}
                Above::Stop => break,
            }
            r = up;
        }
        top
    }

    /// The names of the columns from the rows h1..=h2 of a header (and
    /// under them `leaf`, the names of an Excel table's columns), and the
    /// groups of columns.
    fn names(&self, rows: Option<(u32, u32)>, (c1, c2): (u32, u32), leaf: &[String]) -> (Vec<String>, Vec<Group>) {
        let mut names: Vec<Vec<String>> = vec![Vec::new(); (c2 - c1 + 1) as usize];
        let mut groups: Vec<Group> = Vec::new();
        if let Some((h1, h2)) = rows {
            for r in h1..=h2 {
                for l in self.labels(r, c1, c2) {
                    let (a, b) = (l.c1.max(c1), l.c2.min(c2));
                    for c in a..=b {
                        let parts = &mut names[(c - c1) as usize];
                        if parts.last() != Some(&l.text) {
                            parts.push(l.text.clone());
                        }
                    }
                    // A name over several columns with names under it.
                    let under = || !leaf.is_empty() || (l.r2 + 1..=h2).any(|x| (a..=b).any(|c| self.label(x, c).is_some()));
                    if a < b && r == l.r1.max(h1) && under() {
                        groups.push(Group { name: l.text, c1: a, c2: b });
                    }
                }
            }
        }
        for (parts, name) in names.iter_mut().zip(leaf) {
            let name = one_line(name);
            if !name.is_empty() {
                parts.push(name);
            }
        }
        (names.into_iter().map(|p| p.join(" / ")).collect(), groups)
    }

    /// Whether the row `a` is unlike the row `b` under it by its look (a
    /// fill of its own, bold).
    fn stands_out(&self, a: u32, b: u32, c1: u32, c2: u32) -> bool {
        self.valued(a, c1, c2).iter().any(|c| {
            let (x, y) = (self.xf(a, c.col), self.xf(b, c.col));
            (x.fill != 0 && x.fill != y.fill) || (x.bold && !y.bold)
        })
    }

    /// Whether the row r has a text over a column of figures: the next
    /// rows (some, down to the row `end`) hold no text in it.
    fn over_figures(&self, r: u32, end: u32, c1: u32, c2: u32) -> bool {
        const ROWS: u32 = 10;
        self.valued(r, c1, c2).iter().any(|c| {
            let mut under = (r + 1..=end.min(r + ROWS)).filter_map(|x| self.key(x, c.col)).peekable();
            under.peek().is_some() && under.all(|k| !matches!(k, Key::Str(_)))
        })
    }

    /// Whether the row `p` holds data as the row `d` under it does: the
    /// same kinds of values with the same look, not text only.
    fn data_like(&self, p: u32, d: u32, c1: u32, c2: u32) -> bool {
        if self.sums(p, c1, c2) {
            return false;
        }
        let mut strong = false;
        for c in self.g.row_in(p, c1, c2) {
            let (x, y) = (self.xf(p, c.col), self.xf(d, c.col));
            if x.fill != y.fill || x.bold != y.bold {
                return false;
            }
            let under = self.g.at(d, c.col);
            match (self.cx.key(c), under.and_then(|u| self.cx.key(u))) {
                (Some(a), Some(b)) if kind(&a) != kind(&b) => return false,
                (Some(Key::Str(_)), _) | (None, _) => {}
                (Some(_), _) => strong = true,
            }
            if c.formula && under.is_some_and(|u| u.formula) {
                strong = true;
            }
        }
        strong
    }

    /// The columns c1..=c2 widened to the ones next to them with cells in
    /// the rows ra..=rb (outside what is taken).
    fn spread(&self, ra: u32, rb: u32, (c1, c2): (u32, u32), taken: &[Rect]) -> (u32, u32) {
        let mut used = vec![false; MAX_COL as usize + 2];
        for row in self.g.rows_in(ra, rb) {
            for c in row {
                if c.col <= MAX_COL && !taken.iter().any(|t| inside(t, c.col, c.row)) {
                    used[c.col as usize] = true;
                }
            }
        }
        let (mut a, mut b) = (c1, c2);
        while a > 1 && used[a as usize - 1] {
            a -= 1;
        }
        while b < MAX_COL && used[b as usize + 1] {
            b += 1;
        }
        (a, b)
    }

    /// The first row a header may start at above the row r: under what is
    /// taken above it.
    fn floor(&self, r: u32, c1: u32, c2: u32, taken: &[Rect]) -> u32 {
        taken.iter().filter(|t| t.c1 <= c2 && c1 <= t.c2 && t.r2 < r).map(|t| t.r2 + 1).max().unwrap_or(1)
    }

    fn table(&self, tb: &Table) -> Option<Found> {
        let (c1, c2) = (tb.rect.c1, tb.rect.c2);
        let r1 = tb.rect.r1 + tb.header_rows;
        let r2 = tb.rect.r2.checked_sub(tb.totals_rows)?;
        if r2 + 1 < r1 {
            return None;
        }
        // The names of groups of columns are above the table (it has no
        // merged cells).
        let top = if tb.header_rows > 0 { self.header_top(tb.rect.r1, (c1, c2), r1, 1, true) } else { tb.rect.r1 };
        let (names, groups) = self.names((top < tb.rect.r1).then(|| (top, tb.rect.r1 - 1)), (c1, c2), &tb.columns);
        Some(Found {
            name: Some(tb.name.clone()),
            rect: tb.rect,
            c1,
            c2,
            header: (tb.header_rows > 0).then_some((top, r1 - 1)),
            data: (r1, r2),
            totals: (tb.totals_rows > 0).then_some(r2 + 1),
            by: By::Table,
            names,
            groups,
        })
    }

    /// The table of the sheet's filter.
    fn filter(&self, f: &Rect, taken: &[Rect]) -> Found {
        let (c1, c2) = (f.c1, f.c2);
        let (last, r1) = (f.r1, f.r1 + 1);
        // The rows of the filter and the ones filled in under them since.
        let mut r2 = f.r2.min(self.g.last_row()).max(last);
        while self.any(r2 + 1, c1, c2) {
            r2 += 1;
        }
        while r2 >= r1 && !self.any(r2, c1, c2) {
            r2 -= 1;
        }
        let end = r2;
        let mut totals = None;
        if r2 > r1 && self.totals(r2, c1, c2, false) {
            totals = Some(r2);
            r2 -= 1;
        }
        let top = self.header_top(last, (c1, c2), r1, self.floor(last, c1, c2, taken), false);
        let (names, groups) = self.names(Some((top, last)), (c1, c2), &[]);
        Found { name: None, rect: Rect { r1: top, c1, r2: end, c2 }, c1, c2, header: Some((top, last)), data: (r1, r2), totals, by: By::Filter, names, groups }
    }

    /// The table around a formula filled down the rows `seed`.
    fn filled(&self, seed: &Rect, seeds: &[Rect], taken: &[Rect]) -> Found {
        let free = |r: u32, c1: u32, c2: u32| !taken.iter().any(|t| t.r1 <= r && r <= t.r2 && t.c1 <= c2 && c1 <= t.c2);
        let (mut r1, mut r2) = (seed.r1, seed.r2);
        let (mut c1, mut c2) = self.spread(r1, r2, (seed.c1, seed.c2), taken);
        // The other formulas filled down its columns, from a row above
        // or to a row below.
        loop {
            let before = (r1, r2);
            for o in seeds {
                if o.c1 <= c2 && c1 <= o.c2 && o.r1 <= r2 + 1 && r1 <= o.r2 + 1 && !taken.iter().any(|t| overlaps(t, o)) {
                    (r1, r2) = (r1.min(o.r1), r2.max(o.r2));
                }
            }
            if (r1, r2) == before {
                break;
            }
        }
        // Rows of data above without the formula (or with another one).
        while r1 > 1 && free(r1 - 1, c1, c2) && self.data_like(r1 - 1, r1, c1, c2) {
            r1 -= 1;
        }
        // The frozen panes end the header.
        let mut by = By::Formulas;
        if let Some(y) = self.sh.frozen {
            if y + 1 < r1 && (y + 1..r1).all(|r| free(r, c1, c2) && self.any(r, c1, c2)) && !self.header_like(r1 - 1, c1, c2) {
                r1 = y + 1;
            }
            if y + 1 == r1 {
                by = By::Frozen;
            }
        }
        // Rows filled in under the formulas.
        let end = self.g.last_row();
        while r2 < end && free(r2 + 1, c1, c2) && self.any(r2 + 1, c1, c2) {
            r2 += 1;
        }
        // Columns with a name only.
        if r1 > 1 && free(r1 - 1, c1, c2) {
            (c1, c2) = self.spread(r1 - 1, r2, (c1, c2), taken);
        }
        let bottom = r2;
        let mut totals = None;
        let mut last = (r1 > 1 && free(r1 - 1, c1, c2) && self.any(r1 - 1, c1, c2)).then(|| r1 - 1);
        // Totals between the header and the data.
        if let Some(l) = last
            && (self.sums(l, c1, c2) || (l > 1 && self.header_like(l - 1, c1, c2) && self.totals(l, c1, c2, true)))
        {
            totals = Some(l);
            last = (l > 1 && free(l - 1, c1, c2) && self.any(l - 1, c1, c2)).then(|| l - 1);
        }
        if totals.is_none() && r2 > r1 && self.totals(r2, c1, c2, false) {
            totals = Some(r2);
            r2 -= 1;
        }
        let header = last.map(|l| (self.header_top(l, (c1, c2), r1, self.floor(l, c1, c2, taken), false), l));
        let (names, groups) = self.names(header, (c1, c2), &[]);
        let top = header.map_or(totals.map_or(r1, |t| t.min(r1)), |h| h.0);
        Found { name: None, rect: Rect { r1: top, c1, r2: bottom, c2 }, c1, c2, header, data: (r1, r2), totals, by, names, groups }
    }

    /// The table of a block of cells; None when it does not look like one.
    fn block(&self, b: &Rect) -> Option<Found> {
        let (c1, c2) = (b.c1, b.c2);
        // The last row of the header: the frozen rows, or the first row
        // of text near the top (one that stands out, if any).
        let mut by = By::Layout;
        let mut last = None;
        if let Some(y) = self.sh.frozen
            && b.r1 <= y
            && y < b.r2
        {
            last = Some(y);
            by = By::Frozen;
        }
        if last.is_none() {
            let rows: Vec<u32> = (b.r1..=b.r2.min(b.r1 + HEADER_SEARCH - 1)).filter(|r| self.header_like(*r, c1, c2)).collect();
            last = rows.iter().copied().find(|r| *r < b.r2 && self.stands_out(*r, *r + 1, c1, c2)).or(rows.first().copied());
            // More rows of text, over columns that hold no text under
            // them: the header ends at the last of them.
            while let Some(l) = last
                && rows.contains(&(l + 1))
                && !self.stands_out(l, l + 1, c1, c2)
                && self.over_figures(l + 1, b.r2, c1, c2)
            {
                last = Some(l + 1);
            }
        }
        let mut r1 = last.map_or(b.r1, |l| l + 1);
        let mut r2 = b.r2;
        let mut totals = None;
        if last.is_some() && r1 < r2 && self.totals(r1, c1, c2, true) {
            totals = Some(r1);
            r1 += 1;
        } else if r2 > r1 && self.totals(r2, c1, c2, false) {
            totals = Some(r2);
            r2 -= 1;
        }
        let rows = (r2 + 1).saturating_sub(r1);
        if rows < MIN_ROWS {
            // A few rows are a table when the header is unlike them: it
            // stands out, or they hold more than text.
            let l = last?;
            let figures = self.g.row_in(r1, c1, c2).iter().any(|c| c.formula || !matches!(self.cx.key(c), None | Some(Key::Str(_))));
            if rows < 2 || !(figures || self.stands_out(l, r1, c1, c2)) {
                return None;
            }
        }
        let header = last.map(|l| (self.header_top(l, (c1, c2), r1, b.r1, false), l));
        let (names, groups) = self.names(header, (c1, c2), &[]);
        Some(Found { name: None, rect: Rect { r1: header.map_or(b.r1, |h| h.0), c1, r2: b.r2, c2 }, c1, c2, header, data: (r1, r2), totals, by, names, groups })
    }
}

fn find_root(p: &mut [usize], mut i: usize) -> usize {
    while p[i] != i {
        p[i] = p[p[i]];
        i = p[i];
    }
    i
}

fn union_rect(a: &Rect, b: &Rect) -> Rect {
    Rect { r1: a.r1.min(b.r1), c1: a.c1.min(b.c1), r2: a.r2.max(b.r2), c2: a.c2.max(b.c2) }
}

/// The blocks of cells with min_rows rows or more outside what is taken:
/// the cells joined to their neighbours (diagonal ones too), and the
/// blocks that overlap as one.
fn blocks(g: &Grid, taken: &[Rect], min_rows: u32) -> Vec<Rect> {
    let cells = g.cells;
    let free = |c: &Cell| !taken.iter().any(|t| inside(t, c.col, c.row));
    let mut parent: Vec<usize> = (0..cells.len()).collect();
    let union = |p: &mut Vec<usize>, a: usize, b: usize| {
        let (x, y) = (find_root(p, a), find_root(p, b));
        if x != y {
            p[x.max(y)] = x.min(y);
        }
    };
    for k in 0..g.rows.len() {
        let (row, start, end) = g.rows[k];
        let prev = (k > 0 && g.rows[k - 1].0 + 1 == row).then(|| g.rows[k - 1]);
        for i in start..end {
            if !free(&cells[i]) {
                continue;
            }
            let col = cells[i].col;
            if i > start && cells[i - 1].col + 1 == col && free(&cells[i - 1]) {
                union(&mut parent, i, i - 1);
            }
            if let Some((_, ps, pe)) = prev {
                let from = ps + cells[ps..pe].partition_point(|c| c.col + 1 < col);
                for (j, above) in cells.iter().enumerate().take(pe).skip(from) {
                    if above.col > col + 1 {
                        break;
                    }
                    if free(above) {
                        union(&mut parent, i, j);
                    }
                }
            }
        }
    }
    let mut rects: HashMap<usize, Rect> = HashMap::new();
    for (i, c) in cells.iter().enumerate() {
        if !free(c) {
            continue;
        }
        let root = find_root(&mut parent, i);
        let r = rects.entry(root).or_insert(Rect::cell(c.col, c.row));
        r.c1 = r.c1.min(c.col);
        r.c2 = r.c2.max(c.col);
        r.r1 = r.r1.min(c.row);
        r.r2 = r.r2.max(c.row);
    }
    let mut all: Vec<Rect> = rects.into_values().collect();
    all.sort();
    // The big ones take in what overlaps them until nothing does.
    let (mut big, mut small): (Vec<Rect>, Vec<Rect>) = all.into_iter().partition(|r| r.r2 - r.r1 + 1 >= min_rows);
    loop {
        let mut changed = false;
        let mut i = 0;
        while i < big.len() {
            let mut j = i + 1;
            while j < big.len() {
                if overlaps(&big[i], &big[j]) {
                    let o = big.remove(j);
                    big[i] = union_rect(&big[i], &o);
                    changed = true;
                } else {
                    j += 1;
                }
            }
            let before = small.len();
            let mut grown = big[i];
            small.retain(|s| {
                if overlaps(&grown, s) {
                    grown = union_rect(&grown, s);
                    false
                } else {
                    true
                }
            });
            big[i] = grown;
            changed |= small.len() != before;
            i += 1;
        }
        if !changed {
            break;
        }
    }
    big.sort();
    big
}

/// The tables of a sheet, from the top.
pub(super) fn find(cx: &Ctx, sh: &Sheet, g: &Grid) -> Vec<Found> {
    let lk = Look { cx, sh, g, blanks: Grid::new(&sh.blanks) };
    let mut out: Vec<Found> = Vec::new();
    let mut taken: Vec<Rect> = Vec::new();
    let mut add = |f: Found, taken: &mut Vec<Rect>| {
        taken.push(f.taken());
        out.push(f);
    };
    for tb in &sh.tables {
        if let Some(f) = lk.table(tb) {
            add(f, &mut taken);
        }
    }
    if let Some(f) = &sh.filter
        && !taken.iter().any(|t| overlaps(t, f))
    {
        let f = lk.filter(f, &taken);
        add(f, &mut taken);
    }
    let mut seeds: Vec<Rect> = sh.formulas.iter().filter(|f| !f.array).flat_map(|f| f.rects.iter().copied()).filter(|r| r.r2 - r.r1 + 1 >= MIN_RUN).collect();
    seeds.sort_by_key(|r| (Reverse(r.r2 - r.r1), r.r1, r.c1));
    for s in &seeds {
        if taken.iter().any(|t| overlaps(t, s)) {
            continue;
        }
        // A few rows of formulas without a header are not a table.
        let f = lk.filled(s, &seeds, &taken);
        if f.header.is_some() || f.rows() >= MIN_ROWS {
            add(f, &mut taken);
        }
    }
    // What is left: any block of cells when nothing told of a table, else
    // the ones with many rows.
    let min = if taken.is_empty() { MIN_BLOCK } else { MIN_ROWS };
    for b in blocks(g, &taken, min) {
        if let Some(f) = lk.block(&b) {
            add(f, &mut taken);
        }
    }
    out.sort_by_key(|f| (f.header.map_or(f.data.0, |h| h.0), f.c1));
    out
}
