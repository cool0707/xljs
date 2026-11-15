//! xljs: numbers written in kanji, as Japanese Excel shows them with the `[DBNum1]`, `[DBNum2]`
//! and `[DBNum3]` number formats and returns them from NUMBERSTRING. This file is not GridCraft's.
//!
//! Each style writes a number two ways:
//!
//! - with units ([`Kanji::positional`]): 12345 is 一万二千三百四十五 (`[DBNum1]G/標準`),
//!   壱萬弐阡参百四拾伍 (`[DBNum2]G/標準`) or １万２千３百４十５ (`[DBNum3]G/標準`);
//! - digit by digit ([`Kanji::digits`]): 一二三四五, 壱弐参四伍 or １２３４５ (`[DBNumN]0`).

/// A way of writing numbers in kanji.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kanji {
    /// `[DBNum1]`: 一二三 with 十百千万億兆.
    Plain,
    /// `[DBNum2]`: the formal numerals 壱弐参 with 拾百阡萬億兆.
    Formal,
    /// `[DBNum3]`: full-width digits with 十百千万億兆.
    Wide,
}

const PLAIN: [char; 10] = ['〇', '一', '二', '三', '四', '五', '六', '七', '八', '九'];
const FORMAL: [char; 10] = ['〇', '壱', '弐', '参', '四', '伍', '六', '七', '八', '九'];
const WIDE: [char; 10] = ['０', '１', '２', '３', '４', '５', '６', '７', '８', '９'];

/// The units of the groups of four digits, from the second group up.
const GROUPS: [char; 4] = ['万', '億', '兆', '京'];

impl Kanji {
    /// The style of `[DBNumN]`. None for the tags that are not written in kanji (`[DBNum4]`).
    pub fn from_dbnum(n: u32) -> Option<Kanji> {
        match n {
            1 => Some(Kanji::Plain),
            2 => Some(Kanji::Formal),
            3 => Some(Kanji::Wide),
            _ => None,
        }
    }

    /// The style a format section asks for with a `[DBNum1]`-like tag (the text between the
    /// brackets, any case).
    pub fn from_tag(tag: &str) -> Option<Kanji> {
        let tag = tag.trim();
        let (head, n) = tag.split_at_checked(5)?;
        if !head.eq_ignore_ascii_case("dbnum") {
            return None;
        }
        Kanji::from_dbnum(n.parse().ok()?)
    }

