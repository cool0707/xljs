//! Descriptions of the input rules (data validation) and the conditional
//! formats of a sheet, and of the formats (dxf) and colors they use.

use super::xml::Node;
use crate::t;

/// The colors of the workbook's theme (lt1, dk1, lt2, dk2, accent1-6,
/// hlink, folHlink in the order of the theme indexes) as RRGGBB.
pub(crate) type Theme = Vec<String>;

/// Reads the color scheme of theme1.xml.
pub(crate) fn theme(n: &Node) -> Theme {
    let mut found = Vec::new();
    n.find("clrScheme", &mut found);
    let Some(s) = found.first() else {
        return Vec::new();
    };
    let rgb = |name: &str| -> String {
        let Some(e) = s.child(name).and_then(|e| e.children.first()) else {
            return String::new();
        };
        match e.name.as_str() {
            "srgbClr" => e.attr("val").unwrap_or("").to_string(),
            "sysClr" => e.attr("lastClr").unwrap_or("").to_string(),
            _ => String::new(),
        }
    };
    [
        "lt1", "dk1", "lt2", "dk2", "accent1", "accent2", "accent3", "accent4", "accent5",
        "accent6", "hlink", "folHlink",
    ]
    .iter()
    .map(|n| rgb(n))
    .collect()
}

/// The legacy palette (indexed colors 0-63).
const PALETTE: [&str; 64] = [
    "000000", "FFFFFF", "FF0000", "00FF00", "0000FF", "FFFF00", "FF00FF", "00FFFF", "000000",
    "FFFFFF", "FF0000", "00FF00", "0000FF", "FFFF00", "FF00FF", "00FFFF", "800000", "008000",
    "000080", "808000", "800080", "008080", "C0C0C0", "808080", "9999FF", "993366", "FFFFCC",
    "CCFFFF", "660066", "FF8080", "0066CC", "CCCCFF", "000080", "FF00FF", "FFFF00", "00FFFF",
    "800080", "800000", "008080", "0000FF", "00CCFF", "CCFFFF", "CCFFCC", "FFFF99", "99CCFF",
    "FF99CC", "CC99FF", "FFCC99", "3366FF", "33CCCC", "99CC00", "FFCC00", "FF9900", "FF6600",
    "666699", "969696", "003366", "339966", "003300", "333300", "993300", "993366", "333399",
    "333333",
];

/// A color element (rgb, theme with tint, indexed, auto) as #RRGGBB, or
/// as Excel names it when it cannot be resolved.
pub(crate) fn color(n: &Node, theme: &Theme) -> String {
    if n.flag("auto", false) {
        return t!("book.color.auto");
    }
    let tint: f64 = n.attr("tint").and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let base = if let Some(rgb) = n.attr("rgb") {
        let rgb = rgb.trim();
        Some(if rgb.len() == 8 {
            rgb[2..].to_string()
        } else {
            rgb.to_string()
        })
    } else if let Some(i) = n.attr("theme").and_then(|v| v.parse::<usize>().ok()) {
        match theme.get(i).filter(|s| s.len() == 6) {
            Some(s) => Some(s.clone()),
            None => return t!("book.color.theme", i),
        }
    } else if let Some(i) = n.attr("indexed").and_then(|v| v.parse::<usize>().ok()) {
        match PALETTE.get(i) {
            Some(s) => Some(s.to_string()),
            None => return t!("book.color.auto"),
        }
    } else {
        None
    };
    match base {
        Some(s) => format!("#{}", tinted(&s, tint).to_uppercase()),
        None => String::new(),
    }
}

