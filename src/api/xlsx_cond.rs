//! Conditional formatting on paper (book.toPdf / toHtml): the rules of the
//! sheets (highlight rules with their differential formats, top / bottom,
//! above / below average, duplicates, text, dates, formulas, color scales,
//! data bars and icon sets) worked out for the cells.
//!
//! The rules are read from the sheets' XML when the workbook is opened:
//! umya-spreadsheet keeps neither the kind of an icon set nor the Excel 2010
//! extensions (x14) that most data bars and some icon sets are written with.

use std::collections::HashMap;

use umya_spreadsheet::Color;

use super::xlsx::Book;
use super::xlsx_calc::{CalcVal, eval_at, prepare, value_at};
use super::xlsx_chart::{Node, parse};
use super::xlsx_table::{Dxf, Side, color, dxf};

/// A rectangle of cells (1-based, inclusive): c1, r1, c2, r2.
type Area = (u32, u32, u32, u32);

/// A threshold of a color scale, data bar or icon set.
#[derive(Debug, Clone, Default)]
pub struct Cfvo {
    /// min, max, num, percent, percentile, formula, autoMin, autoMax.
    pub kind: String,
    pub val: String,
    /// The value itself counts as reaching the threshold (icon sets).
    pub gte: bool,
}

#[derive(Debug, Clone, Default)]
pub struct BarDef {
    pub color: Option<Color>,
    /// Lengths in percent of the cell for the lowest and the highest values.
    pub min_len: f64,
    pub max_len: f64,
    pub gradient: bool,
    pub border: Option<Color>,
    pub negative: Option<Color>,
    pub negative_border: Option<Color>,
    pub negative_same: bool,
    pub negative_border_same: bool,
    /// automatic, middle, none.
    pub axis: String,
    pub axis_color: Option<Color>,
    pub rtl: bool,
    /// Written with the Excel 2010 extension (negative values from an axis).
    pub ext: bool,
}

#[derive(Debug, Clone, Default)]
pub struct IconDef {
    pub set: String,
    pub reverse: bool,
    /// Icons chosen one by one (set, index) from low to high.
    pub custom: Vec<(String, usize)>,
}

#[derive(Debug, Clone, Default)]
pub struct Rule {
    pub ranges: Vec<Area>,
    pub kind: String,
    pub priority: i32,
    pub stop: bool,
    pub operator: String,
    pub text: String,
    pub formulas: Vec<String>,
    pub rank: u32,
    pub percent: bool,
    pub bottom: bool,
    pub above_average: bool,
    pub equal_average: bool,
    pub std_dev: u32,
    pub time_period: String,
    pub dxf: Option<Dxf>,
    pub cfvo: Vec<Cfvo>,
    pub colors: Vec<Color>,
    pub bar: Option<BarDef>,
    pub icons: Option<IconDef>,
    pub show_value: bool,
}

// ---- reading ----

/// The conditional formatting rules of every sheet of an xlsx file, by sheet name.
pub fn collect(data: &[u8], styles_xml: Option<&[u8]>) -> HashMap<String, Vec<Rule>> {
    let dxfs: Vec<Dxf> = styles_xml.and_then(parse).and_then(|r| r.child("dxfs").map(|d| d.all("dxf").map(dxf).collect())).unwrap_or_default();
    let mut out = HashMap::new();
    super::xlsx_print::for_each_sheet(data, |_, name, xml| {
        let rules = sheet_rules(xml, &dxfs);
        if !rules.is_empty() {
            out.insert(name.to_string(), rules);
        }
    });
    out
}

/// The part of a sheet's XML after the cells (where conditional formatting is).
fn tail(xml: &[u8]) -> Option<Node> {
    let s = String::from_utf8_lossy(xml);
    if !s.contains("conditionalFormatting") {
        return None;
    }
    let start = s.find("</sheetData>").map(|i| i + "</sheetData>".len()).or_else(|| s.find("<sheetData/>").map(|i| i + "<sheetData/>".len()))?;
    let end = s.rfind("</worksheet>").or_else(|| s.rfind("</x:worksheet>")).unwrap_or(s.len()).max(start);
    parse(format!("<r>{}</r>", &s[start..end]).as_bytes())
}

fn flag(n: &Node, name: &str, default: bool) -> bool {
    match n.attr(name) {
        Some("1" | "true") => true,
        Some("0" | "false") => false,
        _ => default,
    }
}

fn num_attr(n: &Node, name: &str) -> Option<f64> {
    n.attr(name).and_then(|v| v.parse().ok())
}

