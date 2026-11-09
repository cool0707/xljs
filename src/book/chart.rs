//! Descriptions of the charts of a sheet (and of chart sheets): kind,
//! title, series with the ranges they show, axes and legend, read from the
//! chart parts (c:chartSpace, and cx:chartSpace of the newer kinds).

use super::code;
use super::xml::Node;
use crate::t;

/// A chart as read: where it is and its part.
#[derive(Debug, Clone)]
pub(crate) struct Chart {
    /// The name of its frame ("グラフ 1").
    pub name: String,
    /// The cells it covers ("L2:S16"), or where it starts ("L2"), with
    /// whether its end is known; empty on a chart sheet.
    pub range: String,
    pub sized: bool,
    pub root: Node,
    /// A chart of the newer kinds (chartEx: waterfall, treemap, ...).
    pub ex: bool,
}

/// The rich text or the text of a title or a series name (c:tx), quoted,
/// or the reference it is taken from (after a space).
fn tx_text(tx: &Node) -> String {
    let mut rich = Vec::new();
    tx.find("rich", &mut rich);
    if let Some(r) = rich.first() {
        let mut ps = Vec::new();
        r.find("p", &mut ps);
        let lines: Vec<String> = ps
            .iter()
            .map(|p| {
                let mut ts = Vec::new();
                p.find("t", &mut ts);
                ts.iter().map(|t| t.all_text()).collect::<String>()
            })
            .filter(|s| !s.is_empty())
            .collect();
        return t!("book.quote", lines.join(" "));
    }
    let mut fs = Vec::new();
    tx.find("f", &mut fs);
    let mut vs = Vec::new();
    tx.find("v", &mut vs);
    let cached = vs.first().map(|v| v.all_text()).filter(|s| !s.is_empty());
    match (fs.first().map(|f| f.all_text()), cached) {
        (Some(f), Some(v)) => format!("{}（{}）", t!("book.quote", v), code(&format!("={f}"))),
        (Some(f), None) => format!(" {}", code(&format!("={f}"))),
        (None, Some(v)) => t!("book.quote", v),
        (None, None) => String::new(),
    }
}

/// The data of a series (c:cat, c:val, ...): the reference, or the values
/// written in the chart.
fn data(n: &Node) -> String {
    let mut fs = Vec::new();
    n.find("f", &mut fs);
    if let Some(f) = fs.first() {
        return code(&format!("={}", f.all_text().trim()));
    }
    let mut vs = Vec::new();
    n.find("v", &mut vs);
    let vals: Vec<String> = vs.iter().map(|v| v.all_text()).collect();
    format!("{{{}}}", vals.join(", "))
}

fn val<'a>(n: &'a Node, name: &str) -> Option<&'a str> {
    n.child(name).and_then(|c| c.attr("val"))
}

/// The kind of a chart group (barChart, lineChart, ...) as Excel names it.
fn kind(g: &Node) -> String {
    let three = g.name.contains("3D");
    let grouping = val(g, "grouping").unwrap_or("standard");
    let stacked = |base: &str| match grouping {
        "stacked" => t!("book.chart.stacked", base),
        "percentStacked" => t!("book.chart.percentStacked", base),
        _ => base.to_string(),
    };
    let s = match g.name.trim_end_matches("3DChart").trim_end_matches("Chart") {
        "bar" => {
            let base = if val(g, "barDir") == Some("bar") {
                t!("book.chart.bar")
            } else {
                t!("book.chart.column")
            };
            match grouping {
                "stacked" | "percentStacked" => stacked(&base),
                _ => t!("book.chart.clustered", base),
            }
        }
        "line" => stacked(&t!("book.chart.line")),
        "area" => stacked(&t!("book.chart.area")),
        "pie" => t!("book.chart.pie"),
        "ofPie" => {
            if val(g, "ofPieType") == Some("bar") {
                t!("book.chart.barOfPie")
            } else {
                t!("book.chart.pieOfPie")
            }
        }
        "doughnut" => t!("book.chart.doughnut"),
        "scatter" => t!("book.chart.scatter"),
        "bubble" => t!("book.chart.bubble"),
        "radar" => {
            if val(g, "radarStyle") == Some("filled") {
                t!("book.chart.radarFilled")
            } else {
                t!("book.chart.radar")
            }
        }
        "stock" => t!("book.chart.stock"),
        "surface" => t!("book.chart.surface"),
        other => other.to_string(),
    };
    if three { format!("3-D {s}") } else { s }
}

/// The newer kinds (cx:series layoutId).
fn ex_kind(layout: &str) -> String {
    match layout {
        "waterfall" | "funnel" | "treemap" | "sunburst" | "boxWhisker" | "paretoLine"
        | "regionMap" => t!(&format!("book.chart.{layout}")),
        "clusteredColumn" => t!("book.chart.histogram"),
        other => other.to_string(),
    }
}

