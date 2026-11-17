//! The columns of the tables of a sheet with many rows (MIN_ROWS of data
//! or more), for making input rules: for each column its name and what its
//! values are like (their kinds, blanks, ranges, digits, lengths, leading
//! zeros, patterns, the values of a short list, duplicates).
//!
//! Where the tables are (Excel tables, the filter, formulas filled down,
//! blocks of cells) and the rows of their headers: layout.rs.

use std::collections::HashMap;

use super::formula::{self, Rect};
use super::layout;
use super::{Sheet, Workbook, code};
use crate::xml::Node;
use crate::api::xlsx::{builtin_date_fmt, date_format_code, from_serial};
use crate::t;

/// The rows of data a table needs to be described.
pub(super) const MIN_ROWS: u32 = 100;
/// The cells of values read of a sheet; the rows after them are not read.
pub(super) const MAX_CELLS: usize = 5_000_000;
/// The distinct values listed (a column of a few values: a list).
const MAX_LIST: usize = 20;
/// The patterns of the text listed.
const MAX_PATTERNS: usize = 5;
/// The examples of a kind of value that few cells have.
const MAX_EXAMPLES: usize = 3;
/// The characters of a value shown.
const MAX_TEXT: usize = 30;

/// A cell's value as stored.
#[derive(Debug, Clone)]
pub(super) enum Val {
    Blank,
    Num(f64),
    /// An index into the shared strings.
    Shared(u32),
    Text(Box<str>),
    Bool(bool),
    Err(Box<str>),
}

/// A cell with a value or a formula.
#[derive(Debug, Clone)]
pub(super) struct Cell {
    pub row: u32,
    pub col: u32,
    pub style: u32,
    pub formula: bool,
    pub val: Val,
}

/// An Excel table (ListObject).
#[derive(Debug, Clone)]
pub(super) struct Table {
    pub name: String,
    pub rect: Rect,
    pub header_rows: u32,
    pub totals_rows: u32,
    pub columns: Vec<String>,
}

/// Reads an Excel table's part.
pub(super) fn table(n: &Node) -> Option<Table> {
    let rect = formula::parse_range(n.attr("ref")?)?;
    let num = |k: &str, d: u32| n.attr(k).and_then(|v| v.parse().ok()).unwrap_or(d);
    Some(Table {
        name: n.attr("displayName").or(n.attr("name")).unwrap_or("").to_string(),
        rect,
        header_rows: num("headerRowCount", 1),
        totals_rows: num("totalsRowCount", 0),
        columns: n.child("tableColumns").map(|c| c.children("tableColumn").map(|c| c.attr("name").unwrap_or("").to_string()).collect()).unwrap_or_default(),
    })
}

/// The text of a string item (a shared string, an inline string): its
/// runs without the phonetic guides.
pub(super) fn rich_text(n: &Node) -> String {
    let mut s = String::new();
    for c in &n.children {
        match c.name.as_str() {
            "t" => s.push_str(&c.all_text()),
            "r" => {
                if let Some(t) = c.child("t") {
                    s.push_str(&t.all_text());
                }
            }
            _ => {}
        }
    }
    s
}

/// The cell read from its element; None when it has neither a value nor
/// a formula.
pub(super) fn cell(c: &Node, col: u32, row: u32) -> Option<Cell> {
    let formula = c.child("f").is_some();
    let v = c.child("v").map(|v| v.all_text());
    let val = match c.attr("t").unwrap_or("n") {
        "s" => v.and_then(|v| v.trim().parse().ok()).map(Val::Shared),
        "str" | "d" => v.map(|v| Val::Text(v.into())),
        "inlineStr" => c.child("is").map(|i| Val::Text(rich_text(i).into())),
        "b" => v.map(|v| Val::Bool(v.trim() == "1")),
        "e" => v.map(|v| Val::Err(v.into())),
        _ => v.and_then(|v| v.trim().parse().ok()).map(Val::Num),
    }
    .unwrap_or(Val::Blank);
    if !formula && matches!(val, Val::Blank) {
        return None;
    }
    Some(Cell { row, col, style: c.attr("s").and_then(|s| s.parse().ok()).unwrap_or(0), formula, val })
}

