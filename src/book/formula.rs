//! References in formulas: the shape of a formula (its references in R1C1
//! form, so that a formula filled down or across has one shape), moving a
//! formula to another cell, and ranges of cells.

use std::collections::HashMap;

pub const MAX_COL: u32 = 16384;
pub const MAX_ROW: u32 = 1_048_576;

/// A rectangle of cells (1-based, inclusive).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Rect {
    pub r1: u32,
    pub c1: u32,
    pub r2: u32,
    pub c2: u32,
}

impl Rect {
    pub fn cell(col: u32, row: u32) -> Rect {
        Rect { r1: row, c1: col, r2: row, c2: col }
    }

    /// A1, A1:B5, A:C (whole columns), 3:5 (whole rows).
    pub fn display(&self) -> String {
        if self.r1 == 1 && self.r2 == MAX_ROW {
            return format!("{}:{}", col_name(self.c1), col_name(self.c2));
        }
        if self.c1 == 1 && self.c2 == MAX_COL {
            return format!("{}:{}", self.r1, self.r2);
        }
        let a = format!("{}{}", col_name(self.c1), self.r1);
        if self.r1 == self.r2 && self.c1 == self.c2 { a } else { format!("{a}:{}{}", col_name(self.c2), self.r2) }
    }
}

/// "A" for 1 ("" outside A–XFD).
pub fn col_name(n: u32) -> String {
    crate::api::xlsx::column_name(n as i64).unwrap_or_default()
}

/// The column of A–XFD (upper case only).
fn col_number(s: &str) -> Option<u32> {
    if s.is_empty() || s.len() > 3 || !s.bytes().all(|b| b.is_ascii_uppercase()) {
        return None;
    }
    let n = s.bytes().fold(0u32, |n, b| n * 26 + (b - b'A' + 1) as u32);
    (n <= MAX_COL).then_some(n)
}

fn row_number(s: &str) -> Option<u32> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) || s.len() > 7 {
        return None;
    }
    let n: u32 = s.parse().ok()?;
    (1..=MAX_ROW).contains(&n).then_some(n)
}

/// A cell reference as (column, row), ignoring $.
pub fn parse_cell(s: &str) -> Option<(u32, u32)> {
    let s = s.replace('$', "");
    let i = s.find(|c: char| c.is_ascii_digit())?;
    Some((col_number(&s[..i])?, row_number(&s[i..])?))
}

/// A1, A1:B5, A:C or 3:5 (with or without $).
pub fn parse_range(s: &str) -> Option<Rect> {
    let s = s.trim();
    let (a, b) = s.split_once(':').unwrap_or((s, s));
    if let (Some((c1, r1)), Some((c2, r2))) = (parse_cell(a), parse_cell(b)) {
        return Some(Rect { r1: r1.min(r2), c1: c1.min(c2), r2: r1.max(r2), c2: c1.max(c2) });
    }
    let (a, b) = (a.replace('$', ""), b.replace('$', ""));
    if let (Some(c1), Some(c2)) = (col_number(&a), col_number(&b)) {
        return Some(Rect { r1: 1, c1: c1.min(c2), r2: MAX_ROW, c2: c1.max(c2) });
    }
    if let (Some(r1), Some(r2)) = (row_number(&a), row_number(&b)) {
        return Some(Rect { r1: r1.min(r2), c1: 1, r2: r1.max(r2), c2: MAX_COL });
    }
    None
}

/// The ranges of a sqref attribute ("A1:B5 D2").
pub fn parse_sqref(s: &str) -> Vec<Rect> {
    s.split_whitespace().filter_map(parse_range).collect()
}

/// A part of a reference.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Part {
    Col { abs: bool, n: u32 },
    Row { abs: bool, n: u32 },
}

fn ident_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '.' | '$' | '\\' | '?')
}