/// RRGGBB lightened (tint > 0) or darkened (tint < 0) as Excel does it,
/// in HLS.
fn tinted(rgb: &str, tint: f64) -> String {
    if tint == 0.0 || rgb.len() != 6 {
        return rgb.to_string();
    }
    let ch = |i: usize| u8::from_str_radix(&rgb[i..i + 2], 16).unwrap_or(0) as f64 / 255.0;
    let (r, g, b) = (ch(0), ch(2), ch(4));
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let mut l = (max + min) / 2.0;
    let d = max - min;
    let (h, s) = if d == 0.0 {
        (0.0, 0.0)
    } else {
        let s = if l > 0.5 {
            d / (2.0 - max - min)
        } else {
            d / (max + min)
        };
        let h = if max == r {
            ((g - b) / d).rem_euclid(6.0)
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        };
        (h / 6.0, s)
    };
    l = if tint < 0.0 {
        l * (1.0 + tint)
    } else {
        l * (1.0 - tint) + tint
    };
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let hue = |mut t: f64| {
        t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    let (r, g, b) = if s == 0.0 {
        (l, l, l)
    } else {
        (hue(h + 1.0 / 3.0), hue(h), hue(h - 1.0 / 3.0))
    };
    let to = |v: f64| format!("{:02X}", (v.clamp(0.0, 1.0) * 255.0).round() as u8);
    format!("{}{}{}", to(r), to(g), to(b))
}

/// What a format (dxf) changes: bold, colors, fill, borders, number
/// format.
pub(crate) fn dxf(n: &Node, theme: &Theme) -> String {
    let sep = t!("vba.separator");
    let mut parts = Vec::new();
    let on = |e: Option<&Node>| e.is_some_and(|e| !matches!(e.attr("val"), Some("0" | "false")));
    if let Some(f) = n.child("font") {
        if on(f.child("b")) {
            parts.push(t!("book.fmt.bold"));
        }
        if on(f.child("i")) {
            parts.push(t!("book.fmt.italic"));
        }
        if f.child("u").is_some_and(|u| u.attr("val") != Some("none")) {
            parts.push(t!("book.fmt.underline"));
        }
        if on(f.child("strike")) {
            parts.push(t!("book.fmt.strike"));
        }
        if let Some(c) = f
            .child("color")
            .map(|c| color(c, theme))
            .filter(|c| !c.is_empty())
        {
            parts.push(t!("book.fmt.color", c));
        }
    }
    if let Some(fill) = n.child("fill") {
        if let Some(p) = fill.child("patternFill") {
            if p.attr("patternType") != Some("none") {
                // A dxf's solid fill is its background color.
                let c = p
                    .child("bgColor")
                    .or_else(|| p.child("fgColor"))
                    .map(|c| color(c, theme))
                    .unwrap_or_default();
                if !c.is_empty() {
                    parts.push(t!("book.fmt.fill", c));
                }
            }
        } else if fill.child("gradientFill").is_some() {
            parts.push(t!("book.fmt.gradient"));
        }
    }
    if let Some(b) = n.child("border") {
        let sides: Vec<&str> = ["left", "right", "top", "bottom"]
            .into_iter()
            .filter(|s| {
                b.child(s)
                    .is_some_and(|e| e.attr("style").is_some_and(|v| v != "none"))
            })
            .collect();
        if !sides.is_empty() {
            parts.push(t!("book.fmt.border"));
        }
    }
    if let Some(code) = n.child("numFmt").and_then(|f| f.attr("formatCode")) {
        parts.push(t!("book.fmt.numFmt", code));
    }
    parts.join(&sep)
}

/// Turns a formula of a rule into text (inline code, or a key for
/// comparing rules).
pub(crate) type Fmt<'a> = &'a dyn Fn(&str) -> String;

fn op_text(op: &str, a: &str, b: &str) -> String {
    match op {
        "between" => t!("book.op.between", a, b),
        "notBetween" => t!("book.op.notBetween", a, b),
        "equal" => format!("= {a}"),
        "notEqual" => format!("<> {a}"),
        "greaterThan" => format!("> {a}"),
        "lessThan" => format!("< {a}"),
        "greaterThanOrEqual" => format!(">= {a}"),
        "lessThanOrEqual" => format!("<= {a}"),
        _ => a.to_string(),
    }
}

/// Excel's date serial (1900 system) as yyyy/mm/dd; the time of day of a
/// fraction as h:mm(:ss).
fn date_text(serial: f64, date1904: bool) -> Option<String> {
    let days = serial.floor() as i64;
    // Days from 1970-01-01. In the 1900 system 1900-02-29 (60) does not
    // exist and later serials count it.
    let z = if date1904 {
        days - 24107
    } else if days < 1 {
        return None;
    } else if days < 61 {
        days - 25568
    } else {
        days - 25569
    };
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    Some(format!("{y}/{m:02}/{d:02}"))
}

fn time_text(v: f64) -> String {
    let secs = ((v - v.floor()) * 86400.0).round() as i64;
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    if s == 0 {
        format!("{h}:{m:02}")
    } else {
        format!("{h}:{m:02}:{s:02}")
    }
}

/// Describes an input rule (dataValidation, with its formulas through
/// fmt).
pub(crate) fn validation(n: &Node, fmt: Fmt, date1904: bool) -> String {
    let sep = t!("vba.separator");
    let kind = n.attr("type").unwrap_or("none");
    let formula = |name: &str| {
        n.child(name)
            .map(|f| f.all_text())
            .filter(|s| !s.is_empty())
    };
    let (f1, f2) = (formula("formula1"), formula("formula2"));
    // A value of a condition: dates and times of plain numbers shown as such.
    let value = |f: &Option<String>| -> String {
        let Some(f) = f else { return String::new() };
        if let Ok(v) = f.trim().parse::<f64>() {
            match kind {
                "date" => {
                    if let Some(d) = date_text(v, date1904) {
                        return d;
                    }
                }
                "time" => return time_text(v),
                _ => return f.trim().to_string(),
            }
        }
        fmt(f)
    };
    let op = n.attr("operator").unwrap_or("between");
    let cond = || op_text(op, &value(&f1), &value(&f2));
    let mut s = match kind {
        "list" => match &f1 {
            Some(f) if f.starts_with('"') && f.ends_with('"') && f.len() >= 2 => {
                t!("book.dv.listItems", f[1..f.len() - 1].replace("\"\"", "\""))
            }
            Some(f) => t!("book.dv.list", fmt(f)),
            None => t!("book.dv.list", ""),
        },
        "whole" => t!("book.dv.whole", cond()),
        "decimal" => t!("book.dv.decimal", cond()),
        "date" => t!("book.dv.date", cond()),
        "time" => t!("book.dv.time", cond()),
        "textLength" => t!("book.dv.textLength", cond()),
        "custom" => t!("book.dv.custom", f1.as_deref().map(fmt).unwrap_or_default()),
        _ => t!("book.dv.any"),
    };
    let mut notes = Vec::new();
    if kind == "list" && n.flag("showDropDown", false) {
        // showDropDown="1" hides the list.
        notes.push(t!("book.dv.noDropDown"));
    }
    if kind != "none" && !n.flag("allowBlank", false) {
        notes.push(t!("book.dv.noBlank"));
    }
    let msg = |title: Option<&str>, text: Option<&str>| -> String {
        let one = |s: &str| s.replace("\r\n", " ").replace(['\r', '\n'], " ");
        match (
            title.filter(|s| !s.is_empty()),
            text.filter(|s| !s.is_empty()),
        ) {
            (Some(a), Some(b)) => format!("「{}: {}」", one(a), one(b)),
            (Some(a), None) => format!("「{}」", one(a)),
            (None, Some(b)) => format!("「{}」", one(b)),
            (None, None) => String::new(),
        }
    };
    if n.flag("showInputMessage", false) {
        let m = msg(n.attr("promptTitle"), n.attr("prompt"));
        if !m.is_empty() {
            notes.push(t!("book.dv.prompt", m));
        }
    }
    if kind != "none" {
        if n.flag("showErrorMessage", false) {
            let style = match n.attr("errorStyle").unwrap_or("stop") {
                "warning" => t!("book.dv.warning"),
                "information" => t!("book.dv.information"),
                _ => t!("book.dv.stop"),
            };
            notes.push(t!(
                "book.dv.error",
                style,
                msg(n.attr("errorTitle"), n.attr("error"))
            ));
        } else {
            notes.push(t!("book.dv.noError"));
        }
    }
    if let Some(ime) = n.attr("imeMode").filter(|m| *m != "noControl") {
        notes.push(t!("book.dv.ime", t!(&format!("book.ime.{ime}"))));
    }
    if !notes.is_empty() {
        s.push_str(&format!("（{}）", notes.join(&sep)));
    }
    s
}

/// A threshold of a color scale, a data bar or an icon set.
fn cfvo(n: &Node, fmt: Fmt) -> String {
    let val = n
        .attr("val")
        .map(str::to_string)
        .or_else(|| n.child("f").map(|f| f.all_text()))
        .unwrap_or_default();
    let num = |v: &str| {
        if v.parse::<f64>().is_ok() {
            v.to_string()
        } else {
            fmt(v)
        }
    };
    let s = match n.attr("type").unwrap_or("") {
        "min" | "autoMin" => t!("book.cfvo.min"),
        "max" | "autoMax" => t!("book.cfvo.max"),
        "num" => num(&val),
        "percent" => t!("book.cfvo.percent", num(&val)),
        "percentile" => t!("book.cfvo.percentile", num(&val)),
        "formula" => fmt(&val),
        other => other.to_string(),
    };
    if n.attr("gte") == Some("0") {
        t!("book.cfvo.greater", s)
    } else {
        s
    }
}

/// Describes a conditional format (cfRule, with its formulas through fmt
/// and dxfs: the formats of styles.xml).
pub(crate) fn conditional(n: &Node, fmt: Fmt, dxfs: &[Node], theme: &Theme) -> String {
    let sep = t!("vba.separator");
    let formulas: Vec<String> = n
        .children
        .iter()
        .filter(|c| c.name == "formula" || c.name == "f")
        .map(|c| c.all_text())
        .collect();
    let f = |i: usize| match formulas.get(i) {
        Some(s) if s.trim().parse::<f64>().is_ok() => s.trim().to_string(),
        Some(s) => fmt(s),
        None => String::new(),
    };
    let text = n.attr("text").unwrap_or("").to_string();
    let rank = n.attr("rank").unwrap_or("10");
    let mut s = match n.attr("type").unwrap_or("") {
        "cellIs" => t!(
            "book.cf.cellIs",
            op_text(n.attr("operator").unwrap_or("between"), &f(0), &f(1))
        ),
        "expression" => t!("book.cf.expression", f(0)),
        "containsText" => t!("book.cf.containsText", text),
        "notContainsText" => t!("book.cf.notContainsText", text),
        "beginsWith" => t!("book.cf.beginsWith", text),
        "endsWith" => t!("book.cf.endsWith", text),
        "containsBlanks" => t!("book.cf.containsBlanks"),
        "notContainsBlanks" => t!("book.cf.notContainsBlanks"),
        "containsErrors" => t!("book.cf.containsErrors"),
        "notContainsErrors" => t!("book.cf.notContainsErrors"),
        "duplicateValues" => t!("book.cf.duplicateValues"),
        "uniqueValues" => t!("book.cf.uniqueValues"),
        "top10" => {
            let key = match (n.flag("bottom", false), n.flag("percent", false)) {
                (false, false) => "book.cf.top",
                (false, true) => "book.cf.topPercent",
                (true, false) => "book.cf.bottom",
                (true, true) => "book.cf.bottomPercent",
            };
            t!(key, rank)
        }
        "aboveAverage" => {
            let above = n.flag("aboveAverage", true);
            let key = match (above, n.flag("equalAverage", false)) {
                (true, false) => "book.cf.above",
                (true, true) => "book.cf.aboveEqual",
                (false, false) => "book.cf.below",
                (false, true) => "book.cf.belowEqual",
            };
            let mut s = t!(key);
            if let Some(sd) = n.attr("stdDev") {
                s.push_str(&t!("book.cf.stdDev", sd));
            }
            s
        }
        "timePeriod" => t!(
            "book.cf.timePeriod",
            t!(&format!(
                "book.period.{}",
                n.attr("timePeriod").unwrap_or("today")
            ))
        ),
        "colorScale" => {
            let Some(cs) = n.child("colorScale") else {
                return t!("book.cf.colorScale", "");
            };
            let colors: Vec<String> = cs.children("color").map(|c| color(c, theme)).collect();
            let steps: Vec<String> = cs
                .children("cfvo")
                .enumerate()
                .map(|(i, v)| {
                    format!(
                        "{} {}",
                        cfvo(v, fmt),
                        colors.get(i).cloned().unwrap_or_default()
                    )
                    .trim()
                    .to_string()
                })
                .collect();
            t!("book.cf.colorScale", steps.join(" → "))
        }
        "dataBar" => {
            let Some(db) = n.child("dataBar") else {
                return t!("book.cf.dataBar", "");
            };
            let c = db
                .child("color")
                .or_else(|| db.child("fillColor"))
                .map(|c| color(c, theme))
                .unwrap_or_default();
            let vs: Vec<String> = db.children("cfvo").map(|v| cfvo(v, fmt)).collect();
            let mut d = c;
            if vs.len() == 2 {
                d.push_str(&format!("（{} 〜 {}）", vs[0], vs[1]));
            }
            t!("book.cf.dataBar", d.trim())
        }
        "iconSet" => {
            let Some(is) = n.child("iconSet") else {
                return t!("book.cf.iconSet", "");
            };
            let mut d = is.attr("iconSet").unwrap_or("3TrafficLights1").to_string();
            let vs: Vec<String> = is.children("cfvo").skip(1).map(|v| cfvo(v, fmt)).collect();
            if !vs.is_empty() {
                d.push_str(&format!("（{}）", t!("book.cf.thresholds", vs.join(&sep))));
            }
            if is.flag("reverse", false) {
                d.push_str(&t!("book.cf.reverse"));
            }
            if !is.flag("showValue", true) {
                d.push_str(&t!("book.cf.iconOnly"));
            }
            t!("book.cf.iconSet", d)
        }
        other => other.to_string(),
    };
    // The format: by its index in styles.xml or inline (x14).
    let format = match n.attr("dxfId").and_then(|i| i.parse::<usize>().ok()) {
        Some(i) => dxfs.get(i).map(|d| dxf(d, theme)).unwrap_or_default(),
        None => n.child("dxf").map(|d| dxf(d, theme)).unwrap_or_default(),
    };
    if !format.is_empty() {
        s.push_str(&format!(" → {format}"));
    }
    if n.flag("stopIfTrue", false) {
        s.push_str(&t!("book.cf.stopIfTrue"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors() {
        assert_eq!(tinted("4472C4", 0.0), "4472C4");
        assert_eq!(tinted("4472C4", 0.7999816888943144), "DAE3F3");
        assert_eq!(tinted("FFFFFF", -0.249977111117893), "BFBFBF");
    }

    #[test]
    fn dates() {
        assert_eq!(date_text(45658.0, false).unwrap(), "2025/01/01");
        assert_eq!(date_text(1.0, false).unwrap(), "1900/01/01");
        assert_eq!(date_text(0.0, true).unwrap(), "1904/01/01");
        assert_eq!(time_text(0.375), "9:00");
    }
}
