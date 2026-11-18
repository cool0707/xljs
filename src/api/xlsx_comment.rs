//! Notes and threaded comments of cells. Excel calls the old comments
//! "notes" (メモ) and the threaded ones "comments" (コメント). A threaded
//! comment is kept in xl/threadedComments (the first of the thread and its
//! replies, by `parentId`), its authors in xl/persons; Excel also writes a
//! note in its place for the versions without them (author `tc={id}`,
//! `xr:uid` the id of the thread), which is kept up to date here. A cell has
//! either a note or a thread.

use std::hash::{BuildHasher, Hasher};

use serde_json::{Value as Json, json};
use umya_spreadsheet::structs::office2019::person::Person;
use umya_spreadsheet::structs::office2019::threaded_comment::ThreadedComment;
use umya_spreadsheet::{Comment, Worksheet};

use super::xlsx::{Book, cell_name, parse_ref, with_book};
use super::xlsx_merge::{anchor_name, anchor_of};
use super::{N, js};
use crate::{Result, err, t};

/// The author of the note Excel writes in place of the thread `id`.
fn fallback_author(id: &str) -> String {
    format!("tc={id}")
}

/// Whether a note is the one in place of a thread.
fn is_fallback(c: &Comment) -> bool {
    c.author().starts_with("tc=")
}

fn at(col: u32, row: u32) -> impl Fn(&umya_spreadsheet::Coordinate) -> bool {
    move |c| c.col_num() == col && c.row_num() == row
}

fn note_text(c: &Comment) -> String {
    let t = c.text();
    match (t.text(), t.rich_text()) {
        (Some(t), _) => t.value().to_string(),
        (None, Some(r)) => r.text().into_owned(),
        _ => String::new(),
    }
}

/// A new id like Excel's: "{8-4-4-4-12}" in upper-case hexadecimal.
fn new_guid() -> String {
    static COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let mut bits = [0u64; 2];
    for (i, b) in bits.iter_mut().enumerate() {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        h.write_usize(i);
        h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
        *b = h.finish();
    }
    let n = (u128::from(bits[0]) << 64) | u128::from(bits[1]);
    let s = format!("{n:032X}");
    format!("{{{}-{}-{}-{}-{}}}", &s[..8], &s[8..12], &s[12..16], &s[16..20], &s[20..])
}

/// When a comment was written (dT, in UTC) as JavaScript reads it, or null.
fn date_out(d: &str) -> Json {
    let d = d.trim_end_matches('Z');
    chrono::NaiveDateTime::parse_from_str(d, "%Y-%m-%dT%H:%M:%S%.f")
        .map_or(Json::Null, |t| Json::String(t.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()))
}

/// The dT of a comment written at `iso` (Date.toISOString), as Excel writes it.
fn date_in(iso: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(iso)
        .map(|t| {
            // Hundredths of a second, as Excel writes them.
            let mut s = t.with_timezone(&chrono::Utc).format("%Y-%m-%dT%H:%M:%S%.3f").to_string();
            s.pop();
            s
        })
        .unwrap_or_default()
}

fn author(persons: &[Person], id: &str) -> String {
    persons.iter().find(|p| p.id == id).map(|p| p.display_name.clone()).unwrap_or_default()
}

/// The id of the person named `name`, added when the workbook has none.
fn person_id(b: &mut Book, name: &str) -> String {
    let persons = b.wb.persons_mut();
    if let Some(p) = persons.iter().find(|p| p.display_name == name) {
        return p.id.clone();
    }
    let id = new_guid();
    persons.push(Person { display_name: name.to_string(), id: id.clone(), user_id: name.to_string(), provider_id: "None".into() });
    id
}

/// The threads of a sheet (those of the cell `only` when given) in the
/// order of the cells, as the prelude reads them.
fn threads(b: &Book, ws: &Worksheet, only: Option<(u32, u32)>) -> Json {
    let persons = b.wb.persons();
    let list = ws.threaded_comments();
    let entry = |c: &ThreadedComment| {
        json!({ "id": c.id(), "author": author(persons, c.person_id()), "date": date_out(c.d_t()), "text": c.text() })
    };
    let mut roots: Vec<&ThreadedComment> = list
        .iter()
        .filter(|c| c.parent_id().is_empty() || !list.iter().any(|p| p.id() == c.parent_id()))
        .filter(|c| only.is_none_or(|(col, row)| at(col, row)(c.coordinate())))
        .collect();
    roots.sort_by_key(|c| (c.coordinate().row_num(), c.coordinate().col_num()));
    let out = roots.into_iter().map(|r| {
        let mut o = entry(r);
        o["address"] = json!(r.coordinate().to_string());
        o["resolved"] = json!(r.done());
        o["replies"] = list.iter().filter(|c| c.parent_id() == r.id() && !r.id().is_empty()).map(entry).collect();
        o
    });
    Json::Array(out.collect())
}