pub(super) fn sheet_rules(xml: &[u8], dxfs: &[Dxf]) -> Vec<Rule> {
    let Some(root) = tail(xml) else { return Vec::new() };
    let mut rules = Vec::new();
    // Excel 2010 rules by id, to join the data bars written in both places.
    let mut ext: HashMap<String, (Node, Vec<Area>)> = HashMap::new();
    let mut ext_order = Vec::new();
    if let Some(list) = root.child("extLst") {
        for e in list.all("ext") {
            let Some(cfs) = e.child("conditionalFormattings") else { continue };
            for cf in cfs.all("conditionalFormatting") {
                let ranges = cf.child("sqref").map(|s| sqref(&s.text)).unwrap_or_default();
                for r in cf.all("cfRule") {
                    let id = r.attr("id").unwrap_or("").to_string();
                    ext_order.push(id.clone());
                    ext.insert(id, (clone(r), ranges.clone()));
                }
            }
        }
    }
    for cf in root.all("conditionalFormatting") {
        let ranges = cf.attr("sqref").map(sqref).unwrap_or_default();
        for r in cf.all("cfRule") {
            let mut rule = read_rule(r, dxfs, false);
            rule.ranges = ranges.clone();
            // <extLst><ext><x14:id>{...}</x14:id></ext></extLst>
            if let Some(id) = r.child("extLst").and_then(|l| l.find("id")).map(|i| i.text.trim().to_string())
                && let Some((x, _)) = ext.remove(&id)
            {
                let more = read_rule(&x, dxfs, true);
                if let Some(mut bar) = more.bar {
                    // The color stays in the first rule unless the extension has one.
                    if bar.color.is_none() {
                        bar.color = rule.bar.as_ref().and_then(|b| b.color.clone());
                    }
                    rule.bar = Some(bar);
                    rule.cfvo = more.cfvo;
                }
            }
            rules.push(rule);
        }
    }
    for id in ext_order {
        if let Some((x, ranges)) = ext.remove(&id) {
            let mut rule = read_rule(&x, dxfs, true);
            rule.ranges = ranges;
            rules.push(rule);
        }
    }
    rules.retain(|r| !r.ranges.is_empty());
    rules.sort_by_key(|r| r.priority);
    rules
}

fn clone(n: &Node) -> Node {
    Node { name: n.name.clone(), attrs: n.attrs.clone(), children: n.children.iter().map(clone).collect(), text: n.text.clone() }
}

/// "A1:B5 D1" (also whole columns and rows) as areas.
fn sqref(s: &str) -> Vec<Area> {
    s.split_whitespace()
        .filter_map(|p| {
            let p = p.replace('$', "").to_uppercase();
            let (a, b) = p.split_once(':').unwrap_or((&p, &p));
            let corner = |s: &str, last: bool| -> Option<(u32, u32)> {
                if !s.is_empty() && s.bytes().all(|c| c.is_ascii_alphabetic()) {
                    return Some((super::xlsx::column_number(s).ok()?, if last { 1048576 } else { 1 }));
                }
                if !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()) {
                    return Some((if last { 16384 } else { 1 }, s.parse().ok()?));
                }
                super::xlsx::parse_ref(s).ok()
            };
            let (c1, r1) = corner(a, false)?;
            let (c2, r2) = corner(b, true)?;
            Some((c1.min(c2), r1.min(r2), c1.max(c2), r1.max(r2)))
        })
        .collect()
}

fn cfvo(n: &Node) -> Cfvo {
    Cfvo {
        kind: n.attr("type").unwrap_or("num").to_string(),
        val: n.attr("val").map(str::to_string).or_else(|| n.child("f").map(|f| f.text.trim().to_string())).unwrap_or_default(),
        gte: flag(n, "gte", true),
    }
}

fn read_rule(r: &Node, dxfs: &[Dxf], x14: bool) -> Rule {
    let mut rule = Rule {
        kind: r.attr("type").unwrap_or("").to_string(),
        priority: r.attr("priority").and_then(|v| v.parse().ok()).unwrap_or(i32::MAX),
        stop: flag(r, "stopIfTrue", false),
        operator: r.attr("operator").unwrap_or("").to_string(),
        text: r.attr("text").unwrap_or("").to_string(),
        rank: r.attr("rank").and_then(|v| v.parse().ok()).unwrap_or(10),
        percent: flag(r, "percent", false),
        bottom: flag(r, "bottom", false),
        above_average: flag(r, "aboveAverage", true),
        equal_average: flag(r, "equalAverage", false),
        std_dev: r.attr("stdDev").and_then(|v| v.parse().ok()).unwrap_or(0),
        time_period: r.attr("timePeriod").unwrap_or("").to_string(),
        show_value: true,
        ..Default::default()
    };
    // <formula> (x14: <xm:f>) children.
    rule.formulas = r.children.iter().filter(|c| c.name == "formula" || c.name == "f").map(|c| c.text.trim().to_string()).collect();
    rule.dxf = r.attr("dxfId").and_then(|v| v.parse::<usize>().ok()).and_then(|i| dxfs.get(i).cloned()).or_else(|| r.child("dxf").map(dxf));
    if let Some(cs) = r.child("colorScale") {
        rule.cfvo = cs.all("cfvo").map(cfvo).collect();
        rule.colors = cs.all("color").filter_map(color).collect();
    }
    if let Some(db) = r.child("dataBar") {
        rule.cfvo = db.all("cfvo").map(cfvo).collect();
        rule.show_value = flag(db, "showValue", true);
        let c = |name: &str| db.child(name).and_then(color);
        rule.bar = Some(BarDef {
            color: c("color").or_else(|| c("fillColor")),
            min_len: num_attr(db, "minLength").unwrap_or(10.0),
            max_len: num_attr(db, "maxLength").unwrap_or(90.0),
            gradient: flag(db, "gradient", true),
            border: if x14 && flag(db, "border", false) { c("borderColor") } else { None },
            negative: c("negativeFillColor"),
            negative_border: c("negativeBorderColor"),
            negative_same: flag(db, "negativeBarColorSameAsPositive", false),
            negative_border_same: flag(db, "negativeBarBorderColorSameAsPositive", true),
            axis: db.attr("axisPosition").unwrap_or("automatic").to_string(),
            axis_color: c("axisColor"),
            rtl: db.attr("direction") == Some("rightToLeft"),
            ext: x14,
        });
    }
    if let Some(is) = r.child("iconSet") {
        rule.cfvo = is.all("cfvo").map(cfvo).collect();
        rule.show_value = flag(is, "showValue", true);
        let custom = if flag(is, "custom", false) {
            is.all("cfIcon").map(|i| (i.attr("iconSet").unwrap_or("NoIcons").to_string(), i.attr("iconId").and_then(|v| v.parse().ok()).unwrap_or(0))).collect()
        } else {
            Vec::new()
        };
        rule.icons = Some(IconDef { set: is.attr("iconSet").unwrap_or("3TrafficLights1").to_string(), reverse: flag(is, "reverse", false), custom });
    }
    rule
}