    fn table(self) -> &'static [char; 10] {
        match self {
            Kanji::Plain => &PLAIN,
            Kanji::Formal => &FORMAL,
            Kanji::Wide => &WIDE,
        }
    }

    /// One digit (0–9).
    pub fn digit(self, d: u8) -> char {
        self.table().get(usize::from(d)).copied().unwrap_or('〇')
    }

    /// 十, 百, 千 (the place in a group of four digits, 1–3).
    fn place(self, p: usize) -> char {
        match (self, p) {
            (Kanji::Formal, 1) => '拾',
            (Kanji::Formal, 3) => '阡',
            (_, 1) => '十',
            (_, 2) => '百',
            _ => '千',
        }
    }

    /// 万, 億, 兆, 京 (the group of four digits, 1–4).
    fn group(self, g: usize) -> char {
        match (self, g) {
            (Kanji::Formal, 1) => '萬',
            _ => GROUPS.get(g.wrapping_sub(1)).copied().unwrap_or('京'),
        }
    }

    /// `text` with every ASCII digit written in this style, one for one: 2026 is 二〇二六.
    /// Everything else stays as it is.
    pub fn digits(self, text: &str) -> String {
        text.chars().map(|c| if c.is_ascii_digit() { self.digit(c as u8 - b'0') } else { c }).collect()
    }

    /// A whole number, given by its ASCII digits, written with units: 1234 is 千二百三十四,
    /// 壱阡弐百参拾四 or 千２百３十４, and zero is 〇 (０). The plain and the full-width styles leave
    /// out the 1 before 十, 百 and 千; the formal one writes it (壱拾).
    ///
    /// A number of more than twenty digits has no units to be written with: digit by digit then.
    /// So is a text that is not all digits.
    pub fn positional(self, digits: &str) -> String {
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return self.digits(digits);
        }
        let ds = digits.trim_start_matches('0').as_bytes();
        if ds.is_empty() {
            return self.digit(0).to_string();
        }
        if ds.len() > 4 * (GROUPS.len() + 1) {
            return self.digits(digits);
        }
        let mut out = String::new();
        let n = ds.len();
        let mut any_in_group = false;
        for (i, b) in ds.iter().enumerate() {
            // The digit's place counted from the right: its group of four and the place in it.
            let at = n - 1 - i;
            let (g, p) = (at / 4, at % 4);
            let d = b - b'0';
            if d != 0 {
                any_in_group = true;
                if !(d == 1 && p > 0 && self != Kanji::Formal) {
                    out.push(self.digit(d));
                }
                if p > 0 {
                    out.push(self.place(p));
                }
            }
            if p == 0 {
                if any_in_group && g > 0 {
                    out.push(self.group(g));
                }
                any_in_group = false;
            }
        }
        out
    }

    /// A non-negative number as the General format writes it (`1234.5`, digits and at most one
    /// point), in this style: the whole part with units and the decimals digit by digit
    /// (千二百三十四.五). A text of another shape (`1.5E+20`) only has its digits replaced.
    pub fn general(self, text: &str) -> String {
        let (whole, rest) = match text.split_once('.') {
            Some((w, f)) => (w, Some(f)),
            None => (text, None),
        };
        let plain = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
        if whole.is_empty() || !plain(whole) || !rest.is_none_or(plain) {
            return self.digits(text);
        }
        let mut out = self.positional(whole);
        if let Some(f) = rest {
            out.push('.');
            out.push_str(&self.digits(f));
        }
        out
    }

    /// The number of a date or time part (`2026`, `07`, `12`) as a format with this style shows
    /// it. Full-width digits stay digits (２０２６年１０月７日). In kanji a four-digit year and a
    /// number with a leading zero are written digit by digit (二〇二六, 〇七), any other with
    /// units (十二月二十三日).
    pub fn date_part(self, text: &str) -> String {
        let by_digit = self == Kanji::Wide || text.len() >= 4 || (text.len() > 1 && text.starts_with('0'));
        if by_digit { self.digits(text) } else { self.positional(text) }
    }
}

#[cfg(test)]
mod tests {
    use super::Kanji::{Formal, Plain, Wide};
    use super::*;

    #[test]
    fn tags() {
        assert_eq!(Kanji::from_tag("DBNum1"), Some(Plain));
        assert_eq!(Kanji::from_tag("dbnum2"), Some(Formal));
        assert_eq!(Kanji::from_tag(" DBNUM3 "), Some(Wide));
        assert_eq!(Kanji::from_tag("DBNum4"), None);
        assert_eq!(Kanji::from_tag("DBNum"), None);
        assert_eq!(Kanji::from_tag("Red"), None);
        assert_eq!(Kanji::from_tag("$-411"), None);
        assert_eq!(Kanji::from_tag("標準の"), None);
    }

    #[test]
    fn with_units() {
        assert_eq!(Plain.positional("12345"), "一万二千三百四十五");
        assert_eq!(Formal.positional("12345"), "壱萬弐阡参百四拾伍");
        assert_eq!(Wide.positional("12345"), "１万２千３百４十５");
        assert_eq!(Plain.positional("123"), "百二十三");
        assert_eq!(Formal.positional("123"), "壱百弐拾参");
        assert_eq!(Plain.positional("1234"), "千二百三十四");
        assert_eq!(Formal.positional("1234"), "壱阡弐百参拾四");
        assert_eq!(Wide.positional("1234"), "千２百３十４");
        assert_eq!(Plain.positional("123456"), "十二万三千四百五十六");
        assert_eq!(Formal.positional("123456"), "壱拾弐萬参阡四百伍拾六");
        assert_eq!(Plain.positional("987654321"), "九億八千七百六十五万四千三百二十一");
        assert_eq!(Formal.positional("987654321"), "九億八阡七百六拾伍萬四阡参百弐拾壱");
        assert_eq!(Wide.positional("987654321"), "９億８千７百６十５万４千３百２十１");
        assert_eq!(Formal.positional("123456789"), "壱億弐阡参百四拾伍萬六阡七百八拾九");
        assert_eq!(Plain.positional("1234567890"), "十二億三千四百五十六万七千八百九十");
    }

