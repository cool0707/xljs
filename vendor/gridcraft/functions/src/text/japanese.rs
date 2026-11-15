//! xljs: the text functions as Japanese Excel has them. This file is not GridCraft's.
//!
//! - The byte functions (LENB, LEFTB, RIGHTB, MIDB, REPLACEB, FINDB, SEARCHB) count a full-width
//!   character as two bytes when the host says its Excel is a double-byte one ([`set_dbcs`]);
//!   otherwise they are the character functions, as in GridCraft.
//! - ASC and JIS (DBCS) convert between full-width and half-width letters, digits, symbols and
//!   katakana.
//! - YEN, DATESTRING and NUMBERSTRING, which only Japanese Excel has.

use std::sync::atomic::{AtomicBool, Ordering};

use gridcraft_core::kanji::Kanji;
use gridcraft_core::{CellError, Value};

use super::{check_len, chars, count_arg, find, left, len, mid, replace, right, search};
use crate::util::{R, num, num_val, opt_num, round_half_away, text, text_val};
use crate::{Arg, Ctx};

static DBCS: AtomicBool = AtomicBool::new(false);

/// Whether the byte functions count as an Excel with a double-byte default language does
/// (Japanese, Chinese, Korean). For the whole process.
pub fn set_dbcs(on: bool) {
    DBCS.store(on, Ordering::Relaxed);
}

fn dbcs() -> bool {
    DBCS.load(Ordering::Relaxed)
}

/// Bytes of a character in the Japanese code page: ASCII and half-width katakana take one.
fn width(c: char) -> usize {
    if c.is_ascii() || ('\u{FF61}'..='\u{FF9F}').contains(&c) { 1 } else { 2 }
}

/// The bytes `from..to` (0-based, `to` exclusive) of `s`. Half of a two-byte character is a space.
fn slice(s: &[char], from: usize, to: usize) -> String {
    let mut out = String::new();
    let mut at = 0usize;
    for &c in s {
        let w = width(c);
        let (start, end) = (at, at + w);
        at = end;
        if end <= from {
            continue;
        }
        if start >= to {
            break;
        }
        if start >= from && end <= to { out.push(c) } else { out.push(' ') }
    }
    out
}

fn bytes(s: &[char]) -> usize {
    s.iter().map(|c| width(*c)).sum()
}

pub(super) fn lenb(a: &[Arg], c: &mut dyn Ctx) -> R<Value> {
    if !dbcs() {
        return len(a, c);
    }
    num_val(bytes(&chars(&text(a, 0)?)) as f64)
}

pub(super) fn leftb(a: &[Arg], c: &mut dyn Ctx) -> R<Value> {
    if !dbcs() {
        return left(a, c);
    }
    let s = chars(&text(a, 0)?);
    let n = count_arg(a, 1, 1.0)?;
    text_val(slice(&s, 0, n))
}

pub(super) fn rightb(a: &[Arg], c: &mut dyn Ctx) -> R<Value> {
    if !dbcs() {
        return right(a, c);
    }
    let s = chars(&text(a, 0)?);
    let total = bytes(&s);
    let n = count_arg(a, 1, 1.0)?.min(total);
    text_val(slice(&s, total - n, total))
}

pub(super) fn midb(a: &[Arg], c: &mut dyn Ctx) -> R<Value> {
    if !dbcs() {
        return mid(a, c);
    }
    let s = chars(&text(a, 0)?);
    let start = num(a, 1)?;
    if start.is_nan() || start < 1.0 {
        return Err(CellError::Value);
    }
    let n = count_arg(a, 2, 0.0)?;
    let start = if start > 1e9 { 1_000_000_000 } else { start as usize } - 1;
    text_val(slice(&s, start, start.saturating_add(n)))
}

pub(super) fn replaceb(a: &[Arg], c: &mut dyn Ctx) -> R<Value> {
    if !dbcs() {
        return replace(a, c);
    }
    let s = chars(&text(a, 0)?);
    let start = num(a, 1)?;
    if start.is_nan() || start < 1.0 {
        return Err(CellError::Value);
    }
    let n = count_arg(a, 2, 0.0)?;
    let new = text(a, 3)?;
    let total = bytes(&s);
    let start = (if start > 1e9 { 1_000_000_000 } else { start as usize } - 1).min(total);
    let end = start.saturating_add(n).min(total);
    let mut out = slice(&s, 0, start);
    out.push_str(&new);
    out.push_str(&slice(&s, end, total));
    check_len(out)
}

