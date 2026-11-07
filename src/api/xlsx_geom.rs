//! The geometry of DrawingML shapes: the preset shapes (prstGeom, from the
//! definitions of ECMA-376, `preset_shapes.xml`) and freeforms (custGeom)
//! evaluated with their adjust values and guides into SVG paths.
//!
//! Guides are evaluated in EMU (as the spec does). Paths with their own
//! w/h are in that coordinate space; the result is in pt.

use std::collections::HashMap;
use std::f64::consts::PI;
use std::fmt::Write as _;
use std::sync::OnceLock;

use super::xlsx_chart::{Node, parse};
use super::xlsx_print::n;

const EMU_PT: f64 = 12700.0;

/// How a path is filled: as the shape, not at all, or shaded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fill {
    None,
    Norm,
    Lighten,
    LightenLess,
    Darken,
    DarkenLess,
}

/// One path of a shape (SVG path data in pt).
#[derive(Debug)]
pub struct GPath {
    pub d: String,
    pub fill: Fill,
    pub stroke: bool,
    /// Whether the path ends open (arrowheads go on open paths).
    pub open: bool,
    /// The path as points (pt; curves and arcs sampled), for text warps.
    pub points: Vec<(f64, f64)>,
}

#[derive(Debug)]
pub struct Geom {
    pub paths: Vec<GPath>,
    /// The text box (left, top, right, bottom in pt from the shape's box).
    pub text: Option<(f64, f64, f64, f64)>,
}

/// The preset shape definitions by name.
fn presets() -> &'static HashMap<String, Node> {
    static P: OnceLock<HashMap<String, Node>> = OnceLock::new();
    P.get_or_init(|| {
        let root = parse(include_bytes!("preset_shapes.xml")).unwrap_or_default();
        root.children.into_iter().map(|c| (c.name.clone(), c)).collect()
    })
}

/// The geometry of a preset (prstGeom with its avLst) in a w × h (pt) box;
/// None for presets that are not defined.
pub fn preset(prst: &str, geom: Option<&Node>, w: f64, h: f64) -> Option<Geom> {
    let def = presets().get(prst)?;
    let mut g = evaluate(def, geom.and_then(|g| g.child("avLst")), w, h);
    // Some definitions (chartPlus, chartStar, chartX) put their lines
    // before the fill that would hide them: those go on top.
    let lead = g.paths.iter().take_while(|p| p.fill == Fill::None).count();
    if lead < g.paths.len() {
        g.paths.rotate_left(lead);
    }
    Some(g)
}

/// The preset text warp definitions (prstTxWarp) by name.
fn text_warps() -> &'static HashMap<String, Node> {
    static P: OnceLock<HashMap<String, Node>> = OnceLock::new();
    P.get_or_init(|| {
        let root = parse(include_bytes!("preset_text_warps.xml")).unwrap_or_default();
        root.children.into_iter().map(|c| (c.name.clone(), c)).collect()
    })
}

/// The paths of a text warp (bodyPr's prstTxWarp with its avLst) in a w × h
/// (pt) text box, as points; None for textNoShape and unknown names.
pub fn text_warp(prst: &str, warp: Option<&Node>, w: f64, h: f64) -> Option<Vec<Vec<(f64, f64)>>> {
    let def = text_warps().get(prst)?;
    let g = evaluate(def, warp.and_then(|g| g.child("avLst")), w, h);
    let paths: Vec<Vec<(f64, f64)>> = g.paths.into_iter().map(|p| p.points).filter(|p| p.len() >= 2).collect();
    (!paths.is_empty()).then_some(paths)
}

/// The geometry of a freeform (custGeom) in a w × h (pt) box.
pub fn custom(geom: &Node, w: f64, h: f64) -> Geom {
    evaluate(geom, None, w, h)
}

struct Guides {
    vars: HashMap<String, f64>,
}

impl Guides {
    fn new(w: f64, h: f64) -> Guides {
        let mut v = HashMap::new();
        let ss = w.min(h);
        for (k, x) in [("w", w), ("h", h), ("l", 0.0), ("t", 0.0), ("r", w), ("b", h), ("hc", w / 2.0), ("vc", h / 2.0), ("ss", ss), ("ls", w.max(h))] {
            v.insert(k.to_string(), x);
        }
        for d in 2..=32 {
            v.insert(format!("wd{d}"), w / d as f64);
            v.insert(format!("hd{d}"), h / d as f64);
            v.insert(format!("ssd{d}"), ss / d as f64);
        }
        for (k, x) in [("cd2", 10800000.0), ("cd4", 5400000.0), ("cd8", 2700000.0), ("3cd4", 16200000.0), ("3cd8", 8100000.0), ("5cd8", 13500000.0), ("7cd8", 18900000.0)] {
            v.insert(k.to_string(), x);
        }
        Guides { vars: v }
    }

