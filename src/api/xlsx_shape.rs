//! The drawings of a sheet for the print layout (xlsx_print.rs): shapes,
//! text boxes, lines and connectors, groups and pictures as HTML/SVG, in
//! the order of the sheet's drawing, with the places of its charts.
//!
//! The geometry of shapes (all the preset shapes, and freeforms) is in
//! xlsx_geom.rs.

use std::collections::HashMap;
use std::fmt::Write as _;

use base64::Engine as _;

use super::xlsx::Book;
use super::xlsx_chart::{Anchor, ChartParts, Node, anchor_of, color_of};
use super::xlsx_geom as geom;
use super::xlsx_print::{esc, font_family, n};

const EMU_PT: f64 = 12700.0;

/// One object of a sheet's drawing.
pub enum Drawn {
    /// The index-th chart of sheet.charts.
    Chart(usize),
    /// The HTML of the object, filling a box of the anchor's size.
    Html(String),
}

pub struct DrawObj {
    pub anchor: Anchor,
    pub drawn: Drawn,
}

/// What drawing a shape needs.
struct Ctx<'a> {
    parts: &'a ChartParts,
    rels: HashMap<String, (String, String)>,
    theme: HashMap<String, String>,
    font: (String, f64),
    ids: &'a std::cell::Cell<usize>,
}

/// The objects of a sheet's drawing in drawing order (later ones on top),
/// or None when the workbook has no drawing parts (a new workbook).
///
/// size gives the size (pt) of an anchor's box on the sheet.
pub fn sheet_drawing(
    b: &Book,
    sheet: &str,
    ids: &std::cell::Cell<usize>,
    size: &dyn Fn(&Anchor) -> Option<(f64, f64)>,
) -> Option<Vec<DrawObj>> {
    let parts = b.charts.as_ref()?;
    if parts.files.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    let Some(sheet_part) = parts.sheet_part(sheet) else {
        return Some(out);
    };
    let mut drawings: Vec<String> = parts
        .rels(&sheet_part)
        .into_values()
        .filter(|(t, _)| t.ends_with("/drawing"))
        .map(|(_, p)| p)
        .collect();
    drawings.sort();
    let theme = parts.theme();
    let mut chart = 0;
    for d in drawings {
        let Some(drawing) = parts.xml(&d) else {
            continue;
        };
        let ctx = Ctx {
            parts,
            rels: parts.rels(&d),
            theme: theme.clone(),
            font: b.default_font.clone(),
            ids,
        };
        for a in drawing
            .children
            .iter()
            .filter(|c| c.name.ends_with("Anchor"))
        {
            // Charts in the order of sheet_charts (xlsx_chart::each_chart).
            if let Some(frame) = a.find("graphicFrame")
                && let Some(data) = frame.find("graphicData")
                && let Some(id) = data
                    .children
                    .iter()
                    .find(|c| c.name == "chart")
                    .and_then(|c| c.attr("id"))
                && ctx.rels.contains_key(id)
            {
                out.push(DrawObj {
                    anchor: anchor_of(a),
                    drawn: Drawn::Chart(chart),
                });
                chart += 1;
                continue;
            }
            if a.child("clientData")
                .and_then(|c| c.attr("fPrintsWithSheet"))
                == Some("0")
            {
                continue;
            }
            let Some(obj) = a
                .children
                .iter()
                .find(|c| matches!(c.name.as_str(), "sp" | "cxnSp" | "grpSp" | "pic"))
                .or_else(|| {
                    // Shapes that newer Excel wraps in mc:AlternateContent.
                    a.find("Fallback").and_then(|f| {
                        f.children
                            .iter()
                            .find(|c| matches!(c.name.as_str(), "sp" | "cxnSp" | "grpSp" | "pic"))
                    })
                })
            else {
                continue;
            };
            let anchor = anchor_of(a);
            // The box comes from the anchor (the shape's xfrm says the same in EMU).
            let ext = size(&anchor)
                .or_else(|| xfrm(obj).map(|x| x.ext))
                .filter(|e| e.0 > 0.0 && e.1 > 0.0);
            let html = object_html(&ctx, obj, None, ext);
            if !html.is_empty() {
                out.push(DrawObj {
                    anchor,
                    drawn: Drawn::Html(html),
                });
            }
        }
    }
    Some(out)
}