/// FINDB and SEARCHB: the start and the result are byte positions.
fn findb_impl(a: &[Arg], c: &mut dyn Ctx, by_chars: fn(&[Arg], &mut dyn Ctx) -> R<Value>) -> R<Value> {
    if !dbcs() {
        return by_chars(a, c);
    }
    let hay = chars(&text(a, 1)?);
    let start = opt_num(a, 2, 1.0)?;
    if start.is_nan() || start < 1.0 || start > bytes(&hay).max(1) as f64 {
        return Err(CellError::Value);
    }
    // The first character that begins at or after the start byte.
    let start = start as usize - 1;
    let mut at = 0usize;
    let first = hay.iter().position(|ch| {
        let here = at >= start;
        at += width(*ch);
        here
    });
    let mut args: Vec<Arg> = a.iter().take(2).cloned().collect();
    args.push(Arg::val(first.unwrap_or(hay.len()) as f64 + 1.0));
    match by_chars(&args, c)? {
        Value::Number(pos) => num_val(bytes(&hay[..(pos as usize).saturating_sub(1).min(hay.len())]) as f64 + 1.0),
        v => Ok(v),
    }
}

pub(super) fn findb(a: &[Arg], c: &mut dyn Ctx) -> R<Value> {
    findb_impl(a, c, find)
}

pub(super) fn searchb(a: &[Arg], c: &mut dyn Ctx) -> R<Value> {
    findb_impl(a, c, search)
}

// ---------------------------------------------------------------------------------------------
// ASC and JIS

/// Full-width katakana and marks with their half-width forms (a voiced one is two characters).
const KANA: &[(char, &str)] = &[
    ('。', "｡"), ('「', "｢"), ('」', "｣"), ('、', "､"), ('・', "･"), ('ー', "ｰ"), ('゛', "ﾞ"), ('゜', "ﾟ"),
    ('ヲ', "ｦ"), ('ァ', "ｧ"), ('ィ', "ｨ"), ('ゥ', "ｩ"), ('ェ', "ｪ"), ('ォ', "ｫ"), ('ャ', "ｬ"), ('ュ', "ｭ"), ('ョ', "ｮ"), ('ッ', "ｯ"),
    ('ア', "ｱ"), ('イ', "ｲ"), ('ウ', "ｳ"), ('エ', "ｴ"), ('オ', "ｵ"),
    ('カ', "ｶ"), ('キ', "ｷ"), ('ク', "ｸ"), ('ケ', "ｹ"), ('コ', "ｺ"),
    ('サ', "ｻ"), ('シ', "ｼ"), ('ス', "ｽ"), ('セ', "ｾ"), ('ソ', "ｿ"),
    ('タ', "ﾀ"), ('チ', "ﾁ"), ('ツ', "ﾂ"), ('テ', "ﾃ"), ('ト', "ﾄ"),
    ('ナ', "ﾅ"), ('ニ', "ﾆ"), ('ヌ', "ﾇ"), ('ネ', "ﾈ"), ('ノ', "ﾉ"),
    ('ハ', "ﾊ"), ('ヒ', "ﾋ"), ('フ', "ﾌ"), ('ヘ', "ﾍ"), ('ホ', "ﾎ"),
    ('マ', "ﾏ"), ('ミ', "ﾐ"), ('ム', "ﾑ"), ('メ', "ﾒ"), ('モ', "ﾓ"),
    ('ヤ', "ﾔ"), ('ユ', "ﾕ"), ('ヨ', "ﾖ"),
    ('ラ', "ﾗ"), ('リ', "ﾘ"), ('ル', "ﾙ"), ('レ', "ﾚ"), ('ロ', "ﾛ"),
    ('ワ', "ﾜ"), ('ン', "ﾝ"),
    ('ガ', "ｶﾞ"), ('ギ', "ｷﾞ"), ('グ', "ｸﾞ"), ('ゲ', "ｹﾞ"), ('ゴ', "ｺﾞ"),
    ('ザ', "ｻﾞ"), ('ジ', "ｼﾞ"), ('ズ', "ｽﾞ"), ('ゼ', "ｾﾞ"), ('ゾ', "ｿﾞ"),
    ('ダ', "ﾀﾞ"), ('ヂ', "ﾁﾞ"), ('ヅ', "ﾂﾞ"), ('デ', "ﾃﾞ"), ('ド', "ﾄﾞ"),
    ('バ', "ﾊﾞ"), ('ビ', "ﾋﾞ"), ('ブ', "ﾌﾞ"), ('ベ', "ﾍﾞ"), ('ボ', "ﾎﾞ"),
    ('パ', "ﾊﾟ"), ('ピ', "ﾋﾟ"), ('プ', "ﾌﾟ"), ('ペ', "ﾍﾟ"), ('ポ', "ﾎﾟ"),
    ('ヴ', "ｳﾞ"),
];