/// The cells of a sheet by row (sorted by row and column).
pub(super) struct Grid<'a> {
    pub cells: &'a [Cell],
    /// (row, first cell, end).
    pub rows: Vec<(u32, usize, usize)>,
}

impl<'a> Grid<'a> {
    pub fn new(cells: &'a [Cell]) -> Grid<'a> {
        let mut rows: Vec<(u32, usize, usize)> = Vec::new();
        for (i, c) in cells.iter().enumerate() {
            match rows.last_mut() {
                Some(r) if r.0 == c.row => r.2 = i + 1,
                _ => rows.push((c.row, i, i + 1)),
            }
        }
        Grid { cells, rows }
    }

    pub fn row(&self, r: u32) -> &'a [Cell] {
        match self.rows.binary_search_by_key(&r, |x| x.0) {
            Ok(i) => &self.cells[self.rows[i].1..self.rows[i].2],
            Err(_) => &[],
        }
    }

    /// The cells of the row in the columns c1..=c2.
    pub fn row_in(&self, r: u32, c1: u32, c2: u32) -> &'a [Cell] {
        let row = self.row(r);
        let a = row.partition_point(|c| c.col < c1);
        let b = row.partition_point(|c| c.col <= c2);
        &row[a..b]
    }

    pub fn at(&self, r: u32, c: u32) -> Option<&'a Cell> {
        self.row_in(r, c, c).first()
    }

    /// The last row with cells (0 without any).
    pub fn last_row(&self) -> u32 {
        self.rows.last().map_or(0, |r| r.0)
    }

    /// The cells of the rows r1..=r2.
    pub fn rows_in(&self, r1: u32, r2: u32) -> impl Iterator<Item = &'a [Cell]> + '_ {
        let start = self.rows.partition_point(|x| x.0 < r1);
        self.rows[start..].iter().take_while(move |x| x.0 <= r2).map(|x| &self.cells[x.1..x.2])
    }
}

/// What a cell holds, for the statistics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) enum Kind {
    Num,
    Date,
    Time,
    Str,
    Bool,
    Err,
}

/// A distinct value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum Key {
    Num(u64),
    Date(u64),
    Time(u64),
    Str(String),
    Bool(bool),
    Err(String),
}

pub(super) struct Ctx<'a> {
    pub wb: &'a Workbook,
}

impl Ctx<'_> {
    pub fn text<'b>(&'b self, v: &'b Val) -> Option<&'b str> {
        match v {
            Val::Shared(i) => Some(self.wb.strings.get(*i as usize).map(String::as_str).unwrap_or("")),
            Val::Text(s) => Some(s),
            _ => None,
        }
    }

    pub fn format(&self, style: u32) -> (u32, &str) {
        self.wb.formats.get(style as usize).map(|(id, c)| (*id, c.as_str())).unwrap_or((0, "General"))
    }

    pub fn is_date(&self, style: u32) -> bool {
        let (id, code) = self.format(style);
        builtin_date_fmt(id) || date_format_code(code)
    }

    /// A cell's value as a key; None for a blank (an empty text too).
    pub fn key(&self, c: &Cell) -> Option<Key> {
        Some(match &c.val {
            Val::Blank => return None,
            Val::Num(f) if self.is_date(c.style) => {
                if *f < 1.0 && *f >= 0.0 {
                    Key::Time(f.to_bits())
                } else {
                    Key::Date(f.to_bits())
                }
            }
            Val::Num(f) => Key::Num(f.to_bits()),
            Val::Bool(b) => Key::Bool(*b),
            Val::Err(e) => Key::Err(e.to_string()),
            v => {
                let s = self.text(v).unwrap_or("");
                if s.is_empty() {
                    return None;
                }
                Key::Str(s.to_string())
            }
        })
    }

    /// The text of a header cell.
    pub fn header(&self, c: &Cell) -> String {
        match self.key(c) {
            Some(Key::Str(s)) => one_line(&s),
            Some(k) => show(&k, self.wb.date1904),
            None => String::new(),
        }
    }
}