// ---- working out ----

/// A data bar of a cell, in fractions of the cell's width.
#[derive(Debug, Clone, PartialEq)]
pub struct Bar {
    pub start: f64,
    pub len: f64,
    pub fill: String,
    pub border: Option<String>,
    pub gradient: bool,
    /// The bar grows to the left (a negative value, or right to left).
    pub leftward: bool,
    /// The axis between negative and positive values: where, and its color.
    pub axis: Option<(f64, String)>,
}

/// What the conditional formats give a cell.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Fx {
    pub fill: Option<String>,
    pub color: Option<String>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub strike: Option<bool>,
    pub underline: Option<bool>,
    pub num_fmt: Option<String>,
    /// left, right, top, bottom.
    pub sides: [Option<Side>; 4],
    pub bar: Option<Bar>,
    /// An icon (set, index in the set from the lowest).
    pub icon: Option<(String, usize)>,
    /// The value is not shown (a data bar or icon set "show icon only").
    pub hide_value: bool,
    stopped: bool,
}

impl Fx {
    fn add_dxf(&mut self, d: &Dxf, css: &dyn Fn(&Color) -> Option<String>) {
        if self.fill.is_none() {
            self.fill = d.fill.as_ref().and_then(css);
        }
        if self.color.is_none() {
            self.color = d.color.as_ref().and_then(css);
        }
        self.bold = self.bold.or(d.bold);
        self.italic = self.italic.or(d.italic);
        self.strike = self.strike.or(d.strike);
        self.underline = self.underline.or(d.underline);
        if self.num_fmt.is_none() {
            self.num_fmt = d.num_fmt.clone();
        }
        for i in 0..4 {
            if self.sides[i].is_none() {
                self.sides[i] = d.sides[i].clone();
            }
        }
    }
}

fn num(v: &CalcVal) -> Option<f64> {
    match v {
        CalcVal::Num(n) => Some(*n),
        _ => None,
    }
}

fn text(v: &CalcVal) -> String {
    match v {
        CalcVal::Num(n) => super::numfmt::general(*n),
        CalcVal::Str(s) => s.clone(),
        CalcVal::Bool(b) => (if *b { "TRUE" } else { "FALSE" }).into(),
        _ => String::new(),
    }
}

fn truthy(v: &CalcVal) -> bool {
    match v {
        CalcVal::Bool(b) => *b,
        CalcVal::Num(n) => *n != 0.0,
        CalcVal::Str(s) => s.eq_ignore_ascii_case("true"),
        _ => false,
    }
}

/// Excel's comparison of a cell's value with an operand: numbers below text
/// below booleans, text without case; an empty cell is 0 or "".
fn cmp(a: &CalcVal, b: &CalcVal) -> Option<std::cmp::Ordering> {
    use CalcVal::*;
    let rank = |v: &CalcVal| match v {
        Num(_) => 0,
        Str(_) => 1,
        Bool(_) => 2,
        _ => 3,
    };
    match (a, b) {
        (Err, _) | (_, Err) => None,
        (Empty, Empty) => Some(std::cmp::Ordering::Equal),
        (Empty, Num(_)) => cmp(&Num(0.0), b),
        (Empty, Str(_)) => cmp(&Str(String::new()), b),
        (Empty, Bool(_)) => cmp(&Bool(false), b),
        (_, Empty) => cmp(b, a).map(|o| o.reverse()),
        (Num(x), Num(y)) => x.partial_cmp(y),
        (Str(x), Str(y)) => Some(x.to_lowercase().cmp(&y.to_lowercase())),
        (Bool(x), Bool(y)) => Some(x.cmp(y)),
        _ => Some(rank(a).cmp(&rank(b))),
    }
}

