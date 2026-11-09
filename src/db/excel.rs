//! Excel-compatible SQL functions (ROUNDUP, LEFT, TEXT, EDATE, DATEDIF ...)
//! that every connection of xljs has, including the ones of repair().
//!
//! They are registered as deterministic and innocuous, so that generated
//! columns, views, indexes and triggers may use them although connections
//! run with trusted_schema=OFF. The names avoid SQLite's own functions
//! (also the math functions, which other tools enable): ROUNDHALF is
//! Excel's ROUND, because round() of SQLite gives other results
//! (round(2.675, 2) = 2.67, negative digits ignored).
//!
//! Like SQLite's date functions they return NULL instead of failing: for
//! NULL arguments and for values that are not numbers or dates (a failing
//! function would make an INSERT into a table with a generated column fail).
//! Dates are text ("2026-04-01", "2026/4/1", "2026-04-01 09:30:00", as
//! xljs stores Date values) or Excel serial numbers; the date functions
//! return text ("2026-04-01").

use std::sync::LazyLock;

use chrono::{Datelike, Months, NaiveDate, NaiveDateTime, Timelike};
use regex::Regex;
use rusqlite::Connection;
use rusqlite::functions::FunctionFlags;
use rusqlite::types::{Value, ValueRef};

use crate::api::numfmt;
use crate::api::xlsx::{date_format_code, from_serial, to_serial};

type Func = fn(&[ValueRef]) -> Option<Value>;

/// (name, smallest and largest number of arguments, function).
const FUNCTIONS: &[(&str, i32, i32, Func)] = &[
    ("ROUNDHALF", 1, 2, |a| round_fn(a, Mode::Half)),
    ("ROUNDUP", 1, 2, |a| round_fn(a, Mode::Up)),
    ("ROUNDDOWN", 1, 2, |a| round_fn(a, Mode::Down)),
    ("INT", 1, 1, |a| Some(int_value(sig15(num(a[0])?).floor()))),
    ("QUOTIENT", 2, 2, quotient),
    ("LEFT", 1, 2, left),
    ("RIGHT", 1, 2, right),
    ("MID", 3, 3, mid),
    ("LEN", 1, 1, |a| Some(Value::Integer(text(a[0])?.chars().count() as i64))),
    ("FIND", 2, 3, find),
    ("TEXT", 2, 2, text_fn),
    ("YEAR", 1, 1, |a| Some(Value::Integer(date(a[0])?.year() as i64))),
    ("MONTH", 1, 1, |a| Some(Value::Integer(date(a[0])?.month() as i64))),
    ("DAY", 1, 1, |a| Some(Value::Integer(date(a[0])?.day() as i64))),
    ("WEEKDAY", 1, 2, weekday),
    ("EDATE", 2, 2, edate),
    ("EOMONTH", 2, 2, eomonth),
    ("DATEDIF", 3, 3, datedif),
];

/// The names of the functions (for messages and tests).
pub fn names() -> impl Iterator<Item = &'static str> {
    FUNCTIONS.iter().map(|f| f.0)
}

/// Registers the functions on a connection.
pub(crate) fn register(conn: &Connection) -> rusqlite::Result<()> {
    let flags = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC | FunctionFlags::SQLITE_INNOCUOUS;
    for &(name, min, max, f) in FUNCTIONS {
        for n in min..=max {
            conn.create_scalar_function(name, n, flags, move |ctx| {
                let args: Vec<ValueRef> = (0..ctx.len()).map(|i| ctx.get_raw(i)).collect();
                Ok(f(&args).unwrap_or(Value::Null))
            })?;
        }
    }
    Ok(())
}

// ---- arguments

/// A number: integers, reals and text that reads as a number.
fn num(v: ValueRef) -> Option<f64> {
    match v {
        ValueRef::Integer(i) => Some(i as f64),
        ValueRef::Real(f) => Some(f),
        ValueRef::Text(t) => std::str::from_utf8(t).ok()?.trim().parse::<f64>().ok().filter(|f| f.is_finite()),
        _ => None,
    }
}

/// An argument that counts (digits, lengths): truncated like Excel does.
fn count(v: Option<&ValueRef>, def: i64) -> Option<i64> {
    match v {
        None => Some(def),
        Some(v) => Some(num(*v)?.trunc() as i64),
    }
}