pub(super) fn kind(k: &Key) -> Kind {
    match k {
        Key::Num(_) => Kind::Num,
        Key::Date(_) => Kind::Date,
        Key::Time(_) => Kind::Time,
        Key::Str(_) => Kind::Str,
        Key::Bool(_) => Kind::Bool,
        Key::Err(_) => Kind::Err,
    }
}

pub(super) fn one_line(s: &str) -> String {
    s.replace("\r\n", " ").replace(['\r', '\n'], " ").trim().to_string()
}

fn short(s: &str) -> String {
    let s = one_line(s);
    if s.chars().count() > MAX_TEXT { format!("{}…", s.chars().take(MAX_TEXT).collect::<String>()) } else { s }
}

fn date_text(f: f64, date1904: bool, date: bool, time: bool) -> String {
    let Some((y, mo, d, h, mi, s, _)) = from_serial(f, date1904) else { return crate::api::numfmt::general(f) };
    let mut out = Vec::new();
    if date {
        out.push(format!("{y:04}/{mo:02}/{d:02}"));
    }
    if time {
        out.push(if s > 0 { format!("{h:02}:{mi:02}:{s:02}") } else { format!("{h:02}:{mi:02}") });
    }
    out.join(" ")
}

/// A value as shown in the text.
fn show(k: &Key, date1904: bool) -> String {
    match k {
        Key::Num(b) => crate::api::numfmt::general(f64::from_bits(*b)),
        Key::Date(b) => {
            let f = f64::from_bits(*b);
            date_text(f, date1904, true, f.fract() != 0.0)
        }
        Key::Time(b) => date_text(f64::from_bits(*b), date1904, false, true),
        Key::Str(s) => t!("book.quote", short(s)).trim().to_string(),
        Key::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_string(),
        Key::Err(e) => e.clone(),
    }
}

/// A share as a percentage: 2.5%, 100%, <0.1%.
fn pct(n: usize, total: usize) -> String {
    if total == 0 {
        return "0%".into();
    }
    let p = n as f64 * 100.0 / total as f64;
    if n > 0 && p < 0.1 {
        return "<0.1%".into();
    }
    let s = format!("{p:.1}");
    format!("{}%", s.strip_suffix(".0").unwrap_or(&s))
}

/// "3" or "1〜6".
fn span(a: usize, b: usize) -> String {
    if a == b { a.to_string() } else { t!("book.col.span", a, b) }
}

/// The decimal places of a number (as Excel shows it, 15 significant
/// digits), up to 10.
fn decimals(f: f64) -> usize {
    let s = format!("{:.*e}", 14, f);
    let (mant, exp) = s.split_once('e').unwrap_or((&s, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let digits = mant.replace(['-', '.'], "");
    let sig = digits.trim_end_matches('0').len() as i32;
    (sig - 1 - exp).clamp(0, 10) as usize
}

/// The pattern of an ASCII text: 9 for a digit, A and a for letters.
fn pattern(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '0'..='9' => '9',
            'A'..='Z' => 'A',
            'a'..='z' => 'a',
            c => c,
        })
        .collect()
}

fn is_half(c: char) -> bool {
    (' '..='~').contains(&c) || ('\u{FF61}'..='\u{FF9F}').contains(&c)
}

fn has_space_around(s: &str) -> bool {
    let sp = |c: char| c.is_whitespace() || c == '\u{3000}';
    s.starts_with(sp) || s.ends_with(sp)
}