    fn get(&self, s: &str) -> f64 {
        s.parse::<f64>().ok().or_else(|| self.vars.get(s).copied()).unwrap_or(0.0)
    }

    fn opt(&self, s: Option<&str>) -> f64 {
        s.map(|s| self.get(s)).unwrap_or(0.0)
    }

    /// The value of a guide formula.
    fn eval(&self, fmla: &str) -> f64 {
        let mut it = fmla.split_whitespace();
        let op = it.next().unwrap_or("");
        let a: Vec<f64> = it.map(|s| self.get(s)).collect();
        let x = |i: usize| a.get(i).copied().unwrap_or(0.0);
        let ang = |v: f64| v / 60000.0 * PI / 180.0;
        let deg = |r: f64| r * 180.0 / PI * 60000.0;
        let div = |p: f64, q: f64| if q == 0.0 { 0.0 } else { p / q };
        match op {
            "val" => x(0),
            "*/" => div(x(0) * x(1), x(2)),
            "+-" => x(0) + x(1) - x(2),
            "+/" => div(x(0) + x(1), x(2)),
            "?:" => {
                if x(0) > 0.0 {
                    x(1)
                } else {
                    x(2)
                }
            }
            "abs" => x(0).abs(),
            "at2" => deg(x(1).atan2(x(0))),
            "cat2" => x(0) * x(2).atan2(x(1)).cos(),
            "sat2" => x(0) * x(2).atan2(x(1)).sin(),
            "cos" => x(0) * ang(x(1)).cos(),
            "sin" => x(0) * ang(x(1)).sin(),
            "tan" => x(0) * ang(x(1)).tan(),
            "max" => x(0).max(x(1)),
            "min" => x(0).min(x(1)),
            "mod" => (x(0) * x(0) + x(1) * x(1) + x(2) * x(2)).sqrt(),
            "pin" => {
                if x(1) < x(0) {
                    x(0)
                } else if x(1) > x(2) {
                    x(2)
                } else {
                    x(1)
                }
            }
            "sqrt" => x(0).max(0.0).sqrt(),
            _ => 0.0,
        }
    }

    fn add(&mut self, list: Option<&Node>) {
        for gd in list.into_iter().flat_map(|l| l.all("gd")) {
            if let (Some(name), Some(f)) = (gd.attr("name"), gd.attr("fmla")) {
                let v = self.eval(f);
                self.vars.insert(name.to_string(), if v.is_finite() { v } else { 0.0 });
            }
        }
    }
}

fn evaluate(def: &Node, av: Option<&Node>, w: f64, h: f64) -> Geom {
    let (we, he) = (w * EMU_PT, h * EMU_PT);
    let mut g = Guides::new(we, he);
    g.add(def.child("avLst"));
    g.add(av);
    g.add(def.child("gdLst"));
    let text = def.child("rect").map(|r| {
        let v = |k: &str| g.opt(r.attr(k)) / EMU_PT;
        (v("l"), v("t"), v("r"), v("b"))
    });
    // An empty text box (as some writers give freeforms) is the whole box.
    let text = text.filter(|(l, t, r, b)| r > l && b > t);
    let mut paths = Vec::new();
    for p in def.child("pathLst").into_iter().flat_map(|l| l.all("path")) {
        paths.push(path(&g, p, we, he));
    }
    Geom { paths, text }
}

