//! Workbooks (xlsx) on top of umya-spreadsheet.

use std::io::Cursor;

use rquickjs::class::Trace;
use rquickjs::{Class, Ctx, JsLifetime, Value};
use umya_spreadsheet::{CellRawValue, NumberingFormat, Workbook, Worksheet};

use super::js::{self, elems, nullish};
use super::xlsx_merge::{Merges, anchor_of};
use super::{A, Host, N, R};
use crate::{Error, Result, err, i18n};

/// The handle of an open workbook in JavaScript.
#[derive(Trace, JsLifetime, Clone)]
#[rquickjs::class(rename = "Workbook")]
pub struct Handle {
    #[qjs(skip_trace)]
    pub id: u64,
}

/// An open workbook.
pub struct Book {
    pub wb: Workbook,
    pub path: String,
    pub date1904: bool,
    /// A workbook from create() whose default sheet has not been used yet;
    /// the first addSheet renames it.
    pub fresh: bool,
    pub closed: bool,
    /// The font of the Normal style (name, size).
    pub default_font: (String, f64),
    /// The parts of the file with its charts (None for a new workbook).
    pub charts: Option<super::xlsx_chart::ChartParts>,
    /// The workbook changed where charts are (Book::parts_changed).
    pub parts_stale: bool,
    /// The workbook's own table styles (tableStyles in styles.xml) by name.
    pub table_styles: std::collections::HashMap<String, super::xlsx_table::StyleDef>,
    /// The pictures placed in cells (empty for a new workbook).
    pub cell_images: super::xlsx_cellimage::CellImages,
    /// The formula engine's copy of the workbook (None until something is
    /// calculated; xlsx_calc.rs).
    pub(crate) calc: std::cell::RefCell<Option<Box<super::xlsx_calc::Engine>>>,
    /// The cells written since the engine last calculated.
    pub(crate) touched: std::cell::RefCell<super::xlsx_calc::Touched>,
    /// The sheets, names and tables the formulas scripts write and read
    /// are checked against (None until one is).
    pub(crate) shape: std::cell::RefCell<Option<Box<super::xlsx_calc::Shape>>>,
    /// The areas the array formulas fill, by sheet name (read from the
    /// sheet the first time a script writes into it).
    pub(crate) areas: std::collections::HashMap<String, super::xlsx_calc::Areas>,
}

pub(crate) fn zerr(e: impl std::fmt::Display) -> Error {
    let s = e.to_string();
    if s.contains("InvalidArchive") || s.contains("invalid Zip archive") || s.contains("Could not find EOCD") || s.contains("zip") {
        return Error::msg("zip: not a valid zip file");
    }
    Error::msg(s)
}

impl Book {
    fn new(wb: Workbook, path: String) -> Book {
        let default_font = wb.default_font().unwrap_or(("Calibri".into(), 11.0));
        let date1904 = wb.date1904();
        let table_styles = super::xlsx_table::custom_styles(&wb);
        Book { wb, path, date1904, fresh: false, closed: false, default_font, charts: None, parts_stale: false, table_styles, cell_images: Default::default(), calc: Default::default(), touched: Default::default(), shape: Default::default(), areas: Default::default() }
    }

    pub fn sheet(&self, name: &str) -> Result<&Worksheet> {
        self.wb.sheet_by_name(name).map_err(|_| Error::msg(format!("sheet {name} does not exist")))
    }

    /// A sheet to change in any way. The formula engine cannot follow what
    /// is done with it: everything is calculated again the next time. Changes
    /// it need not know of go through layout_mut, the content of one cell
    /// through cell_mut.
    pub fn sheet_mut(&mut self, name: &str) -> Result<&mut Worksheet> {
        self.calc_reset();
        self.layout_mut(name)
    }

    /// A sheet to change what no formula depends on: styles, sizes, panes.
    pub fn layout_mut(&mut self, name: &str) -> Result<&mut Worksheet> {
        if self.fresh && name == "Sheet1" {
            self.fresh = false;
        }
        self.wb.sheet_by_name_mut(name).map_err(|_| Error::msg(format!("sheet {name} does not exist")))
    }

    /// A cell to change the value or the formula of. The area of an array
    /// formula the cell is in is given up (xlsx_calc::release_area), and so
    /// is the picture placed in the cell (xlsx_cellimage).
    pub fn cell_mut(&mut self, sheet: &str, col: u32, row: u32) -> Result<&mut umya_spreadsheet::Cell> {
        super::xlsx_calc::release_area(self, sheet, col, row);
        self.touched.get_mut().cell(sheet, col, row);
        let c = self.layout_mut(sheet)?.cell_mut((col, row));
        c.set_value_meta_index(0);
        Ok(c)
    }

    /// The sheets changed (added, removed, renamed) or something else the
    /// formula engine does not follow cell by cell.
    pub fn calc_reset(&mut self) {
        *self.calc.get_mut() = None;
        *self.shape.get_mut() = None;
        self.areas.clear();
        self.touched.get_mut().everything();
    }

    /// The drawings and charts moved (rows and columns inserted or deleted,
    /// sheets copied or renamed): the parts read for them (charts) are read
    /// again from the workbook before they are next used (fresh_parts).
    pub fn parts_changed(&mut self) {
        if self.charts.as_ref().is_some_and(|c| !c.files.is_empty()) {
            self.parts_stale = true;
        }
    }

    /// Reads the parts of the charts again when they changed.
    pub fn fresh_parts(&mut self) {
        if !std::mem::take(&mut self.parts_stale) {
            return;
        }
        if let Ok(data) = self.to_bytes() {
            self.charts = Some(super::xlsx_chart::ChartParts::collect(&data));
        }
    }

    /// The workbook as xlsx bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut buf = Cursor::new(Vec::new());
        umya_spreadsheet::writer::xlsx::write_writer(&self.wb, &mut buf).map_err(zerr)?;
        Ok(buf.into_inner())
    }
}