const GROUPS: &[&str] = &[
    "barChart",
    "bar3DChart",
    "lineChart",
    "line3DChart",
    "pieChart",
    "pie3DChart",
    "ofPieChart",
    "doughnutChart",
    "areaChart",
    "area3DChart",
    "scatterChart",
    "bubbleChart",
    "radarChart",
    "stockChart",
    "surfaceChart",
    "surface3DChart",
];

/// What the data labels show (c:dLbls), "" for none.
fn labels(n: &Node) -> String {
    let Some(d) = n.child("dLbls") else {
        return String::new();
    };
    let on = |k: &str| val(d, k).is_some_and(|v| v == "1" || v == "true");
    let parts: Vec<String> = [
        ("showVal", "book.chart.lblVal"),
        ("showPercent", "book.chart.lblPercent"),
        ("showCatName", "book.chart.lblCat"),
        ("showSerName", "book.chart.lblSer"),
    ]
    .into_iter()
    .filter(|(k, _)| on(k))
    .map(|(_, key)| t!(key))
    .collect();
    if parts.is_empty() {
        String::new()
    } else {
        t!("book.chart.labels", parts.join(&t!("vba.separator")))
    }
}

/// The axis with the id.
fn axis<'a>(plot: &'a Node, id: &str) -> Option<&'a Node> {
    plot.children.iter().find(|a| {
        matches!(a.name.as_str(), "catAx" | "valAx" | "dateAx" | "serAx")
            && val(a, "axId") == Some(id)
    })
}

/// The lines describing a chart (without its heading).
pub(crate) fn describe(c: &Chart) -> Vec<String> {
    if c.ex {
        return describe_ex(&c.root);
    }
    let sep = t!("vba.separator");
    let mut out = Vec::new();
    let Some(chart) = c.root.child("chart") else {
        return out;
    };
    let Some(plot) = chart.child("plotArea") else {
        return out;
    };
    let groups: Vec<&Node> = plot
        .children
        .iter()
        .filter(|g| GROUPS.contains(&g.name.as_str()))
        .collect();
    let kinds: Vec<String> = groups.iter().map(|g| kind(g)).collect();
    let mut uniq: Vec<String> = Vec::new();
    for k in &kinds {
        if !uniq.contains(k) {
            uniq.push(k.clone());
        }
    }
    out.push(t!("book.chart.kind", uniq.join(&sep)));
    if let Some(p) = c.root.child("pivotSource").and_then(|p| p.child("name")) {
        out.push(t!("book.chart.pivot", p.all_text()));
    }
    if let Some(title) = chart.child("title")
        && !chart
            .child("autoTitleDeleted")
            .is_some_and(|d| d.flag("val", true))
    {
        let text = title.child("tx").map(tx_text).unwrap_or_default();
        out.push(t!(
            "book.chart.title",
            if text.is_empty() {
                t!("book.chart.autoTitle")
            } else {
                text
            }
        ));
    }
    // The axes of the first group are the primary ones.
    let primary: Vec<&str> = groups
        .first()
        .map(|g| g.children("axId").filter_map(|a| a.attr("val")).collect())
        .unwrap_or_default();
    let mut axis_names: Vec<(String, String)> = Vec::new();
    for g in &groups {
        let horizontal_bars = val(g, "barDir") == Some("bar");
        for (i, id) in g.children("axId").filter_map(|a| a.attr("val")).enumerate() {
            if axis_names.iter().any(|(x, _)| x == id) {
                continue;
            }
            let value_axis = axis(plot, id).is_some_and(|a| a.name == "valAx");
            let role = if value_axis {
                t!("book.chart.valueRole")
            } else {
                t!("book.chart.catRole")
            };
            let name = match (i, horizontal_bars) {
                (0, false) | (1, true) => t!("book.chart.hAxis", role),
                (1, false) | (0, true) => t!("book.chart.vAxis", role),
                _ => t!("book.chart.depthAxis"),
            };
            let name = if primary.contains(&id) {
                name
            } else {
                t!("book.chart.secondary", name)
            };
            axis_names.push((id.to_string(), name));
        }
    }
    for (gi, g) in groups.iter().enumerate() {
        let secondary = g
            .children("axId")
            .any(|a| a.attr("val").is_some_and(|v| !primary.contains(&v)));
        let group_labels = labels(g);
        for s in g.children("ser") {
            let name = s
                .child("tx")
                .map(tx_text)
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| t!("book.chart.noName"));
            let mut notes = Vec::new();
            if uniq.len() > 1 {
                notes.push(kinds[gi].clone());
            }
            if secondary {
                notes.push(t!("book.chart.onSecondary"));
            }
            let notes = if notes.is_empty() {
                String::new()
            } else {
                format!("（{}）", notes.join(&sep))
            };
            let mut parts = Vec::new();
            for (k, key) in [
                ("cat", "book.chart.cat"),
                ("val", "book.chart.val"),
                ("xVal", "book.chart.x"),
                ("yVal", "book.chart.y"),
                ("bubbleSize", "book.chart.size"),
            ] {
                if let Some(d) = s.child(k) {
                    parts.push(format!("{} {}", t!(key), data(d)));
                }
            }
            let l = labels(s);
            if !l.is_empty() || !group_labels.is_empty() {
                parts.push(if l.is_empty() {
                    group_labels.clone()
                } else {
                    l
                });
            }
            for tl in s.children("trendline") {
                let ty = val(tl, "trendlineType").unwrap_or("linear");
                parts.push(t!("book.chart.trendline", t!(&format!("book.trend.{ty}"))));
            }
            out.push(t!("book.chart.series", name, notes, parts.join(&sep)));
        }
    }
    for (id, name) in &axis_names {
        let Some(a) = axis(plot, id) else { continue };
        let mut parts = Vec::new();
        if a.child("delete").is_some_and(|d| d.flag("val", true)) {
            parts.push(t!("book.hidden"));
        }
        if let Some(tt) = a.child("title") {
            let text = tt.child("tx").map(tx_text).unwrap_or_default();
            parts.push(t!(
                "book.chart.title",
                if text.is_empty() {
                    t!("book.chart.autoTitle")
                } else {
                    text
                }
            ));
        }
        if let Some(sc) = a.child("scaling") {
            if let Some(v) = val(sc, "min") {
                parts.push(t!("book.chart.min", v));
            }
            if let Some(v) = val(sc, "max") {
                parts.push(t!("book.chart.max", v));
            }
            if let Some(v) = val(sc, "logBase") {
                parts.push(t!("book.chart.log", v));
            }
            if val(sc, "orientation") == Some("maxMin") {
                parts.push(t!("book.chart.reversed"));
            }
        }
        if let Some(v) = val(a, "majorUnit") {
            parts.push(t!("book.chart.unit", v));
        }
        if let Some(nf) = a.child("numFmt").filter(|n| !n.flag("sourceLinked", false))
            && let Some(f) = nf.attr("formatCode").filter(|f| *f != "General")
        {
            parts.push(t!("book.fmt.numFmt", code(f)));
        }
        if !parts.is_empty() {
            out.push(format!("{name}: {}", parts.join(&sep)));
        }
    }
    match chart.child("legend") {
        Some(l) => {
            let pos = match val(l, "legendPos").unwrap_or("r") {
                "b" => t!("book.chart.bottom"),
                "t" => t!("book.chart.top"),
                "l" => t!("book.chart.left"),
                "tr" => t!("book.chart.topRight"),
                _ => t!("book.chart.right"),
            };
            out.push(t!("book.chart.legend", pos));
        }
        None => out.push(t!("book.chart.noLegend")),
    }
    if plot.child("dTable").is_some() {
        out.push(t!("book.chart.dataTable"));
    }
    out
}