fn path(g: &Guides, p: &Node, we: f64, he: f64) -> GPath {
    let num = |k: &str| p.attr(k).and_then(|v| v.parse::<f64>().ok()).filter(|v| *v > 0.0);
    // Path coordinates → pt.
    let kx = num("w").map(|pw| we / pw).unwrap_or(1.0) / EMU_PT;
    let ky = num("h").map(|ph| he / ph).unwrap_or(1.0) / EMU_PT;
    let fill = match p.attr("fill") {
        Some("none") => Fill::None,
        Some("lighten") => Fill::Lighten,
        Some("lightenLess") => Fill::LightenLess,
        Some("darken") => Fill::Darken,
        Some("darkenLess") => Fill::DarkenLess,
        _ => Fill::Norm,
    };
    let stroke = !matches!(p.attr("stroke"), Some("0" | "false"));
    let pt = |c: &Node| (g.opt(c.attr("x")), g.opt(c.attr("y")));
    let xy = |(x, y): (f64, f64)| format!("{},{}", n(x * kx), n(y * ky));
    let mut d = String::new();
    let mut cur = (0.0, 0.0);
    let mut start = (0.0, 0.0);
    let mut open = false;
    let mut points: Vec<(f64, f64)> = Vec::new();
    let to_pt = |(x, y): (f64, f64)| (x * kx, y * ky);
    // Samples of a curve from cur, at t in (0, 1].
    let curve = |points: &mut Vec<(f64, f64)>, f: &dyn Fn(f64) -> (f64, f64)| {
        for i in 1..=24 {
            points.push(to_pt(f(i as f64 / 24.0)));
        }
    };
    for c in &p.children {
        let ps: Vec<(f64, f64)> = c.all("pt").map(pt).collect();
        match c.name.as_str() {
            "moveTo" if !ps.is_empty() => {
                cur = ps[0];
                start = cur;
                let _ = write!(d, "M{} ", xy(cur));
                open = false;
                points.push(to_pt(cur));
            }
            "lnTo" if !ps.is_empty() => {
                cur = ps[0];
                let _ = write!(d, "L{} ", xy(cur));
                open = true;
                points.push(to_pt(cur));
            }
            "cubicBezTo" if ps.len() == 3 => {
                let (p0, [p1, p2, p3]) = (cur, [ps[0], ps[1], ps[2]]);
                curve(&mut points, &|t| {
                    let u = 1.0 - t;
                    let k = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
                    (k[0] * p0.0 + k[1] * p1.0 + k[2] * p2.0 + k[3] * p3.0, k[0] * p0.1 + k[1] * p1.1 + k[2] * p2.1 + k[3] * p3.1)
                });
                cur = ps[2];
                let _ = write!(d, "C{} {} {} ", xy(ps[0]), xy(ps[1]), xy(ps[2]));
                open = true;
            }
            "quadBezTo" if ps.len() == 2 => {
                let (p0, p1, p2) = (cur, ps[0], ps[1]);
                curve(&mut points, &|t| {
                    let u = 1.0 - t;
                    (u * u * p0.0 + 2.0 * u * t * p1.0 + t * t * p2.0, u * u * p0.1 + 2.0 * u * t * p1.1 + t * t * p2.1)
                });
                cur = ps[1];
                let _ = write!(d, "Q{} {} ", xy(ps[0]), xy(ps[1]));
                open = true;
            }
            "arcTo" => {
                if d.is_empty() {
                    let _ = write!(d, "M{} ", xy(cur));
                    start = cur;
                    points.push(to_pt(cur));
                }
                let (wr, hr) = (g.opt(c.attr("wR")), g.opt(c.attr("hR")));
                let (st, sw) = (g.opt(c.attr("stAng")), g.opt(c.attr("swAng")));
                cur = arc(&mut d, &mut points, cur, wr, hr, st, sw, kx, ky);
                open = true;
            }
            "close" => {
                d.push_str("Z ");
                cur = start;
                points.push(to_pt(cur));
                open = false;
            }
            _ => {}
        }
    }
    let d = d.trim_end().to_string();
    GPath { d, fill, stroke, open, points }
}