/// What the values of a column are like.
#[derive(Default)]
struct Stats {
    rows: usize,
    formulas: usize,
    kinds: HashMap<Kind, usize>,
    examples: HashMap<Kind, Vec<Key>>,
    values: HashMap<Key, usize>,
    /// The first value of each distinct one, in order.
    order: Vec<Key>,
    formats: HashMap<String, usize>,
    // Numbers.
    num_min: f64,
    num_max: f64,
    ints: usize,
    int_digits: (usize, usize),
    decimals: usize,
    // Dates and times.
    date_min: f64,
    date_max: f64,
    with_time: usize,
    // Text.
    len: (usize, usize),
    digit_only: usize,
    digit_len: (usize, usize),
    zero_lead: usize,
    ascii: usize,
    patterns: HashMap<String, usize>,
    half: usize,
    full: usize,
    half_kana: usize,
    spaces: usize,
    newlines: usize,
}

fn widen(r: &mut (usize, usize), n: usize, first: bool) {
    if first {
        *r = (n, n);
    } else {
        r.0 = r.0.min(n);
        r.1 = r.1.max(n);
    }
}

impl Stats {
    fn count(&self, k: Kind) -> usize {
        self.kinds.get(&k).copied().unwrap_or(0)
    }

    fn add(&mut self, cx: &Ctx, c: &Cell) {
        if c.formula {
            self.formulas += 1;
        }
        let Some(key) = cx.key(c) else { return };
        let k = kind(&key);
        let n = self.count(k);
        *self.kinds.entry(k).or_default() += 1;
        let (_, code) = cx.format(c.style);
        *self.formats.entry(code.to_string()).or_default() += 1;
        match &key {
            Key::Num(b) => {
                let f = f64::from_bits(*b);
                if n == 0 {
                    (self.num_min, self.num_max) = (f, f);
                } else {
                    self.num_min = self.num_min.min(f);
                    self.num_max = self.num_max.max(f);
                }
                if f.fract() == 0.0 && f.abs() < 1e15 {
                    let d = (f.abs() as u64).to_string().len();
                    widen(&mut self.int_digits, d, self.ints == 0);
                    self.ints += 1;
                } else {
                    self.decimals = self.decimals.max(decimals(f));
                }
            }
            Key::Date(b) | Key::Time(b) => {
                let f = f64::from_bits(*b);
                let first = self.count(Kind::Date) + self.count(Kind::Time) == 1;
                if first {
                    (self.date_min, self.date_max) = (f, f);
                } else {
                    self.date_min = self.date_min.min(f);
                    self.date_max = self.date_max.max(f);
                }
                if k == Kind::Date && f.fract() != 0.0 {
                    self.with_time += 1;
                }
            }
            Key::Str(s) => {
                let len = s.chars().count();
                widen(&mut self.len, len, n == 0);
                if s.bytes().all(|b| b.is_ascii_digit()) {
                    widen(&mut self.digit_len, len, self.digit_only == 0);
                    self.digit_only += 1;
                    if len > 1 && s.starts_with('0') {
                        self.zero_lead += 1;
                    }
                }
                if s.chars().all(|c| (' '..='~').contains(&c)) {
                    self.ascii += 1;
                    if self.patterns.len() <= MAX_PATTERNS || self.patterns.contains_key(&pattern(s)) {
                        *self.patterns.entry(pattern(s)).or_default() += 1;
                    }
                }
                let half = s.chars().filter(|c| is_half(*c)).count();
                if half == len {
                    self.half += 1;
                } else if half == 0 {
                    self.full += 1;
                }
                if s.chars().any(|c| ('\u{FF61}'..='\u{FF9F}').contains(&c)) {
                    self.half_kana += 1;
                }
                if has_space_around(s) {
                    self.spaces += 1;
                }
                if s.contains(['\r', '\n']) {
                    self.newlines += 1;
                }
            }
            _ => {}
        }
        let ex = self.examples.entry(k).or_default();
        if ex.len() < MAX_EXAMPLES && !ex.contains(&key) {
            ex.push(key.clone());
        }
        match self.values.get_mut(&key) {
            Some(v) => *v += 1,
            None => {
                self.values.insert(key.clone(), 1);
                self.order.push(key);
            }
        }
    }