/// PERCENTILE.INC of sorted values.
fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let k = p.clamp(0.0, 1.0) * (sorted.len() - 1) as f64;
    let i = k.floor() as usize;
    let f = k - i as f64;
    if i + 1 < sorted.len() { sorted[i] + f * (sorted[i + 1] - sorted[i]) } else { sorted[i] }
}

fn rgb(css: &str) -> Option<(f64, f64, f64)> {
    let h = css.trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    let p = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok().map(f64::from);
    Some((p(0)?, p(2)?, p(4)?))
}

fn hex((r, g, b): (f64, f64, f64)) -> String {
    format!("#{:02X}{:02X}{:02X}", r.round().clamp(0.0, 255.0) as u8, g.round().clamp(0.0, 255.0) as u8, b.round().clamp(0.0, 255.0) as u8)
}

/// a and b mixed: t = 0 is a, 1 is b.
pub fn mix(a: &str, b: &str, t: f64) -> String {
    match (rgb(a), rgb(b)) {
        (Some(x), Some(y)) => hex((x.0 + (y.0 - x.0) * t, x.1 + (y.1 - x.1) * t, x.2 + (y.2 - x.2) * t)),
        _ => a.to_string(),
    }
}

struct Ctx<'a> {
    b: &'a Book,
    sheet: &'a str,
    values: HashMap<(u32, u32), CalcVal>,
    last: (u32, u32),
}

impl Ctx<'_> {
    fn value(&mut self, c: u32, r: u32) -> CalcVal {
        if let Some(v) = self.values.get(&(c, r)) {
            return v.clone();
        }
        let v = value_at(self.b, self.sheet, c, r);
        self.values.insert((c, r), v.clone());
        v
    }
    /// The cells of a rule (clipped to the used part of the sheet).
    fn cells(&self, rule: &Rule) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for &(c1, r1, c2, r2) in &rule.ranges {
            for r in r1..=r2.min(self.last.1) {
                for c in c1..=c2.min(self.last.0) {
                    if seen.insert((c, r)) {
                        out.push((c, r));
                    }
                }
            }
        }
        out
    }
    /// A threshold's value among the numbers of a rule (sorted).
    fn threshold(&mut self, v: &Cfvo, nums: &[f64]) -> Option<f64> {
        let (min, max) = (*nums.first()?, *nums.last()?);
        let given = |ctx: &mut Self| -> Option<f64> {
            v.val.trim().parse::<f64>().ok().or_else(|| prepare(&v.val).and_then(|f| num(&eval_at(ctx.b, ctx.sheet, &f, 0, 0))))
        };
        Some(match v.kind.as_str() {
            "min" => min,
            "max" => max,
            "autoMin" => min.min(0.0),
            "autoMax" => max.max(0.0),
            "percent" => min + given(self)? / 100.0 * (max - min),
            "percentile" => percentile(nums, given(self)? / 100.0),
            _ => given(self)?,
        })
    }
}

/// What the conditional formats give the cells of a sheet. css turns a
/// color into #RRGGBB.
pub fn effects(b: &Book, sheet: &str, rules: &[Rule], css: &dyn Fn(&Color) -> Option<String>, today: chrono::NaiveDate) -> HashMap<(u32, u32), Fx> {
    let mut out: HashMap<(u32, u32), Fx> = HashMap::new();
    let Ok(ws) = b.sheet(sheet) else { return out };
    let mut ctx = Ctx { b, sheet, values: HashMap::new(), last: (ws.highest_column().max(1), ws.highest_row().max(1)) };
    for rule in rules {
        let cells = ctx.cells(rule);
        if cells.is_empty() {
            continue;
        }
        let base = rule.ranges.first().map(|a| (a.0, a.1)).unwrap_or((1, 1));
        let vals: Vec<CalcVal> = cells.iter().map(|&(c, r)| ctx.value(c, r)).collect();
        let mut nums: Vec<f64> = vals.iter().filter_map(num).collect();
        nums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let hits = hits(&mut ctx, rule, &cells, &vals, &nums, base, today, css);
        for (cell, eff) in hits {
            let fx = out.entry(cell).or_default();
            if fx.stopped {
                continue;
            }
            match eff {
                Eff::Dxf => {
                    if let Some(d) = &rule.dxf {
                        fx.add_dxf(d, css);
                    }
                }
                Eff::Fill(c) => {
                    if fx.fill.is_none() {
                        fx.fill = Some(c);
                    }
                }
                Eff::Bar(bar) => {
                    if fx.bar.is_none() && fx.icon.is_none() {
                        fx.bar = Some(bar);
                        fx.hide_value |= !rule.show_value;
                    }
                }
                Eff::Icon(i) => {
                    if fx.icon.is_none() && fx.bar.is_none() {
                        fx.icon = i;
                        fx.hide_value |= !rule.show_value;
                    }
                }
            }
            if rule.stop {
                fx.stopped = true;
            }
        }
    }
    out.retain(|_, f| *f != Fx { stopped: f.stopped, ..Default::default() });
    out
}