/// Reads the bytes of a part of an xlsx file.
#[cfg(test)]
pub(super) fn zip_part(data: &[u8], name: &str) -> Option<Vec<u8>> {
    let mut zr = zip::ZipArchive::new(Cursor::new(data)).ok()?;
    crate::zippart::read(&mut zr, name)
}

/// Opens xlsx bytes.
pub fn open_bytes(data: &[u8], path: String) -> Result<Book> {
    if data.starts_with(&[0xD0, 0xCF, 0x11, 0xE0]) {
        return Err(Error::msg("the encrypted file is not supported"));
    }
    let wb = umya_spreadsheet::reader::xlsx::read_reader(Cursor::new(data), true).map_err(zerr)?;
    let mut b = Book::new(wb, path);
    b.charts = Some(super::xlsx_chart::ChartParts::collect(data));
    b.cell_images = super::xlsx_cellimage::CellImages::collect(data);
    Ok(b)
}

// ---- cell names ----

/// "A" for 1, "AB" for 28.
pub fn column_name(mut n: i64) -> Result<String> {
    if !(1..=16384).contains(&n) {
        return Err(Error::msg(format!("the column number must be greater than or equal to 1 and less than or equal to 16384, got {n}")));
    }
    let mut s = Vec::new();
    while n > 0 {
        n -= 1;
        s.push(b'A' + (n % 26) as u8);
        n /= 26;
    }
    s.reverse();
    Ok(String::from_utf8(s).unwrap())
}

pub fn column_number(name: &str) -> Result<u32> {
    let up = name.trim().to_uppercase();
    if up.is_empty() || !up.bytes().all(|b| b.is_ascii_uppercase()) {
        return Err(Error::msg(format!("invalid column name \"{name}\"")));
    }
    let mut n: u64 = 0;
    for b in up.bytes() {
        n = n * 26 + (b - b'A' + 1) as u64;
        if n > 16384 {
            return Err(Error::msg(format!("invalid column name \"{name}\"")));
        }
    }
    Ok(n as u32)
}

pub fn cell_name(col: u32, row: u32) -> Result<String> {
    if !(1..=1_048_576).contains(&row) {
        return Err(Error::msg(format!("invalid cell reference [{col}, {row}]")));
    }
    Ok(format!("{}{row}", column_name(col as i64)?))
}

pub(crate) fn cell(col: u32, row: u32) -> String {
    cell_name(col, row).unwrap_or_default()
}

/// The (column, row) of "B3" (1-based).
pub fn parse_ref(r: &str) -> Result<(u32, u32)> {
    let bad = || Error::msg(format!("cannot convert cell \"{r}\" to coordinates: invalid cell name \"{r}\""));
    let s = r.replace('$', "");
    let i = s.find(|c: char| c.is_ascii_digit()).ok_or_else(bad)?;
    let (c, rr) = s.split_at(i);
    if c.is_empty() || !c.bytes().all(|b| b.is_ascii_alphabetic()) {
        return Err(bad());
    }
    let col = column_number(c).map_err(|_| bad())?;
    let row: u32 = rr.parse().map_err(|_| bad())?;
    if !(1..=1_048_576).contains(&row) {
        return Err(bad());
    }
    Ok((col, row))
}

// ---- dates ----

/// The Excel serial number of a wall-clock time.
pub fn to_serial(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32, ms: u32, date1904: bool) -> f64 {
    let date = chrono::NaiveDate::from_ymd_opt(y, mo, d).unwrap_or_default();
    let base = if date1904 { chrono::NaiveDate::from_ymd_opt(1904, 1, 1).unwrap() } else { chrono::NaiveDate::from_ymd_opt(1899, 12, 30).unwrap() };
    let mut days = (date - base).num_days() as f64;
    if !date1904 && days < 61.0 {
        days -= 1.0; // Excel's 1900-02-29
    }
    days + (h as f64 * 3600.0 + mi as f64 * 60.0 + s as f64 + ms as f64 / 1000.0) / 86400.0
}

/// The wall-clock time of an Excel serial number, rounded to the
/// millisecond: (y, month, d, h, mi, s, ms).
pub fn from_serial(f: f64, date1904: bool) -> Option<(i32, u32, u32, u32, u32, u32, u32)> {
    if !f.is_finite() || !(0.0..=2_958_466.0).contains(&f) {
        return None;
    }
    let mut days = f.floor();
    let mut ms = ((f - days) * 86_400_000.0).round() as i64;
    if ms >= 86_400_000 {
        days += 1.0;
        ms -= 86_400_000;
    }
    let base = if date1904 {
        chrono::NaiveDate::from_ymd_opt(1904, 1, 1).unwrap()
    } else if days < 61.0 {
        if days == 60.0 {
            days = 59.0; // 1900-02-29 does not exist
        }
        chrono::NaiveDate::from_ymd_opt(1899, 12, 31).unwrap()
    } else {
        chrono::NaiveDate::from_ymd_opt(1899, 12, 30).unwrap()
    };
    let date = base + chrono::Duration::days(days as i64);
    use chrono::Datelike;
    Some((
        date.year(),
        date.month(),
        date.day(),
        (ms / 3_600_000) as u32,
        (ms / 60_000 % 60) as u32,
        (ms / 1000 % 60) as u32,
        (ms % 1000) as u32,
    ))
}

/// Built-in number format ids that display dates or times, including the
/// Japanese locale specific ones.
pub fn builtin_date_fmt(id: u32) -> bool {
    matches!(id, 14..=22 | 27..=36 | 45..=47 | 50..=58 | 71..=81)
}