    /// The description of the column: its parts.
    fn describe(&self, date1904: bool) -> Vec<String> {
        let mut out = Vec::new();
        let filled: usize = self.kinds.values().sum();
        if filled == 0 {
            out.push(t!("book.col.allBlank"));
            if self.formulas > 0 {
                out.push(self.formulas_text());
            }
            return out;
        }
        let label = |k: Kind| -> String {
            match k {
                Kind::Num if self.ints == self.count(Kind::Num) => t!("book.col.int"),
                Kind::Num => t!("book.col.num"),
                Kind::Date if self.with_time == self.count(Kind::Date) => t!("book.col.datetime"),
                Kind::Date => t!("book.col.date"),
                Kind::Time => t!("book.col.time"),
                Kind::Str => t!("book.col.str"),
                Kind::Bool => t!("book.col.bool"),
                Kind::Err => t!("book.col.err"),
            }
        };
        let mut kinds: Vec<(Kind, usize)> = self.kinds.iter().map(|(k, n)| (*k, *n)).collect();
        kinds.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        if kinds.len() == 1 {
            out.push(t!("book.col.only", label(kinds[0].0)));
        } else {
            let parts: Vec<String> = kinds
                .iter()
                .enumerate()
                .map(|(i, (k, n))| {
                    let mut s = format!("{} {}", label(*k), pct(*n, filled));
                    if i > 0 {
                        let ex: Vec<String> = self.examples[k].iter().map(|v| show(v, date1904)).collect();
                        s.push_str(&t!("book.col.examples", ex.join(&t!("vba.separator"))));
                    }
                    s
                })
                .collect();
            out.push(parts.join(&t!("book.col.kindSep")));
        }
        let blank = self.rows - filled;
        out.push(if blank == 0 { t!("book.col.noBlank") } else { t!("book.col.blank", blank, pct(blank, self.rows)) });
        if self.formulas > 0 {
            out.push(self.formulas_text());
        }
        // The details of the kinds that most of the values are.
        let nums = self.count(Kind::Num);
        if nums * 2 >= filled {
            let (a, b) = (crate::api::numfmt::general(self.num_min), crate::api::numfmt::general(self.num_max));
            out.push(if a == b { t!("book.col.always", a) } else { t!("book.col.range", a, b) });
            if self.ints == nums {
                out.push(t!("book.col.digits", span(self.int_digits.0, self.int_digits.1)));
            } else if self.decimals > 0 {
                out.push(t!("book.col.decimals", self.decimals));
            }
        }
        let dates = self.count(Kind::Date) + self.count(Kind::Time);
        if dates * 2 >= filled {
            let only_time = self.count(Kind::Date) == 0;
            let time = only_time || self.with_time > 0;
            let (a, b) = (date_text(self.date_min, date1904, !only_time, time), date_text(self.date_max, date1904, !only_time, time));
            out.push(if a == b { t!("book.col.always", a) } else { t!("book.col.range", a, b) });
            if self.with_time > 0 && self.with_time < self.count(Kind::Date) {
                out.push(t!("book.col.withTime", self.with_time));
            }
        }
        let strs = self.count(Kind::Str);
        if strs * 2 >= filled {
            out.push(t!("book.col.length", span(self.len.0, self.len.1)));
            if self.digit_only == strs {
                out.push(t!("book.col.digitsOnly"));
            } else if self.digit_only > 0 {
                out.push(t!("book.col.digitsSome", self.digit_only, span(self.digit_len.0, self.digit_len.1)));
            }
            if self.zero_lead > 0 {
                out.push(t!("book.col.zeroLead", self.zero_lead));
            }
            let mut shown_pattern = false;
            if self.ascii == strs && self.digit_only < strs && self.patterns.len() <= MAX_PATTERNS {
                let mut p: Vec<(&String, &usize)> = self.patterns.iter().collect();
                p.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
                let list: Vec<String> = if p.len() == 1 {
                    vec![code(p[0].0)]
                } else {
                    p.iter().map(|(s, n)| format!("{} {}", code(s), pct(**n, strs))).collect()
                };
                out.push(t!("book.col.pattern", list.join(&t!("vba.separator"))));
                shown_pattern = true;
            }
            if !shown_pattern && self.digit_only < strs {
                if self.half == strs {
                    out.push(t!("book.col.halfOnly"));
                } else if self.full == strs {
                    out.push(t!("book.col.fullOnly"));
                } else if self.half * 5 >= strs * 4 {
                    out.push(t!("book.col.someFull", strs - self.half));
                } else if self.full * 5 >= strs * 4 {
                    out.push(t!("book.col.someHalf", strs - self.full));
                }
            }
            if self.half_kana > 0 && self.half != strs {
                out.push(t!("book.col.halfKana", self.half_kana));
            }
            if self.spaces > 0 {
                out.push(t!("book.col.spaces", self.spaces));
            }
            if self.newlines > 0 {
                out.push(t!("book.col.newlines", self.newlines));
            }
        }
        // The distinct values.
        let distinct = self.values.len();
        if distinct == filled && filled > 1 {
            out.push(t!("book.col.unique"));
        } else if distinct <= MAX_LIST && distinct > 0 {
            let mut v: Vec<(usize, &Key)> = self.order.iter().enumerate().collect();
            v.sort_by(|a, b| self.values[b.1].cmp(&self.values[a.1]).then(a.0.cmp(&b.0)));
            let list: Vec<String> = v.iter().map(|(_, k)| t!("book.col.valueCount", show(k, date1904), self.values[*k])).collect();
            out.push(t!("book.col.values", distinct, list.join(&t!("vba.separator"))));
        } else {
            out.push(t!("book.col.distinct", distinct));
            if distinct * 10 >= filled * 9 {
                out.push(t!("book.col.dups", filled - distinct));
            }
        }
        // The number format most of the values have.
        if let Some((fmt, n)) = self.formats.iter().max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
            && *n * 2 >= filled
            && !fmt.eq_ignore_ascii_case("General")
            && !fmt.is_empty()
        {
            let mut s = t!("book.fmt.numFmt", code(fmt));
            let first = fmt.split(';').next().unwrap_or("");
            if first.len() >= 2 && first.bytes().all(|b| b == b'0') {
                s.push_str(&t!("book.col.zeroPad", first.len()));
            } else if first == "@" {
                s.push_str(&t!("book.col.textFmt"));
            }
            out.push(s);
        }
        out
    }