/// Text: numbers as Excel shows them without a format (2 not 2.0).
fn text(v: ValueRef) -> Option<String> {
    match v {
        ValueRef::Text(t) => Some(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Integer(i) => Some(i.to_string()),
        ValueRef::Real(f) => Some(numfmt::general(f)),
        _ => None,
    }
}

static DATE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d{4})[-/](\d{1,2})[-/](\d{1,2})(?:[ T](\d{1,2}):(\d{2})(?::(\d{2})(?:\.(\d{1,9}))?)?)?$").unwrap());

/// A date and time: text, or an Excel serial number (also as text).
fn date(v: ValueRef) -> Option<NaiveDateTime> {
    if let ValueRef::Text(t) = v {
        let s = std::str::from_utf8(t).ok()?.trim();
        if let Some(c) = DATE_RE.captures(s) {
            let n = |i: usize| c.get(i).map(|m| m.as_str().parse::<u32>().unwrap_or(0)).unwrap_or(0);
            let ms = c.get(7).map(|m| format!("{:0<3}", &m.as_str()[..m.as_str().len().min(3)]).parse::<u32>().unwrap_or(0)).unwrap_or(0);
            return NaiveDate::from_ymd_opt(n(1) as i32, n(2), n(3))?.and_hms_milli_opt(n(4), n(5), n(6), ms);
        }
    }
    let f = num(v)?;
    if f < 1.0 {
        return None;
    }
    let (y, mo, d, h, mi, s, ms) = from_serial(f, false)?;
    NaiveDate::from_ymd_opt(y, mo, d)?.and_hms_milli_opt(h, mi, s, ms)
}

fn date_text(d: NaiveDate) -> Value {
    Value::Text(d.format("%Y-%m-%d").to_string())
}

// ---- numbers

/// Rounds to 15 significant digits, as Excel does before it rounds, so
/// that ROUNDUP(0.1 + 0.2, 1) is 0.3 and ROUNDHALF(1.005, 2) is 1.01.
fn sig15(x: f64) -> f64 {
    if x == 0.0 || !x.is_finite() {
        return x;
    }
    format!("{x:.14e}").parse().unwrap_or(x)
}

/// An integer when it is one (and exact), a real otherwise.
fn int_value(f: f64) -> Value {
    if f.fract() == 0.0 && f.abs() < 9_007_199_254_740_992.0 { Value::Integer(f as i64) } else { Value::Real(f) }
}

#[derive(Clone, Copy)]
enum Mode {
    /// Half away from zero (Excel's ROUND).
    Half,
    /// Away from zero.
    Up,
    /// Toward zero.
    Down,
}

fn round(x: f64, digits: i64, mode: Mode) -> f64 {
    let d = digits.clamp(-15, 15) as i32;
    let m = 10f64.powi(d.abs());
    let x = sig15(x);
    let s = sig15(if d >= 0 { x * m } else { x / m });
    let r = match mode {
        Mode::Half => (s.abs() + 0.5).floor(),
        Mode::Up => s.abs().ceil(),
        Mode::Down => s.abs().floor(),
    } * s.signum();
    let r = if d >= 0 { r / m } else { r * m };
    if r == 0.0 { 0.0 } else { r }
}

fn round_fn(a: &[ValueRef], mode: Mode) -> Option<Value> {
    let x = num(a[0])?;
    let d = count(a.get(1), 0)?;
    let r = round(x, d, mode);
    Some(if d <= 0 { int_value(r) } else { Value::Real(r) })
}

fn quotient(a: &[ValueRef]) -> Option<Value> {
    let (x, y) = (num(a[0])?, num(a[1])?);
    if y == 0.0 {
        return None;
    }
    Some(int_value(sig15(x / y).trunc()))
}

// ---- text

fn left(a: &[ValueRef]) -> Option<Value> {
    let s = text(a[0])?;
    let n = count(a.get(1), 1)?;
    if n < 0 {
        return None;
    }
    Some(Value::Text(s.chars().take(n as usize).collect()))
}

fn right(a: &[ValueRef]) -> Option<Value> {
    let s: Vec<char> = text(a[0])?.chars().collect();
    let n = count(a.get(1), 1)?;
    if n < 0 {
        return None;
    }
    Some(Value::Text(s[s.len().saturating_sub(n as usize)..].iter().collect()))
}