/// Whether a custom number format displays a date or time. Literal text,
/// escapes and bracketed sections are ignored; elapsed time formats
/// ([h]:mm) are treated as plain numbers.
pub fn date_format_code(code: &str) -> bool {
    let code = code.split(';').next().unwrap_or("");
    // "G/標準" is General as Japanese Excel shows it, not the era.
    let lower = code.to_lowercase().replace("g/標準", "");
    if lower.contains("[h") || lower.contains("[m") || lower.contains("[s") {
        return false;
    }
    let b = lower.as_bytes();
    let mut out = String::new();
    let mut in_quote = false;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if in_quote {
            if c == b'"' {
                in_quote = false;
            }
        } else if c == b'"' {
            in_quote = true;
        } else if c == b'\\' || c == b'_' || c == b'*' {
            i += 1;
        } else if c == b'[' {
            if let Some(j) = lower[i..].find(']') {
                i += j;
            }
        } else {
            out.push(c as char);
        }
        i += 1;
    }
    let out = out.replace("general", "");
    // g / e: Japanese era and era year (e is the exponent when the code has digits).
    // aaa / aaaa: the Japanese weekday.
    out.contains(['y', 'm', 'd', 'h', 's', 'g']) || out.contains("aaa") || (out.contains('e') && !out.contains(['0', '#', '?']))
}

/// Whether a style's number format displays dates.
pub(crate) fn style_is_date(nf: Option<&NumberingFormat>) -> bool {
    let Some(nf) = nf else { return false };
    let id = nf.number_format_id();
    if id < 164 && builtin_code(id).is_some() {
        return builtin_date_fmt(id) || date_format_code(nf.format_code());
    }
    date_format_code(nf.format_code())
}

/// The code of a built-in number format (as Japanese Excel shows them: checked
/// with Excel 2021, tools/excel-verify/make_builtin.py).
pub fn builtin_code(id: u32) -> Option<&'static str> {
    Some(match id {
        0 => "General",
        1 => "0",
        2 => "0.00",
        3 => "#,##0",
        4 => "#,##0.00",
        5 | 63 => r##""¥"#,##0;"¥"\-#,##0"##,
        6 | 64 => r##""¥"#,##0;[Red]"¥"\-#,##0"##,
        7 | 65 => r##""¥"#,##0.00;"¥"\-#,##0.00"##,
        8 | 66 => r##""¥"#,##0.00;[Red]"¥"\-#,##0.00"##,
        9 => "0%",
        10 => "0.00%",
        11 => "0.00E+00",
        12 => "# ?/?",
        13 => "# ??/??",
        14 => "yyyy/m/d",
        15 => "d-mmm-yy",
        16 => "d-mmm",
        17 => "mmm-yy",
        18 => "h:mm AM/PM",
        19 => "h:mm:ss AM/PM",
        20 => "h:mm",
        21 => "h:mm:ss",
        22 => "yyyy/m/d h:mm",
        23 => r##""$"#,##0_);\("$"#,##0\)"##,
        24 => r##""$"#,##0_);[Red]\("$"#,##0\)"##,
        25 => r##""$"#,##0.00_);\("$"#,##0.00\)"##,
        26 => r##""$"#,##0.00_);[Red]\("$"#,##0.00\)"##,
        27 | 36 | 50 | 57 => "[$-411]ge.m.d",
        28 | 29 | 51 | 54 | 58 => "[$-411]ggge\"年\"m\"月\"d\"日\"",
        30 => "m/d/yy",
        31 => "yyyy\"年\"m\"月\"d\"日\"",
        32 => "h\"時\"mm\"分\"",
        33 => "h\"時\"mm\"分\"ss\"秒\"",
        34 | 52 | 55 => "yyyy\"年\"m\"月\"",
        35 | 53 | 56 => "m\"月\"d\"日\"",
        37 => r"#,##0;\-#,##0",
        38 => r"#,##0;[Red]\-#,##0",
        39 => r"#,##0.00;\-#,##0.00",
        40 => r"#,##0.00;[Red]\-#,##0.00",
        41 => r#"_ * #,##0_ ;_ * \-#,##0_ ;_ * "-"_ ;_ @_ "#,
        42 => r#"_ "¥"* #,##0_ ;_ "¥"* \-#,##0_ ;_ "¥"* "-"_ ;_ @_ "#,
        43 => r#"_ * #,##0.00_ ;_ * \-#,##0.00_ ;_ * "-"??_ ;_ @_ "#,
        44 => r#"_ "¥"* #,##0.00_ ;_ "¥"* \-#,##0.00_ ;_ "¥"* "-"??_ ;_ @_ "#,
        45 | 79 => "mm:ss",
        46 | 80 => "[h]:mm:ss",
        47 | 81 => "mm:ss.0",
        48 => "##0.0E+0",
        49 => "@",
        // The ids Thai Excel shows with Thai digits are plain in Japanese Excel.
        59 => "0",
        60 => "0.00",
        61 => "#,##0",
        62 => "#,##0.00",
        67 => "0%",
        68 => "0.00%",
        69 => "# ?/?",
        70 => "# ??/??",
        71 | 72 => "yyyy/m/d",
        73 => "d-mmm-yy",
        74 => "d-mmm",
        75 => "mmm-yy",
        76 => "h:mm",
        77 => "h:mm:ss",
        78 => "yyyy/m/d h:mm",
        _ => return None,
    })
}

/// The code a number format displays with.
pub(crate) fn format_code(nf: Option<&NumberingFormat>) -> String {
    let Some(nf) = nf else { return "General".into() };
    let id = nf.number_format_id();
    if id < 164
        && let Some(c) = builtin_code(id)
    {
        return c.to_string();
    }
    let c = nf.format_code();
    if c.is_empty() { "General".into() } else { c.to_string() }
}

// ---- values ----

/// A number as Go's strconv.FormatFloat(f, 'f', -1, 64) would show it in
/// the General format (no exponent for usual magnitudes).
pub fn fmt_num(f: f64) -> String {
    if f.is_nan() {
        return "NaN".into();
    }
    if f == f.trunc() && f.abs() < 1e21 {
        return format!("{}", f as i128);
    }
    let s = format!("{f}");
    s
}

/// The text a cell displays (its value formatted with its number format).
pub(crate) fn cell_text(b: &Book, c: &umya_spreadsheet::Cell) -> String {
    match c.raw_value() {
        CellRawValue::Empty => String::new(),
        CellRawValue::Bool(v) => if *v { "TRUE".into() } else { "FALSE".into() },
        CellRawValue::Numeric(f) => {
            let nf = c.style().number_format();
            super::numfmt::format_serial(*f, &format_code(nf), style_is_date(nf), b.date1904)
        }
        other => other.to_string(),
    }
}