/// The transform of a shape, picture or group (pt).
#[derive(Clone, Copy, Debug, Default)]
struct Xfrm {
    off: (f64, f64),
    ext: (f64, f64),
    ch_off: (f64, f64),
    ch_ext: (f64, f64),
    rot: f64,
    flip_h: bool,
    flip_v: bool,
}

fn xfrm(obj: &Node) -> Option<Xfrm> {
    let props = obj
        .children
        .iter()
        .find(|c| matches!(c.name.as_str(), "spPr" | "grpSpPr"))?;
    let x = props.child("xfrm")?;
    let pair = |n: &str, a: &str, b: &str| -> (f64, f64) {
        x.child(n)
            .map(|e| {
                let g =
                    |k: &str| e.attr(k).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0) / EMU_PT;
                (g(a), g(b))
            })
            .unwrap_or((0.0, 0.0))
    };
    let flag = |k: &str| matches!(x.attr(k), Some("1" | "true"));
    Some(Xfrm {
        off: pair("off", "x", "y"),
        ext: pair("ext", "cx", "cy"),
        ch_off: pair("chOff", "x", "y"),
        ch_ext: pair("chExt", "cx", "cy"),
        rot: x
            .attr("rot")
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0)
            / 60000.0,
        flip_h: flag("flipH"),
        flip_v: flag("flipV"),
    })
}

/// The HTML of a shape, connector, picture or group. Without place it
/// fills its container (a top-level object, sized by its anchor); in a
/// group, place is its box as fractions of the group's box. size is the
/// object's size in pt.
fn object_html(
    ctx: &Ctx,
    obj: &Node,
    place: Option<(f64, f64, f64, f64)>,
    size: Option<(f64, f64)>,
) -> String {
    let nv = obj
        .children
        .iter()
        .find(|c| c.name.starts_with("nv"))
        .and_then(|c| c.child("cNvPr"));
    if nv
        .and_then(|c| c.attr("hidden"))
        .is_some_and(|h| h == "1" || h == "true")
    {
        return String::new();
    }
    let x = xfrm(obj).unwrap_or_default();
    let (w, h) = size.unwrap_or(x.ext);
    let pct = |v: f64| n(v * 100.0);
    let mut style = match place {
        Some((l, t, pw, ph)) => format!(
            "left:{}%;top:{}%;width:{}%;height:{}%;",
            pct(l),
            pct(t),
            pct(pw),
            pct(ph)
        ),
        None => "left:0;top:0;width:100%;height:100%;".to_string(),
    };
    if x.rot != 0.0 {
        let _ = write!(style, "transform:rotate({}deg);", n(x.rot));
    }
    let inner = match obj.name.as_str() {
        "grpSp" => group_html(ctx, obj, &x, w, h),
        "pic" => picture_html(ctx, obj, &x),
        _ => shape_html(ctx, obj, &x, w, h),
    };
    if inner.is_empty() {
        return String::new();
    }
    format!("<div class=\"xljs-shape\" style=\"{style}\">{inner}</div>")
}

fn group_html(ctx: &Ctx, g: &Node, x: &Xfrm, w: f64, h: f64) -> String {
    // Children are in the group's child coordinates (chOff, chExt).
    let (cw, ch) = (
        if x.ch_ext.0 > 0.0 {
            x.ch_ext.0
        } else {
            x.ext.0.max(0.01)
        },
        if x.ch_ext.1 > 0.0 {
            x.ch_ext.1
        } else {
            x.ext.1.max(0.01)
        },
    );
    let mut out = String::new();
    for c in g
        .children
        .iter()
        .filter(|c| matches!(c.name.as_str(), "sp" | "cxnSp" | "grpSp" | "pic"))
    {
        let Some(cx) = xfrm(c) else { continue };
        let place = (
            (cx.off.0 - x.ch_off.0) / cw,
            (cx.off.1 - x.ch_off.1) / ch,
            cx.ext.0 / cw,
            cx.ext.1 / ch,
        );
        out.push_str(&object_html(
            ctx,
            c,
            Some(place),
            Some((cx.ext.0 * w / cw, cx.ext.1 * h / ch)),
        ));
    }
    if x.flip_h || x.flip_v {
        return format!(
            "<div style=\"position:absolute;inset:0;transform:scale({},{})\">{out}</div>",
            if x.flip_h { -1 } else { 1 },
            if x.flip_v { -1 } else { 1 }
        );
    }
    out
}

