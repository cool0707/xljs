//! Number formats: the text a cell shows (Excel's formatting of numbers and
//! dates with a format code), for the common codes.

use gridcraft_core::kanji::Kanji;

fn sections(code: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut in_quote = false;
    let mut esc = false;
    for c in code.chars() {
        if esc {
            esc = false;
            out.last_mut().unwrap().push(c);
            continue;
        }
        match c {
            '\\' => {
                esc = true;
                out.last_mut().unwrap().push(c);
            }
            '"' => {
                in_quote = !in_quote;
                out.last_mut().unwrap().push(c);
            }
            ';' if !in_quote => out.push(String::new()),
            _ => out.last_mut().unwrap().push(c),
        }
    }
    out
}

/// Removes [Red], [$-411] and similar bracketed parts (not [h] etc.).
fn strip_brackets(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    let mut in_quote = false;
    while let Some(c) = chars.next() {
        if c == '"' {
            in_quote = !in_quote;
        }
        if c == '[' && !in_quote {
            let mut inner = String::new();
            for d in chars.by_ref() {
                if d == ']' {
                    break;
                }
                inner.push(d);
            }
            let l = inner.to_lowercase();
            if l.chars().all(|c| matches!(c, 'h' | 'm' | 's')) && !l.is_empty() {
                out.push('[');
                out.push_str(&inner);
                out.push(']');
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// The kanji a section's `[DBNum1]`, `[DBNum2]` or `[DBNum3]` tag asks its
/// numbers to be written in, as Japanese Excel does (漢数字, 大字, 全角).
fn kanji_of(sec: &str) -> Option<Kanji> {
    let mut chars = sec.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                chars.by_ref().find(|d| *d == '"');
            }
            '\\' | '_' | '*' => {
                chars.next();
            }
            '[' => {
                let inner: String = chars.by_ref().take_while(|d| *d != ']').collect();
                if let Some(k) = Kanji::from_tag(&inner) {
                    return Some(k);
                }
            }
            _ => {}
        }
    }
    None
}

/// The text of the literal parts of a section (quotes, escapes and spacers
/// resolved).
fn literal(part: &str) -> String {
    let mut out = String::new();
    let mut chars = part.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => out.extend(chars.by_ref().take_while(|d| *d != '"')),
            '\\' => out.extend(chars.next()),
            '_' => {
                chars.next();
                out.push(' ');
            }
            '*' => {
                chars.next();
            }
            _ => out.push(c),
        }
    }
    out
}