/// Calls m for the column and the row of every reference in the formula
/// (outside strings, quoted sheet names and brackets) and puts what it
/// returns in their place; None for any part turns the reference into
/// #REF!.
pub fn map_refs(f: &str, mut m: impl FnMut(Part) -> Option<String>) -> String {
    let chars: Vec<char> = f.chars().collect();
    let mut out = String::with_capacity(f.len() + 8);
    let mut i = 0;
    // The word starting at i: its end.
    let word_end = |i: usize| {
        let mut j = i;
        while j < chars.len() && ident_char(chars[j]) {
            j += 1;
        }
        j
    };
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' | '\'' => {
                // A string or a quoted sheet name; a doubled quote is part of it.
                let mut j = i + 1;
                while j < chars.len() {
                    if chars[j] == c {
                        if chars.get(j + 1) == Some(&c) {
                            j += 2;
                            continue;
                        }
                        break;
                    }
                    j += 1;
                }
                let end = (j + 1).min(chars.len());
                out.extend(&chars[i..end]);
                i = end;
            }
            '[' => {
                // A structured reference or an external workbook: copied as is.
                let mut depth = 0;
                let mut j = i;
                while j < chars.len() {
                    match chars[j] {
                        '[' => depth += 1,
                        ']' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        '\'' => j += 1, // an escaped character in a column name
                        _ => {}
                    }
                    j += 1;
                }
                let end = (j + 1).min(chars.len());
                out.extend(&chars[i..end]);
                i = end;
            }
            _ if ident_char(c) && (i == 0 || !ident_char(chars[i - 1])) => {
                let j = word_end(i);
                let word: String = chars[i..j].iter().collect();
                let next = chars.get(j).copied();
                if matches!(next, Some('(') | Some('!')) {
                    out.push_str(&word);
                    i = j;
                    continue;
                }
                // A1, $A$1
                if let Some(s) = cell_ref(&word, &mut m) {
                    out.push_str(&s);
                    i = j;
                    continue;
                }
                // A:C, 3:5
                if next == Some(':') && j + 1 < chars.len() && ident_char(chars[j + 1]) {
                    let k = word_end(j + 1);
                    let second: String = chars[j + 1..k].iter().collect();
                    if !matches!(chars.get(k), Some('(') | Some('!'))
                        && let Some(s) = line_ref(&word, &second, &mut m)
                    {
                        out.push_str(&s);
                        i = k;
                        continue;
                    }
                }
                out.push_str(&word);
                i = j;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

fn split_abs(s: &str) -> (bool, &str) {
    match s.strip_prefix('$') {
        Some(r) => (true, r),
        None => (false, s),
    }
}

fn cell_ref(word: &str, m: &mut impl FnMut(Part) -> Option<String>) -> Option<String> {
    let (ca, rest) = split_abs(word);
    let i = rest.find(|c: char| !c.is_ascii_uppercase())?;
    let col = col_number(&rest[..i])?;
    let (ra, rs) = split_abs(&rest[i..]);
    let row = row_number(rs)?;
    let a = m(Part::Col { abs: ca, n: col });
    let b = m(Part::Row { abs: ra, n: row });
    Some(match (a, b) {
        (Some(a), Some(b)) => format!("{a}{b}"),
        _ => "#REF!".into(),
    })
}

fn line_ref(a: &str, b: &str, m: &mut impl FnMut(Part) -> Option<String>) -> Option<String> {
    let ((aa, ar), (ba, br)) = (split_abs(a), split_abs(b));
    let parts = if let (Some(x), Some(y)) = (col_number(ar), col_number(br)) {
        (Part::Col { abs: aa, n: x }, Part::Col { abs: ba, n: y })
    } else if let (Some(x), Some(y)) = (row_number(ar), row_number(br)) {
        (Part::Row { abs: aa, n: x }, Part::Row { abs: ba, n: y })
    } else {
        return None;
    };
    Some(match (m(parts.0), m(parts.1)) {
        (Some(x), Some(y)) => format!("{x}:{y}"),
        _ => "#REF!".into(),
    })
}

/// The shape of the formula in the cell: the same for the cells of a
/// formula filled down or across.
pub fn shape(f: &str, col: u32, row: u32) -> String {
    map_refs(f, |p| {
        Some(match p {
            Part::Col { abs: true, n } => format!("C{n}"),
            Part::Col { abs: false, n } => format!("C[{}]", n as i64 - col as i64),
            Part::Row { abs: true, n } => format!("R{n}"),
            Part::Row { abs: false, n } => format!("R[{}]", n as i64 - row as i64),
        })
    })
}

/// The formula of the cell (col, row) copied to (to_col, to_row).
pub fn shift(f: &str, (col, row): (u32, u32), (to_col, to_row): (u32, u32)) -> String {
    let (dc, dr) = (to_col as i64 - col as i64, to_row as i64 - row as i64);
    if dc == 0 && dr == 0 {
        return f.to_string();
    }
    map_refs(f, |p| match p {
        Part::Col { abs, n } => {
            let n = if abs { n as i64 } else { n as i64 + dc };
            (1..=MAX_COL as i64).contains(&n).then(|| format!("{}{}", if abs { "$" } else { "" }, col_name(n as u32)))
        }
        Part::Row { abs, n } => {
            let n = if abs { n as i64 } else { n as i64 + dr };
            (1..=MAX_ROW as i64).contains(&n).then(|| format!("{}{n}", if abs { "$" } else { "" }))
        }
    })
}

/// The formula as Excel shows it: without the prefixes of newer
/// functions (_xlfn.XLOOKUP, _xlfn._xlws.FILTER, _xlpm.x of LAMBDA).
pub fn display(f: &str) -> String {
    f.replace("_xlfn._xlws.", "").replace("_xlfn.", "").replace("_xlpm.", "")
}

/// Rectangles that cover the cells ((column, row), each once): runs along
/// the rows, joined down when they span the same columns. In reading
/// order.
pub fn cells_to_rects(mut cells: Vec<(u32, u32)>) -> Vec<Rect> {
    cells.sort_by_key(|&(c, r)| (r, c));
    cells.dedup();
    let mut out: Vec<Rect> = Vec::new();
    // The rectangle ending at the previous row, by its columns.
    let mut open: HashMap<(u32, u32), usize> = HashMap::new();
    let mut i = 0;
    while i < cells.len() {
        let (c1, row) = cells[i];
        let mut c2 = c1;
        while i + 1 < cells.len() && cells[i + 1] == (c2 + 1, row) {
            c2 += 1;
            i += 1;
        }
        i += 1;
        match open.get(&(c1, c2)) {
            Some(&k) if out[k].r2 + 1 == row => out[k].r2 = row,
            _ => {
                open.insert((c1, c2), out.len());
                out.push(Rect { r1: row, c1, r2: row, c2 });
            }
        }
    }
    out.sort();
    out
}

/// Joins rectangles that make a larger one (side by side with the same
/// rows, or one above the other with the same columns) and drops those
/// inside others. In reading order.
pub fn merge_rects(mut v: Vec<Rect>) -> Vec<Rect> {
    v.sort();
    v.dedup();
    // Many fragments are left as they are.
    if v.len() > 2000 {
        return v;
    }
    loop {
        let mut changed = false;
        'outer: for i in 0..v.len() {
            for j in 0..v.len() {
                if i == j {
                    continue;
                }
                let (a, b) = (v[i], v[j]);
                let inside = b.r1 >= a.r1 && b.r2 <= a.r2 && b.c1 >= a.c1 && b.c2 <= a.c2;
                let down = a.c1 == b.c1 && a.c2 == b.c2 && b.r1 >= a.r1 && b.r1 <= a.r2.saturating_add(1);
                let across = a.r1 == b.r1 && a.r2 == b.r2 && b.c1 >= a.c1 && b.c1 <= a.c2.saturating_add(1);
                if inside || down || across {
                    v[i] = Rect { r1: a.r1.min(b.r1), c1: a.c1.min(b.c1), r2: a.r2.max(b.r2), c2: a.c2.max(b.c2) };
                    v.remove(j);
                    changed = true;
                    break 'outer;
                }
            }
        }
        if !changed {
            break;
        }
    }
    v.sort();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes() {
        assert_eq!(shape("B2*C2", 4, 2), shape("B3*C3", 4, 3));
        assert_ne!(shape("B2*C2", 4, 2), shape("B2*C3", 4, 3));
        assert_eq!(shape("SUM($B$2:B5)", 2, 5), "SUM(C2R2:C[0]R[0])");
        // Not references: functions, sheet names, strings, names, numbers.
        assert_eq!(shape("LOG10(A1)&\"B2\"&Sheet1!A1&'B 2'!A1+1.5E+10", 1, 1), "LOG10(C[0]R[0])&\"B2\"&Sheet1!C[0]R[0]&'B 2'!C[0]R[0]+1.5E+10");
        assert_eq!(shape("Table1[[#This Row],[A1]]+税率", 1, 1), "Table1[[#This Row],[A1]]+税率");
        assert_eq!(shape("SUM(A:A,$2:$3)", 3, 3), "SUM(C[-2]:C[-2],R2:R3)");
        assert_eq!(shape("TRUE+XFE1", 1, 1), "TRUE+XFE1");
    }

    #[test]
    fn shifts() {
        assert_eq!(shift("B2*$C$2+C$1+$D3", (4, 2), (5, 4)), "C4*$C$2+D$1+$D5");
        assert_eq!(shift("A1", (2, 2), (1, 2)), "#REF!");
        assert_eq!(shift("SUM(A:A)", (1, 1), (2, 1)), "SUM(B:B)");
        assert_eq!(display("_xlfn.XLOOKUP(A1,_xlfn._xlws.FILTER(B:B,1))"), "XLOOKUP(A1,FILTER(B:B,1))");
    }

    #[test]
    fn ranges() {
        assert_eq!(parse_range("$B$2:A1").unwrap().display(), "A1:B2");
        assert_eq!(parse_range("A:C").unwrap().display(), "A:C");
        assert_eq!(parse_range("A1:A1048576").unwrap().display(), "A:A");
        assert_eq!(parse_range("3:5").unwrap().display(), "3:5");
        assert_eq!(parse_sqref("A1 B2:C3").len(), 2);
        let r = cells_to_rects(vec![(1, 1), (2, 1), (1, 2), (2, 2), (1, 3), (4, 1)]);
        let d: Vec<String> = r.iter().map(Rect::display).collect();
        assert_eq!(d, ["A1:B2", "D1", "A3"]);
        let m = merge_rects(parse_sqref("A1:A5 A6:A10 B1:B10 C3 A2"));
        assert_eq!(m.iter().map(Rect::display).collect::<Vec<_>>(), ["A1:B10", "C3"]);
    }
}