fn mid(a: &[ValueRef]) -> Option<Value> {
    let s = text(a[0])?;
    let (start, n) = (count(a.get(1), 1)?, count(a.get(2), 0)?);
    if start < 1 || n < 0 {
        return None;
    }
    Some(Value::Text(s.chars().skip(start as usize - 1).take(n as usize).collect()))
}

/// FIND(find, within, start): the position (from 1) of find in within, case
/// sensitive; NULL when it is not there.
fn find(a: &[ValueRef]) -> Option<Value> {
    let needle: Vec<char> = text(a[0])?.chars().collect();
    let hay: Vec<char> = text(a[1])?.chars().collect();
    let start = count(a.get(2), 1)?;
    if start < 1 || start as usize > hay.len() + 1 {
        return None;
    }
    let from = start as usize - 1;
    if needle.is_empty() {
        return Some(Value::Integer(start));
    }
    (from..=hay.len().saturating_sub(needle.len())).find(|&i| hay[i..].starts_with(&needle)).map(|i| Value::Integer(i as i64 + 1))
}

/// TEXT(value, format): the value formatted like a cell with the number
/// format. Dates (text) go with date formats as dates and with other
/// formats as serial numbers; other text stays as it is.
fn text_fn(a: &[ValueRef]) -> Option<Value> {
    let code = text(a[1])?;
    if matches!(a[0], ValueRef::Null) {
        return None;
    }
    let as_date = match a[0] {
        ValueRef::Text(_) => date(a[0]).filter(|_| num(a[0]).is_none()),
        _ => None,
    };
    let out = if date_format_code(&code) {
        let d = as_date.or_else(|| date(a[0]));
        match d {
            Some(d) => numfmt::format_date((d.year(), d.month(), d.day(), d.hour(), d.minute(), d.second(), d.nanosecond() / 1_000_000), &code),
            None => text(a[0])?,
        }
    } else if let Some(d) = as_date {
        numfmt::format_number(to_serial(d.year(), d.month(), d.day(), d.hour(), d.minute(), d.second(), d.nanosecond() / 1_000_000, false), &code)
    } else {
        match num(a[0]) {
            Some(f) => numfmt::format_number(f, &code),
            None => text(a[0])?,
        }
    };
    Some(Value::Text(out))
}

// ---- dates

/// WEEKDAY(date, type): 1 Sunday = 1 (the default), 2 Monday = 1,
/// 3 Monday = 0, 11 to 17 Monday to Sunday = 1.
fn weekday(a: &[ValueRef]) -> Option<Value> {
    let d = date(a[0])?;
    let from_sun = d.weekday().num_days_from_sunday() as i64; // Sunday 0
    let from_mon = d.weekday().num_days_from_monday() as i64; // Monday 0
    let r = match count(a.get(1), 1)? {
        1 => from_sun + 1,
        2 | 11 => from_mon + 1,
        3 => from_mon,
        t @ 12..=17 => (from_mon - (t - 11)).rem_euclid(7) + 1,
        _ => return None,
    };
    Some(Value::Integer(r))
}

fn add_months(d: NaiveDate, months: i64) -> Option<NaiveDate> {
    if months >= 0 { d.checked_add_months(Months::new(months.try_into().ok()?)) } else { d.checked_sub_months(Months::new((-months).try_into().ok()?)) }
}

/// The last day of the month of d.
fn month_end(d: NaiveDate) -> Option<NaiveDate> {
    add_months(d.with_day(1)?, 1)?.pred_opt()
}

fn edate(a: &[ValueRef]) -> Option<Value> {
    let d = date(a[0])?.date();
    Some(date_text(add_months(d, count(a.get(1), 0)?)?))
}

fn eomonth(a: &[ValueRef]) -> Option<Value> {
    let d = date(a[0])?.date();
    Some(date_text(month_end(add_months(d.with_day(1)?, count(a.get(1), 0)?)?)?))
}

/// DATE(y, m, d) of Excel: months and days beyond the end carry over.
fn excel_date(y: i32, m: i64, d: i64) -> Option<NaiveDate> {
    let first = add_months(NaiveDate::from_ymd_opt(y, 1, 1)?, m - 1)?;
    first.checked_add_signed(chrono::Duration::days(d - 1))
}