/// The value of a cell as JavaScript.
pub(crate) fn cell_value<'js>(ctx: &Ctx<'js>, b: &Book, c: Option<&umya_spreadsheet::Cell>) -> R<'js> {
    let Some(c) = c else { return Ok(Value::new_null(ctx.clone())) };
    match c.raw_value() {
        CellRawValue::Empty => Ok(Value::new_null(ctx.clone())),
        CellRawValue::Bool(v) => Ok(Value::new_bool(ctx.clone(), *v)),
        CellRawValue::Numeric(f) => {
            if style_is_date(c.style().number_format())
                && let Some((y, mo, d, h, mi, s, ms)) = from_serial(*f, b.date1904)
            {
                return js::date_local(ctx, y, mo, d, h, mi, s, ms);
            }
            Ok(Value::new_number(ctx.clone(), *f))
        }
        CellRawValue::String(s) | CellRawValue::Lazy(s) => {
            if s.is_empty() {
                return Ok(Value::new_null(ctx.clone()));
            }
            Ok(js::v(ctx, s.to_string()))
        }
        other => {
            let s = other.to_string();
            if s.is_empty() { Ok(Value::new_null(ctx.clone())) } else { Ok(js::v(ctx, s)) }
        }
    }
}

fn has_value(c: &umya_spreadsheet::Cell) -> bool {
    match c.raw_value() {
        CellRawValue::Empty => false,
        CellRawValue::String(s) | CellRawValue::Lazy(s) => !s.is_empty(),
        _ => true,
    }
}

/// What makes a cell a used one, for the last row and column of a sheet
/// (lastRow(), lastColumn(), dimension, append()): a value or a formula.
#[derive(Clone, Copy, Default)]
pub(crate) struct Used {
    /// A cell with only a format of its own is used too.
    pub formats: bool,
    /// A cell whose value is empty or only spaces is not: a formula that
    /// gives "" (and one that was never calculated).
    pub skip_blank: bool,
}

impl Used {
    pub(crate) fn has(self, c: &umya_spreadsheet::Cell) -> bool {
        let filled = match c.raw_value() {
            CellRawValue::Empty => false,
            CellRawValue::String(s) | CellRawValue::Lazy(s) => !if self.skip_blank { s.trim().is_empty() } else { s.is_empty() },
            _ => true,
        };
        filled || (!self.skip_blank && c.is_formula()) || (self.formats && *c.style() != umya_spreadsheet::Style::default())
    }

    /// The used cells of the sheet as (first column, first row, last column, last row).
    /// A merged area counts as its top-left cell and reaches to its last one.
    pub(crate) fn area(self, ws: &Worksheet) -> Option<(u32, u32, u32, u32)> {
        let merges = Merges::new(ws);
        let mut area: Option<(u32, u32, u32, u32)> = None;
        for c in ws.cells() {
            if self.has(c) {
                let (col, row) = (c.coordinate().col_num(), c.coordinate().row_num());
                let Some((c2, r2)) = self.reach(&merges, col, row) else { continue };
                let a = area.get_or_insert((col, row, c2, r2));
                *a = (a.0.min(col), a.1.min(row), a.2.max(c2), a.3.max(r2));
            }
        }
        area
    }

    /// The last column and row a used cell counts up to: its own, those of
    /// the merged area it is the top-left cell of (xlsx_merge::reach), or
    /// None for a cell a merged area covers (Excel shows nothing of it).
    pub(crate) fn reach(self, merges: &Merges, col: u32, row: u32) -> Option<(u32, u32)> {
        if merges.is_empty() {
            return Some((col, row));
        }
        match merges.area(col, row) {
            Some(m) if (m.0, m.1) == (col, row) => Some(super::xlsx_merge::reach(m)),
            Some(_) => None,
            None => Some((col, row)),
        }
    }
}

/// The used cells of a sheet as rows (like excelize's GetRows): every row
/// up to the last one with a value, each up to its last value.
pub(crate) fn rows(ws: &Worksheet) -> Vec<Vec<(u32, &umya_spreadsheet::Cell)>> {
    let mut by_row: std::collections::BTreeMap<u32, Vec<(u32, &umya_spreadsheet::Cell)>> = Default::default();
    for c in ws.cells() {
        if has_value(c) {
            let co = c.coordinate();
            by_row.entry(co.row_num()).or_default().push((co.col_num(), c));
        }
    }
    let last = by_row.keys().next_back().copied().unwrap_or(0);
    let mut out = Vec::with_capacity(last as usize);
    for r in 1..=last {
        let mut v = by_row.remove(&r).unwrap_or_default();
        v.sort_by_key(|(c, _)| *c);
        out.push(v);
    }
    out
}

/// The used cells of a sheet as scripts read them (toArrays, toRowObjects,
/// range.values): the rows of `rows`, with merged cells as xlsx_merge tells.
/// What a merged area covers is left out, and with `fill` every cell of an
/// area is its top-left cell (a department merged over three rows is read
/// in each of them).
pub(crate) fn grid(ws: &Worksheet, fill: bool) -> Vec<Vec<Option<&umya_spreadsheet::Cell>>> {
    let merges = Merges::new(ws);
    let mut by_row: std::collections::BTreeMap<u32, std::collections::BTreeMap<u32, &umya_spreadsheet::Cell>> = Default::default();
    let (mut width, mut height) = (0, 0);
    for c in ws.cells() {
        if has_value(c) {
            let co = c.coordinate();
            if !merges.is_empty() && merges.covered(co.col_num(), co.row_num()) {
                continue;
            }
            by_row.entry(co.row_num()).or_default().insert(co.col_num(), c);
            (width, height) = (width.max(co.col_num()), height.max(co.row_num()));
        }
    }
    if fill {
        let valued: Vec<_> = merges.rects.iter().filter_map(|&m| Some((m, *by_row.get(&m.1)?.get(&m.0)?))).collect();
        for (m, _) in &valued {
            let (c, r) = super::xlsx_merge::reach(*m);
            (width, height) = (width.max(c), height.max(r));
        }
        for ((c1, r1, c2, r2), c) in valued {
            for row in r1..=r2.min(height) {
                let line = by_row.entry(row).or_default();
                for col in c1..=c2.min(width) {
                    line.insert(col, c);
                }
            }
        }
    }
    let last = by_row.keys().next_back().copied().unwrap_or(0);
    let mut out = Vec::with_capacity(last as usize);
    for r in 1..=last {
        let line = by_row.remove(&r).unwrap_or_default();
        let n = line.keys().next_back().copied().unwrap_or(0);
        let mut v = vec![None; n as usize];
        for (col, c) in line {
            v[col as usize - 1] = Some(c);
        }
        out.push(v);
    }
    out
}

