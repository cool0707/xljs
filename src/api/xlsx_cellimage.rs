//! Pictures placed in cells (Excel's "Place in Cell"). Excel keeps them as
//! rich values, which umya-spreadsheet does not read: the cell is an error
//! (`t="e"`, `#VALUE!`) with a value metadata index (`vm`), metadata.xml
//! maps it to a rich value (xl/richData/rdrichvalue.xml), whose structure
//! (rdrichvaluestructure.xml) names the key `_rvRel:LocalImageIdentifier`,
//! an index into richValueRel.xml, whose relationship is the picture in
//! xl/media. Formulas that refer to such a cell (`=B2`) show the picture
//! too; Excel saves them with a `vm` of their own.

use std::collections::HashMap;
use std::io::Cursor;

use super::xlsx_chart::{Node, parse, resolve};

/// A picture of the workbook placed in cells.
#[derive(Debug, Clone, PartialEq)]
pub struct CellImage {
    /// The media type, None when a browser cannot show it (EMF/WMF) or the
    /// picture is not in the file (IMAGE of a web address).
    pub mime: Option<&'static str>,
    pub data: Vec<u8>,
    /// The alternative text.
    pub alt: String,
}

/// The pictures in the cells of a workbook, as read when it was opened.
#[derive(Debug, Clone, Default)]
pub struct CellImages {
    pub images: Vec<CellImage>,
    /// Sheet name -> (column, row) -> index in images.
    pub cells: HashMap<String, HashMap<(u32, u32), usize>>,
}

impl CellImages {
    /// The picture placed in a cell when the file was opened.
    pub fn at(&self, sheet: &str, col: u32, row: u32) -> Option<usize> {
        self.cells.get(sheet)?.get(&(col, row)).copied()
    }

    pub fn collect(data: &[u8]) -> CellImages {
        let mut out = CellImages::default();
        let Ok(mut zr) = zip::ZipArchive::new(Cursor::new(data)) else { return out };
        if !zr.file_names().any(|n| n.starts_with("xl/richData/")) {
            return out;
        }
        let mut read = |name: &str| crate::zippart::read_any_case(&mut zr, name);
        let xml = |b: Option<Vec<u8>>| b.and_then(|b| parse(&b));
        // The parts by the type of their relationship from the workbook.
        let wb_rels = rels(xml(read("xl/_rels/workbook.xml.rels")).as_ref(), "xl");
        let part = |suffix: &str, default: &str| {
            wb_rels.values().find(|(t, _)| t.rsplit('/').next() == Some(suffix)).map(|(_, p)| p.clone()).unwrap_or_else(|| default.to_string())
        };
        let (meta_part, rv_part, st_part, rel_part) = (
            part("sheetMetadata", "xl/metadata.xml"),
            part("rdRichValue", "xl/richData/rdrichvalue.xml"),
            part("rdRichValueStructure", "xl/richData/rdrichvaluestructure.xml"),
            part("richValueRel", "xl/richData/richValueRel.xml"),
        );
        let (Some(meta), Some(rv), Some(st)) = (xml(read(&meta_part)), xml(read(&rv_part)), xml(read(&st_part))) else { return out };
        // richValueRel.xml: the pictures by position.
        let rel_dir = rel_part.rsplit_once('/').map(|x| x.0).unwrap_or("");
        let rel_file = rel_part.rsplit('/').next().unwrap_or("");
        let pic_rels = rels(xml(read(&format!("{rel_dir}/_rels/{rel_file}.rels"))).as_ref(), rel_dir);
        let pictures: Vec<Option<String>> = xml(read(&rel_part))
            .map(|r| r.all("rel").map(|x| x.attr("id").and_then(|id| pic_rels.get(id)).map(|(_, p)| p.clone())).collect())
            .unwrap_or_default();
        // The structures' types and key names.
        let structs: Vec<(String, Vec<String>)> = st.all("s").map(|s| (s.attr("t").unwrap_or("").to_string(), s.all("k").map(|k| k.attr("n").unwrap_or("").to_string()).collect())).collect();
        // Rich value -> picture (index in out.images).
        let mut by_part: HashMap<String, usize> = HashMap::new();
        let mut rich: Vec<Option<usize>> = Vec::new();
        for v in rv.all("rv") {
            let Some((kind, keys)) = v.attr("s").and_then(|s| s.parse::<usize>().ok()).and_then(|s| structs.get(s)) else {
                rich.push(None);
                continue;
            };
            let vals: Vec<&str> = v.all("v").map(|x| x.text.as_str()).collect();
            let get = |key: &str| keys.iter().position(|k| k == key).and_then(|i| vals.get(i).copied());
            let alt = get("Text").unwrap_or("").to_string();
            let img = match get("_rvRel:LocalImageIdentifier").and_then(|i| i.trim().parse::<usize>().ok()) {
                Some(i) => {
                    let Some(p) = pictures.get(i).cloned().flatten() else {
                        rich.push(None);
                        continue;
                    };
                    match by_part.get(&p) {
                        Some(&k) => Some(k),
                        None => {
                            let data = read(&p).unwrap_or_default();
                            let mime = if data.is_empty() { None } else { image_mime(&p) };
                            out.images.push(CellImage { mime, data, alt });
                            by_part.insert(p, out.images.len() - 1);
                            Some(out.images.len() - 1)
                        }
                    }
                }
                // IMAGE("https://..."): the picture is not in the file.
                None if kind == "_webImage" => {
                    out.images.push(CellImage { mime: None, data: Vec::new(), alt });
                    Some(out.images.len() - 1)
                }
                None => None,
            };
            rich.push(img);
        }
        // metadata.xml: vm (1-based) -> valueMetadata bk -> rc t (1-based
        // metadataType) v (0-based) -> futureMetadata XLRICHVALUE bk -> rvb i.
        let types: Vec<&str> = meta.child("metadataTypes").map(|t| t.all("metadataType").map(|x| x.attr("name").unwrap_or("")).collect()).unwrap_or_default();
        let Some(rv_type) = types.iter().position(|t| *t == "XLRICHVALUE").map(|i| i + 1) else { return out };
        let future: Vec<Option<usize>> = meta
            .all("futureMetadata")
            .find(|f| f.attr("name") == Some("XLRICHVALUE"))
            .map(|f| f.all("bk").map(|bk| bk.find("rvb").and_then(|r| r.attr("i")).and_then(|i| i.parse().ok())).collect())
            .unwrap_or_default();
        let vm: Vec<Option<usize>> = meta
            .child("valueMetadata")
            .map(|m| {
                m.all("bk")
                    .map(|bk| {
                        let rc = bk.child("rc")?;
                        if rc.attr("t")?.parse::<usize>().ok()? != rv_type {
                            return None;
                        }
                        let f = future.get(rc.attr("v")?.parse::<usize>().ok()?).copied().flatten()?;
                        rich.get(f).copied().flatten()
                    })
                    .collect()
            })
            .unwrap_or_default();
        if vm.iter().all(|v| v.is_none()) {
            return out;
        }
        super::xlsx_print::for_each_sheet(data, |_, name, sheet| {
            let cells = vm_cells(sheet, &vm);
            if !cells.is_empty() {
                out.cells.insert(name.to_string(), cells);
            }
        });
        out
    }
}

