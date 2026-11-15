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

/// The digits of `a` (> 0) to 15 significant digits, as Excel holds numbers:
/// the digits and the position of the decimal point in them (1234.5 →
/// "123450000000000", 4; 0.00123 → "123000000000000", -2).
fn digits15(a: f64) -> (Vec<u8>, i32) {
    let s = format!("{a:.14e}");
    let (m, e) = s.split_once('e').unwrap();
    let digits = m.bytes().filter(u8::is_ascii_digit).map(|b| b - b'0').collect();
    (digits, e.parse::<i32>().unwrap() + 1)
}

/// `a` (>= 0) with `dec` decimals, rounding half away from zero as Excel does
/// (1234.5 → "1235" with none, 1.005 → "1.01" with two).
fn round_fixed(a: f64, dec: usize) -> String {
    if a == 0.0 || !a.is_finite() {
        return if dec == 0 { "0".into() } else { format!("0.{}", "0".repeat(dec)) };
    }
    let (d, point) = digits15(a);
    // All the digits with the point at `int`: zeros in front when it is not in them.
    let lead = (-point).max(0) as usize;
    let mut all: Vec<u8> = std::iter::repeat_n(0, lead).chain(d).collect();
    let int = point.max(0) as usize;
    if all.len() < int + dec + 1 {
        all.resize(int + dec + 1, 0);
    }
    let up = all[int + dec] >= 5;
    all.truncate(int + dec);
    if up {
        let mut i = all.len();
        loop {
            if i == 0 {
                all.insert(0, 1);
                break;
            }
            i -= 1;
            if all[i] == 9 {
                all[i] = 0;
            } else {
                all[i] += 1;
                break;
            }
        }
    }
    let int = all.len() - dec;
    let ip: String = all[..int].iter().map(|d| (b'0' + d) as char).collect();
    let ip = ip.trim_start_matches('0');
    let ip = if ip.is_empty() { "0" } else { ip };
    if dec == 0 {
        ip.to_string()
    } else {
        let dp: String = all[int..].iter().map(|d| (b'0' + d) as char).collect();
        format!("{ip}.{dp}")
    }
}

/// `a` (> 0) as a mantissa of `sig` significant digits (rounded half away
/// from zero) and the exponent: 123456789012 → ("1.23457", 11) for 6.
fn round_sig(a: f64, sig: usize) -> (String, i32) {
    let (_, point) = digits15(a);
    let exp = point - 1;
    let m = round_fixed(a / 10f64.powi(exp), sig - 1);
    // 9.999999 → "10.00000": one more in the exponent.
    if m.starts_with("10") {
        return (round_fixed(a / 10f64.powi(exp + 1), sig - 1), exp + 1);
    }
    (m, exp)
}

/// The significant digits a decimal text shows ("0.000015" → 2, "1000" → 1).
fn significant(s: &str) -> usize {
    let d: String = s.chars().filter(char::is_ascii_digit).collect();
    d.trim_start_matches('0').trim_end_matches('0').len()
}

/// Without the zeros a decimal text ends with ("1.500" → "1.5", "2.0" → "2").
fn trim_decimals(s: &str) -> &str {
    if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.') } else { s }
}