fn date_style(code: &str) -> NumberingFormat {
    let mut nf = NumberingFormat::default();
    nf.set_format_code(code);
    nf
}

/// Writes a JavaScript value into a cell.
pub(crate) fn set_value(b: &mut Book, sheet: &str, (col, row): (u32, u32), v: &Value) -> Result<()> {
    let date1904 = b.date1904;
    let c = b.cell_mut(sheet, col, row)?;
    if nullish(v) {
        c.cell_value_mut().set_blank();
        return Ok(());
    }
    if js::is_date(v) {
        let Some((y, mo, d, h, mi, s, ms)) = js::date_fields(v) else {
            c.cell_value_mut().set_blank();
            return Ok(());
        };
        c.cell_value_mut().remove_formula();
        c.set_value_number(to_serial(y, mo, d, h, mi, s, ms, date1904));
        if !style_is_date(c.style().number_format()) {
            let code = if h == 0 && mi == 0 && s == 0 && ms == 0 { "yyyy/m/d" } else { "yyyy/m/d h:mm" };
            c.style_mut().set_number_format(date_style(code));
        }
        return Ok(());
    }
    c.cell_value_mut().remove_formula();
    if let Some(f) = v.as_number() {
        if f.is_finite() {
            c.set_value_number(f);
        } else {
            c.cell_value_mut().set_blank();
        }
    } else if let Some(x) = v.as_bool() {
        c.set_value_bool(x);
    } else {
        c.set_value_string(js::to_str(v));
    }
    Ok(())
}

/// The furigana of a text as PHONETIC gives them: each run's reading in
/// place of the characters it is of (`start`..`end` in UTF-16 code units),
/// the others as they are. None when there are no runs.
pub(crate) fn phonetic_text(text: &str, runs: &[(u32, u32, String)]) -> Option<String> {
    if runs.is_empty() {
        return None;
    }
    let units: Vec<u16> = text.encode_utf16().collect();
    let mut out = String::new();
    let mut at = 0usize;
    for (start, end, reading) in runs {
        let (start, end) = (*start as usize, *end as usize);
        if start < at || end <= start || end > units.len() {
            continue;
        }
        out.push_str(&String::from_utf16_lossy(&units[at..start]));
        out.push_str(reading);
        at = end;
    }
    out.push_str(&String::from_utf16_lossy(&units[at..]));
    Some(out)
}

/// How Excel is to write furigana given as `reading`: in hiragana when they
/// are (Excel would show them in katakana otherwise), half-width katakana
/// likewise; None for full-width katakana, the default.
fn phonetic_kind(reading: &str) -> Option<&'static str> {
    let letters = || reading.chars().filter(|c| !c.is_whitespace() && !matches!(c, 'ー' | '・'));
    if letters().all(|c| ('ぁ'..='ゟ').contains(&c)) {
        Some("Hiragana")
    } else if letters().all(|c| ('｡'..='ﾟ').contains(&c)) {
        Some("halfwidthKatakana")
    } else {
        None
    }
}

// ---- natives ----

pub(crate) fn handle_id(v: &Value) -> Option<u64> {
    let o = v.as_object()?;
    let c = Class::<Handle>::from_object(o)?;
    Some(c.borrow().id)
}

/// Runs f with the open workbook of the handle.
pub(crate) fn with_book<'js, T>(h: &Host, a: &A<'js>, f: impl FnOnce(&mut Book) -> rquickjs::Result<T>) -> rquickjs::Result<T> {
    let Some(id) = handle_id(&a.arg(0)) else { return Err(a.throw(&err!("xlsx.invalidHandle"))) };
    let mut books = h.books.borrow_mut();
    let Some(b) = books.get_mut(&id) else { return Err(a.throw(&err!("xlsx.invalidHandle"))) };
    if b.closed {
        return Err(a.throw(&err!("xlsx.closed")));
    }
    f(b)
}

fn track<'js>(h: &Host, ctx: &Ctx<'js>, b: Book) -> R<'js> {
    let id = h.next_id();
    h.books.borrow_mut().insert(id, b);
    Ok(Class::instance(ctx.clone(), Handle { id })?.into_value())
}

fn upper(s: String) -> String {
    s.to_uppercase()
}

/// The cell a script means by `r` ("B3"): the top-left cell of the merged
/// area it is in, or the cell itself (xlsx_merge).
fn cell_at<'js>(a: &A<'js>, b: &Book, sheet: &str, r: &str) -> rquickjs::Result<(u32, u32)> {
    anchor_of(b, sheet, r).map_err(|e| a.throw(&e))
}

/// Checks a new sheet name like Excel (and excelize) do.
pub(crate) fn check_sheet_name(name: &str) -> Result<()> {
    if name.trim().is_empty() {
        return Err(Error::msg("the sheet name can not be blank"));
    }
    if name.chars().count() > 31 {
        return Err(Error::msg("the sheet name length exceeds the 31 characters limit"));
    }
    if name.contains([':', '\\', '/', '?', '*', '[', ']']) {
        return Err(Error::msg("the sheet can not contain any of the characters :\\/?*[or]"));
    }
    Ok(())
}

