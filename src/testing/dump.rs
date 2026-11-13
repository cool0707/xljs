//! The files a run wrote: the folder is compared before and after, and the
//! contents of the new and changed files become events of the transcript.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value, json};

/// The cells and lines of one file that are told one by one; the rest is
/// only counted (with a checksum).
const MAX_CELLS: usize = 20_000;
const MAX_LINES: usize = 5_000;

const TEXT: &[&str] = &["csv", "tsv", "txt", "json", "md", "html", "htm", "xml", "js", "ts", "css", "log", "ini", "yaml", "yml"];
const BOOK: &[&str] = &["xlsx", "xlsm"];
const DATABASE: &[&str] = &["db", "sqlite", "sqlite3"];

/// Relative path ("/" separated) -> (size, checksum).
pub type Snapshot = BTreeMap<String, (u64, u32)>;

/// Whether the entry is left out of the copy and of the comparison:
/// folders of tools (.xljs, .git) and the lock files of Excel.
pub fn skipped(name: &str, is_dir: bool) -> bool {
    (is_dir && name.starts_with('.')) || name.starts_with("~$")
}

fn walk(dir: &Path) -> impl Iterator<Item = walkdir::DirEntry> {
    walkdir::WalkDir::new(dir)
        .min_depth(1)
        .sort_by_file_name()
        .into_iter()
        .filter_entry(|e| !skipped(&e.file_name().to_string_lossy(), e.file_type().is_dir()))
        .flatten()
        .filter(|e| e.file_type().is_file())
}

fn rel(dir: &Path, p: &Path) -> String {
    p.strip_prefix(dir).unwrap_or(p).components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/")
}

pub fn snapshot(dir: &Path) -> Snapshot {
    walk(dir)
        .filter_map(|e| {
            let data = std::fs::read(e.path()).ok()?;
            Some((rel(dir, e.path()), (data.len() as u64, crc32fast::hash(&data))))
        })
        .collect()
}

fn ext(name: &str) -> String {
    Path::new(name).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn crc(data: &[u8]) -> String {
    format!("{:08x}", crc32fast::hash(data))
}

/// The text of a file: UTF-8 (without the BOM), else Shift_JIS.
fn decode(data: &[u8]) -> Option<String> {
    let data = data.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(data);
    match std::str::from_utf8(data) {
        Ok(s) => Some(s.to_string()),
        Err(_) => {
            let (s, _, bad) = encoding_rs::SHIFT_JIS.decode(data);
            (!bad).then(|| s.into_owned())
        }
    }
}

fn book_events(file: &str, data: &[u8], out: &mut Vec<Value>) -> bool {
    let Ok(b) = crate::api::xlsx::open_bytes(data, file.to_string()) else { return false };
    out.push(json!({ "type": "file", "file": file, "kind": "book" }));
    let mut cells = Vec::new();
    for ws in b.wb.sheet_collection() {
        let mut list: Vec<&umya_spreadsheet::Cell> = ws.cells().into_iter().collect();
        list.sort_by_key(|c| (c.coordinate().row_num(), c.coordinate().col_num()));
        for c in list {
            let text = crate::api::xlsx::cell_text(&b, c);
            let formula = c.formula().to_string();
            if text.is_empty() && formula.is_empty() {
                continue;
            }
            let at = format!("{}{}", crate::api::xlsx::column_name(c.coordinate().col_num() as i64).unwrap_or_default(), c.coordinate().row_num());
            let mut e = json!({ "type": "cell", "file": file, "sheet": ws.name(), "cell": at, "text": text });
            if !formula.is_empty() {
                e["formula"] = formula.into();
            }
            cells.push(e);
        }
    }
    more(file, "cells", cells, MAX_CELLS, out);
    true
}

/// Adds the first max events; the others are counted, with a checksum.
fn more(file: &str, what: &str, mut events: Vec<Value>, max: usize, out: &mut Vec<Value>) {
    if events.len() > max {
        let rest = events.split_off(max);
        let text: String = rest.iter().map(|e| e.to_string() + "\n").collect();
        out.append(&mut events);
        out.push(json!({ "type": "more", "file": file, what: rest.len(), "crc": crc(text.as_bytes()) }));
    } else {
        out.append(&mut events);
    }
}

fn file_events(file: &str, data: &[u8], out: &mut Vec<Value>) {
    let x = ext(file);
    if DATABASE.contains(&x.as_str()) {
        // Never the same bytes twice: only that it is there.
        out.push(json!({ "type": "file", "file": file, "kind": "database" }));
        return;
    }
    if BOOK.contains(&x.as_str()) && book_events(file, data, out) {
        return;
    }
    if TEXT.contains(&x.as_str())
        && let Some(text) = decode(data)
    {
        out.push(json!({ "type": "file", "file": file, "kind": "text" }));
        let lines = text.lines().map(|l| json!({ "type": "line", "file": file, "text": l })).collect();
        more(file, "lines", lines, MAX_LINES, out);
        return;
    }
    out.push(json!({ "type": "file", "file": file, "kind": "binary", "size": data.len(), "crc": crc(data) }));
}

/// The events for the files of dir that are new or changed since before
/// (in the order of their paths), then for those that are gone.
pub fn changes(dir: &Path, before: &Snapshot) -> Vec<Value> {
    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for e in walk(dir) {
        let name = rel(dir, e.path());
        seen.insert(name.clone());
        let Ok(data) = std::fs::read(e.path()) else { continue };
        if before.get(&name) == Some(&(data.len() as u64, crc32fast::hash(&data))) {
            continue;
        }
        file_events(&name, &data, &mut out);
    }
    for name in before.keys().filter(|n| !seen.contains(*n)) {
        out.push(json!({ "type": "removed", "file": name }));
    }
    out
}