enum Eff {
    Dxf,
    Fill(String),
    Bar(Bar),
    Icon(Option<(String, usize)>),
}

#[allow(clippy::too_many_arguments)]
fn hits(ctx: &mut Ctx, rule: &Rule, cells: &[(u32, u32)], vals: &[CalcVal], nums: &[f64], base: (u32, u32), today: chrono::NaiveDate, css: &dyn Fn(&Color) -> Option<String>) -> Vec<((u32, u32), Eff)> {
    let mut out = Vec::new();
    let formulas: Vec<_> = rule.formulas.iter().map(|f| prepare(f)).collect();
    let operand = |ctx: &mut Ctx, i: usize, c: u32, r: u32| -> CalcVal {
        match formulas.get(i) {
            Some(Some(f)) => eval_at(ctx.b, ctx.sheet, f, c as i64 - base.0 as i64, r as i64 - base.1 as i64),
            _ => CalcVal::Err,
        }
    };
    match rule.kind.as_str() {
        "colorScale" => {
            let th: Option<Vec<f64>> = rule.cfvo.iter().map(|v| ctx.threshold(v, nums)).collect();
            let colors: Vec<String> = rule.colors.iter().filter_map(css).collect();
            let Some(th) = th else { return out };
            if th.len() < 2 || colors.len() != th.len() {
                return out;
            }
            for (cell, v) in cells.iter().zip(vals) {
                let Some(x) = num(v) else { continue };
                let k = th.len() - 1;
                let c = if x <= th[0] {
                    colors[0].clone()
                } else if x >= th[k] {
                    colors[k].clone()
                } else {
                    let i = (0..k).find(|&i| x <= th[i + 1]).unwrap_or(k - 1);
                    let span = th[i + 1] - th[i];
                    mix(&colors[i], &colors[i + 1], if span > 0.0 { (x - th[i]) / span } else { 0.0 })
                };
                out.push((*cell, Eff::Fill(c)));
            }
        }
        "dataBar" => {
            let Some(def) = &rule.bar else { return out };
            let (Some(lo), Some(hi)) = (rule.cfvo.first().and_then(|v| ctx.threshold(v, nums)), rule.cfvo.get(1).and_then(|v| ctx.threshold(v, nums))) else { return out };
            let fill = def.color.as_ref().and_then(css).unwrap_or_else(|| "#638EC6".into());
            let neg_fill = if def.negative_same { fill.clone() } else { def.negative.as_ref().and_then(css).unwrap_or_else(|| "#FF0000".into()) };
            let border = def.border.as_ref().and_then(css);
            let neg_border = if def.negative_border_same { border.clone() } else { def.negative_border.as_ref().and_then(css).or(border.clone()) };
            let axis_color = def.axis_color.as_ref().and_then(css).unwrap_or_else(|| "#000000".into());
            let (min_l, max_l) = (def.min_len / 100.0, def.max_len / 100.0);
            for (cell, v) in cells.iter().zip(vals) {
                let Some(x) = num(v) else { continue };
                let mut bar = if def.ext && def.axis != "none" && (lo < 0.0 || def.axis == "middle") {
                    // Bars grow both ways from the axis.
                    let (a, neg, pos) = if def.axis == "middle" {
                        let m = lo.abs().max(hi.abs());
                        (0.5, m, m)
                    } else {
                        let (n, p) = (-lo.min(0.0), hi.max(0.0));
                        (if n + p > 0.0 { n / (n + p) } else { 0.5 }, n, p)
                    };
                    let x = x.clamp(lo.min(0.0), hi.max(0.0));
                    if x >= 0.0 {
                        let len = if pos > 0.0 { (1.0 - a) * (x / pos) } else { 0.0 };
                        Bar { start: a, len, fill: fill.clone(), border: border.clone(), gradient: def.gradient, leftward: false, axis: Some((a, axis_color.clone())) }
                    } else {
                        let len = if neg > 0.0 { a * (-x / neg) } else { 0.0 };
                        Bar { start: a - len, len, fill: neg_fill.clone(), border: neg_border.clone(), gradient: def.gradient, leftward: true, axis: Some((a, axis_color.clone())) }
                    }
                } else {
                    let t = if hi > lo { ((x - lo) / (hi - lo)).clamp(0.0, 1.0) } else { 0.0 };
                    let len = min_l + t * (max_l - min_l);
                    Bar { start: 0.0, len, fill: fill.clone(), border: border.clone(), gradient: def.gradient, leftward: false, axis: None }
                };
                if def.rtl {
                    bar.start = 1.0 - bar.start - bar.len;
                    bar.leftward = !bar.leftward;
                    if let Some((a, c)) = bar.axis.take() {
                        bar.axis = Some((1.0 - a, c));
                    }
                }
                out.push((*cell, Eff::Bar(bar)));
            }
        }
        "iconSet" => {
            let Some(def) = &rule.icons else { return out };
            let th: Vec<Option<f64>> = rule.cfvo.iter().map(|v| ctx.threshold(v, nums)).collect();
            let n = th.len();
            if n == 0 {
                return out;
            }
            for (cell, v) in cells.iter().zip(vals) {
                let Some(x) = num(v) else { continue };
                let mut idx = 0;
                for i in (1..n).rev() {
                    if let Some(t) = th[i]
                        && (if rule.cfvo[i].gte { x >= t } else { x > t })
                    {
                        idx = i;
                        break;
                    }
                }
                if def.reverse {
                    idx = n - 1 - idx;
                }
                let icon = match def.custom.get(idx) {
                    Some((set, _)) if set == "NoIcons" => None,
                    Some((set, i)) => Some((set.clone(), *i)),
                    None if def.set == "NoIcons" => None,
                    None => Some((def.set.clone(), idx)),
                };
                out.push((*cell, Eff::Icon(icon)));
            }
        }
        _ => {
            // The rules that give the differential format.
            let avg = if nums.is_empty() { 0.0 } else { nums.iter().sum::<f64>() / nums.len() as f64 };
            let sd = if nums.len() > 1 { (nums.iter().map(|x| (x - avg).powi(2)).sum::<f64>() / (nums.len() - 1) as f64).sqrt() } else { 0.0 };
            let cut = if rule.kind == "top10" && !nums.is_empty() {
                let k = if rule.percent { ((nums.len() as f64 * rule.rank as f64 / 100.0).floor() as usize).max(1) } else { rule.rank.max(1) as usize };
                let k = k.min(nums.len());
                Some(if rule.bottom { nums[k - 1] } else { nums[nums.len() - k] })
            } else {
                None
            };
            let key = |v: &CalcVal| match v {
                CalcVal::Num(n) => Some(format!("n{n}")),
                CalcVal::Str(s) if !s.is_empty() => Some(format!("s{}", s.to_lowercase())),
                CalcVal::Bool(b) => Some(format!("b{b}")),
                _ => None,
            };
            let mut counts: HashMap<String, usize> = HashMap::new();
            if matches!(rule.kind.as_str(), "duplicateValues" | "uniqueValues") {
                for v in vals {
                    if let Some(k) = key(v) {
                        *counts.entry(k).or_default() += 1;
                    }
                }
            }
            let needle = rule.text.to_lowercase();
            for (&(c, r), v) in cells.iter().zip(vals) {
                let hit = match rule.kind.as_str() {
                    "cellIs" => {
                        use std::cmp::Ordering::*;
                        let a = operand(ctx, 0, c, r);
                        let o = cmp(v, &a);
                        match rule.operator.as_str() {
                            "lessThan" => o == Some(Less),
                            "lessThanOrEqual" => matches!(o, Some(Less | Equal)),
                            "equal" => o == Some(Equal),
                            "notEqual" => o.is_some_and(|o| o != Equal),
                            "greaterThanOrEqual" => matches!(o, Some(Greater | Equal)),
                            "greaterThan" => o == Some(Greater),
                            "between" | "notBetween" => {
                                let z = operand(ctx, 1, c, r);
                                // The two bounds in either order.
                                let (lo, hi) = if cmp(&a, &z) == Some(Greater) { (z, a) } else { (a, z) };
                                let inside = matches!(cmp(v, &lo), Some(Greater | Equal)) && matches!(cmp(v, &hi), Some(Less | Equal));
                                if rule.operator == "between" { inside } else { cmp(v, &lo).is_some() && !inside }
                            }
                            _ => false,
                        }
                    }
                    "expression" => truthy(&operand(ctx, 0, c, r)),
                    "containsText" => text(v).to_lowercase().contains(&needle),
                    "notContainsText" => !text(v).to_lowercase().contains(&needle),
                    "beginsWith" => text(v).to_lowercase().starts_with(&needle),
                    "endsWith" => text(v).to_lowercase().ends_with(&needle),
                    "containsBlanks" => matches!(v, CalcVal::Empty) || matches!(v, CalcVal::Str(s) if s.trim().is_empty()),
                    "notContainsBlanks" => !(matches!(v, CalcVal::Empty) || matches!(v, CalcVal::Str(s) if s.trim().is_empty())),
                    "containsErrors" => matches!(v, CalcVal::Err),
                    "notContainsErrors" => !matches!(v, CalcVal::Err),
                    "top10" => match (num(v), cut) {
                        (Some(x), Some(k)) => if rule.bottom { x <= k } else { x >= k },
                        _ => false,
                    },
                    "aboveAverage" => match num(v) {
                        Some(x) if !nums.is_empty() => {
                            let d = rule.std_dev as f64 * sd;
                            match (rule.above_average, rule.equal_average) {
                                (true, false) => x > avg + d,
                                (true, true) => x >= avg + d,
                                (false, false) => x < avg - d,
                                (false, true) => x <= avg - d,
                            }
                        }
                        _ => false,
                    },
                    "duplicateValues" => key(v).is_some_and(|k| counts.get(&k).copied().unwrap_or(0) > 1),
                    "uniqueValues" => key(v).is_some_and(|k| counts.get(&k).copied().unwrap_or(0) == 1),
                    "timePeriod" => num(v).and_then(|x| super::xlsx::from_serial(x, ctx.b.date1904)).and_then(|t| chrono::NaiveDate::from_ymd_opt(t.0, t.1, t.2)).is_some_and(|d| in_period(d, today, &rule.time_period)),
                    _ => false,
                };
                if hit {
                    out.push(((c, r), Eff::Dxf));
                }
            }
        }
    }
    out
}