    fn formulas_text(&self) -> String {
        if self.formulas == self.rows { t!("book.col.allFormulas") } else { t!("book.col.formulas", self.formulas, pct(self.formulas, self.rows)) }
    }
}

fn overlaps(a: &Rect, b: &Rect) -> bool {
    a.c1 <= b.c2 && b.c1 <= a.c2 && a.r1 <= b.r2 && b.r1 <= a.r2
}

/// The heading of a table: an Excel table's name with its range, or the
/// range.
pub(super) fn title(f: &layout::Found) -> String {
    match &f.name {
        Some(n) => t!("book.tbl.table", n, f.rect.display()),
        None => f.rect.display(),
    }
}

/// Where the header and the data of a table are, and what told so (the
/// parts of a sentence). totals is the message for a row of totals.
pub(super) fn info(f: &layout::Found, totals: &str) -> Vec<String> {
    let mut info = Vec::new();
    match f.header {
        Some((a, z)) if a == z => info.push(t!("book.tbl.header", a)),
        Some((a, z)) => info.push(t!("book.tbl.headers", a, z)),
        None => info.push(t!("book.tbl.noHeader")),
    }
    if f.rows() == 0 {
        info.push(t!("book.tbl.noData", f.data.0));
    } else if f.rows() == 1 {
        info.push(t!("book.tbl.dataRow", f.data.0));
    } else {
        info.push(t!("book.tbl.data", f.data.0, f.data.1, f.rows()));
    }
    if let Some(r) = f.totals {
        info.push(t!(totals, r));
    }
    if let Some(k) = f.by.key() {
        info.push(t!("book.tbl.guess", t!(k)));
    }
    info
}