fn picture_html(ctx: &Ctx, pic: &Node, x: &Xfrm) -> String {
    // An SVG picture is a PNG fallback in a:blip (rendered small, at the
    // picture's size on screen) with the SVG itself in the blip's extension
    // (asvg:svgBlip); use the SVG so that it stays sharp in a PDF.
    let blip = pic.find("blip");
    let found = |b: Option<&Node>| {
        let id = b?.attr("embed")?;
        let (_, part) = ctx.rels.get(id)?;
        Some((part, ctx.parts.files.get(part)?))
    };
    let Some((part, data)) = found(blip.and_then(|b| b.find("svgBlip"))).or_else(|| found(blip))
    else {
        return String::new();
    };
    let lower = part.to_lowercase();
    let mime = match lower.rsplit('.').next().unwrap_or("") {
        "png" => "image/png",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "jpg" | "jpeg" => "image/jpeg",
        _ => return String::new(), // EMF/WMF: browsers cannot show them
    };
    let svg;
    let data = if mime == "image/svg+xml" {
        svg = svg_stretched(data);
        svg.as_slice()
    } else {
        data.as_slice()
    };
    let flip = if x.flip_h || x.flip_v {
        format!(
            "transform:scale({},{});",
            if x.flip_h { -1 } else { 1 },
            if x.flip_v { -1 } else { 1 }
        )
    } else {
        String::new()
    };
    format!(
        "<img alt=\"\" src=\"data:{mime};base64,{}\" style=\"position:absolute;inset:0;width:100%;height:100%;{flip}\">",
        base64::engine::general_purpose::STANDARD.encode(data)
    )
}

/// An SVG that fills the picture's box as Excel draws it: stretched
/// (preserveAspectRatio="none") and scaled (a viewBox from width and height
/// when it has none; without one a browser draws it at its own size).
fn svg_stretched(data: &[u8]) -> Vec<u8> {
    let s = String::from_utf8_lossy(data);
    let Some(start) = s
        .match_indices("<svg")
        .map(|(i, _)| i)
        .find(|&i| s[i + 4..].starts_with(|c: char| c.is_whitespace() || c == '>' || c == '/'))
    else {
        return data.to_vec();
    };
    let Some(len) = s[start..].find('>') else {
        return data.to_vec();
    };
    let mut end = start + len;
    if s[..end].ends_with('/') {
        end -= 1;
    }
    let tag = &s[start..end];
    let attr = |name: &str| {
        let i = tag.match_indices(name).map(|(i, _)| i).find(|&i| {
            tag[..i].ends_with(char::is_whitespace)
                && tag[i + name.len()..].trim_start().starts_with('=')
        })?;
        let rest = tag[i + name.len()..].trim_start()[1..].trim_start();
        let q = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        rest[1..].split(q).next()
    };
    let mut add = String::new();
    if attr("viewBox").is_none() {
        let size = |name| {
            attr(name)
                .and_then(|v| v.trim().trim_end_matches("px").trim().parse::<f64>().ok())
                .filter(|v| *v > 0.0)
        };
        if let (Some(w), Some(h)) = (size("width"), size("height")) {
            let _ = write!(add, " viewBox=\"0 0 {} {}\"", n(w), n(h));
        }
    }
    if attr("preserveAspectRatio").is_none() {
        add.push_str(" preserveAspectRatio=\"none\"");
    }
    format!("{}{add}{}", &s[..end], &s[end..]).into_bytes()
}

/// A fill or line color: "none", a color, or None (not said).
fn paint(props: Option<&Node>, theme: &HashMap<String, String>) -> Option<(String, f64)> {
    let p = props?;
    if p.child("noFill").is_some() {
        return Some(("none".into(), 1.0));
    }
    let fill = p
        .child("solidFill")
        .or_else(|| p.child("gradFill").and_then(|g| g.find("gs")))?;
    let color = color_of(fill, theme)?;
    let alpha = fill
        .children
        .first()
        .and_then(|c| c.child("alpha"))
        .and_then(|a| a.attr("val"))
        .and_then(|v| v.parse::<f64>().ok())
        .map(|v| v / 100000.0)
        .unwrap_or(1.0);
    Some((color, alpha))
}