/// The lines of a chart of the newer kinds (cx:chartSpace).
fn describe_ex(root: &Node) -> Vec<String> {
    let sep = t!("vba.separator");
    let mut out = Vec::new();
    let mut series = Vec::new();
    root.find("series", &mut series);
    let mut kinds: Vec<String> = Vec::new();
    for s in &series {
        let k = ex_kind(s.attr("layoutId").unwrap_or(""));
        if !kinds.contains(&k) {
            kinds.push(k);
        }
    }
    out.push(t!("book.chart.kind", kinds.join(&sep)));
    let mut titles = Vec::new();
    if let Some(chart) = root.child("chart") {
        chart.find("title", &mut titles);
    }
    if let Some(title) = titles.first() {
        let text = title.child("tx").map(tx_text).unwrap_or_default();
        out.push(t!(
            "book.chart.title",
            if text.is_empty() {
                t!("book.chart.autoTitle")
            } else {
                text
            }
        ));
    }
    let mut datas = Vec::new();
    root.find("data", &mut datas);
    for s in &series {
        let name = s
            .child("tx")
            .map(tx_text)
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| t!("book.chart.noName"));
        let id = s.child("dataId").and_then(|d| d.attr("val")).unwrap_or("0");
        let mut parts = Vec::new();
        if let Some(d) = datas.iter().find(|d| d.attr("id") == Some(id)) {
            for dim in d
                .children
                .iter()
                .filter(|c| c.name == "numDim" || c.name == "strDim")
            {
                let key = match dim.attr("type").unwrap_or("") {
                    "cat" => "book.chart.cat",
                    "x" => "book.chart.x",
                    "y" => "book.chart.y",
                    "size" => "book.chart.size",
                    _ => "book.chart.val",
                };
                parts.push(format!("{} {}", t!(key), data(dim)));
            }
        }
        out.push(t!("book.chart.series", name, "", parts.join(&sep)));
    }
    out
}