fn in_period(d: chrono::NaiveDate, today: chrono::NaiveDate, period: &str) -> bool {
    use chrono::{Datelike, Duration};
    let days = (d - today).num_days();
    // Weeks start on Sunday.
    let week = |t: chrono::NaiveDate| t - Duration::days(t.weekday().num_days_from_sunday() as i64);
    let month = |t: chrono::NaiveDate, add: i32| {
        let m = t.year() * 12 + t.month0() as i32 + add;
        (m.div_euclid(12), m.rem_euclid(12) as u32 + 1)
    };
    match period {
        "today" => days == 0,
        "yesterday" => days == -1,
        "tomorrow" => days == 1,
        "last7Days" => (-6..=0).contains(&days),
        "thisWeek" => week(d) == week(today),
        "lastWeek" => week(d) == week(today) - Duration::days(7),
        "nextWeek" => week(d) == week(today) + Duration::days(7),
        "thisMonth" => (d.year(), d.month()) == month(today, 0),
        "lastMonth" => (d.year(), d.month()) == month(today, -1),
        "nextMonth" => (d.year(), d.month()) == month(today, 1),
        _ => false,
    }
}

// ---- icons ----

const GREEN: &str = "#33A02C";
const YELLOW: &str = "#F2B400";
const RED: &str = "#D93025";
const GRAY: &str = "#8C8C8C";
const BLACK: &str = "#262626";