/// The color of a style reference (lnRef, fillRef, fontRef) when its index is not 0.
fn style_ref(sp: &Node, name: &str, theme: &HashMap<String, String>) -> Option<(String, usize)> {
    let r = sp.child("style")?.child(name)?;
    let idx = r
        .attr("idx")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    let color = color_of(r, theme)?;
    if name != "fontRef" && idx == 0 {
        return None;
    }
    Some((color, idx))
}

/// An arrowhead of a line end (headEnd / tailEnd), as an SVG marker.
fn marker(end: Option<&Node>, color: &str, id: &str, width: f64) -> Option<String> {
    let e = end?;
    let kind = e.attr("type")?;
    let size = |k: &str| match e.attr(k) {
        Some("sm") => 2.0,
        Some("lg") => 5.0,
        _ => 3.0,
    };
    // As Excel: in proportion to the line, but not tiny on thin lines.
    let unit = width.max(2.0);
    let (mw, mh) = (size("w") * unit, size("len") * unit);
    let shape = match kind {
        "triangle" => "<path d=\"M0,0 L10,5 L0,10 Z\"/>".to_string(),
        "stealth" => "<path d=\"M0,0 L10,5 L0,10 L3,5 Z\"/>".to_string(),
        "arrow" => format!(
            "<path d=\"M0,0 L10,5 L0,10\" fill=\"none\" stroke=\"{color}\" stroke-width=\"1.5\"/>"
        ),
        "diamond" => "<path d=\"M0,5 L5,0 L10,5 L5,10 Z\"/>".to_string(),
        "oval" => "<circle cx=\"5\" cy=\"5\" r=\"5\"/>".to_string(),
        _ => return None,
    };
    let refx = if matches!(kind, "diamond" | "oval") {
        5
    } else {
        9
    };
    Some(format!(
        "<marker id=\"{id}\" viewBox=\"0 0 10 10\" refX=\"{refx}\" refY=\"5\" markerWidth=\"{}\" markerHeight=\"{}\" orient=\"auto-start-reverse\" markerUnits=\"userSpaceOnUse\" fill=\"{color}\" overflow=\"visible\">{shape}</marker>",
        n(mh),
        n(mw)
    ))
}

fn dash(ln: Option<&Node>, width: f64) -> String {
    let Some(d) = ln
        .and_then(|l| l.child("prstDash"))
        .and_then(|p| p.attr("val"))
    else {
        return String::new();
    };
    let k = width.max(0.5);
    let pattern: &[f64] = match d {
        "dash" => &[4.0, 3.0],
        "sysDash" => &[3.0, 1.0],
        "dot" | "sysDot" => &[1.0, 1.0],
        "lgDash" => &[8.0, 3.0],
        "dashDot" | "sysDashDot" => &[4.0, 3.0, 1.0, 3.0],
        "lgDashDot" => &[8.0, 3.0, 1.0, 3.0],
        "lgDashDotDot" | "sysDashDotDot" => &[8.0, 3.0, 1.0, 3.0, 1.0, 3.0],
        _ => return String::new(),
    };
    let v: Vec<String> = pattern.iter().map(|x| n(x * k)).collect();
    format!(" stroke-dasharray=\"{}\"", v.join(" "))
}