/// General, as Excel shows it in a column wide enough: at most 11 characters
/// besides the sign. Integers up to 11 digits in full, other numbers rounded
/// to fit; in the exponent form (`1.23457E+11`, `3.33333E-05`, `1E+20`) when
/// that keeps more significant digits.
pub fn general(f: f64) -> String {
    if f == 0.0 || !f.is_finite() {
        return if f.is_nan() { "NaN".into() } else { "0".into() };
    }
    let sign = if f < 0.0 { "-" } else { "" };
    let a = f.abs();
    let (_, point) = digits15(a);
    let exp_digits = if (point - 1).abs() >= 100 { 3 } else { 2 };
    let (m, exp) = round_sig(a, 11 - 3 - exp_digits);
    let m = trim_decimals(&m);
    let sci = format!("{m}E{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs());
    let decimal = (a < 1e11).then(|| {
        let int = point.max(1) as usize;
        let s = round_fixed(a, 10usize.saturating_sub(int));
        trim_decimals(&s).to_string()
    });
    match decimal {
        Some(d) if d.len() <= 11 && significant(&d) >= significant(m) => format!("{sign}{d}"),
        _ => format!("{sign}{sci}"),
    }
}

/// The whole number in General for the kanji of `[DBNum1]General`, which
/// Excel writes out however long it is (千二百三十四億…).
fn general_digits(f: f64) -> String {
    if f == f.trunc() && f.abs() < 1e15 {
        return format!("{}", f as i64);
    }
    let a = f.abs();
    if (1e-9..1e15).contains(&a) {
        let digits = (10 - a.log10().floor() as i32).clamp(0, 15) as usize;
        let s = round_fixed(a, digits);
        return format!("{}{}", if f < 0.0 { "-" } else { "" }, trim_decimals(&s));
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
        let number = match kanji {
            Some(k) => k.general(&general_digits(val.abs())),
            None => general(val.abs()),
        };
        return format!("{}{}{number}{}", if neg_sign { "-" } else { "" }, literal(before), literal(after));
    }
    if let Some(s) = fraction(val.abs(), &sec, neg_sign) {
        return s;
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
                // Scientific: `0.00E+00`, and with `#` in front an exponent that is
                // a multiple of their count (`##0.0E+0`: 46.1E+3, 500.0E-3).
                let plus = chars.next() == Some('+');
                let mut exp_width = 0;
                while chars.peek().is_some_and(|c| matches!(c, '0' | '#' | '?')) {
                    chars.next();
                    exp_width += 1;
                }
                let rest: String = chars.collect();
                let (int_pat, dec_pat) = num.split_once('.').unwrap_or((&num, ""));
                let n_int = int_pat.chars().filter(|c| matches!(c, '0' | '#' | '?')).count().max(1) as i32;
                let dec = dec_pat.chars().filter(|c| matches!(c, '0' | '#' | '?')).count();
                let a = val.abs();
                let exp_of = |a: f64| {
                    if a == 0.0 {
                        return 0;
                    }
                    let e = digits15(a).1 - 1;
                    if int_pat.contains('#') { e.div_euclid(n_int) * n_int } else { e - (n_int - 1) }
                };
                let mut exp = exp_of(a);
                let mut m = round_fixed(a / 10f64.powi(exp), dec);
                // Rounded up to one more digit (9.99 → 10.0): the next exponent.
                let rounded = m.parse::<f64>().unwrap_or(0.0) * 10f64.powi(exp);
                if a != 0.0 && exp_of(rounded) != exp {
                    exp = exp_of(rounded);
                    m = round_fixed(a / 10f64.powi(exp), dec);
                }
                let min_int = int_pat.chars().filter(|c| *c == '0').count();
                let (ip, dp) = m.split_once('.').unwrap_or((&m, ""));
                let ip = if ip == "0" && min_int == 0 { "" } else { ip };
                let mantissa = if dec_pat.is_empty() && !num.contains('.') { ip.to_string() } else { format!("{ip}.{dp}") };
                let sign = if exp < 0 { "-" } else if plus { "+" } else { "" };
                let e = format!("{:0w$}", exp.abs(), w = exp_width.max(1));
                return format!("{}{prefix}{}E{sign}{}{suffix}{}", if neg_sign { "-" } else { "" }, shown(&mantissa), shown(&e), literal(&rest));
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
    let s = round_fixed(v, max_dec);
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

/// The fraction of a section with `/` between digit placeholders (`# ?/?`,
/// `# ??/??`, `?/?`, `# ?/4`): the whole part (when the section has one) and
/// the closest fraction whose denominator has at most as many digits, as
/// Excel finds it (the last convergent of the continued fraction: 0.5625 is
/// 4/7 with one digit, 9/16 with two). None for other sections.
fn fraction(a: f64, sec: &str, neg_sign: bool) -> Option<String> {
    let is_ph = |c: char| matches!(c, '0' | '#' | '?');
    let chars: Vec<char> = sec.chars().collect();
    // The slash outside quotes, with a placeholder right before it.
    let mut slash = None;
    let mut quoted = false;
    for (i, c) in chars.iter().enumerate() {
        match c {
            '"' => quoted = !quoted,
            '/' if !quoted && i > 0 && is_ph(chars[i - 1]) => {
                slash = Some(i);
                break;
            }
            _ => {}
        }
    }
    let slash = slash?;
    let mut num_start = slash;
    while num_start > 0 && is_ph(chars[num_start - 1]) {
        num_start -= 1;
    }
    let mut den_end = slash + 1;
    while den_end < chars.len() && (is_ph(chars[den_end]) || chars[den_end].is_ascii_digit()) {
        den_end += 1;
    }
    let den_pat: String = chars[slash + 1..den_end].iter().collect();
    if den_pat.is_empty() {
        return None;
    }
    // The whole part: the placeholders before the numerator, and what stands
    // before and after them.
    let head: String = chars[..num_start].iter().collect();
    let (before, int_pat, gap) = match head.rfind(is_ph) {
        Some(e) => {
            let e = e + 1;
            let start = head[..e].rfind(|c: char| !is_ph(c)).map_or(0, |i| i + 1);
            (&head[..start], &head[start..e], &head[e..])
        }
        None => (head.as_str(), "", ""),
    };
    let num_pat: String = chars[num_start..slash].iter().collect();
    let tail: String = chars[den_end..].iter().collect();

    let fixed = den_pat.chars().all(|c| c.is_ascii_digit()) && !den_pat.starts_with('0');
    let (mut whole, frac) = if int_pat.is_empty() { (0.0, a) } else { (a.trunc(), a.fract()) };
    let (mut n, d) = if fixed {
        let d: f64 = den_pat.parse().ok()?;
        ((frac * d).round(), d)
    } else {
        convergent(frac, 10f64.powi(den_pat.len() as i32) - 1.0)
    };
    if !int_pat.is_empty() && n >= d {
        whole += 1.0;
        n = 0.0;
    }
    // `?` pads with spaces, `0` with zeros: the numerator on the left, the denominator on the right.
    let pad = |pat: &str, s: String, left: bool| {
        let width = pat.chars().filter(|c| matches!(c, '0' | '?')).count();
        let fill = if pat.contains('0') { '0' } else { ' ' };
        let fill: String = std::iter::repeat_n(fill, width.saturating_sub(s.len())).collect();
        if left { format!("{fill}{s}") } else { format!("{s}{fill}") }
    };
    let mut out = String::from(if neg_sign && (whole != 0.0 || n != 0.0) { "-" } else { "" });
    out.push_str(&literal(before));
    let show_whole = whole != 0.0 || int_pat.contains('0') || (n == 0.0 && !int_pat.is_empty());
    out.push_str(&pad(int_pat, if show_whole { format!("{whole}") } else { String::new() }, true));
    if n == 0.0 && !int_pat.is_empty() {
        // No fraction: spaces where it would be.
        let width = literal(gap).chars().count() + num_pat.len() + 1 + den_pat.len();
        out.push_str(&" ".repeat(width));
    } else {
        out.push_str(&literal(gap));
        out.push_str(&pad(&num_pat, format!("{n}"), true));
        out.push('/');
        let den = format!("{d}");
        out.push_str(&if fixed { den } else { pad(&den_pat, den, false) });
    }
    out.push_str(&literal(&tail));
    Some(out)
}

/// The last convergent of the continued fraction of `x` (0 <= x) whose
/// denominator is at most `max`: (numerator, denominator).
fn convergent(x: f64, max: f64) -> (f64, f64) {
    let (mut h0, mut h1, mut k0, mut k1) = (0.0, 1.0, 1.0, 0.0);
    let mut r = x;
    for _ in 0..64 {
        let a = r.floor();
        let (h2, k2) = (a * h1 + h0, a * k1 + k0);
        if k2 > max {
            break;
        }
        (h0, h1, k0, k1) = (h1, h2, k1, k2);
        let f = r - a;
        if f < 1e-12 || (h1 / k1 - x).abs() < 1e-15 {
            break;
        }
        r = 1.0 / f;
    }
    if k1 == 0.0 { (x.round(), 1.0) } else { (h1, k1) }
}

/// An elapsed time (`[h]:mm:ss`, `[mm]:ss`, `[s]`): the unit in brackets
/// counts the whole time, those after it within the next unit up.
fn format_elapsed(f: f64, sec: &str) -> String {
    let total_ms = (f * 86_400_000.0).round() as i64;
    let chars: Vec<char> = sec.chars().collect();
    let run = |i: usize, c: char| chars[i..].iter().take_while(|x| x.eq_ignore_ascii_case(&c)).count();
    // The unit in brackets: it is not taken modulo the next one up.
    let mut top = ' ';
    let value = |top: char, unit: char| -> i64 {
        match unit {
            'h' if top == 'h' => total_ms / 3_600_000,
            'h' => total_ms / 3_600_000 % 24,
            'm' if top == 'm' => total_ms / 60_000,
            'm' => total_ms / 60_000 % 60,
            _ if top == 's' => total_ms / 1000,
            _ => total_ms / 1000 % 60,
        }
    };
    let mut out = String::new();
    let mut i = 0;
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
                out.extend(chars.get(i + 1));
                i += 2;
            }
            '_' => {
                out.push(' ');
                i += 2;
            }
            '*' => i += 2,
            '[' => {
                let u = chars.get(i + 1).map_or(' ', |c| c.to_ascii_lowercase());
                let n = if matches!(u, 'h' | 'm' | 's') { run(i + 1, u) } else { 0 };
                if n > 0 && chars.get(i + 1 + n) == Some(&']') {
                    top = u;
                    out.push_str(&format!("{:0n$}", value(top, u)));
                    i += n + 2;
                    if u == 's' {
                        i += seconds_fraction(&chars[i..], total_ms, &mut out);
                    }
                } else {
                    // Another tag ([Red], [$-411]): not shown.
                    i += chars[i..].iter().position(|c| *c == ']').map_or(1, |p| p + 1);
                }
            }
            'h' | 'm' | 's' => {
                let n = run(i, lc);
                // m is minutes here (an elapsed time has no month).
                out.push_str(&format!("{:0w$}", value(top, lc), w = n.min(2)));
                i += n;
                if lc == 's' {
                    i += seconds_fraction(&chars[i..], total_ms, &mut out);
                }
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// The fraction of a second after the seconds (`.0`, `.00`, `.000`): writes
/// it and gives the number of characters of the code it took.
fn seconds_fraction(rest: &[char], total_ms: i64, out: &mut String) -> usize {
    if rest.first() != Some(&'.') {
        return 0;
    }
    let z = rest[1..].iter().take_while(|c| **c == '0').count();
    if z == 0 {
        return 0;
    }
    let frac = format!("{:03}", total_ms % 1000);
    out.push('.');
    out.push_str(&frac[..z.min(3)]);
    1 + z
}

/// Whether a section counts elapsed time (`[h]`, `[mm]`, `[ss]`).
fn is_elapsed(sec: &str) -> bool {
    let chars: Vec<char> = sec.chars().collect();
    let mut quoted = false;
    for (i, c) in chars.iter().enumerate() {
        match c {
            '"' => quoted = !quoted,
            '[' if !quoted => {
                let inner: String = chars[i + 1..].iter().take_while(|c| **c != ']').collect();
                let l = inner.to_ascii_lowercase();
                if !l.is_empty() && ['h', 'm', 's'].iter().any(|u| l.chars().all(|c| c == *u)) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// What Excel shows in a cell holding the number `f` with this format code:
/// an elapsed time, a date or time (`date`: the code displays one), or the
/// number. `########` for what a date cannot show (a negative number, one
/// past 9999-12-31), as Excel fills the cell with it.
pub fn format_serial(f: f64, code: &str, date: bool, date1904: bool) -> String {
    let first = sections(code).into_iter().next().unwrap_or_default();
    let elapsed = is_elapsed(&first);
    if !(date || elapsed) {
        return format_number(f, code);
    }
    if f < 0.0 || !f.is_finite() {
        return "########".into();
    }
    if elapsed {
        return format_elapsed(f, &first);
    }
    match super::xlsx::from_serial(f, date1904) {
        // 0 is 1900/1/0 in Excel (the day before 1900/1/1).
        Some((_, _, _, h, mi, s, ms)) if !date1904 && f < 1.0 => format_date((1900, 1, 0, h, mi, s, ms), code),
        Some(t) => format_date(t, code),
        None => "########".into(),
    }
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

    /// What Excel 2021 shows (tools/excel-verify).
    #[test]
    fn rounding_as_excel() {
        // Half away from zero, on the 15 digits Excel keeps.
        assert_eq!(format_number(1234.5, "#,##0"), "1,235");
        assert_eq!(format_number(-1234.5, "#,##0"), "-1,235");
        assert_eq!(format_number(0.5, "0"), "1");
        assert_eq!(format_number(1.005, "0.00"), "1.01");
        assert_eq!(format_number(2.5, "0"), "3");
        assert_eq!(format_number(-1234.5678, "#,##0.00"), "-1,234.57");
        assert_eq!(format_number(99.995, "0.00"), "100.00");
    }

    #[test]
    fn general_as_excel() {
        for (f, want) in [
            (1.0 / 3.0, "0.333333333"),
            (-1.0 / 3.0, "-0.333333333"),
            (1.0 / 7.0 * 1000.0, "142.8571429"),
            (1.0 / 7.0 / 1000.0, "0.000142857"),
            (1.0 / 7.0 / 100000.0, "1.42857E-06"),
            (std::f64::consts::PI * 1e10, "31415926536"),
            (std::f64::consts::PI * 1e11, "3.14159E+11"),
            (12345678901.0, "12345678901"),
            (123456789012.0, "1.23457E+11"),
            (100000000000.0, "1E+11"),
            (1e15, "1E+15"),
            (1e20, "1E+20"),
            (1e100, "1E+100"),
            (1.5e-5, "0.000015"),
            (0.0001234567891, "0.000123457"),
            (1e-10, "1E-10"),
            (1e-4 / 3.0, "3.33333E-05"),
            (999999999.99, "1000000000"),
            (12345.678901234, "12345.6789"),
            (0.1 + 0.2, "0.3"),
            (-0.000001, "-0.000001"),
            (123.456, "123.456"),
        ] {
            assert_eq!(general(f), want, "{f}");
        }
    }

    #[test]
    fn scientific_and_fractions() {
        assert_eq!(format_number(-1234.5678, "0.00E+00"), "-1.23E+03");
        assert_eq!(format_number(1234.5678, "0.00E+00"), "1.23E+03");
        assert_eq!(format_number(0.00012, "0.0E+00"), "1.2E-04");
        assert_eq!(format_number(46113.5625, "##0.0E+0"), "46.1E+3");
        assert_eq!(format_number(0.5, "##0.0E+0"), "500.0E-3");
        assert_eq!(format_number(1234.5, "##0.0E+0"), "1.2E+3");
        assert_eq!(format_number(-1234.5678, "##0.0E+0"), "-1.2E+3");
        assert_eq!(format_number(0.75, "# ?/?"), " 3/4");
        assert_eq!(format_number(46113.5625, "# ?/?"), "46113 4/7");
        assert_eq!(format_number(46113.5625, "# ??/??"), "46113  9/16");
        assert_eq!(format_number(-1234.5678, "# ??/??"), "-1234 46/81");
        assert_eq!(format_number(-1234.5678, "# ?/?"), "-1234 4/7");
        assert_eq!(format_number(1.25, "# ??/??"), "1  1/4 ");
        assert_eq!(format_number(0.5, "# ??/??"), "  1/2 ");
        assert_eq!(format_number(0.5, "?/?"), "1/2");
        assert_eq!(format_number(1.5, "?/?"), "3/2");
        assert_eq!(format_number(2.3, "# ?/4"), "2 1/4");
        assert_eq!(format_number(3.0, "# ?/?"), "3    ");
    }

    #[test]
    fn serials_as_excel() {
        let s = |f: f64, code: &str, date: bool| format_serial(f, code, date, false);
        // Elapsed time.
        assert_eq!(s(1.5, "[h]:mm", false), "36:00");
        assert_eq!(s(46113.5625, "[h]:mm:ss", true), "1106725:30:00");
        assert_eq!(s(1234.5, "[h]:mm:ss", true), "29628:00:00");
        assert_eq!(s(0.5, "[h]:mm:ss", true), "12:00:00");
        assert_eq!(s(0.0625, "[mm]:ss", false), "90:00");
        assert_eq!(s(1.0 / 86400.0 * 75.5, "[s].0", false), "75.5");
        // Dates and times.
        assert_eq!(s(46113.0, "aaaa", true), "水曜日");
        assert_eq!(s(46113.5625, "yyyy/m/d h:mm", true), "2026/4/1 13:30");
        assert_eq!(s(0.5, "yyyy/m/d", true), "1900/1/0");
        assert_eq!(s(0.5, "h:mm", true), "12:00");
        assert_eq!(s(-1234.5678, "yyyy/m/d", true), "########");
        // Numbers.
        assert_eq!(s(-1234.5678, "#,##0", false), "-1,235");
        assert!(crate::api::xlsx::date_format_code("aaaa"));
        assert!(crate::api::xlsx::date_format_code("aaa"));
        assert!(!crate::api::xlsx::date_format_code("\"aaa\"0"));
    }

    #[test]
    fn built_in_formats_of_japanese_excel() {
        use crate::api::xlsx::builtin_code;
        let s = |f: f64, id: u32| format_serial(f, builtin_code(id).unwrap(), crate::api::xlsx::builtin_date_fmt(id), false);
        assert_eq!(s(-1234.5678, 5), "¥-1,235");
        assert_eq!(s(1234.5, 8), "¥1,234.50");
        assert_eq!(s(-1234.5678, 23), "($1,235)");
        assert_eq!(s(1234.5, 24), "$1,235 ");
        assert_eq!(s(-1234.5678, 38), "-1,235");
        assert_eq!(s(46113.5625, 40), "46,113.56");
        assert_eq!(s(0.5, 59), "1");
        assert_eq!(s(46113.5625, 72), "2026/4/1");
        assert_eq!(s(46113.5625, 80), "1106725:30:00");
        assert_eq!(s(46113.5625, 81), "30:00.0");
        assert_eq!(s(-1234.5678, 41).trim(), "-1,235");
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