    #[test]
    fn zeros_and_ones() {
        assert_eq!(Plain.positional("0"), "〇");
        assert_eq!(Formal.positional("0"), "〇");
        assert_eq!(Wide.positional("000"), "０");
        assert_eq!(Plain.positional("10"), "十");
        assert_eq!(Plain.positional("11"), "十一");
        assert_eq!(Plain.positional("110"), "百十");
        assert_eq!(Plain.positional("1000"), "千");
        assert_eq!(Plain.positional("1001"), "千一");
        assert_eq!(Plain.positional("10000"), "一万");
        assert_eq!(Plain.positional("100000"), "十万");
        assert_eq!(Plain.positional("10000000"), "千万");
        assert_eq!(Plain.positional("100000000"), "一億");
        assert_eq!(Plain.positional("100010000"), "一億一万");
        assert_eq!(Plain.positional("100000001"), "一億一");
        assert_eq!(Plain.positional("1000000000000"), "一兆");
        assert_eq!(Plain.positional("007"), "七");
        assert_eq!(Formal.positional("10"), "壱拾");
        assert_eq!(Formal.positional("1000"), "壱阡");
        assert_eq!(Formal.positional("10000000"), "壱阡萬");
        assert_eq!(Wide.positional("10"), "十");
        assert_eq!(Wide.positional("21"), "２十１");
        assert_eq!(Plain.positional("12345678901234567890"), "千二百三十四京五千六百七十八兆九千十二億三千四百五十六万七千八百九十");
        // No unit above 京, and no number in what is not one.
        assert_eq!(Plain.positional("123456789012345678901"), "一二三四五六七八九〇一二三四五六七八九〇一");
        assert_eq!(Plain.positional("1,234"), "一,二三四");
        assert_eq!(Plain.positional(""), "");
    }

    #[test]
    fn digit_by_digit() {
        assert_eq!(Plain.digits("123456789"), "一二三四五六七八九");
        assert_eq!(Plain.digits("1,200.50"), "一,二〇〇.五〇");
        assert_eq!(Formal.digits("12345"), "壱弐参四伍");
        assert_eq!(Wide.digits("2026/10/07"), "２０２６/１０/０７");
    }

    #[test]
    fn general_numbers() {
        assert_eq!(Plain.general("1234.5"), "千二百三十四.五");
        assert_eq!(Plain.general("0.25"), "〇.二五");
        assert_eq!(Wide.general("1234.5"), "千２百３十４.５");
        assert_eq!(Plain.general("1.5E+20"), "一.五E+二〇");
        assert_eq!(Plain.general("0"), "〇");
    }

    #[test]
    fn date_parts() {
        assert_eq!(Plain.date_part("2026"), "二〇二六");
        assert_eq!(Plain.date_part("30"), "三十");
        assert_eq!(Plain.date_part("12"), "十二");
        assert_eq!(Plain.date_part("7"), "七");
        assert_eq!(Plain.date_part("07"), "〇七");
        assert_eq!(Plain.date_part("0"), "〇");
        assert_eq!(Formal.date_part("31"), "参拾壱");
        assert_eq!(Formal.date_part("2019"), "弐〇壱九");
        assert_eq!(Wide.date_part("22"), "２２");
        assert_eq!(Wide.date_part("2007"), "２００７");
    }
}