/// Symbols whose full-width form is not the ASCII one moved to U+FF00: (half, full).
const SYMBOLS: &[(char, char)] = &[(' ', '\u{3000}'), ('\\', '￥'), ('\'', '’'), ('"', '”')];

fn to_half(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if let Some((half, _)) = SYMBOLS.iter().find(|(_, full)| *full == c) {
            out.push(*half);
        } else if ('\u{FF01}'..='\u{FF5E}').contains(&c) {
            out.extend(char::from_u32(c as u32 - 0xFEE0));
        } else if let Some((_, half)) = KANA.iter().find(|(full, _)| *full == c) {
            out.push_str(half);
        } else {
            out.push(c);
        }
    }
    out
}

fn to_full(s: &str) -> String {
    let cs = chars(s);
    let mut out = String::with_capacity(s.len() * 3);
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        i += 1;
        if let Some((_, full)) = SYMBOLS.iter().find(|(half, _)| *half == c) {
            out.push(*full);
        } else if ('!'..='~').contains(&c) {
            out.extend(char::from_u32(c as u32 + 0xFEE0));
        } else if ('\u{FF61}'..='\u{FF9F}').contains(&c) {
            // A voiced or semi-voiced mark joins the kana before it.
            let pair = cs.get(i).filter(|m| matches!(**m, 'ﾞ' | 'ﾟ')).and_then(|m| {
                let two: String = [c, *m].iter().collect();
                KANA.iter().find(|(_, half)| *half == two)
            });
            match pair {
                Some((full, _)) => {
                    out.push(*full);
                    i += 1;
                }
                None => out.push(KANA.iter().find(|(_, half)| half.chars().eq([c])).map_or(c, |(full, _)| *full)),
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub(super) fn asc(a: &[Arg], _c: &mut dyn Ctx) -> R<Value> {
    text_val(to_half(&text(a, 0)?))
}

pub(super) fn jis(a: &[Arg], _c: &mut dyn Ctx) -> R<Value> {
    check_len(to_full(&text(a, 0)?))
}

// ---------------------------------------------------------------------------------------------
// YEN, DATESTRING and NUMBERSTRING

/// `YEN(number, [decimals])`: the number rounded (to whole yen unless decimals says otherwise)
/// with thousands separators after a yen sign, as text.
pub(super) fn yen(a: &[Arg], _c: &mut dyn Ctx) -> R<Value> {
    let (r, d) = super::rounded(a, 0.0)?;
    let negative = r < 0.0;
    let body = super::format_fixed(r, d, true);
    let out = if negative { format!("-¥{body}") } else { format!("¥{body}") };
    if out.len() > 254 {
        return Err(CellError::Value);
    }
    text_val(out)
}

/// `DATESTRING(serial)`: the date in the Japanese era, 令和08年04月01日.
pub(super) fn datestring(a: &[Arg], c: &mut dyn Ctx) -> R<Value> {
    let serial = num(a, 0)?;
    let dt = gridcraft_core::date::datetime_from_serial(c.date_system(), serial.floor()).ok_or(CellError::Num)?;
    let day = (dt.year, dt.month, dt.day);
    const ERAS: [((i32, u32, u32), &str); 5] = [((1868, 9, 8), "明治"), ((1912, 7, 30), "大正"), ((1926, 12, 25), "昭和"), ((1989, 1, 8), "平成"), ((2019, 5, 1), "令和")];
    let (start, name) = ERAS.iter().rev().find(|(start, _)| day >= *start).unwrap_or(&ERAS[0]);
    text_val(format!("{name}{:02}年{:02}月{:02}日", dt.year - start.0 + 1, dt.month, dt.day))
}

/// `NUMBERSTRING(number, type)`: the number, rounded to a whole one, in kanji. Type 1 writes it
/// with units (一万二千三百四十五), type 2 with the formal numerals (壱萬弐阡参百四拾伍) and type 3
/// digit by digit (一二三四五). A negative number and any other type are `#NUM!`.
pub(super) fn numberstring(a: &[Arg], _c: &mut dyn Ctx) -> R<Value> {
    let n = round_half_away(num(a, 0)?, 0);
    let kind = num(a, 1)?.trunc();
    if !n.is_finite() || n < 0.0 {
        return Err(CellError::Num);
    }
    let digits = whole_digits(n);
    text_val(if kind == 1.0 {
        Kanji::Plain.positional(&digits)
    } else if kind == 2.0 {
        Kanji::Formal.positional(&digits)
    } else if kind == 3.0 {
        Kanji::Plain.digits(&digits)
    } else {
        return Err(CellError::Num);
    })
}

/// The digits of a non-negative whole number as Excel knows it: fifteen significant digits, then
/// zeros.
fn whole_digits(n: f64) -> String {
    if n < 1e15 {
        return format!("{n:.0}");
    }
    let s = format!("{n:.14e}");
    let Some((mantissa, exp)) = s.split_once('e') else { return format!("{n:.0}") };
    let Ok(exp) = exp.parse::<usize>() else { return format!("{n:.0}") };
    let mut out: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    // A double is below 1E+309: the text stays short.
    let len = (exp + 1).min(400);
    while out.len() < len {
        out.push('0');
    }
    out.truncate(len);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::testutil::*;

    /// The switch is the process's: these tests take turns.
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn with_dbcs<T>(on: bool, f: impl FnOnce() -> T) -> T {
        let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        set_dbcs(on);
        let out = f();
        set_dbcs(false);
        out
    }

    #[test]
    fn byte_functions_count_full_width_as_two() {
        with_dbcs(true, || {
            close(ev("LENB", vec![t("あいうabc")]), 9.0);
            close(ev("LENB", vec![t("ｱｲｳ")]), 3.0);
            is_text(ev("LEFTB", vec![t("東京都abc"), n(4.0)]), "東京");
            // Half of a character is a space.
            is_text(ev("LEFTB", vec![t("東京都"), n(3.0)]), "東 ");
            is_text(ev("RIGHTB", vec![t("東京都"), n(3.0)]), " 都");
            is_text(ev("RIGHTB", vec![t("abc東京"), n(5.0)]), "c東京");
            is_text(ev("MIDB", vec![t("東京都千代田区"), n(7.0), n(6.0)]), "千代田");
            is_text(ev("MIDB", vec![t("a東京"), n(3.0), n(2.0)]), "  ");
            is_text(ev("REPLACEB", vec![t("東京都港区"), n(7.0), n(2.0), t("中央")]), "東京都中央区");
            close(ev("FINDB", vec![t("都"), t("東京都")]), 5.0);
            close(ev("FINDB", vec![t("b"), t("あab")]), 4.0);
            close(ev("FINDB", vec![t("京"), t("京都東京"), n(2.0)]), 7.0);
            close(ev("SEARCHB", vec![t("?都"), t("東京都")]), 3.0);
            is_err(ev("FINDB", vec![t("x"), t("東京都")]), CellError::Value);
        });
    }

    #[test]
    fn byte_functions_follow_the_characters_without_dbcs() {
        with_dbcs(false, || {
            close(ev("LENB", vec![t("あいうabc")]), 6.0);
            is_text(ev("LEFTB", vec![t("東京都"), n(2.0)]), "東京");
            close(ev("FINDB", vec![t("都"), t("東京都")]), 3.0);
        });
    }

    #[test]
    fn asc_and_jis_change_the_width() {
        is_text(ev("ASC", vec![t("ＡＢＣ１２３　アイウ")]), "ABC123 ｱｲｳ");
        is_text(ev("ASC", vec![t("ガギグパ・ヴー「」。、")]), "ｶﾞｷﾞｸﾞﾊﾟ･ｳﾞｰ｢｣｡､");
        is_text(ev("ASC", vec![t("￥１，２００（税込）")]), "\\1,200(税込)");
        is_text(ev("ASC", vec![t("ひらがな漢字")]), "ひらがな漢字");
        is_text(ev("JIS", vec![t("ABC123 ｱｲｳ")]), "ＡＢＣ１２３　アイウ");
        is_text(ev("JIS", vec![t("ｶﾞｷﾞｸﾞﾊﾟ･ｳﾞｰ｢｣｡､ﾞ")]), "ガギグパ・ヴー「」。、゛");
        is_text(ev("JIS", vec![t("\\1,200")]), "￥１，２００");
        is_text(ev("DBCS", vec![t("a")]), "ａ");
        is_text(ev("JIS", vec![t("ﾊﾞﾅﾅ")]), "バナナ");
    }

    #[test]
    fn yen_and_datestring() {
        is_text(ev("YEN", vec![n(1234.567)]), "¥1,235");
        is_text(ev("YEN", vec![n(1234.567), n(1.0)]), "¥1,234.6");
        is_text(ev("YEN", vec![n(-1234.5)]), "-¥1,235");
        is_text(ev("YEN", vec![n(1234.0), n(-2.0)]), "¥1,200");
        is_text(ev("DATESTRING", vec![n(46113.0)]), "令和08年04月01日");
        is_text(ev("DATESTRING", vec![n(43585.0)]), "平成31年04月30日");
        is_text(ev("DATESTRING", vec![n(43586.5)]), "令和01年05月01日");
    }

    #[test]
    fn numberstring_writes_kanji() {
        is_text(ev("NUMBERSTRING", vec![n(12345.0), n(1.0)]), "一万二千三百四十五");
        is_text(ev("NUMBERSTRING", vec![n(12345.0), n(2.0)]), "壱萬弐阡参百四拾伍");
        is_text(ev("NUMBERSTRING", vec![n(12345.0), n(3.0)]), "一二三四五");
        is_text(ev("NUMBERSTRING", vec![n(123.0), n(1.0)]), "百二十三");
        is_text(ev("NUMBERSTRING", vec![n(123.0), n(2.0)]), "壱百弐拾参");
        is_text(ev("NUMBERSTRING", vec![n(123.0), n(3.0)]), "一二三");
        is_text(ev("NUMBERSTRING", vec![n(1234567890.0), n(1.0)]), "十二億三千四百五十六万七千八百九十");
        is_text(ev("NUMBERSTRING", vec![n(1000.0), n(1.0)]), "千");
        is_text(ev("NUMBERSTRING", vec![n(1000.0), n(2.0)]), "壱阡");
        is_text(ev("NUMBERSTRING", vec![n(1000.0), n(3.0)]), "一〇〇〇");
        is_text(ev("NUMBERSTRING", vec![n(0.0), n(1.0)]), "〇");
        is_text(ev("NUMBERSTRING", vec![n(0.0), n(3.0)]), "〇");
        // Rounded to a whole number.
        is_text(ev("NUMBERSTRING", vec![n(1234.5), n(1.0)]), "千二百三十五");
        is_text(ev("NUMBERSTRING", vec![n(0.4), n(1.0)]), "〇");
        is_text(ev("NUMBERSTRING", vec![t("10800"), n(2.0)]), "壱萬八百");
        is_text(ev("NUMBERSTRING", vec![n(1e15), n(1.0)]), "千兆");
        is_text(ev("NUMBERSTRING", vec![n(123456789012345678.0), n(3.0)]), "一二三四五六七八九〇一二三四六〇〇〇");
        is_err(ev("NUMBERSTRING", vec![n(-1.0), n(1.0)]), CellError::Num);
        is_err(ev("NUMBERSTRING", vec![n(1.0), n(4.0)]), CellError::Num);
        is_err(ev("NUMBERSTRING", vec![n(1.0), n(0.0)]), CellError::Num);
        is_err(ev("NUMBERSTRING", vec![t("abc"), n(1.0)]), CellError::Value);
    }
}