/// Writes the note in place of the thread `root` again (or removes it when
/// the thread is gone).
fn sync_fallback(ws: &mut Worksheet, root: &str) {
    let list = ws.threaded_comments();
    let Some(first) = list.iter().find(|c| c.id() == root) else {
        ws.comments_mut().retain(|c| !(c.uid() == root || c.author() == fallback_author(root)));
        return;
    };
    let (col, row) = (first.coordinate().col_num(), first.coordinate().row_num());
    let mut text = format!("{}\n\n{}\n    {}", t!("xlsx.comment.fallback"), t!("xlsx.comment.fallbackComment"), first.text());
    for r in list.iter().filter(|c| c.parent_id() == root) {
        text.push_str(&format!("\n{}\n{}", t!("xlsx.comment.fallbackReply"), r.text()));
    }
    let notes = ws.comments_mut();
    let i = match notes.iter().position(|c| c.uid() == root || c.author() == fallback_author(root)) {
        Some(i) => i,
        None => {
            let mut c = Comment::default();
            c.new_comment((col, row));
            c.set_author(fallback_author(root)).set_uid(root);
            notes.push(c);
            notes.len() - 1
        }
    };
    notes[i].set_text_string(text);
}

/// The thread a comment `id` belongs to (its own id for the first).
fn root_of(ws: &Worksheet, id: &str) -> Option<String> {
    let c = ws.threaded_comments().iter().find(|c| c.id() == id)?;
    Some(if c.parent_id().is_empty() { c.id().to_string() } else { c.parent_id().to_string() })
}

fn add_comment(b: &mut Book, sheet: &str, r: &str, parent: &str, text: &str, name: &str, date: &str) -> Result<String> {
    if text.is_empty() {
        return Err(err!("xlsx.comment.empty"));
    }
    let (col, row) = if parent.is_empty() {
        let (col, row) = parse_ref(r)?;
        let ws = b.sheet(sheet)?;
        if ws.comments().iter().any(|c| at(col, row)(c.coordinate()) && !is_fallback(c)) {
            return Err(err!("xlsx.comment.hasNote", r));
        }
        if ws.threaded_comments().iter().any(|c| at(col, row)(c.coordinate())) {
            return Err(err!("xlsx.comment.hasComment", r));
        }
        (col, row)
    } else {
        let ws = b.sheet(sheet)?;
        let p = ws.threaded_comments().iter().find(|c| c.id() == parent).ok_or_else(|| err!("xlsx.comment.deleted"))?;
        (p.coordinate().col_num(), p.coordinate().row_num())
    };
    let person = person_id(b, name);
    let id = new_guid();
    let mut c = ThreadedComment::default();
    c.coordinate_mut().set_col_num(col).set_row_num(row);
    c.set_id(&id).set_person_id(person).set_parent_id(parent).set_text(text).set_d_t(date_in(date));
    let ws = b.layout_mut(sheet)?;
    ws.add_threaded_comments(c);
    sync_fallback(ws, if parent.is_empty() { &id } else { parent });
    Ok(id)
}

fn edit_comment(b: &mut Book, sheet: &str, id: &str, field: &str, v: &rquickjs::Value) -> Result<()> {
    let ws = b.layout_mut(sheet)?;
    let root = root_of(ws, id).ok_or_else(|| err!("xlsx.comment.deleted"))?;
    match field {
        "text" => {
            let text = js::to_str(v);
            if text.is_empty() {
                return Err(err!("xlsx.comment.empty"));
            }
            if let Some(c) = ws.threaded_comments_mut().iter_mut().find(|c| c.id() == id) {
                c.set_text(text);
            }
        }
        _ => {
            let done = v.as_bool().unwrap_or(false);
            if let Some(c) = ws.threaded_comments_mut().iter_mut().find(|c| c.id() == root) {
                c.set_done(done);
            }
        }
    }
    sync_fallback(ws, &root);
    Ok(())
}

fn delete_comment(b: &mut Book, sheet: &str, id: &str) -> Result<()> {
    let ws = b.layout_mut(sheet)?;
    let Some(root) = root_of(ws, id) else { return Ok(()) };
    // The first of a thread takes its replies with it.
    ws.threaded_comments_mut().retain(|c| c.id() != id && c.parent_id() != id);
    sync_fallback(ws, &root);
    Ok(())
}

fn set_note(b: &mut Book, sheet: &str, r: &str, text: Option<String>, name: &str) -> Result<()> {
    let (col, row) = parse_ref(r)?;
    let ws = b.sheet(sheet)?;
    let thread = ws.threaded_comments().iter().any(|c| at(col, row)(c.coordinate()));
    match text {
        Some(_) if thread => return Err(err!("xlsx.note.hasComment", r)),
        // The note in place of the thread stays.
        None if thread => return Ok(()),
        _ => {}
    }
    let ws = b.layout_mut(sheet)?;
    let notes = ws.comments_mut();
    let found = notes.iter().position(|c| at(col, row)(c.coordinate()));
    match (text, found) {
        (None, Some(i)) => {
            notes.remove(i);
        }
        (None, None) => {}
        (Some(text), Some(i)) => {
            notes[i].set_text_string(text);
        }
        (Some(text), None) => {
            let mut c = Comment::default();
            c.new_comment((col, row));
            c.set_author(name).set_text_string(text);
            notes.push(c);
        }
    }
    Ok(())
}