/// DATEDIF(start, end, unit) with the units Y, M, D, YM, YD and MD.
fn datedif(a: &[ValueRef]) -> Option<Value> {
    let (s, e) = (date(a[0])?.date(), date(a[1])?.date());
    let unit = text(a[2])?.trim().to_ascii_uppercase();
    if s > e {
        return None;
    }
    let months = (e.year() - s.year()) as i64 * 12 + e.month() as i64 - s.month() as i64 - (e.day() < s.day()) as i64;
    let r = match unit.as_str() {
        "Y" => months / 12,
        "M" => months,
        "D" => (e - s).num_days(),
        "YM" => months % 12,
        "YD" => {
            let mut start = excel_date(e.year(), s.month() as i64, s.day() as i64)?;
            if start > e {
                start = excel_date(e.year() - 1, s.month() as i64, s.day() as i64)?;
            }
            (e - start).num_days()
        }
        "MD" => {
            if e.day() >= s.day() {
                (e.day() - s.day()) as i64
            } else {
                (e - excel_date(e.year(), e.month() as i64 - 1, s.day() as i64)?).num_days()
            }
        }
        _ => return None,
    };
    Some(Value::Integer(r))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        register(&c).unwrap();
        c
    }

    fn q(c: &Connection, expr: &str) -> Value {
        c.query_row(&format!("SELECT {expr}"), [], |r| r.get::<_, Value>(0)).unwrap()
    }

    fn t(s: &str) -> Value {
        Value::Text(s.into())
    }

    #[test]
    fn numbers() {
        let c = conn();
        let cases: &[(&str, Value)] = &[
            ("ROUNDHALF(2.675, 2)", Value::Real(2.68)),
            ("ROUNDHALF(1.005, 2)", Value::Real(1.01)),
            ("ROUNDHALF(-2.5)", Value::Integer(-3)),
            ("ROUNDHALF(1234.5, -2)", Value::Integer(1200)),
            ("ROUNDHALF('12.5')", Value::Integer(13)),
            ("ROUNDUP(0.1 + 0.2, 1)", Value::Real(0.3)),
            ("ROUNDUP(-1.21, 1)", Value::Real(-1.3)),
            ("ROUNDUP(1201, -2)", Value::Integer(1300)),
            ("ROUNDDOWN(7.9)", Value::Integer(7)),
            ("ROUNDDOWN(-7.9)", Value::Integer(-7)),
            ("ROUNDDOWN(3.14159, 3)", Value::Real(3.141)),
            ("ROUNDDOWN(1980 * 1.1)", Value::Integer(2178)),
            ("INT(-7.5)", Value::Integer(-8)),
            ("INT((0.1 + 0.7) * 10)", Value::Integer(8)),
            ("QUOTIENT(-7, 2)", Value::Integer(-3)),
            ("QUOTIENT(1, 0)", Value::Null),
            ("ROUNDDOWN(NULL)", Value::Null),
            ("ROUNDDOWN('abc')", Value::Null),
        ];
        for (e, want) in cases {
            assert_eq!(&q(&c, e), want, "{e}");
        }
    }

    #[test]
    fn texts() {
        let c = conn();
        let cases: &[(&str, Value)] = &[
            ("LEFT('東京都千代田区', 3)", t("東京都")),
            ("LEFT('abc')", t("a")),
            ("RIGHT('abc', 5)", t("abc")),
            ("RIGHT(12345, 2)", t("45")),
            ("MID('2026年4月1日', 6, 2)", t("4月")),
            ("MID('abc', 0, 1)", Value::Null),
            ("LEN('ねじ M5')", Value::Integer(5)),
            ("LEN(2.0)", Value::Integer(1)),
            ("FIND('-', 'A-12-3')", Value::Integer(2)),
            ("FIND('-', 'A-12-3', 3)", Value::Integer(5)),
            ("FIND('a', 'ABC')", Value::Null),
            ("FIND('', 'abc')", Value::Integer(1)),
            ("TEXT(1234567.891, '#,##0')", t("1,234,568")),
            ("TEXT(0.256, '0.0%')", t("25.6%")),
            ("TEXT('2026-04-01', 'yyyy\"年\"m\"月\"d\"日\"(aaa)')", t("2026年4月1日(水)")),
            ("TEXT('2026/4/1 9:05', 'm/d h:mm')", t("4/1 9:05")),
            ("TEXT(46113, 'yyyy/mm/dd')", t("2026/04/01")),
            ("TEXT('2026-04-01', '0')", t("46113")),
            ("TEXT('ねじ', '0')", t("ねじ")),
        ];
        for (e, want) in cases {
            assert_eq!(&q(&c, e), want, "{e}");
        }
    }

    #[test]
    fn dates() {
        let c = conn();
        let cases: &[(&str, Value)] = &[
            ("YEAR('2026-04-01')", Value::Integer(2026)),
            ("MONTH('2026-04-01 09:30:00')", Value::Integer(4)),
            ("DAY(46113)", Value::Integer(1)),
            ("YEAR('2026-02-30')", Value::Null),
            ("YEAR('abc')", Value::Null),
            ("WEEKDAY('2026-04-01')", Value::Integer(4)),
            ("WEEKDAY('2026-04-01', 2)", Value::Integer(3)),
            ("WEEKDAY('2026-04-01', 3)", Value::Integer(2)),
            ("WEEKDAY('2026-04-01', 13)", Value::Integer(1)),
            ("WEEKDAY('2026-04-05', 17)", Value::Integer(1)),
            ("WEEKDAY('2026-04-01', 9)", Value::Null),
            ("EDATE('2026-01-31', 1)", t("2026-02-28")),
            ("EDATE('2026-03-31', -1)", t("2026-02-28")),
            ("EDATE('2026-04-01 09:30:00', 12)", t("2027-04-01")),
            ("EOMONTH('2026-04-15', 0)", t("2026-04-30")),
            ("EOMONTH('2026-04-15', -2)", t("2026-02-28")),
            ("EOMONTH('2024-01-31', 1)", t("2024-02-29")),
            ("DATEDIF('2000-04-02', '2026-04-01', 'Y')", Value::Integer(25)),
            ("DATEDIF('2000-04-01', '2026-04-01', 'Y')", Value::Integer(26)),
            ("DATEDIF('2026-01-31', '2026-03-30', 'M')", Value::Integer(1)),
            ("DATEDIF('2026-01-01', '2026-03-01', 'D')", Value::Integer(59)),
            ("DATEDIF('2020-05-10', '2026-04-01', 'ym')", Value::Integer(10)),
            ("DATEDIF('2020-05-10', '2026-04-01', 'YD')", Value::Integer(326)),
            ("DATEDIF('2026-01-10', '2026-04-05', 'MD')", Value::Integer(26)),
            ("DATEDIF('2026-04-02', '2026-04-01', 'D')", Value::Null),
            ("DATEDIF('2026-01-01', '2026-04-01', 'X')", Value::Null),
        ];
        for (e, want) in cases {
            assert_eq!(&q(&c, e), want, "{e}");
        }
    }

    #[test]
    fn names_are_free() {
        // None of the names may shadow a function of SQLite.
        let c = Connection::open_in_memory().unwrap();
        for n in names() {
            let used: bool = c.query_row("SELECT count(*) > 0 FROM pragma_function_list WHERE name = lower(?)", [n], |r| r.get(0)).unwrap();
            assert!(!used, "{n} is a function of SQLite");
        }
    }

    #[test]
    fn usable_in_the_schema() {
        let c = conn();
        c.execute_batch(
            "PRAGMA trusted_schema=OFF;
             CREATE TABLE t(price REAL, sold DATE,
               tax INTEGER AS (ROUNDDOWN(price * 0.1)) STORED,
               due DATE AS (EOMONTH(sold, 1)) VIRTUAL);
             CREATE INDEX t_month ON t(MONTH(sold));
             CREATE VIEW v AS SELECT TEXT(sold, 'm/d') AS d FROM t;
             INSERT INTO t(price, sold) VALUES(1980, '2026-04-15');",
        )
        .unwrap();
        let r: (i64, String, String) = c.query_row("SELECT tax, due, (SELECT d FROM v) FROM t", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
        assert_eq!(r, (198, "2026-05-31".into(), "4/15".into()));
    }
}