fn shape_html(ctx: &Ctx, sp: &Node, x: &Xfrm, w: f64, h: f64) -> String {
    let props = sp.child("spPr");
    let geom = if let Some(g) = props.and_then(|p| p.child("prstGeom")) {
        // Presets that are not defined are drawn as their rectangle.
        geom::preset(g.attr("prst").unwrap_or("rect"), Some(g), w, h)
            .or_else(|| geom::preset("rect", None, w, h))
    } else if let Some(g) = props.and_then(|p| p.child("custGeom")) {
        Some(geom::custom(g, w, h))
    } else if sp.name == "cxnSp" {
        geom::preset("line", None, w, h)
    } else {
        None
    };
    let paths = geom
        .as_ref()
        .map(|g| g.paths.as_slice())
        .unwrap_or_default();
    let ln = props.and_then(|p| p.child("ln"));
    // Fill: the shape's own, else the theme's by its style.
    let fill = paint(props, &ctx.theme)
        .or_else(|| style_ref(sp, "fillRef", &ctx.theme).map(|(c, _)| (c, 1.0)))
        .unwrap_or(("none".into(), 1.0));
    let stroke = paint(ln, &ctx.theme)
        .or_else(|| style_ref(sp, "lnRef", &ctx.theme).map(|(c, _)| (c, 1.0)))
        .unwrap_or(("none".into(), 1.0));
    let width = ln
        .and_then(|l| l.attr("w"))
        .and_then(|v| v.parse::<f64>().ok())
        .map(|v| v / EMU_PT)
        .unwrap_or_else(
            || match style_ref(sp, "lnRef", &ctx.theme).map(|(_, i)| i) {
                Some(1) => 0.5,
                Some(3) => 1.5,
                Some(_) => 1.0,
                None => 0.75,
            },
        );
    let mut body = String::new();
    let mut defs = String::new();
    let mut ends = String::new();
    let op = |name: &str, a: f64| {
        if a < 1.0 {
            format!(" {name}-opacity=\"{}\"", n(a))
        } else {
            String::new()
        }
    };
    for p in paths {
        let f = if p.fill == geom::Fill::None || fill.0 == "none" {
            "none".to_string()
        } else {
            geom::shade(&fill.0, p.fill)
        };
        let s = if p.stroke { stroke.0.as_str() } else { "none" };
        if p.d.is_empty() || (f == "none" && s == "none") {
            continue;
        }
        let _ = write!(body, "<path d=\"{}\" fill=\"{f}\"", p.d);
        if f != "none" {
            body.push_str(&op("fill", fill.1));
        }
        if s != "none" {
            let _ = write!(
                body,
                " stroke=\"{s}\" stroke-width=\"{}\"{}{} stroke-linejoin=\"round\"",
                n(width),
                op("stroke", stroke.1),
                dash(ln, width)
            );
            if p.open {
                if ends.is_empty() {
                    let id = ctx.ids.get();
                    ctx.ids.set(id + 1);
                    for (k, attr) in [("headEnd", "marker-start"), ("tailEnd", "marker-end")] {
                        let mid = format!("xm{id}{k}");
                        if let Some(m) = marker(ln.and_then(|l| l.child(k)), s, &mid, width) {
                            defs.push_str(&m);
                            let _ = write!(ends, " {attr}=\"url(#{mid})\"");
                        }
                    }
                    if ends.is_empty() {
                        ends.push(' ');
                    }
                }
                body.push_str(ends.trim_end());
            }
        } else {
            body.push_str(" stroke=\"none\"");
        }
        body.push_str("/>");
    }
    let mut svg = String::new();
    if !body.is_empty() {
        let flip = if x.flip_h || x.flip_v {
            format!(
                " transform=\"translate({},{}) scale({},{})\"",
                if x.flip_h { n(w) } else { "0".into() },
                if x.flip_v { n(h) } else { "0".into() },
                if x.flip_h { -1 } else { 1 },
                if x.flip_v { -1 } else { 1 }
            )
        } else {
            String::new()
        };
        let _ = write!(
            svg,
            "<svg viewBox=\"0 0 {w} {h}\" preserveAspectRatio=\"none\" style=\"position:absolute;inset:0;width:100%;height:100%;overflow:visible\">{}<g{flip}>{body}</g></svg>",
            if defs.is_empty() {
                String::new()
            } else {
                format!("<defs>{defs}</defs>")
            },
            w = n(w.max(0.01)),
            h = n(h.max(0.01)),
        );
    }
    // The text box of the geometry, mirrored with the shape.
    let rect = geom.as_ref().and_then(|g| g.text).map(|(l, t, r, b)| {
        let (l, r) = if x.flip_h { (w - r, w - l) } else { (l, r) };
        let (t, b) = if x.flip_v { (h - b, h - t) } else { (t, b) };
        (l.max(0.0), t.max(0.0), (w - r).max(0.0), (h - b).max(0.0))
    });
    let text = sp
        .child("txBody")
        .map(|t| text_html(ctx, sp, t, rect.unwrap_or_default()))
        .unwrap_or_default();
    format!("{svg}{text}")
}