/// The relationships of a .rels part: id -> (type, part).
fn rels(root: Option<&Node>, dir: &str) -> HashMap<String, (String, String)> {
    let mut out = HashMap::new();
    let Some(root) = root else { return out };
    for r in root.all("Relationship") {
        let (Some(id), Some(target)) = (r.attr("Id"), r.attr("Target")) else { continue };
        if r.attr("TargetMode") == Some("External") {
            continue;
        }
        out.insert(id.to_string(), (r.attr("Type").unwrap_or("").to_string(), resolve(dir, target)));
    }
    out
}

/// The type of a picture by the extension of its name; None for those
/// browsers cannot show (EMF, WMF).
pub(super) fn image_mime(part: &str) -> Option<&'static str> {
    Some(match part.rsplit('.').next().unwrap_or("").to_lowercase().as_str() {
        "png" => "image/png",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => return None,
    })
}

/// The cells of a sheet's XML with a vm that is a picture.
fn vm_cells(xml: &[u8], vm: &[Option<usize>]) -> HashMap<(u32, u32), usize> {
    static CELL: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r#"<c\b([^>]*\bvm="[^"]*"[^>]*)>"#).unwrap());
    static ATTR: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r#"\b(r|vm)="([^"]*)""#).unwrap());
    let s = String::from_utf8_lossy(xml);
    let mut out = HashMap::new();
    if !s.contains(" vm=\"") {
        return out;
    }
    for c in CELL.captures_iter(&s) {
        let (mut r, mut v) = (None, None);
        for a in ATTR.captures_iter(&c[1]) {
            match &a[1] {
                "r" => r = Some(a[2].to_string()),
                _ => v = a[2].parse::<usize>().ok(),
            }
        }
        if let (Some(r), Some(v)) = (r, v)
            && let Some(Some(img)) = v.checked_sub(1).and_then(|i| vm.get(i))
            && let Ok(pos) = super::xlsx::parse_ref(&r)
        {
            out.insert(pos, *img);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_with_value_metadata() {
        let xml = br#"<worksheet><sheetData><row r="2"><c r="A2" t="s"><v>0</v></c><c r="B2" t="e" vm="1"><v>#VALUE!</v></c><c r="C2" vm="2" t="e"/><c r="D2" t="e" vm="3"><f>B2</f><v>#VALUE!</v></c></row></sheetData></worksheet>"#;
        let cells = vm_cells(xml, &[Some(0), None, Some(0)]);
        assert_eq!(cells.len(), 2);
        assert_eq!(cells.get(&(2, 2)), Some(&0));
        assert_eq!(cells.get(&(4, 2)), Some(&0));
    }
}