/// An arcTo from cur: an arc of the ellipse wr × hr from the angle st by
/// sw (60000ths of a degree; the angles are of the ellipse as drawn, not
/// its parameter). Returns the end point.
#[allow(clippy::too_many_arguments)]
fn arc(d: &mut String, points: &mut Vec<(f64, f64)>, cur: (f64, f64), wr: f64, hr: f64, st: f64, sw: f64, kx: f64, ky: f64) -> (f64, f64) {
    let rad = |v: f64| v / 60000.0 * PI / 180.0;
    // The parameter of the point at a visual angle.
    let param = |a: f64| (wr * a.sin()).atan2(hr * a.cos());
    let (a1, a2) = (rad(st), rad(st + sw));
    let p1 = param(a1);
    let center = (cur.0 - wr * p1.cos(), cur.1 - hr * p1.sin());
    // The parameter's sweep follows the visual sweep (same turns and direction).
    let mut p2 = param(a2);
    let turns = (sw / 21600000.0).trunc();
    let rem = rad(sw) - turns * 2.0 * PI;
    if rem > 0.0 {
        while p2 <= p1 {
            p2 += 2.0 * PI;
        }
        while p2 - p1 > 2.0 * PI {
            p2 -= 2.0 * PI;
        }
    } else if rem < 0.0 {
        while p2 >= p1 {
            p2 -= 2.0 * PI;
        }
        while p1 - p2 > 2.0 * PI {
            p2 += 2.0 * PI;
        }
    } else {
        p2 = p1;
    }
    p2 += turns * 2.0 * PI;
    let at = |p: f64| (center.0 + wr * p.cos(), center.1 + hr * p.sin());
    let end = at(p2);
    let steps = ((p2 - p1).abs() / (PI / 48.0)).ceil().clamp(1.0, 1000.0) as usize;
    for i in 1..=steps {
        let q = at(p1 + (p2 - p1) * i as f64 / steps as f64);
        points.push((q.0 * kx, q.1 * ky));
    }
    let (rx, ry) = (n((wr * kx).abs()), n((hr * ky).abs()));
    if wr == 0.0 || hr == 0.0 || sw == 0.0 {
        let _ = write!(d, "L{},{} ", n(end.0 * kx), n(end.1 * ky));
        return end;
    }
    // SVG arcs cannot do a whole turn: in pieces of at most half a turn.
    let total = p2 - p1;
    let pieces = (total.abs() / PI).ceil().max(1.0) as usize;
    let sweep = if total > 0.0 { 1 } else { 0 };
    for i in 1..=pieces {
        let q = at(p1 + total * i as f64 / pieces as f64);
        let _ = write!(d, "A{rx},{ry} 0 0 {sweep} {},{} ", n(q.0 * kx), n(q.1 * ky));
    }
    end
}