/// The text of a shape (txBody).
/// box is the text box's distance from the shape's edges (left, top, right, bottom).
fn text_html(ctx: &Ctx, sp: &Node, tx: &Node, bx: (f64, f64, f64, f64)) -> String {
    let body = tx.child("bodyPr");
    let ins = |k: &str, d: f64| {
        body.and_then(|b| b.attr(k))
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(d)
            / EMU_PT
    };
    let (l, t, r, b) = (
        bx.0 + ins("lIns", 91440.0),
        bx.1 + ins("tIns", 45720.0),
        bx.2 + ins("rIns", 91440.0),
        bx.3 + ins("bIns", 45720.0),
    );
    let anchor = match body.and_then(|b| b.attr("anchor")) {
        Some("ctr") => "center",
        Some("b") => "flex-end",
        _ => "flex-start",
    };
    let vert = matches!(
        body.and_then(|b| b.attr("vert")),
        Some("vert" | "eaVert" | "wordArtVert" | "mongolianVert")
    );
    let nowrap = body.and_then(|b| b.attr("wrap")) == Some("none");
    let base_color = style_ref(sp, "fontRef", &ctx.theme)
        .map(|(c, _)| c)
        .unwrap_or_else(|| "#000000".into());
    // Defaults from the list style (lvl1pPr/defRPr).
    let lst = tx.child("lstStyle").and_then(|l| l.child("lvl1pPr"));
    let mut paras = String::new();
    let mut any = false;
    for p in tx.all("p") {
        let ppr = p.child("pPr");
        let algn = ppr
            .and_then(|x| x.attr("algn"))
            .or_else(|| lst.and_then(|x| x.attr("algn")));
        let align = match algn {
            Some("ctr") => "center",
            Some("r") => "right",
            Some("just") | Some("dist") => "justify",
            _ => "left",
        };
        let def = ppr
            .and_then(|x| x.child("defRPr"))
            .or_else(|| lst.and_then(|x| x.child("defRPr")));
        let mut runs = String::new();
        if let Some(bu) = ppr
            .and_then(|x| x.child("buChar"))
            .and_then(|c| c.attr("char"))
        {
            let _ = write!(runs, "{}&nbsp;", esc(bu));
        }
        let mut size_last = def
            .and_then(|d| d.attr("sz"))
            .and_then(|v| v.parse::<f64>().ok())
            .map(|v| v / 100.0);
        for c in &p.children {
            match c.name.as_str() {
                "r" | "fld" => {
                    let s = c.child("t").map(|t| t.text.clone()).unwrap_or_default();
                    if s.is_empty() {
                        continue;
                    }
                    any = true;
                    let rpr = c.child("rPr");
                    let css = run_css(ctx, rpr, def, &base_color);
                    if let Some(sz) = rpr
                        .and_then(|x| x.attr("sz"))
                        .and_then(|v| v.parse::<f64>().ok())
                    {
                        size_last = Some(sz / 100.0);
                    }
                    let _ = write!(runs, "<span style=\"{css}\">{}</span>", esc(&s));
                }
                "br" => runs.push_str("<br>"),
                _ => {}
            }
        }
        if runs.is_empty() {
            // An empty paragraph keeps its line.
            let sz = p
                .child("endParaRPr")
                .and_then(|x| x.attr("sz"))
                .and_then(|v| v.parse::<f64>().ok())
                .map(|v| v / 100.0)
                .or(size_last)
                .unwrap_or(11.0);
            runs = format!("<span style=\"font-size:{}pt\">&nbsp;</span>", n(sz));
        }
        let _ = write!(paras, "<p style=\"margin:0;text-align:{align}\">{runs}</p>");
    }
    if !any {
        return String::new();
    }
    let mut style = format!(
        "position:absolute;inset:0;box-sizing:border-box;padding:{}pt {}pt {}pt {}pt;display:flex;flex-direction:column;justify-content:{anchor};overflow:visible;line-height:1.2;font-family:{};font-size:11pt;color:{base_color};",
        n(t),
        n(r),
        n(b),
        n(l),
        esc(&font_family(&ctx.font.0))
    );
    style.push_str(if nowrap {
        "white-space:pre;"
    } else {
        "white-space:pre-wrap;overflow-wrap:anywhere;"
    });
    if vert {
        style.push_str("writing-mode:vertical-rl;");
    }
    format!("<div style=\"{style}\">{paras}</div>")
}

