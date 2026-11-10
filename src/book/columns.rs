//! The columns of the tables of a sheet with many rows (MIN_ROWS of data
//! or more), for making input rules: for each column its name and what its
//! values are like (their kinds, blanks, ranges, digits, lengths, leading
//! zeros, patterns, the values of a short list, duplicates).
//!
//! The tables are the Excel tables (ListObjects) of the sheet and the
//! blocks of cells around which the rows and columns are empty (as
//! Excel's CurrentRegion), with the header row found from the frozen
//! panes, the filter or the cells (a row of text only).

use std::collections::{HashMap, HashSet};

use super::formula::{self, Rect};
use crate::xml::Node;
use super::{Sheet, Workbook, code};
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
/// The rows searched for the header from the top of a block.
const HEADER_SEARCH: u32 = 10;

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
struct Grid<'a> {
    cells: &'a [Cell],
    /// (row, first cell, end).
    rows: Vec<(u32, usize, usize)>,
}

impl<'a> Grid<'a> {
    fn new(cells: &'a [Cell]) -> Grid<'a> {
        let mut rows: Vec<(u32, usize, usize)> = Vec::new();
        for (i, c) in cells.iter().enumerate() {
            match rows.last_mut() {
                Some(r) if r.0 == c.row => r.2 = i + 1,
                _ => rows.push((c.row, i, i + 1)),
            }
        }
        Grid { cells, rows }
    }

    fn row(&self, r: u32) -> &'a [Cell] {
        match self.rows.binary_search_by_key(&r, |x| x.0) {
            Ok(i) => &self.cells[self.rows[i].1..self.rows[i].2],
            Err(_) => &[],
        }
    }

    /// The cells of the row in the columns c1..=c2.
    fn row_in(&self, r: u32, c1: u32, c2: u32) -> &'a [Cell] {
        let row = self.row(r);
        let a = row.partition_point(|c| c.col < c1);
        let b = row.partition_point(|c| c.col <= c2);
        &row[a..b]
    }

    /// The cells of the rows r1..=r2.
    fn rows_in(&self, r1: u32, r2: u32) -> impl Iterator<Item = &'a [Cell]> + '_ {
        let start = self.rows.partition_point(|x| x.0 < r1);
        self.rows[start..].iter().take_while(move |x| x.0 <= r2).map(|x| &self.cells[x.1..x.2])
    }
}

/// What a cell holds, for the statistics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Kind {
    Num,
    Date,
    Time,
    Str,
    Bool,
    Err,
}

/// A distinct value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Key {
    Num(u64),
    Date(u64),
    Time(u64),
    Str(String),
    Bool(bool),
    Err(String),
}

struct Ctx<'a> {
    wb: &'a Workbook,
}

impl Ctx<'_> {
    fn text<'b>(&'b self, v: &'b Val) -> Option<&'b str> {
        match v {
            Val::Shared(i) => Some(self.wb.strings.get(*i as usize).map(String::as_str).unwrap_or("")),
            Val::Text(s) => Some(s),
            _ => None,
        }
    }

    fn format(&self, style: u32) -> (u32, &str) {
        self.wb.formats.get(style as usize).map(|(id, c)| (*id, c.as_str())).unwrap_or((0, "General"))
    }

    fn is_date(&self, style: u32) -> bool {
        let (id, code) = self.format(style);
        builtin_date_fmt(id) || date_format_code(code)
    }

    /// A cell's value as a key; None for a blank (an empty text too).
    fn key(&self, c: &Cell) -> Option<Key> {
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
    fn header(&self, c: &Cell) -> String {
        match self.key(c) {
            Some(Key::Str(s)) => one_line(&s),
            Some(k) => show(&k, self.wb.date1904),
            None => String::new(),
        }
    }
}

fn kind(k: &Key) -> Kind {
    match k {
        Key::Num(_) => Kind::Num,
        Key::Date(_) => Kind::Date,
        Key::Time(_) => Kind::Time,
        Key::Str(_) => Kind::Str,
        Key::Bool(_) => Kind::Bool,
        Key::Err(_) => Kind::Err,
    }
}

