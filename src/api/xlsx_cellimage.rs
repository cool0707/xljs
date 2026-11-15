//! Pictures placed in cells (Excel's "Place in Cell"). Excel keeps them as
//! rich values: the cell is an error (`t="e"`, `#VALUE!`) with a value
//! metadata index (`vm`), metadata.xml maps it to a rich value
//! (xl/richData/rdrichvalue.xml), whose structure
//! (rdrichvaluestructure.xml) names the key `_rvRel:LocalImageIdentifier`,
//! an index into richValueRel.xml, whose relationship is the picture in
//! xl/media. Formulas that refer to such a cell (`=B2`) show the picture
//! too; Excel saves them with a `vm` of their own.
//!
//! umya-spreadsheet (the fork) keeps the `vm` of the cells and writes the
//! rich values back as they were, without looking into them; what a `vm`
//! shows is read here.

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
    /// The picture (index in images) of each block of value metadata: what
    /// the `vm` of a cell, a 1-based index, refers to.
    vm: Vec<Option<usize>>,
}

impl CellImages {
    /// The picture placed in a cell: its value metadata refers to one, and
    /// the cell still is the error that stands for it (a value or a formula
    /// a script writes takes the metadata off, Book::cell_mut).
    pub fn of(&self, cell: &umya_spreadsheet::Cell) -> Option<usize> {
        if !matches!(cell.raw_value(), umya_spreadsheet::CellRawValue::Error(_)) {
            return None;
        }
        self.vm.get((cell.value_meta_index() as usize).checked_sub(1)?).copied().flatten()
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
        out.vm = vm;
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

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;
    use crate::api::xlsx::{Book, open_bytes};

    fn book() -> (Vec<u8>, Book) {
        let data = std::fs::read(crate::testutil::repo_path("testdata/xlsx/cellimages.xlsx")).unwrap();
        let b = open_bytes(&data, String::new()).unwrap();
        (data, b)
    }

    fn parts(data: &[u8]) -> HashMap<String, Vec<u8>> {
        let mut zr = zip::ZipArchive::new(Cursor::new(data)).unwrap();
        (0..zr.len())
            .map(|i| {
                let mut f = zr.by_index(i).unwrap();
                let mut v = Vec::new();
                f.read_to_end(&mut v).unwrap();
                (f.name().to_string(), v)
            })
            .collect()
    }

    fn picture(b: &Book, sheet: &str, at: (u32, u32)) -> Option<usize> {
        b.cell_images.of(b.sheet(sheet).unwrap().cell(at)?)
    }

    #[test]
    fn pictures_by_the_value_metadata_of_the_cells() {
        let (_, mut b) = book();
        assert_eq!(b.cell_images.images.len(), 3);
        assert_eq!(picture(&b, "商品", (2, 2)), Some(0));
        assert_eq!(picture(&b, "商品", (2, 4)), Some(2));
        assert_eq!(picture(&b, "商品", (1, 2)), None);
        assert_eq!(picture(&b, "参照", (1, 2)), Some(1));
        // They go with their cells.
        b.sheet_mut("商品").unwrap().insert_new_row(3, 2);
        assert_eq!(picture(&b, "商品", (2, 2)), Some(0));
        assert_eq!(picture(&b, "商品", (2, 3)), None);
        assert_eq!(picture(&b, "商品", (2, 5)), Some(1));
        assert_eq!(picture(&b, "商品", (2, 6)), Some(2));
        // A value a script writes takes the picture off, an error too.
        b.cell_mut("商品", 2, 5).unwrap().set_value_string("x");
        b.cell_mut("商品", 2, 6).unwrap().set_error("#N/A");
        assert_eq!(picture(&b, "商品", (2, 5)), None);
        assert_eq!(picture(&b, "商品", (2, 6)), None);
    }

    #[test]
    fn saved_with_the_workbook() {
        let (data, mut b) = book();
        let (before, after) = (parts(&data), parts(&b.to_bytes().unwrap()));
        for name in before.keys().filter(|n| n.starts_with("xl/richData/") || n.starts_with("xl/media/")) {
            if name.ends_with(".rels") {
                assert!(after.contains_key(name), "{name}");
            } else {
                assert_eq!(after.get(name), before.get(name), "{name}");
            }
        }
        let text = |name: &str| String::from_utf8_lossy(&after[name]).into_owned();
        assert!(text("xl/worksheets/sheet1.xml").contains(r#"<c r="B2" t="e" vm="1"><v>#VALUE!</v></c>"#));
        assert!(text("xl/metadata.xml").contains(r#"<valueMetadata count="3">"#));
        assert!(text("xl/_rels/workbook.xml.rels").contains("richData/rdrichvalue.xml"));
        assert!(text("[Content_Types].xml").contains("/xl/richData/richValueRel.xml"));
        // Read again, the cells show what they did.
        let again = open_bytes(&b.to_bytes().unwrap(), String::new()).unwrap();
        assert_eq!(again.cell_images.images, b.cell_images.images);
        for (sheet, at) in [("商品", (2, 2)), ("商品", (4, 2)), ("商品", (2, 3)), ("商品", (2, 4)), ("参照", (1, 2)), ("参照", (2, 2))] {
            assert_eq!(picture(&again, sheet, at), picture(&b, sheet, at), "{sheet} {at:?}");
        }

        // With no picture left in a cell, the rich values are not written.
        for (sheet, col, row) in [("商品", 2, 2), ("商品", 2, 3), ("商品", 2, 4), ("商品", 4, 2), ("商品", 4, 3), ("参照", 1, 2)] {
            b.cell_mut(sheet, col, row).unwrap().cell_value_mut().set_blank();
        }
        let after = parts(&b.to_bytes().unwrap());
        assert!(!after.keys().any(|n| n.starts_with("xl/richData/") || n.starts_with("xl/media/") || n == "xl/metadata.xml"), "{:?}", after.keys());
    }
}