pub(super) fn install(n: &N) {
    // xlsxGetNote(book, sheet, ref) -> the text of the note, or null
    n.set("xlsxGetNote", |h, a| {
        let (sheet, r) = (a.str(1), a.str(2).to_uppercase());
        let text = with_book(h, a, |b| {
            let (col, row) = anchor_of(b, &sheet, &r).map_err(|e| a.throw(&e))?;
            let ws = b.sheet(&sheet).map_err(|e| a.throw(&e))?;
            Ok(ws.comments().iter().find(|c| at(col, row)(c.coordinate()) && !is_fallback(c)).map(note_text))
        })?;
        match text {
            Some(t) => a.ret(t),
            None => a.null(),
        }
    });
    // xlsxSetNote(book, sheet, ref, text | null, author)
    n.set("xlsxSetNote", |h, a| {
        let (sheet, r, v, name) = (a.str(1), a.str(2).to_uppercase(), a.arg(3), a.str(4));
        let text = (!js::nullish(&v)).then(|| js::to_str(&v));
        with_book(h, a, |b| {
            let r = anchor_name(b, &sheet, &r).map_err(|e| a.throw(&e))?;
            set_note(b, &sheet, &r, text, &name).map_err(|e| a.throw(&e))
        })?;
        a.undefined()
    });
    // xlsxNotes(book, sheet) -> JSON [{ address, text, author }] in the order of the cells
    n.set("xlsxNotes", |h, a| {
        let sheet = a.str(1);
        let out = with_book(h, a, |b| {
            let ws = b.sheet(&sheet).map_err(|e| a.throw(&e))?;
            let mut notes: Vec<&Comment> = ws.comments().iter().filter(|c| !is_fallback(c)).collect();
            notes.sort_by_key(|c| (c.coordinate().row_num(), c.coordinate().col_num()));
            let list = notes.into_iter().map(|c| {
                let address = cell_name(c.coordinate().col_num(), c.coordinate().row_num()).unwrap_or_default();
                json!({ "address": address, "text": note_text(c), "author": c.author() })
            });
            Ok(Json::Array(list.collect()).to_string())
        })?;
        a.ret(out)
    });
    // xlsxComments(book, sheet, ref | "") -> JSON [{ id, address, author, date, text, resolved,
    // replies: [{ id, author, date, text }] }]: the threads of the cell, or of the sheet
    n.set("xlsxComments", |h, a| {
        let (sheet, r) = (a.str(1), a.str(2).to_uppercase());
        let out = with_book(h, a, |b| {
            let only = if r.is_empty() { None } else { Some(anchor_of(b, &sheet, &r).map_err(|e| a.throw(&e))?) };
            let ws = b.sheet(&sheet).map_err(|e| a.throw(&e))?;
            Ok(threads(b, ws, only).to_string())
        })?;
        a.ret(out)
    });
    // xlsxAddComment(book, sheet, ref, parentId | "", text, author, date ISO) -> the id
    n.set("xlsxAddComment", |h, a| {
        let (sheet, r, parent, text, name, date) = (a.str(1), a.str(2).to_uppercase(), a.str(3), a.str(4), a.str(5), a.str(6));
        let id = with_book(h, a, |b| {
            let r = anchor_name(b, &sheet, &r).map_err(|e| a.throw(&e))?;
            add_comment(b, &sheet, &r, &parent, &text, &name, &date).map_err(|e| a.throw(&e))
        })?;
        a.ret(id)
    });
    // xlsxEditComment(book, sheet, id, "text" | "resolved", value)
    n.set("xlsxEditComment", |h, a| {
        let (sheet, id, field, v) = (a.str(1), a.str(2), a.str(3), a.arg(4));
        with_book(h, a, |b| edit_comment(b, &sheet, &id, &field, &v).map_err(|e| a.throw(&e)))?;
        a.undefined()
    });
    // xlsxDeleteComment(book, sheet, id): a reply, or the thread with its replies
    n.set("xlsxDeleteComment", |h, a| {
        let (sheet, id) = (a.str(1), a.str(2));
        with_book(h, a, |b| delete_comment(b, &sheet, &id).map_err(|e| a.throw(&e)))?;
        a.undefined()
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guids_look_like_excels() {
        let (a, b) = (new_guid(), new_guid());
        assert_ne!(a, b);
        assert!(regex::Regex::new(r"^\{[0-9A-F]{8}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{12}\}$").unwrap().is_match(&a), "{a}");
    }

    #[test]
    fn dates_in_utc() {
        assert_eq!(date_out("2026-11-15T23:10:33.81"), json!("2026-11-15T23:10:33.810Z"));
        assert_eq!(date_out(""), Json::Null);
        assert_eq!(date_in("2026-11-15T23:10:33.812Z"), "2026-11-15T23:10:33.81");
    }
}