/// A color shaded for a path's fill mode ("#RRGGBB").
pub fn shade(color: &str, fill: Fill) -> String {
    let (f, white) = match fill {
        Fill::Darken => (0.6, false),
        Fill::DarkenLess => (0.8, false),
        Fill::Lighten => (0.6, true),
        Fill::LightenLess => (0.8, true),
        _ => return color.to_string(),
    };
    let Some(hex) = color.strip_prefix('#').filter(|h| h.len() == 6) else {
        return color.to_string();
    };
    let Ok(v) = u32::from_str_radix(hex, 16) else {
        return color.to_string();
    };
    let ch = |s: u32| {
        let c = ((v >> s) & 0xff) as f64;
        let c = if white { 255.0 - (255.0 - c) * f } else { c * f };
        c.round().clamp(0.0, 255.0) as u32
    };
    format!("#{:02X}{:02X}{:02X}", ch(16), ch(8), ch(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(prst: &str, w: f64, h: f64) -> Vec<String> {
        preset(prst, None, w, h).unwrap().paths.into_iter().map(|p| p.d).collect()
    }

    #[test]
    fn all_presets() {
        // Every definition evaluates to finite paths.
        assert!(presets().len() >= 180, "{}", presets().len());
        for name in presets().keys() {
            let g = preset(name, None, 100.0, 60.0).unwrap();
            assert!(!g.paths.is_empty(), "{name}");
            for p in &g.paths {
                assert!(!p.d.contains("NaN") && !p.d.contains("inf"), "{name}: {}", p.d);
            }
        }
    }

    #[test]
    fn simple() {
        assert_eq!(d("rect", 10.0, 5.0), ["M0,0 L10,0 L10,5 L0,5 Z"]);
        assert_eq!(d("diamond", 10.0, 6.0), ["M0,3 L5,0 L10,3 L5,6 Z"]);
        // A right arrow: shaft half the height, head half the short side.
        assert_eq!(d("rightArrow", 20.0, 10.0), ["M0,2.5 L15,2.5 L15,0 L20,5 L15,10 L15,7.5 L0,7.5 Z"]);
        let line = preset("straightConnector1", None, 10.0, 0.0).unwrap();
        assert!(line.paths.iter().all(|p| p.fill == Fill::None && p.open));
        // An ellipse: two half turns from the left.
        assert_eq!(d("ellipse", 10.0, 6.0), ["M0,3 A5,3 0 0 1 5,0 A5,3 0 0 1 10,3 A5,3 0 0 1 5,6 A5,3 0 0 1 0,3 Z"]);
        // A rounded rectangle with its adjust value.
        let av = parse(br#"<prstGeom><avLst><gd name="adj" fmla="val 50000"/></avLst></prstGeom>"#).unwrap();
        let g = preset("roundRect", Some(&av), 10.0, 4.0).unwrap();
        assert!(g.paths[0].d.starts_with("M0,2 A2,2 0 0 1 2,0 L8,0"), "{}", g.paths[0].d);
        assert!(preset("noSuchShape", None, 10.0, 10.0).is_none());
        // The lines of chartPlus are drawn over its fill.
        let g = preset("chartPlus", None, 10.0, 10.0).unwrap();
        assert_eq!((g.paths[0].fill, g.paths[1].fill), (Fill::Norm, Fill::None));
    }

    #[test]
    fn text_rect() {
        // The text of an ellipse is in the rectangle inside it.
        let g = preset("ellipse", None, 100.0, 100.0).unwrap();
        let (l, t, r, b) = g.text.unwrap();
        assert!((l - 14.645).abs() < 0.01 && (t - l).abs() < 1e-9 && (r - 85.355).abs() < 0.01 && (b - r).abs() < 1e-9, "{:?}", g.text);
    }

    #[test]
    fn all_text_warps() {
        // Every text warp evaluates to finite paths of points.
        assert!(text_warps().len() >= 40, "{}", text_warps().len());
        for name in text_warps().keys() {
            let paths = text_warp(name, None, 100.0, 60.0).unwrap_or_else(|| panic!("{name}"));
            for p in paths.iter().flatten() {
                assert!(p.0.is_finite() && p.1.is_finite(), "{name}: {p:?}");
            }
        }
        assert!(text_warp("textNoShape", None, 100.0, 60.0).is_none());
        // Plain: the top and the bottom of the box.
        assert_eq!(text_warp("textPlain", None, 100.0, 60.0).unwrap(), vec![vec![(0.0, 0.0), (100.0, 0.0)], vec![(0.0, 60.0), (100.0, 60.0)]]);
        // Arch up: one path over the top half of the ellipse, from the left to the right.
        let arch = text_warp("textArchUp", None, 100.0, 60.0).unwrap();
        assert_eq!(arch.len(), 1);
        let (first, last) = (arch[0][0], *arch[0].last().unwrap());
        assert!((first.0).abs() < 1e-6 && (first.1 - 30.0).abs() < 1e-6 && (last.0 - 100.0).abs() < 1e-6, "{first:?} {last:?}");
        assert!(arch[0].iter().all(|p| p.1 <= 30.0 + 1e-6) && arch[0].iter().any(|p| p.1.abs() < 0.1));
        // An adjust value (pinned to at most 20%): the ends of an inflated band come in by it.
        let av = parse(br#"<prstTxWarp><avLst><gd name="adj" fmla="val 25000"/></avLst></prstTxWarp>"#).unwrap();
        let top = text_warp("textInflate", Some(&av), 100.0, 60.0).unwrap().remove(0);
        let min = top.iter().map(|p| p.1).fold(f64::MAX, f64::min);
        assert!(min.abs() < 0.5 && (top[0].1 - 12.0).abs() < 1e-6, "{min} {:?}", top[0]);
    }

    #[test]
    fn shades() {
        assert_eq!(shade("#FF8000", Fill::Darken), "#994D00");
        assert_eq!(shade("#000000", Fill::Lighten), "#666666");
        assert_eq!(shade("#123456", Fill::Norm), "#123456");
    }

    #[test]
    fn freeform() {
        let g = parse(br#"<custGeom><gdLst><gd name="x" fmla="*/ w 1 2"/></gdLst><pathLst><path w="100" h="50"><moveTo><pt x="0" y="0"/></moveTo><lnTo><pt x="100" y="50"/></lnTo></path><path fill="none"><moveTo><pt x="x" y="0"/></moveTo><lnTo><pt x="x" y="h"/></lnTo></path></pathLst></custGeom>"#).unwrap();
        let g = custom(&g, 20.0, 10.0);
        assert_eq!(g.paths[0].d, "M0,0 L20,10");
        assert_eq!(g.paths[1].d, "M10,0 L10,10");
        assert_eq!(g.paths[1].fill, Fill::None);
    }
}