/// The groups of columns of a table ("「得意先」B〜C 列、..."); None
/// without any.
pub(super) fn groups(f: &layout::Found) -> Option<String> {
    let list: Vec<String> = f.groups.iter().map(|g| t!("book.tbl.group", short(&g.name), formula::col_name(g.c1), formula::col_name(g.c2))).collect();
    (!list.is_empty()).then(|| t!("book.tbl.groups", list.join(&t!("vba.separator"))))
}

/// The tables of a sheet and their columns as Markdown (a heading per
/// table); None when it has none with MIN_ROWS rows of data.
pub(super) fn sheet_text(wb: &Workbook, sh: &Sheet) -> Option<String> {
    let cx = Ctx { wb };
    let g = Grid::new(&sh.cells);
    let found: Vec<layout::Found> = layout::find(&cx, sh, &g).into_iter().filter(|f| f.rows() >= MIN_ROWS).collect();
    if found.is_empty() {
        return None;
    }
    let sep = t!("vba.separator");
    let mut b = String::new();
    if let Some(r) = sh.cut {
        b.push_str(&format!("{}\n\n", t!("book.columnsCut", r)));
    }
    for rg in found {
        b.push_str(&format!("### {}\n\n", title(&rg)));
        b.push_str(&format!("{}\n\n", t!("book.tbl.info", info(&rg, "book.tbl.totals").join(&sep))));
        if let Some(groups) = groups(&rg) {
            b.push_str(&format!("{groups}\n\n"));
        }
        let width = (rg.c2 - rg.c1 + 1) as usize;
        let mut stats: Vec<Stats> = (0..width).map(|_| Stats { rows: rg.rows() as usize, ..Default::default() }).collect();
        for row in g.rows_in(rg.data.0, rg.data.1) {
            let a = row.partition_point(|c| c.col < rg.c1);
            for c in row[a..].iter().take_while(|c| c.col <= rg.c2) {
                stats[(c.col - rg.c1) as usize].add(&cx, c);
            }
        }
        for (i, st) in stats.iter().enumerate() {
            let c = rg.c1 + i as u32;
            let name = &rg.names[i];
            if name.is_empty() && st.kinds.is_empty() && st.formulas == 0 {
                continue;
            }
            let mut parts = st.describe(wb.date1904);
            if sh.validations.iter().any(|v| v.rects.iter().any(|x| overlaps(x, &Rect { r1: rg.data.0, c1: c, r2: rg.data.1, c2: c }))) {
                parts.push(t!("book.col.validation"));
            }
            let col = formula::col_name(c);
            let line = if name.is_empty() { t!("book.col.lineNoName", col, parts.join(&sep)) } else { t!("book.col.line", col, name, parts.join(&sep)) };
            b.push_str(&format!("- {line}\n"));
        }
        b.push('\n');
    }
    Some(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert_eq!(decimals(1.25), 2);
        assert_eq!(decimals(0.1 + 0.2), 1);
        assert_eq!(decimals(100.0), 0);
        assert_eq!(decimals(-3.5), 1);
        assert_eq!(pattern("ab-12X"), "aa-99A");
        assert_eq!(pct(1, 3), "33.3%");
        assert_eq!(pct(5, 5), "100%");
        assert_eq!(pct(1, 10000), "<0.1%");
    }
}