fn one_line(s: &str) -> String {
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

/// A table found on a sheet.
struct Region {
    /// The heading (the table's name or the range).
    title: String,
    c1: u32,
    c2: u32,
    /// The rows of the header (first, last).
    header: Option<(u32, u32)>,
    data: (u32, u32),
    /// The names of the columns (an Excel table's).
    names: Vec<String>,
    /// A total row left out after the data.
    totals: Option<u32>,
    /// What the header was found from.
    hint: Option<&'static str>,
}

/// Whether a row looks like a header: text only (no formulas) in at least
/// half of the columns (two of them when there are more).
fn header_like(cx: &Ctx, g: &Grid, r: u32, c1: u32, c2: u32) -> bool {
    let cells = g.row_in(r, c1, c2);
    let width = (c2 - c1 + 1) as usize;
    let need = width.div_ceil(2).max(if width > 1 { 2 } else { 1 });
    let mut n = 0;
    for c in cells {
        match cx.key(c) {
            None => {}
            Some(Key::Str(_)) if !c.formula => n += 1,
            Some(_) => return false,
        }
    }
    n >= need
}

/// Whether the row ends the data with totals: a label (合計, Total) or a
/// sum over the rows above (a SUM or SUBTOTAL formula not filled from the
/// row above).
fn totals_row(cx: &Ctx, sh: &Sheet, g: &Grid, r: u32, c1: u32, c2: u32) -> bool {
    const WORDS: [&str; 7] = ["合計", "総計", "小計", "計", "total", "grand total", "sum"];
    for c in g.row_in(r, c1, c2) {
        if !c.formula
            && let Some(Key::Str(s)) = cx.key(c)
        {
            let s: String = s.chars().filter(|c| !c.is_whitespace() && *c != '\u{3000}').collect::<String>().to_lowercase();
            if WORDS.iter().any(|w| w.replace(' ', "") == s) {
                return true;
            }
        }
    }
    let inside = |x: &Rect, c: u32, r: u32| x.c1 <= c && c <= x.c2 && x.r1 <= r && r <= x.r2;
    for f in sh.formulas.iter().filter(|f| !f.array) {
        let up = f.formula.to_uppercase();
        if !(up.contains("SUM(") || up.contains("SUBTOTAL(") || up.contains("AGGREGATE(")) {
            continue;
        }
        for c in c1..=c2 {
            if f.rects.iter().any(|x| inside(x, c, r)) && !f.rects.iter().any(|x| inside(x, c, r - 1)) {
                return true;
            }
        }
    }
    false
}

fn find(p: &mut [usize], mut i: usize) -> usize {
    while p[i] != i {
        p[i] = p[p[i]];
        i = p[i];
    }
    i
}

fn overlaps(a: &Rect, b: &Rect) -> bool {
    a.c1 <= b.c2 && b.c1 <= a.c2 && a.r1 <= b.r2 && b.r1 <= a.r2
}

/// The blocks of cells with MIN_ROWS rows or more outside the tables: the
/// cells joined to their neighbours (diagonal ones too), and the blocks
/// that overlap as one.
fn blocks(g: &Grid, tables: &[Rect]) -> Vec<Rect> {
    let cells = g.cells;
    let free = |c: &Cell| !tables.iter().any(|t| t.c1 <= c.col && c.col <= t.c2 && t.r1 <= c.row && c.row <= t.r2);
    let mut parent: Vec<usize> = (0..cells.len()).collect();
    let union = |p: &mut Vec<usize>, a: usize, b: usize| {
        let (x, y) = (find(p, a), find(p, b));
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
        let root = find(&mut parent, i);
        let r = rects.entry(root).or_insert(Rect::cell(c.col, c.row));
        r.c1 = r.c1.min(c.col);
        r.c2 = r.c2.max(c.col);
        r.r1 = r.r1.min(c.row);
        r.r2 = r.r2.max(c.row);
    }
    let mut all: Vec<Rect> = rects.into_values().collect();
    all.sort();
    // The big ones take in what overlaps them until nothing does.
    let (mut big, mut small): (Vec<Rect>, Vec<Rect>) = all.into_iter().partition(|r| r.r2 - r.r1 + 1 >= MIN_ROWS);
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

fn union_rect(a: &Rect, b: &Rect) -> Rect {
    Rect { r1: a.r1.min(b.r1), c1: a.c1.min(b.c1), r2: a.r2.max(b.r2), c2: a.c2.max(b.c2) }
}

/// The tables of the sheet with MIN_ROWS rows of data or more.
fn regions(cx: &Ctx, sh: &Sheet, g: &Grid) -> Vec<Region> {
    let mut out = Vec::new();
    for tb in &sh.tables {
        let r1 = tb.rect.r1 + tb.header_rows;
        let Some(r2) = tb.rect.r2.checked_sub(tb.totals_rows) else { continue };
        if r2 < r1 || r2 - r1 + 1 < MIN_ROWS {
            continue;
        }
        out.push(Region {
            title: t!("book.tbl.table", tb.name, tb.rect.display()),
            c1: tb.rect.c1,
            c2: tb.rect.c2,
            header: (tb.header_rows > 0).then_some((tb.rect.r1, r1 - 1)),
            data: (r1, r2),
            names: tb.columns.clone(),
            totals: (tb.totals_rows > 0).then_some(r2 + 1),
            hint: None,
        });
    }
    let tables: Vec<Rect> = sh.tables.iter().map(|t| t.rect).collect();
    for b in blocks(g, &tables) {
        let (c1, c2) = (b.c1, b.c2);
        // The last row of the header: the frozen rows, the filter's top
        // row or the first row of text near the top.
        let mut found = None;
        if let Some(y) = sh.frozen
            && b.r1 <= y
            && y < b.r2
        {
            found = Some((y, "book.tbl.byFrozen"));
        }
        if found.is_none()
            && let Some(f) = sh.filter
            && b.r1 <= f.r1
            && f.r1 < b.r2
            && f.c1 <= c2
            && c1 <= f.c2
        {
            found = Some((f.r1, "book.tbl.byFilter"));
        }
        let header = match found {
            Some((last, hint)) => {
                // Rows of text right above it are part of the header.
                let mut first = last;
                while first > b.r1 && header_like(cx, g, first - 1, c1, c2) && header_like(cx, g, first, c1, c2) {
                    first -= 1;
                }
                Some((first, last, hint))
            }
            None => (b.r1..=b.r2.min(b.r1 + HEADER_SEARCH - 1)).find(|r| header_like(cx, g, *r, c1, c2)).map(|r| (r, r, "book.tbl.byLayout")),
        };
        let r1 = header.map_or(b.r1, |h| h.1 + 1);
        let mut r2 = b.r2;
        let mut totals = None;
        if r2 > r1 && totals_row(cx, sh, g, r2, c1, c2) {
            totals = Some(r2);
            r2 -= 1;
        }
        if r2 < r1 || r2 - r1 + 1 < MIN_ROWS {
            continue;
        }
        out.push(Region {
            title: Rect { r1: header.map_or(r1, |h| h.0), c1, r2: b.r2, c2 }.display(),
            c1,
            c2,
            header: header.map(|h| (h.0, h.1)),
            data: (r1, r2),
            names: Vec::new(),
            totals,
            hint: Some(header.map_or("book.tbl.byLayout", |h| h.2)),
        });
    }
    out.sort_by_key(|r| (r.data.0, r.c1));
    out
}

/// The tables of a sheet and their columns as Markdown (a heading per
/// table); None when it has none with MIN_ROWS rows of data.
pub(super) fn sheet_text(wb: &Workbook, sh: &Sheet) -> Option<String> {
    let cx = Ctx { wb };
    let g = Grid::new(&sh.cells);
    let regions = regions(&cx, sh, &g);
    if regions.is_empty() {
        return None;
    }
    let sep = t!("vba.separator");
    let mut b = String::new();
    if let Some(r) = sh.cut {
        b.push_str(&format!("{}\n\n", t!("book.columnsCut", r)));
    }
    for rg in regions {
        b.push_str(&format!("### {}\n\n", rg.title));
        let mut info = Vec::new();
        match rg.header {
            Some((a, z)) if a == z => info.push(t!("book.tbl.header", a)),
            Some((a, z)) => info.push(t!("book.tbl.headers", a, z)),
            None => info.push(t!("book.tbl.noHeader")),
        }
        info.push(t!("book.tbl.data", rg.data.0, rg.data.1, rg.data.1 - rg.data.0 + 1));
        if let Some(r) = rg.totals {
            info.push(t!("book.tbl.totals", r));
        }
        if let Some(h) = rg.hint {
            info.push(t!("book.tbl.guess", t!(h)));
        }
        b.push_str(&format!("{}\n\n", t!("book.tbl.info", info.join(&sep))));
        let width = (rg.c2 - rg.c1 + 1) as usize;
        let mut stats: Vec<Stats> = (0..width).map(|_| Stats { rows: (rg.data.1 - rg.data.0 + 1) as usize, ..Default::default() }).collect();
        for row in g.rows_in(rg.data.0, rg.data.1) {
            let a = row.partition_point(|c| c.col < rg.c1);
            for c in row[a..].iter().take_while(|c| c.col <= rg.c2) {
                stats[(c.col - rg.c1) as usize].add(&cx, c);
            }
        }
        let names: Vec<String> = (rg.c1..=rg.c2)
            .map(|c| {
                if let Some(n) = rg.names.get((c - rg.c1) as usize) {
                    return one_line(n);
                }
                let Some((h1, h2)) = rg.header else { return String::new() };
                let mut parts: Vec<String> = Vec::new();
                for r in h1..=h2 {
                    if let Some(cell) = g.row_in(r, c, c).first() {
                        let s = cx.header(cell);
                        if !s.is_empty() && parts.last() != Some(&s) {
                            parts.push(s);
                        }
                    }
                }
                parts.join(" / ")
            })
            .collect();
        let mut seen: HashSet<u32> = HashSet::new();
        for (i, st) in stats.iter().enumerate() {
            let c = rg.c1 + i as u32;
            let name = &names[i];
            if name.is_empty() && st.kinds.is_empty() && st.formulas == 0 {
                continue;
            }
            seen.insert(c);
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