pub(super) fn install(n: &N) {
    n.set("xlsxOpen", |h, a| {
        let path = a.str(0);
        let data = std::fs::read(&path).map_err(|e| a.throw(&Error::path("open", path.clone(), e)))?;
        let b = open_bytes(&data, path.clone()).map_err(|e| super::throw_path(&a.ctx, &path, e))?;
        track(h, &a.ctx, b)
    });
    n.set("xlsxOpenBytes", |h, a| {
        let b = open_bytes(&a.bytes(0)?, a.str(1)).map_err(|e| a.throw(&e))?;
        track(h, &a.ctx, b)
    });
    n.set("xlsxCreate", |h, a| {
        let mut b = Book::new(umya_spreadsheet::new_file(), String::new());
        b.fresh = true;
        // New workbooks use the language's default font (游ゴシック for
        // Japanese, like Excel itself); create({ font }) overrides it.
        let mut font = i18n::current().excel.font.clone();
        let o = a.arg(0);
        if o.is_object() && js::has(&o, "font") {
            let f = js::get(&o, "font");
            font = if f.is_null() { String::new() } else { js::to_str(&f) };
        }
        if !font.is_empty() {
            b.wb.set_default_font(&font, b.default_font.1);
            b.default_font.0 = font;
        }
        track(h, &a.ctx, b)
    });
    n.set("xlsxCharts", |h, a| {
        let sheet = a.str(1);
        let charts = with_book(h, a, |b| {
            b.sheet(&sheet).map_err(|e| a.throw(&e))?;
            b.fresh_parts();
            Ok(super::xlsx_chart::sheet_charts(b, &sheet))
        })?;
        a.ret(serde_json::Value::Array(charts).to_string())
    });
    n.set("xlsxPath", |h, a| {
        let p = with_book(h, a, |b| Ok(b.path.clone()))?;
        if p.is_empty() { a.null() } else { a.ret(p) }
    });
    n.set("xlsxSheetNames", |h, a| {
        let names = with_book(h, a, |b| Ok(b.wb.sheet_collection().iter().map(|s| s.name().to_string()).collect::<Vec<_>>()))?;
        a.ret(names)
    });
    n.set("xlsxAddSheet", |h, a| {
        let name = a.str(1);
        with_book(h, a, |b| {
            if b.wb.sheet_by_name(&name).is_ok() {
                return Err(a.throw(&err!("xlsx.sheetExists", name)));
            }
            check_sheet_name(&name).map_err(|e| a.throw(&e))?;
            if b.fresh {
                b.fresh = false;
                b.calc_reset();
                b.wb.set_sheet_name(0, name.clone()).map_err(|e| a.throw(&Error::msg(e.to_string())))?;
                return Ok(());
            }
            b.calc_reset();
            b.wb.new_sheet(name.clone()).map_err(|e| a.throw(&Error::msg(e.to_string())))?;
            Ok(())
        })?;
        a.undefined()
    });
    n.set("xlsxRemoveSheet", |h, a| {
        let name = a.str(1);
        with_book(h, a, |b| {
            if b.wb.sheet_by_name(&name).is_err() {
                return Err(a.throw(&Error::msg(format!("sheet {name} does not exist"))));
            }
            b.calc_reset();
            b.wb.remove_sheet_by_name(&name).map_err(|e| a.throw(&Error::msg(e.to_string())))?;
            Ok(())
        })?;
        a.undefined()
    });
    n.set("xlsxRenameSheet", |h, a| {
        let (from, to) = (a.str(1), a.str(2));
        with_book(h, a, |b| {
            let idx = b.wb.sheet_collection().iter().position(|s| s.name() == from);
            let Some(idx) = idx else { return Err(a.throw(&Error::msg(format!("sheet {from} does not exist")))) };
            if from == to {
                return Ok(());
            }
            check_sheet_name(&to).map_err(|e| a.throw(&e))?;
            if b.wb.sheet_by_name(&to).is_ok() {
                return Err(a.throw(&err!("xlsx.sheetExists", to)));
            }
            b.calc_reset();
            b.wb.set_sheet_name(idx, to.clone()).map_err(|e| a.throw(&Error::msg(e.to_string())))?;
            if b.fresh && from == "Sheet1" {
                b.fresh = false;
            }
            b.parts_changed();
            Ok(())
        })?;
        a.undefined()
    });
    n.set("xlsxCopySheet", |h, a| {
        let (sheet, name) = (a.str(1), a.arg(2));
        let name = (!nullish(&name)).then(|| js::to_str(&name));
        let pos = a.arg(3);
        let pos = (!nullish(&pos)).then(|| a.int(3).max(0) as usize);
        let out = with_book(h, a, |b| super::xlsx_edit::copy_sheet(b, &sheet, name.as_deref(), pos).map_err(|e| a.throw(&e)))?;
        a.ret(out)
    });
    n.set("xlsxEditLines", |h, a| {
        // (book, sheet, columns, at, count, insert)
        let sheet = a.str(1);
        let lines = super::xlsx_edit::Lines { columns: a.arg(2).as_bool() == Some(true), at: a.int(3).clamp(0, u32::MAX as i64) as u32, count: a.int(4).clamp(0, u32::MAX as i64) as u32, insert: a.arg(5).as_bool() == Some(true) };
        with_book(h, a, |b| super::xlsx_edit::edit_lines(b, &sheet, lines).map_err(|e| a.throw(&e)))?;
        a.undefined()
    });
    n.set("xlsxHasSheet", |h, a| {
        let name = a.str(1);
        a.ret(with_book(h, a, |b| Ok(b.wb.sheet_by_name(&name).is_ok()))?)
    });
    n.set("xlsxGet", |h, a| {
        let (sheet, r) = (a.str(1), upper(a.str(2)));
        with_book(h, a, |b| {
            let at = cell_at(a, b, &sheet, &r)?;
            let ws = b.sheet(&sheet).map_err(|e| a.throw(&e))?;
            cell_value(&a.ctx, b, ws.cell(at))
        })
    });
    n.set("xlsxGetText", |h, a| {
        let (sheet, r) = (a.str(1), upper(a.str(2)));
        let s = with_book(h, a, |b| {
            let at = cell_at(a, b, &sheet, &r)?;
            let ws = b.sheet(&sheet).map_err(|e| a.throw(&e))?;
            Ok(ws.cell(at).map(|c| cell_text(b, c)).unwrap_or_default())
        })?;
        a.ret(s)
    });
    n.set("xlsxSet", |h, a| {
        let (sheet, r, v) = (a.str(1), upper(a.str(2)), a.arg(3));
        with_book(h, a, |b| {
            let at = cell_at(a, b, &sheet, &r)?;
            set_value(b, &sheet, at, &v).map_err(|e| a.throw(&e))
        })?;
        a.undefined()
    });
    n.set("xlsxGetFormula", |h, a| {
        let (sheet, r) = (a.str(1), upper(a.str(2)));
        let f = with_book(h, a, |b| {
            let (col, row) = cell_at(a, b, &sheet, &r)?;
            Ok(super::xlsx_calc::formula_shown(b, &sheet, col, row))
        })?;
        match f {
            Some(f) => a.ret(format!("={f}")),
            None => a.null(),
        }
    });
    n.set("xlsxSetFormula", |h, a| {
        let (sheet, r, formula) = (a.str(1), upper(a.str(2)), a.str(3));
        with_book(h, a, |b| {
            let (col, row) = cell_at(a, b, &sheet, &r)?;
            super::xlsx_calc::set_formula(b, &sheet, col, row, &formula).map_err(|e| a.throw(&e))
        })?;
        a.undefined()
    });
    // xlsxGetPhonetic(book, sheet, ref, show) -> the furigana as PHONETIC gives them (null when
    // the text has none), or (show) whether they are shown.
    n.set("xlsxGetPhonetic", |h, a| {
        let (sheet, r, show) = (a.str(1), upper(a.str(2)), a.arg(3).as_bool().unwrap_or(false));
        let got = with_book(h, a, |b| {
            let at = cell_at(a, b, &sheet, &r)?;
            let ws = b.sheet(&sheet).map_err(|e| a.throw(&e))?;
            Ok(ws.cell(at).map(|c| {
                let text = phonetic_text(&c.value(), &c.cell_value().phonetic_runs());
                // As Excel's PHONETIC gives them: in the kind of furigana of the text.
                let kind = c.cell_value().phonetic_type();
                (c.show_phonetic(), text.map(|t| gridcraft_functions::phonetic_form(&t, kind.as_deref())))
            }))
        })?;
        match (show, got) {
            (true, g) => a.ret(g.is_some_and(|(s, _)| s)),
            (false, Some((_, Some(t)))) => a.ret(t),
            (false, _) => a.null(),
        }
    });
    // xlsxSetPhonetic(book, sheet, ref, reading | null) / xlsxSetShowPhonetic(book, sheet, ref, bool)
    n.set("xlsxSetPhonetic", |h, a| {
        let (sheet, r, v) = (a.str(1), upper(a.str(2)), a.arg(3));
        let reading = (!js::nullish(&v)).then(|| js::to_str(&v)).filter(|s| !s.is_empty());
        with_book(h, a, |b| {
            let (col, row) = cell_at(a, b, &sheet, &r)?;
            let c = b.cell_mut(&sheet, col, row).map_err(|e| a.throw(&e))?;
            let text = match c.raw_value() {
                CellRawValue::String(s) | CellRawValue::Lazy(s) => s.to_string(),
                CellRawValue::RichText(t) => t.text().to_string(),
                _ if reading.is_none() => String::new(),
                _ => return Err(a.throw(&err!("xlsx.phoneticNotText", r.clone()))),
            };
            match reading {
                Some(reading) => {
                    let end = u32::try_from(text.encode_utf16().count()).unwrap_or(u32::MAX);
                    let kind = phonetic_kind(&reading);
                    c.cell_value_mut().set_phonetic_runs(&[(0, end, reading)], kind);
                }
                None => {
                    c.cell_value_mut().set_phonetic_runs(&[], None);
                }
            }
            // PHONETIC reads the furigana of the whole workbook.
            b.calc_reset();
            Ok(())
        })?;
        a.undefined()
    });
    n.set("xlsxSetShowPhonetic", |h, a| {
        let (sheet, r, show) = (a.str(1), upper(a.str(2)), a.arg(3).as_bool().unwrap_or(false));
        with_book(h, a, |b| {
            let (col, row) = cell_at(a, b, &sheet, &r)?;
            b.cell_mut(&sheet, col, row).map_err(|e| a.throw(&e))?.set_show_phonetic(show);
            Ok(())
        })?;
        a.undefined()
    });
    n.set("xlsxCalc", |h, a| {
        let (sheet, r) = (a.str(1), upper(a.str(2)));
        let s = with_book(h, a, |b| {
            let (col, row) = cell_at(a, b, &sheet, &r)?;
            super::xlsx_calc::calc_cell(b, &sheet, &cell(col, row)).map_err(|e| a.throw(&e))
        })?;
        a.ret(s)
    });
    // xlsxCalculate(book) -> JSON { formulas, changed, skipped: [{ sheet, address, formula, reason, name?, message }] }
    n.set("xlsxCalculate", |h, a| {
        let sum = with_book(h, a, |b| Ok(super::xlsx_calc::calculate(b)))?;
        let skipped: Vec<serde_json::Value> = sum
            .skipped
            .iter()
            .map(|s| {
                let mut o = serde_json::json!({ "sheet": s.sheet, "address": s.address, "formula": s.formula, "reason": s.skip.reason(), "message": s.skip.message() });
                if let Some(n) = s.skip.name() {
                    o["name"] = n.into();
                }
                o
            })
            .collect();
        a.ret(serde_json::json!({ "formulas": sum.formulas, "changed": sum.changed, "skipped": skipped }).to_string())
    });
    // xlsxReadAll(book, sheet, text[, covered[, firstRow, lastRow]]) -> any[][]: the rows up to the
    // last one with a value, each up to its last value, or only the rows firstRow..lastRow of them.
    // A cell of a merged area is the area's top-left cell; with covered: false only the top-left
    // cell has the value (the others are null).
    n.set("xlsxReadAll", |h, a| {
        let sheet = a.str(1);
        let text = a.bool(2);
        let fill = a.arg(3).as_bool() != Some(false);
        let bound = |i: usize, or: usize| if nullish(&a.arg(i)) { or } else { a.int(i).clamp(1, 1_048_576) as usize };
        let (first, last) = (bound(4, 1), bound(5, 1_048_576));
        with_book(h, a, |b| {
            let ws = b.sheet(&sheet).map_err(|e| a.throw(&e))?;
            let mut out = Vec::new();
            for row in grid(ws, fill).into_iter().take(last).skip(first - 1) {
                let mut vals: Vec<Value> = Vec::with_capacity(row.len());
                for cell in row {
                    vals.push(if text {
                        js::v(&a.ctx, cell.map(|c| cell_text(b, c)).unwrap_or_default())
                    } else {
                        cell_value(&a.ctx, b, cell)?
                    });
                }
                out.push(js::array(&a.ctx, vals)?);
            }
            js::array(&a.ctx, out)
        })
    });
    // xlsxWriteRows(book, sheet, startRef, rows: any[][])
    n.set("xlsxWriteRows", |h, a| {
        let sheet = a.str(1);
        let start = upper(a.opt_str(2, "A1"));
        let data = a.arg(3);
        with_book(h, a, |b| {
            let (c0, r0) = parse_ref(&start).map_err(|e| a.throw(&e))?;
            // What a merged area covers is not written: the value at the
            // place of its top-left cell is the area's.
            let merges = Merges::new(b.sheet(&sheet).map_err(|e| a.throw(&e))?);
            for (r, rowv) in elems(&data).iter().enumerate() {
                for (c, v) in elems(rowv).iter().enumerate() {
                    if nullish(v) {
                        continue;
                    }
                    let (col, row) = (c0 + c as u32, r0 + r as u32);
                    let name = cell_name(col, row).map_err(|e| a.throw(&e))?;
                    if !merges.is_empty() && merges.covered(col, row) {
                        continue;
                    }
                    set_value(b, &sheet, (col, row), v).map_err(|e| a.throw(&e.context(format!("{sheet}!{name}"))))?;
                }
            }
            Ok(())
        })?;
        a.undefined()
    });
    // xlsxDimension(book, sheet) -> the range around the used cells ("B2:E9"), "" for an empty sheet
    n.set("xlsxDimension", |h, a| {
        let sheet = a.str(1);
        let d = with_book(h, a, |b| {
            let area = Used::default().area(b.sheet(&sheet).map_err(|e| a.throw(&e))?);
            Ok(area.map(|(c1, r1, c2, r2)| format!("{}:{}", cell(c1, r1), cell(c2, r2))).unwrap_or_default())
        })?;
        a.ret(d)
    });
    n.set("xlsxSetColWidth", |h, a| {
        let (sheet, c1, c2, w) = (a.str(1), a.str(2), a.str(3), a.f64(4));
        with_book(h, a, |b| set_col_width(b, &sheet, &c1, &c2, w).map_err(|e| a.throw(&e)))?;
        a.undefined()
    });
    n.set("xlsxSave", |h, a| {
        let mut path = a.str(1);
        with_book(h, a, |b| {
            if path.is_empty() {
                path = b.path.clone();
            }
            if path.is_empty() {
                return Err(a.throw(&err!("xlsx.noDestination")));
            }
            let data = b.to_bytes().map_err(|e| a.throw(&e))?;
            std::fs::write(&path, data).map_err(|e| a.throw(&Error::path("open", path.clone(), e)))?;
            b.path = path.clone();
            Ok(())
        })?;
        a.undefined()
    });
    n.set("xlsxToBytes", |h, a| {
        let data = with_book(h, a, |b| b.to_bytes().map_err(|e| a.throw(&e)))?;
        js::array_buffer(&a.ctx, data)
    });
    n.set("xlsxClose", |h, a| {
        if let Some(id) = handle_id(&a.arg(0)) {
            h.books.borrow_mut().remove(&id);
            // A closed handle keeps reporting "closed".
            h.books.borrow_mut().insert(id, Book { closed: true, ..Book::new(Workbook::default(), String::new()) });
        }
        a.undefined()
    });
    n.set("xlsxCellName", |_, a| {
        let s = cell_name(a.int(0).max(0) as u32, a.int(1).max(0) as u32).map_err(|e| a.throw(&e))?;
        a.ret(s)
    });
    n.set("xlsxParseRef", |_, a| {
        let (c, r) = parse_ref(&upper(a.str(0))).map_err(|e| a.throw(&e))?;
        a.ret(vec![c as f64, r as f64])
    });
    n.set("xlsxColumnName", |_, a| {
        let s = column_name(a.int(0)).map_err(|e| a.throw(&e))?;
        a.ret(s)
    });
}

/// The width of a column without a width of its own.
pub(crate) const DEFAULT_COL_WIDTH: f64 = 9.140625;

pub(crate) fn set_col_width(b: &mut Book, sheet: &str, c1: &str, c2: &str, w: f64) -> Result<()> {
    let (mut x, mut y) = (column_number(c1)?, column_number(c2)?);
    if y < x {
        std::mem::swap(&mut x, &mut y);
    }
    if !(0.0..=255.0).contains(&w) {
        return Err(Error::msg("the width of the column must be less than or equal to 255 characters"));
    }
    let ws = b.layout_mut(sheet)?;
    for c in x..=y {
        let name = column_name(c as i64)?;
        ws.column_dimension_mut(&name).set_width(w);
    }
    Ok(())
}

pub(crate) fn col_width(b: &Book, sheet: &str, col: &str) -> Result<f64> {
    let n = column_number(col)?;
    let ws = b.sheet(sheet)?;
    let name = column_name(n as i64)?;
    Ok(ws.column_dimension(&name).map(|c| c.width()).filter(|w| *w > 0.0).unwrap_or(DEFAULT_COL_WIDTH))
}