fn run_css(ctx: &Ctx, rpr: Option<&Node>, def: Option<&Node>, base_color: &str) -> String {
    let get = |k: &str| {
        rpr.and_then(|x| x.attr(k))
            .or_else(|| def.and_then(|x| x.attr(k)))
    };
    let mut css = String::new();
    if let Some(sz) = get("sz").and_then(|v| v.parse::<f64>().ok()) {
        let _ = write!(css, "font-size:{}pt;", n(sz / 100.0));
    }
    if matches!(get("b"), Some("1" | "true")) {
        css.push_str("font-weight:bold;");
    }
    if matches!(get("i"), Some("1" | "true")) {
        css.push_str("font-style:italic;");
    }
    let mut deco = Vec::new();
    if get("u").is_some_and(|u| u != "none") {
        deco.push("underline");
    }
    if get("strike").is_some_and(|s| s != "noStrike") {
        deco.push("line-through");
    }
    if !deco.is_empty() {
        let _ = write!(css, "text-decoration:{};", deco.join(" "));
    }
    let face = |n0: Option<&Node>| -> Option<String> {
        let n0 = n0?;
        for k in ["ea", "latin"] {
            if let Some(f) = n0.child(k).and_then(|c| c.attr("typeface"))
                && !f.is_empty()
                && !f.starts_with('+')
            {
                return Some(f.to_string());
            }
        }
        None
    };
    if let Some(f) = face(rpr).or_else(|| face(def)) {
        let _ = write!(css, "font-family:{};", esc(&font_family(&f)));
    }
    let color = rpr
        .and_then(|x| x.child("solidFill"))
        .or_else(|| def.and_then(|x| x.child("solidFill")))
        .and_then(|f| color_of(f, &ctx.theme));
    if let Some(c) = color
        && c != base_color
    {
        let _ = write!(css, "color:{c};");
    }
    css
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An SVG picture is drawn from the SVG, not from its PNG fallback.
    #[test]
    fn svg_picture() {
        let svg = br#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" width="20px" height="10"><rect width="20" height="10"/></svg>"#;
        let files = HashMap::from([
            ("xl/media/image1.png".to_string(), b"PNG".to_vec()),
            ("xl/media/image2.svg".to_string(), svg.to_vec()),
        ]);
        let parts = ChartParts { files };
        let ids = std::cell::Cell::new(0);
        let rels = HashMap::from([
            (
                "rId1".to_string(),
                ("image".to_string(), "xl/media/image1.png".to_string()),
            ),
            (
                "rId2".to_string(),
                ("image".to_string(), "xl/media/image2.svg".to_string()),
            ),
        ]);
        let ctx = Ctx {
            parts: &parts,
            rels,
            theme: HashMap::new(),
            font: ("Calibri".into(), 11.0),
            ids: &ids,
        };
        let pic = super::super::xlsx_chart::parse(
            br#"<xdr:pic xmlns:xdr="x" xmlns:a="a" xmlns:r="r" xmlns:asvg="s"><xdr:blipFill><a:blip r:embed="rId1"><a:extLst><a:ext uri="{96DAC541-7B7A-43D3-8B79-37D633B846F1}"><asvg:svgBlip r:embed="rId2"/></a:ext></a:extLst></a:blip></xdr:blipFill></xdr:pic>"#,
        )
        .unwrap();
        let html = picture_html(&ctx, &pic, &Xfrm::default());
        let b64 = html
            .split("base64,")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap();
        let out = String::from_utf8(
            base64::engine::general_purpose::STANDARD
                .decode(b64)
                .unwrap(),
        )
        .unwrap();
        assert!(html.contains("data:image/svg+xml;"), "{html}");
        assert!(
            out.contains(r#"height="10" viewBox="0 0 20 10" preserveAspectRatio="none"><rect"#),
            "{out}"
        );
        // Without the extension: the PNG.
        let pic = super::super::xlsx_chart::parse(
            br#"<xdr:pic xmlns:xdr="x" xmlns:a="a" xmlns:r="r"><a:blip r:embed="rId1"/></xdr:pic>"#,
        )
        .unwrap();
        assert!(picture_html(&ctx, &pic, &Xfrm::default()).contains("data:image/png;"));
        // An SVG that says how to fit is kept as it is.
        let keep = br#"<svg viewBox="0 0 1 1" preserveAspectRatio="xMidYMid"/>"#;
        assert_eq!(svg_stretched(keep), keep.to_vec());
    }
}