/// Splits a section (without its tags) at the General keyword, `General` or
/// `G/標準` as Japanese Excel writes it: what stands before and after it.
fn split_general(sec: &str) -> Option<(&str, &str)> {
    let mut quoted = false;
    let mut skip = false;
    for (i, c) in sec.char_indices() {
        if skip {
            skip = false;
            continue;
        }
        match c {
            '"' => quoted = !quoted,
            '\\' | '_' | '*' if !quoted => skip = true,
            'G' | 'g' if !quoted => {
                let rest = &sec[i..];
                for word in ["general", "g/標準"] {
                    if rest.get(..word.len()).is_some_and(|w| w.eq_ignore_ascii_case(word)) {
                        return Some((&sec[..i], &rest[word.len()..]));
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// A number format as files hold it: `G/標準`, which is how Japanese Excel
/// shows the General keyword (`[DBNum1][$-411]G/標準`), is `General` there.
pub fn file_code(code: &str) -> std::borrow::Cow<'_, str> {
    if !code.contains("/標準") {
        return code.into();
    }
    let mut out = String::with_capacity(code.len());
    let mut rest = code;
    let (mut quoted, mut skip) = (false, false);
    while let Some(c) = rest.chars().next() {
        let word = "g/標準";
        if !quoted && !skip && rest.get(..word.len()).is_some_and(|w| w.eq_ignore_ascii_case(word)) {
            out.push_str("General");
            rest = &rest[word.len()..];
            continue;
        }
        match c {
            _ if skip => skip = false,
            '"' => quoted = !quoted,
            '\\' | '_' | '*' if !quoted => skip = true,
            _ => {}
        }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out.into()
}

/// General: up to 11 significant digits, like Excel.
pub fn general(f: f64) -> String {
    if f == f.trunc() && f.abs() < 1e15 {
        return format!("{}", f as i64);
    }
    let a = f.abs();
    if (1e-9..1e15).contains(&a) {
        let digits = 10 - a.log10().floor() as i32;
        let s = format!("{:.*}", digits.clamp(0, 15) as usize, f);
        let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
        return s;
    }
    format!("{f:E}")
}

/// Formats a number with a (non-date) format code.
pub fn format_number(f: f64, code: &str) -> String {
    let secs = sections(code);
    let (sec, val, neg_sign) = if secs.len() >= 2 && f < 0.0 {
        (secs[1].clone(), -f, false)
    } else if secs.len() >= 3 && f == 0.0 {
        (secs[2].clone(), f, false)
    } else {
        (secs[0].clone(), f, f < 0.0)
    };
    let kanji = kanji_of(&sec);
    let sec = strip_brackets(&sec);
    if kanji.is_none() && (sec.eq_ignore_ascii_case("general") || sec.is_empty()) {
        return general(f);
    }
    if sec == "@" {
        return general(f);
    }
    // General with text around it (`General"個"`), and in kanji: `[DBNum1]General`
    // writes the whole part with units (千二百三十四); the tag alone means the same.
    let around = if sec.is_empty() { Some(("", "")) } else { split_general(&sec) };
    if let Some((before, after)) = around {
        let number = general(val.abs());
        let number = kanji.map_or(number.clone(), |k| k.general(&number));
        return format!("{}{}{number}{}", if neg_sign { "-" } else { "" }, literal(before), literal(after));
    }
    // With placeholders the digits are written in kanji one for one (一二三四).
    let shown = |digits: &str| kanji.map_or(digits.to_string(), |k| k.digits(digits));
    // Split into literal parts and the number part.
    let mut prefix = String::new();
    let mut suffix = String::new();
    let mut num = String::new();
    let mut seen_digit = false;
    let mut chars = sec.chars().peekable();
    let mut percent = 0;
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                let mut lit = String::new();
                for d in chars.by_ref() {
                    if d == '"' {
                        break;
                    }
                    lit.push(d);
                }
                if seen_digit { suffix.push_str(&lit) } else { prefix.push_str(&lit) }
            }
            '\\' => {
                if let Some(d) = chars.next() {
                    if seen_digit { suffix.push(d) } else { prefix.push(d) }
                }
            }
            '_' => {
                chars.next();
                if seen_digit { suffix.push(' ') } else { prefix.push(' ') }
            }
            '*' => {
                chars.next();
            }
            '0' | '#' | '?' | ',' | '.' if !(seen_digit && !suffix.is_empty()) => {
                if matches!(c, '0' | '#' | '?') {
                    seen_digit = true;
                }
                if c == ',' && !seen_digit {
                    prefix.push(c);
                } else {
                    num.push(c);
                }
            }
            'E' | 'e' if seen_digit => {
                // Scientific: fall back to Rust's exponent format.
                let dec = num.split('.').nth(1).map(|d| d.len()).unwrap_or(0);
                let s = format!("{:.*E}", dec, val);
                let (m, e) = s.split_once('E').unwrap();
                let e: i32 = e.parse().unwrap_or(0);
                let sign = if e < 0 { '-' } else { '+' };
                return format!("{}{prefix}{}E{sign}{}{suffix}", if neg_sign { "-" } else { "" }, shown(m), shown(&format!("{:02}", e.abs())));
            }
            '%' => {
                percent += 1;
                if seen_digit { suffix.push('%') } else { prefix.push('%') }
            }
            _ => {
                if seen_digit { suffix.push(c) } else { prefix.push(c) }
            }
        }
    }
    let mut v = val.abs() * 100f64.powi(percent);
    // Trailing commas scale by 1000.
    while num.ends_with(',') {
        num.pop();
        v /= 1000.0;
    }
    let (int_pat, dec_pat) = match num.split_once('.') {
        Some((i, d)) => (i.to_string(), d.to_string()),
        None => (num.clone(), String::new()),
    };
    let grouping = int_pat.contains(',');
    let int_pat: String = int_pat.chars().filter(|c| *c != ',').collect();
    let min_dec = dec_pat.chars().filter(|c| *c == '0').count();
    let max_dec = dec_pat.chars().filter(|c| matches!(c, '0' | '#' | '?')).count();
    let s = format!("{:.*}", max_dec, v);
    let (ip, mut dp) = match s.split_once('.') {
        Some((i, d)) => (i.to_string(), d.to_string()),
        None => (s.clone(), String::new()),
    };
    while dp.len() > min_dec && dp.ends_with('0') {
        dp.pop();
    }
    let min_int = int_pat.chars().filter(|c| *c == '0').count();
    let mut ip = if ip == "0" && min_int == 0 { String::new() } else { ip };
    while ip.len() < min_int {
        ip.insert(0, '0');
    }
    if grouping {
        let digits: Vec<char> = ip.chars().collect();
        let mut g = String::new();
        for (i, c) in digits.iter().enumerate() {
            if i > 0 && (digits.len() - i).is_multiple_of(3) {
                g.push(',');
            }
            g.push(*c);
        }
        ip = g;
    }
    let mut out = String::new();
    if neg_sign && (v != 0.0 || !dp.is_empty()) {
        out.push('-');
    }
    out.push_str(&prefix);
    out.push_str(&shown(&ip));
    if !dp.is_empty() || dec_pat.contains('0') {
        out.push('.');
        out.push_str(&shown(&dp));
    }
    out.push_str(&suffix);
    out
}

const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const DAYS_JA: [&str; 7] = ["日", "月", "火", "水", "木", "金", "土"];

/// Japanese eras: start date, initial, name.
const ERAS: [((i32, u32, u32), &str, &str); 5] = [
    ((1868, 9, 8), "M", "明治"),
    ((1912, 7, 30), "T", "大正"),
    ((1926, 12, 25), "S", "昭和"),
    ((1989, 1, 8), "H", "平成"),
    ((2019, 5, 1), "R", "令和"),
];

/// The era (index into ERAS) and the year in it. Dates before Meiji count as Meiji.
fn era_of(y: i32, mo: u32, d: u32) -> (usize, i32) {
    let k = ERAS.iter().rposition(|(s, _, _)| (y, mo, d) >= *s).unwrap_or(0);
    (k, y - ERAS[k].0.0 + 1)
}

/// Formats a wall-clock time (y, month, d, h, mi, s, ms) with a date code.
pub fn format_date(t: (i32, u32, u32, u32, u32, u32, u32), code: &str) -> String {
    let (y, mo, d, h, mi, s, ms) = t;
    let first = sections(code).into_iter().next().unwrap_or_default();
    // [$-F800] / [$-F400]: the system's long date / time format, whatever follows.
    let upper = first.to_uppercase();
    let ja = crate::i18n::lang() == "ja";
    let first = if upper.contains("[$-F800]") {
        (if ja { "yyyy\"年\"m\"月\"d\"日\"" } else { "dddd, mmmm d, yyyy" }).to_string()
    } else if upper.contains("[$-F400]") {
        (if ja { "h:mm:ss" } else { "h:mm:ss AM/PM" }).to_string()
    } else {
        first
    };
    // [$-ja-JP-x-gannen]: the first year of an era as 元年.
    let gannen = upper.contains("GANNEN");
    // [DBNum1]: the numbers in kanji. A four-digit year and a number with a
    // leading zero digit by digit (二〇二六, 〇七), the others with units (三十);
    // [DBNum3] keeps (full-width) digits.
    let kanji = kanji_of(&first);
    let num = |out: &mut String, digits: String| match kanji {
        Some(k) => out.push_str(&k.date_part(&digits)),
        None => out.push_str(&digits),
    };
    let sec = strip_brackets(&first);
    let era = era_of(y, mo, d);
    let weekday = chrono::NaiveDate::from_ymd_opt(y, mo, d).map(|dt| {
        use chrono::Datelike;
        dt.weekday().num_days_from_sunday() as usize
    });
    let ampm = sec.to_lowercase().contains("am/pm") || sec.to_lowercase().contains("a/p");
    let chars: Vec<char> = sec.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let mut last_was_hour = false;
    let run = |i: usize, c: char| chars[i..].iter().take_while(|x| x.eq_ignore_ascii_case(&c)).count();
    while i < chars.len() {
        let c = chars[i];
        let lc = c.to_ascii_lowercase();
        match lc {
            '"' => {
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    out.push(chars[i]);
                    i += 1;
                }
                i += 1;
            }
            '\\' => {
                if i + 1 < chars.len() {
                    out.push(chars[i + 1]);
                }
                i += 2;
            }
            'y' => {
                let n = run(i, 'y');
                num(&mut out, if n <= 2 { format!("{:02}", y % 100) } else { format!("{y:04}") });
                i += n;
            }
            'm' => {
                let n = run(i, 'm');
                // Minutes after hours or before seconds.
                let rest: String = chars[i + n..].iter().collect::<String>().to_lowercase();
                let next_is_sec = rest.trim_start_matches(|c: char| !c.is_ascii_alphabetic()).starts_with('s');
                if (last_was_hour || next_is_sec) && n <= 2 {
                    num(&mut out, if n == 2 { format!("{mi:02}") } else { mi.to_string() });
                } else {
                    match n {
                        1 => num(&mut out, mo.to_string()),
                        2 => num(&mut out, format!("{mo:02}")),
                        3 => out.push_str(&MONTHS[(mo as usize + 11) % 12][..3]),
                        5 => out.push_str(&MONTHS[(mo as usize + 11) % 12][..1]),
                        _ => out.push_str(MONTHS[(mo as usize + 11) % 12]),
                    }
                }
                last_was_hour = false;
                i += n;
            }
            'd' => {
                let n = run(i, 'd');
                match n {
                    1 => num(&mut out, d.to_string()),
                    2 => num(&mut out, format!("{d:02}")),
                    3 => out.push_str(weekday.map(|w| &DAYS[w][..3]).unwrap_or("")),
                    _ => out.push_str(weekday.map(|w| DAYS[w]).unwrap_or("")),
                }
                i += n;
            }
            'a' if chars[i..].iter().collect::<String>().to_lowercase().starts_with("aaa") => {
                let n = run(i, 'a');
                if let Some(w) = weekday {
                    out.push_str(if n >= 4 { ["日曜日", "月曜日", "火曜日", "水曜日", "木曜日", "金曜日", "土曜日"][w] } else { DAYS_JA[w] });
                }
                i += n;
            }
            'a' if chars[i..].iter().collect::<String>().to_lowercase().starts_with("am/pm") => {
                out.push_str(if h < 12 { "AM" } else { "PM" });
                i += 5;
            }
            'a' if chars[i..].iter().collect::<String>().to_lowercase().starts_with("a/p") => {
                out.push(if h < 12 { 'A' } else { 'P' });
                i += 3;
            }
            'g' => {
                let n = run(i, 'g');
                let (k, _) = era;
                match n {
                    1 => out.push_str(ERAS[k].1),
                    2 => out.extend(ERAS[k].2.chars().take(1)),
                    _ => out.push_str(ERAS[k].2),
                }
                i += n;
            }
            'e' => {
                let n = run(i, 'e');
                let (_, ey) = era;
                if gannen && ey == 1 && n == 1 {
                    out.push('元');
                } else if n >= 2 {
                    num(&mut out, format!("{ey:02}"));
                } else {
                    num(&mut out, ey.to_string());
                }
                i += n;
            }
            'h' => {
                let n = run(i, 'h');
                let hh = if ampm { if h % 12 == 0 { 12 } else { h % 12 } } else { h };
                num(&mut out, if n >= 2 { format!("{hh:02}") } else { hh.to_string() });
                last_was_hour = true;
                i += n;
            }
            's' => {
                let n = run(i, 's');
                num(&mut out, if n >= 2 { format!("{s:02}") } else { s.to_string() });
                i += n;
                // Fractions of seconds: .0, .00, .000
                if i < chars.len() && chars[i] == '.' {
                    let z = chars[i + 1..].iter().take_while(|c| **c == '0').count();
                    if z > 0 {
                        let frac = format!("{:03}", ms);
                        out.push('.');
                        let frac = &frac[..z.min(3)];
                        out.push_str(&kanji.map_or(frac.to_string(), |k| k.digits(frac)));
                        i += 1 + z;
                    }
                }
                last_was_hour = false;
            }
            '[' => {
                // [h]:mm elapsed hours (not expected for dates); keep the text.
                i += 1;
            }
            ']' => i += 1,
            '_' => {
                out.push(' ');
                i += 2;
            }
            '*' => i += 2,
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(format_number(1200.0, "#,##0"), "1,200");
        assert_eq!(format_number(1234567.891, "#,##0.00"), "1,234,567.89");
        assert_eq!(format_number(0.256, "0.0%"), "25.6%");
        assert_eq!(format_number(3.0, "0.0"), "3.0");
        assert_eq!(format_number(-5.0, "#,##0;[Red]-#,##0"), "-5");
        assert_eq!(format_number(800.5, "General"), "800.5");
        assert_eq!(format_number(12.0, "0\"円\""), "12円");
        assert_eq!(general(0.1 + 0.2), "0.3");
    }

    #[test]
    fn dates() {
        let t = (2026, 4, 1, 13, 5, 9, 0);
        assert_eq!(format_date(t, "yyyy/m/d"), "2026/4/1");
        assert_eq!(format_date(t, "yyyy/m/d h:mm"), "2026/4/1 13:05");
        assert_eq!(format_date(t, "yyyy\"年\"m\"月\"d\"日\"(aaa)"), "2026年4月1日(水)");
        assert_eq!(format_date(t, "h:mm:ss AM/PM"), "1:05:09 PM");
        assert_eq!(format_date(t, "mm-dd-yy"), "04-01-26");
    }

    #[test]
    fn japanese_eras() {
        let t = (2026, 4, 1, 0, 0, 0, 0);
        assert_eq!(format_date(t, "[$-411]ggge\"年\"m\"月\"d\"日\""), "令和8年4月1日");
        assert_eq!(format_date(t, "[$-411]ge.m.d"), "R8.4.1");
        assert_eq!(format_date(t, "gg ee"), "令 08");
        assert_eq!(format_date((2019, 4, 30, 0, 0, 0, 0), "ggge年"), "平成31年");
        assert_eq!(format_date((2019, 5, 1, 0, 0, 0, 0), "ggge年"), "令和1年");
        assert_eq!(format_date((2019, 5, 1, 0, 0, 0, 0), "[$-ja-JP-x-gannen]ggge\"年\""), "令和元年");
        assert_eq!(format_date((1989, 1, 7, 0, 0, 0, 0), "ggge"), "昭和64");
        assert_eq!(format_date((1926, 12, 25, 0, 0, 0, 0), "ggge"), "昭和1");
        assert!(crate::api::xlsx::date_format_code("[$-411]ggge\"年\"m\"月\"d\"日\""));
        assert!(crate::api::xlsx::date_format_code("ggge"));
        assert!(!crate::api::xlsx::date_format_code("0.00E+00"));
    }

    #[test]
    fn general_with_text() {
        assert_eq!(format_number(3.0, "General\"個\""), "3個");
        assert_eq!(format_number(-1234.5, "\"約\"General"), "-約1234.5");
        assert_eq!(format_number(1234.5, "G/標準"), "1234.5");
        assert_eq!(format_number(12.0, "[$-411]General"), "12");
    }

    #[test]
    fn kanji_numbers() {
        // 漢数字・大字: the whole part with units.
        assert_eq!(format_number(1234.0, "[DBNum1][$-411]General"), "千二百三十四");
        assert_eq!(format_number(12345.0, "[DBNum1]G/標準"), "一万二千三百四十五");
        assert_eq!(format_number(123456.0, "[DBNum1]"), "十二万三千四百五十六");
        assert_eq!(format_number(123456.0, "[DBNum2][$-411]General"), "壱拾弐萬参阡四百伍拾六");
        assert_eq!(format_number(987654321.0, "[DBNum2]General"), "九億八阡七百六拾伍萬四阡参百弐拾壱");
        assert_eq!(format_number(987654321.0, "[DBNum3]General"), "９億８千７百６十５万４千３百２十１");
        assert_eq!(format_number(0.0, "[DBNum1]General"), "〇");
        assert_eq!(format_number(-1234.0, "[DBNum1]General"), "-千二百三十四");
        assert_eq!(format_number(1234.5, "[DBNum1]General"), "千二百三十四.五");
        assert_eq!(format_number(123456789012.0, "[DBNum1]General"), "千二百三十四億五千六百七十八万九千十二");
        assert_eq!(format_number(10800.0, "\"金\"[DBNum2][$-411]General\"円也\""), "金壱萬八百円也");
        assert_eq!(format_number(1500.0, "[DBNum1]General\"円\""), "千五百円");
        // 全角 and the digit placeholders: digit by digit.
        assert_eq!(format_number(123456789.0, "[DBNum1]0"), "一二三四五六七八九");
        assert_eq!(format_number(12345.0, "[DBNum1]#,##0"), "一二,三四五");
        assert_eq!(format_number(10.5, "[DBNum1]0.00"), "一〇.五〇");
        assert_eq!(format_number(12345.0, "[DBNum2]0"), "壱弐参四伍");
        assert_eq!(format_number(1234.0, "[DBNum3][$-411]0"), "１２３４");
        assert_eq!(format_number(1200.0, "[DBNum3][$-411]#,##0\"円\""), "１,２００円");
        assert_eq!(format_number(-12.0, "[DBNum1]0"), "-一二");
        // The digits of literal text are its own; each section has its tag.
        assert_eq!(format_number(12.0, "[DBNum1]\"第1期 \"0"), "第1期 一二");
        assert_eq!(format_number(-12.0, "[DBNum1]0;[DBNum3]0"), "１２");
        assert_eq!(format_number(-12.0, "0;[DBNum1]General"), "十二");
        assert_eq!(format_number(12.0, "[DBNum4]0"), "12");
        assert_eq!(format_number(12.0, "\"[DBNum1]\"0"), "[DBNum1]12");
    }

    #[test]
    fn kanji_dates() {
        let t = (2026, 10, 7, 12, 5, 9, 250);
        assert_eq!(format_date(t, "[DBNum1][$-411]yyyy\"年\"m\"月\"d\"日\""), "二〇二六年十月七日");
        assert_eq!(format_date(t, "[DBNum1][$-411]ggge\"年\"m\"月\"d\"日\""), "令和八年十月七日");
        assert_eq!(format_date((2019, 4, 30, 0, 0, 0, 0), "[DBNum1]ggge\"年\"m\"月\"d\"日\""), "平成三十一年四月三十日");
        assert_eq!(format_date((2026, 12, 22, 0, 0, 0, 0), "[DBNum1]m\"月\"d\"日\"(aaa)"), "十二月二十二日(火)");
        assert_eq!(format_date((2019, 5, 1, 0, 0, 0, 0), "[DBNum1][$-ja-JP-x-gannen]ggge\"年\""), "令和元年");
        assert_eq!(format_date(t, "[DBNum1]yyyy/mm/dd"), "二〇二六/十/〇七");
        assert_eq!(format_date(t, "[DBNum1]h\"時\"mm\"分\""), "十二時〇五分");
        assert_eq!(format_date(t, "[DBNum1]s.00"), "九.二五");
        assert_eq!(format_date(t, "[DBNum1]d-mmm"), "七-Oct");
        assert_eq!(format_date((2019, 4, 30, 0, 0, 0, 0), "[DBNum2]ggge\"年\"m\"月\"d\"日\""), "平成参拾壱年四月参拾日");
        assert_eq!(format_date((2026, 12, 22, 0, 0, 0, 0), "[DBNum3][$-411]yyyy\"年\"m\"月\"d\"日\""), "２０２６年１２月２２日");
        assert_eq!(format_date(t, "[DBNum3]ggge\"年\"mm\"月\"dd\"日\""), "令和８年１０月０７日");
        assert!(crate::api::xlsx::date_format_code("[DBNum1][$-411]ggge\"年\"m\"月\"d\"日\""));
        assert!(!crate::api::xlsx::date_format_code("[DBNum1][$-411]General"));
        assert!(!crate::api::xlsx::date_format_code("[DBNum3][$-411]#,##0"));
    }

    #[test]
    fn system_formats() {
        let t = (2026, 7, 30, 13, 5, 9, 0);
        let code = "[$-F800]dddd\\,\\ mmmm\\ dd\\,\\ yyyy";
        {
            let _g = crate::testutil::lock_lang("ja");
            assert_eq!(format_date(t, code), "2026年7月30日");
            assert_eq!(format_date(t, "[$-F400]h:mm:ss\\ AM/PM"), "13:05:09");
        }
        let _g = crate::testutil::lock_lang("en");
        assert_eq!(format_date(t, code), "Thursday, July 30, 2026");
    }

    #[test]
    fn general_as_files_hold_it() {
        assert_eq!(file_code("[DBNum1][$-411]G/標準"), "[DBNum1][$-411]General");
        assert_eq!(file_code("g/標準\"個\""), "General\"個\"");
        assert_eq!(file_code("G/標準;-G/標準"), "General;-General");
        // Not the keyword: quoted, escaped, or another text.
        assert_eq!(file_code("0\"G/標準\""), "0\"G/標準\"");
        assert_eq!(file_code("#,##0"), "#,##0");
        assert_eq!(file_code("0\"/標準\""), "0\"/標準\"");
        // "G/標準" is no date format (the g is not the era).
        assert!(!crate::api::xlsx::date_format_code("[DBNum1]G/標準"));
        assert!(!crate::api::xlsx::date_format_code("G/標準\"個\""));
    }
}