fn arrow(deg: i32, c: &str) -> String {
    format!("<path transform=\"rotate({deg} 8 8)\" d=\"M8 1.5L14 8H10.2V14.5H5.8V8H2Z\" fill=\"{c}\"/>")
}

fn circle(c: &str) -> String {
    format!("<circle cx=\"8\" cy=\"8\" r=\"6.5\" fill=\"{c}\"/>")
}

fn mark(kind: char, c: &str, circled: bool) -> String {
    let (bg, fg) = if circled { (c, "#fff") } else { ("none", c) };
    let w = if circled { "2" } else { "2.6" };
    let body = match kind {
        'v' => format!("<path d=\"M4.5 8.3L7 10.8L11.6 5.4\" fill=\"none\" stroke=\"{fg}\" stroke-width=\"{w}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>"),
        '!' => format!("<path d=\"M8 4V9\" stroke=\"{fg}\" stroke-width=\"{w}\" stroke-linecap=\"round\"/><circle cx=\"8\" cy=\"12\" r=\"1.2\" fill=\"{fg}\"/>"),
        _ => format!("<path d=\"M5.2 5.2L10.8 10.8M10.8 5.2L5.2 10.8\" stroke=\"{fg}\" stroke-width=\"{w}\" stroke-linecap=\"round\"/>"),
    };
    if circled { format!("<circle cx=\"8\" cy=\"8\" r=\"7\" fill=\"{bg}\"/>{body}") } else { body }
}

fn bars(on: usize) -> String {
    (0..4)
        .map(|i| {
            let h = 3.0 + i as f64 * 3.5;
            let c = if i < on { "#3C78C8" } else { "#D0D0D0" };
            format!("<rect x=\"{}\" y=\"{}\" width=\"2.6\" height=\"{h}\" fill=\"{c}\"/>", 1.5 + i as f64 * 3.5, 14.5 - h)
        })
        .collect()
}

fn quarters(q: usize) -> String {
    let c = "#4A4A4A";
    let ring = format!("<circle cx=\"8\" cy=\"8\" r=\"6.3\" fill=\"#fff\" stroke=\"{c}\" stroke-width=\"1.2\"/>");
    let fill = match q {
        0 => String::new(),
        4 => format!("<circle cx=\"8\" cy=\"8\" r=\"6.3\" fill=\"{c}\"/>"),
        _ => {
            let a = q as f64 * std::f64::consts::FRAC_PI_2;
            let (x, y) = (8.0 + 6.3 * a.sin(), 8.0 - 6.3 * a.cos());
            format!("<path d=\"M8 8V1.7A6.3 6.3 0 {} 1 {x:.2} {y:.2}Z\" fill=\"{c}\"/>", if q > 2 { 1 } else { 0 })
        }
    };
    ring + &fill
}

fn star(fill: f64) -> String {
    let d = "M8 1.2L10 5.8L15 6.2L11.2 9.4L12.4 14.4L8 11.7L3.6 14.4L4.8 9.4L1 6.2L6 5.8Z";
    let c = "#F2B400";
    match fill {
        f if f >= 1.0 => format!("<path d=\"{d}\" fill=\"{c}\" stroke=\"{c}\" stroke-width=\".6\"/>"),
        f if f > 0.0 => format!("<clipPath id=\"h\"><rect width=\"8\" height=\"16\"/></clipPath><path d=\"{d}\" fill=\"#fff\" stroke=\"{c}\" stroke-width=\".8\"/><path d=\"{d}\" fill=\"{c}\" clip-path=\"url(#h)\"/>"),
        _ => format!("<path d=\"{d}\" fill=\"#fff\" stroke=\"{c}\" stroke-width=\".8\"/>"),
    }
}

fn boxes(on: usize) -> String {
    (0..4)
        .map(|i| {
            let (x, y) = (1.5 + (i % 2) as f64 * 7.0, 1.5 + (i / 2) as f64 * 7.0);
            let c = if i < on { "#3C78C8" } else { "#D0D0D0" };
            format!("<rect x=\"{x}\" y=\"{y}\" width=\"6\" height=\"6\" fill=\"{c}\"/>")
        })
        .collect()
}

/// The icon of a set (index from the lowest values) as SVG, or None for an
/// unknown set.
pub fn icon_svg(set: &str, i: usize) -> Option<String> {
    let pick = |v: &[String]| v.get(i).cloned();
    let body = match set {
        "3Arrows" => pick(&[arrow(180, RED), arrow(90, YELLOW), arrow(0, GREEN)]),
        "3ArrowsGray" => pick(&[arrow(180, GRAY), arrow(90, GRAY), arrow(0, GRAY)]),
        "4Arrows" => pick(&[arrow(180, RED), arrow(135, YELLOW), arrow(45, YELLOW), arrow(0, GREEN)]),
        "4ArrowsGray" => pick(&[arrow(180, GRAY), arrow(135, GRAY), arrow(45, GRAY), arrow(0, GRAY)]),
        "5Arrows" => pick(&[arrow(180, RED), arrow(135, YELLOW), arrow(90, YELLOW), arrow(45, YELLOW), arrow(0, GREEN)]),
        "5ArrowsGray" => pick(&[arrow(180, GRAY), arrow(135, GRAY), arrow(90, GRAY), arrow(45, GRAY), arrow(0, GRAY)]),
        "3Flags" => pick(&[RED, YELLOW, GREEN].map(|c| format!("<path d=\"M3.5 1.5V15\" stroke=\"#555\" stroke-width=\"1.3\"/><path d=\"M4 2H13.5L11 5.5L13.5 9H4Z\" fill=\"{c}\"/>"))),
        "3TrafficLights1" => pick(&[circle(RED), circle(YELLOW), circle(GREEN)]),
        "3TrafficLights2" => pick(&[RED, YELLOW, GREEN].map(|c| format!("<rect x=\"1\" y=\"1\" width=\"14\" height=\"14\" rx=\"3\" fill=\"{BLACK}\"/><circle cx=\"8\" cy=\"8\" r=\"5\" fill=\"{c}\"/>"))),
        "4TrafficLights" => pick(&[circle(BLACK), circle(RED), circle(YELLOW), circle(GREEN)]),
        "4RedToBlack" => pick(&[circle(BLACK), circle(GRAY), circle("#F4A6A6"), circle(RED)]),
        "3Signs" => pick(&[
            format!("<path d=\"M8 1L15 8L8 15L1 8Z\" fill=\"{RED}\"/>"),
            format!("<path d=\"M8 1.5L15 14.5H1Z\" fill=\"{YELLOW}\"/>"),
            circle(GREEN),
        ]),
        "3Symbols" => pick(&[mark('x', RED, true), mark('!', YELLOW, true), mark('v', GREEN, true)]),
        "3Symbols2" => pick(&[mark('x', RED, false), mark('!', YELLOW, false), mark('v', GREEN, false)]),
        "4Rating" => pick(&[bars(1), bars(2), bars(3), bars(4)]),
        "5Rating" => pick(&[bars(0), bars(1), bars(2), bars(3), bars(4)]),
        "5Quarters" => pick(&[quarters(0), quarters(1), quarters(2), quarters(3), quarters(4)]),
        "3Stars" => pick(&[star(0.0), star(0.5), star(1.0)]),
        "3Triangles" => pick(&[
            format!("<path d=\"M2 4.5H14L8 13Z\" fill=\"{RED}\"/>"),
            format!("<rect x=\"2.5\" y=\"6.5\" width=\"11\" height=\"3\" fill=\"{YELLOW}\"/>"),
            format!("<path d=\"M2 11.5H14L8 3Z\" fill=\"{GREEN}\"/>"),
        ]),
        "5Boxes" => pick(&[boxes(0), boxes(1), boxes(2), boxes(3), boxes(4)]),
        _ => None,
    }?;
    Some(format!("<svg viewBox=\"0 0 16 16\" width=\"100%\" height=\"100%\">{body}</svg>"))
}

#[cfg(test)]
mod tests;
