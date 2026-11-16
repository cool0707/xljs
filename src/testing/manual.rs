//! Manuals made from a test case (`xljs test --manual`): the case's own
//! answers are replayed, each thing the user does (a form's button, an
//! answer to a dialog, a file chosen) becomes a step with its text and a
//! picture of the screen with what was operated in red boxes, and the
//! steps with the user's notes become a Markdown file.
//! See docs/design/test-design.md ("手順書").
//!
//! Files, in the case's folder:
//! - `manual.json`: the user's notes (title, introduction, a note per step,
//!   hidden steps). Kept when the manual is made again.
//! - `manual/steps.json`: the steps of the last run (made by xljs).
//! - `manual/<case>.md` and `manual/images/<step>.png`: the manual.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{Case, RunOptions, cases, get, target, text};
use crate::ui::{ShotJob, Ui};
use crate::{Error, Result, err, t};

/// The folder of the manual in the case's folder.
pub const DIR: &str = "manual";
const NOTES: &str = "manual.json";
const STEPS: &str = "steps.json";
const IMAGES: &str = "images";

/// The height of the title bar the pages draw on their pictures (shot.js).
const TITLE_BAR: i64 = 32;

fn is_false(b: &bool) -> bool {
    !*b
}

/// What the user wrote for the manual of a case (`manual.json`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Notes {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub title: String,
    /// Before the steps and after them.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub intro: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub outro: String,
    /// By step id ("form-1").
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub steps: BTreeMap<String, StepNote>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct StepNote {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub note: String,
    /// Left out of the manual.
    #[serde(skip_serializing_if = "is_false")]
    pub hide: bool,
}

/// One thing the user does.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Step {
    /// The kind and its count: "form-1" (the first answer to a form),
    /// "confirm-2"... It names the step's note and pictures.
    pub id: String,
    pub heading: String,
    /// Markdown: lines of text, then "- " lines (the values entered).
    pub text: Vec<String>,
    /// The pictures, relative to the manual's folder.
    pub images: Vec<String>,
}

/// The steps of a manual (`manual/steps.json`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Made {
    pub case: String,
    /// The script, relative to its folder.
    pub script: String,
    pub steps: Vec<Step>,
    /// What the script writes, as lines of Markdown.
    pub results: Vec<String>,
    /// The pictures that could not be taken (messages); with no UI to take
    /// them, the reason once.
    pub failed: Vec<String>,
}

/// A step and the pictures to take of it (the "screen" events).
struct Found {
    step: Step,
    screens: Vec<Value>,
}

fn quoted(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// The label of an option of a field (event "field") for its value.
fn option_label(field: &Value, v: &Value) -> Option<String> {
    let want = quoted(v);
    for o in get(field, "options").as_array().into_iter().flatten() {
        match o {
            Value::Array(a) if a.first().map(quoted).as_deref() == Some(want.as_str()) => return a.get(1).map(quoted),
            o if quoted(o) == want => return Some(want),
            _ => {}
        }
    }
    None
}

/// A value entered in a field, as the manual tells it.
fn value_text(field: Option<&Value>, v: &Value) -> String {
    let kind = field.map(|f| text(f, "kind")).unwrap_or("");
    match (kind, v) {
        ("checkbox", Value::Bool(b)) => if *b { t!("manual.checkOn") } else { t!("manual.checkOff") },
        ("table", Value::Array(rows)) => {
            let rows: Vec<String> = rows.iter().filter_map(Value::as_i64).map(|r| (r + 1).to_string()).collect();
            t!("manual.rows", rows.join(", "))
        }
        (_, Value::Array(list)) => {
            let items: Vec<String> = list.iter().map(|x| field.and_then(|f| option_label(f, x)).unwrap_or_else(|| quoted(x))).map(|s| format!("「{s}」")).collect();
            if items.is_empty() { t!("manual.empty") } else { items.join(" ") }
        }
        (_, v) => {
            let s = field.and_then(|f| option_label(f, v)).unwrap_or_else(|| quoted(v));
            if s.is_empty() { t!("manual.empty") } else { format!("「{s}」") }
        }
    }
}

/// The label of a field for a manual (its name when it has none).
fn field_label(f: &Value) -> String {
    match text(f, "label") {
        "" => text(f, "name").to_string(),
        l => l.to_string(),
    }
}

/// The label of the button pressed: "$button" of an answer (an index in
/// the form's "buttons" or a label), else the submit button's.
fn button_label(form: &Value, values: &Value) -> String {
    match values.get("$button") {
        Some(Value::Number(n)) => get(form, "buttons").get(n.as_u64().unwrap_or(0) as usize).map(quoted).unwrap_or_else(|| n.to_string()),
        Some(Value::String(s)) => s.clone(),
        _ => match text(form, "submit") {
            "" => t!("form.ok"),
            s => s.to_string(),
        },
    }
}

/// The steps of a run (a transcript made with run_with(shots)).
fn find(events: &[Value]) -> (Vec<Found>, Vec<String>) {
    let mut out: Vec<Found> = Vec::new();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut form = Value::Null;
    let mut fields: Vec<&Value> = Vec::new();
    let mut results = Vec::new();
    let mut push = |out: &mut Vec<Found>, kind: &'static str, heading: String, text: Vec<String>| {
        let n = counts.entry(kind).or_default();
        *n += 1;
        out.push(Found { step: Step { id: format!("{kind}-{n}"), heading, text, images: Vec::new() }, screens: Vec::new() });
    };
    for e in events {
        match text(e, "type") {
            "form" => {
                form = e.clone();
                fields.clear();
            }
            "field" => fields.push(e),
            "submit" => {
                let values = get(e, "values");
                let title = text(&form, "title");
                let button = button_label(&form, values);
                let mut lines = Vec::new();
                for (k, v) in values.as_object().into_iter().flatten() {
                    if k.starts_with('$') {
                        continue;
                    }
                    let f = fields.iter().copied().find(|f| text(f, "name") == k);
                    let label = f.map(field_label).unwrap_or_else(|| k.clone());
                    lines.push(format!("- **{label}**: {}", value_text(f, v)));
                }
                for name in values.get("$cells").and_then(Value::as_object).into_iter().flatten().map(|(k, _)| k) {
                    let label = fields.iter().find(|f| text(f, "name") == name).map(|f| field_label(f)).unwrap_or_else(|| name.clone());
                    lines.push(format!("- {}", t!("manual.cells", label)));
                }
                let lead = if lines.is_empty() { t!("manual.press", button) } else { t!("manual.enterAndPress", button) };
                let mut text = vec![lead];
                text.extend(lines);
                push(&mut out, "form", t!("manual.formHead", title), text);
            }
            "close" if !form.is_null() => {
                let title = text(&form, "title").to_string();
                push(&mut out, "form", t!("manual.closeHead", title), vec![t!("manual.close", t!("form.cancel"))]);
                form = Value::Null;
            }
            "close" => {} // a table
            "confirm" => {
                let yes = get(e, "answer").as_bool().unwrap_or(false);
                let button = if yes { t!("form.ok") } else { t!("form.cancel") };
                push(&mut out, "confirm", t!("manual.confirmHead"), vec![t!("manual.confirm", text(e, "text"), button)]);
            }
            "alert" => push(&mut out, "alert", t!("manual.alertHead"), vec![t!("manual.alert", text(e, "text"))]),
            "error" => push(&mut out, "error", t!("manual.errorHead"), vec![t!("manual.alert", text(e, "text"))]),
            "prompt" => {
                let line = match get(e, "answer") {
                    Value::String(s) => t!("manual.prompt", text(e, "text"), s),
                    _ => t!("manual.promptCancel", text(e, "text")),
                };
                push(&mut out, "prompt", t!("manual.promptHead"), vec![line]);
            }
            "openFiles" => {
                let files: Vec<String> = get(e, "answer").as_array().into_iter().flatten().map(|f| format!("`{}`", quoted(f))).collect();
                let line = if files.is_empty() { t!("manual.pickerCancel") } else { t!("manual.openFiles", files.join(" ")) };
                push(&mut out, "openFiles", t!("manual.openFilesHead"), vec![line]);
            }
            "saveFile" => {
                let line = match get(e, "answer") {
                    Value::String(s) => t!("manual.saveFile", format!("`{s}`")),
                    _ => t!("manual.pickerCancel"),
                };
                push(&mut out, "saveFile", t!("manual.saveFileHead"), vec![line]);
            }
            "directory" => {
                let line = match get(e, "answer") {
                    Value::String(s) => t!("manual.directory", format!("`{s}`")),
                    _ => t!("manual.pickerCancel"),
                };
                push(&mut out, "directory", t!("manual.directoryHead"), vec![line]);
            }
            "table" => {
                let title = text(e, "title").to_string();
                if e.get("select").is_some() {
                    form = Value::Null;
                    push(&mut out, "table", t!("manual.tableHead", title.clone()), Vec::new());
                } else {
                    push(&mut out, "table", t!("manual.tableHead", title), vec![t!("manual.tableShown")]);
                }
            }
            "select" => {
                if let Some(f) = out.last_mut().filter(|f| f.step.id.starts_with("table-") && f.step.text.is_empty()) {
                    let rows: Vec<String> = get(e, "rows").as_array().into_iter().flatten().filter_map(Value::as_i64).map(|r| (r + 1).to_string()).collect();
                    f.step.text.push(t!("manual.tableSelect", rows.join(", ")));
                }
            }
            "notify" => push(&mut out, "notify", t!("manual.notifyHead"), vec![t!("manual.notify", text(e, "title"), text(e, "text"))]),
            "openPath" => push(&mut out, "openPath", t!("manual.openPathHead"), vec![t!("manual.openPath", format!("`{}`", text(e, "target")))]),
            "showInFolder" => {
                let files: Vec<String> = get(e, "files").as_array().into_iter().flatten().map(|f| format!("`{}`", quoted(f))).collect();
                push(&mut out, "showInFolder", t!("manual.openPathHead"), vec![t!("manual.showInFolder", files.join(" "))]);
            }
            "screen" => {
                if let Some(f) = out.last_mut() {
                    f.screens.push(e.clone());
                }
            }
            "file" => results.push(format!("- {}", t!("manual.fileMade", format!("`{}`", text(e, "file"))))),
            "removed" => results.push(format!("- {}", t!("manual.fileRemoved", format!("`{}`", text(e, "file"))))),
            _ => {}
        }
    }
    // A table closed without a choice.
    for f in &mut out {
        if f.step.text.is_empty() {
            f.step.text.push(t!("manual.close", t!("form.cancel")));
        }
    }
    (out, results)
}

/// The steps of a run (a transcript made with run_with(shots)) and the
/// lines about the files the script writes.
pub fn steps(events: &[Value]) -> (Vec<Step>, Vec<String>) {
    let (found, results) = find(events);
    (found.into_iter().map(|f| f.step).collect(), results)
}

/// The picture of a "screen" event, to save at path.
fn shot_job(screen: &Value, path: &Path) -> Option<ShotJob> {
    let path = path.to_string_lossy().into_owned();
    match text(screen, "page") {
        "form" => Some(ShotJob {
            page: "form.html?shot".into(),
            init: get(screen, "init").to_string(),
            mark: get(screen, "mark").to_string(),
            width: get(screen, "width").as_i64().unwrap_or(600),
            height: get(screen, "height").as_i64().unwrap_or(480) + TITLE_BAR,
            path,
        }),
        "dialog" => {
            let kind = text(screen, "kind");
            let answer = get(screen, "answer");
            let (buttons, pressed) = match kind {
                "confirm" => (vec![t!("form.ok"), t!("form.cancel")], if answer.as_bool().unwrap_or(false) { 0 } else { 1 }),
                "prompt" => (vec![t!("form.ok"), t!("form.cancel")], if answer.is_string() { 0 } else { 1 }),
                _ => (vec![t!("form.ok")], 0),
            };
            let init = json!({ "type": "init", "kind": kind, "title": text(screen, "title"), "text": text(screen, "text"), "value": answer.as_str().unwrap_or(""), "buttons": buttons });
            let mark = json!({ "type": "mark", "button": pressed, "input": kind == "prompt" && answer.is_string() });
            Some(ShotJob { page: "shot.html?shot".into(), init: init.to_string(), mark: mark.to_string(), width: 600, height: 420, path })
        }
        _ => None,
    }
}

/// Takes the pictures with u (on its thread: see ui::run); the messages of
/// those that failed.
fn take(u: Arc<dyn Ui>, jobs: Vec<ShotJob>) -> Vec<(String, Result<()>)> {
    crate::ui::run(u, move |su| {
        jobs.into_iter()
            .map(|job| {
                let path = job.path.clone();
                let (tx, rx) = std::sync::mpsc::channel();
                su.capture(job, Box::new(move |r| {
                    let _ = tx.send(r);
                }));
                let r = rx.recv().unwrap_or_else(|_| Err(err!("manual.shotFailed", "closed")));
                (path, r)
            })
            .collect()
    })
}

/// The folder of a case's manual.
pub fn folder(case_dir: &Path) -> PathBuf {
    case_dir.join(DIR)
}

/// The Markdown file of a case's manual.
pub fn markdown_path(case_dir: &Path) -> PathBuf {
    let name = case_dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    folder(case_dir).join(format!("{name}.md"))
}

fn write(path: &Path, text: &str) -> Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| Error::path("mkdir", d.display().to_string(), e))?;
    }
    std::fs::write(path, text).map_err(|e| Error::path("open", path.display().to_string(), e))
}

/// The notes of a case's manual (none yet: empty ones).
pub fn load_notes(case_dir: &Path) -> Result<Notes> {
    let path = case_dir.join(NOTES);
    match std::fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str(s.strip_prefix('\u{feff}').unwrap_or(&s)).map_err(|e| err!("manual.badNotes", path.display(), e)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Notes::default()),
        Err(e) => Err(Error::path("open", path.display().to_string(), e)),
    }
}

pub fn save_notes(case_dir: &Path, notes: &Notes) -> Result<()> {
    write(&case_dir.join(NOTES), &(serde_json::to_string_pretty(notes)? + "\n"))
}

/// The steps of the last time the manual was made (None: never).
pub fn load_made(case_dir: &Path) -> Option<Made> {
    let s = std::fs::read_to_string(folder(case_dir).join(STEPS)).ok()?;
    serde_json::from_str(&s).ok()
}

/// Text the user wrote, as Markdown: a line break stays one (two spaces
/// at the end of the line), a blank line starts a paragraph.
fn user_text(text: &str) -> String {
    let lines: Vec<&str> = text.trim().lines().map(str::trim_end).collect();
    let mut out = String::new();
    for (i, l) in lines.iter().enumerate() {
        out.push_str(l);
        let next = lines.get(i + 1).is_some_and(|n| !n.trim().is_empty());
        if next && !l.trim().is_empty() && !l.ends_with('\\') {
            out.push_str("  ");
        }
        if i + 1 < lines.len() {
            out.push('\n');
        }
    }
    out
}

/// The manual as Markdown.
pub fn markdown(made: &Made, notes: &Notes) -> String {
    let mut out = String::new();
    let title = if notes.title.trim().is_empty() {
        let stem = Path::new(&made.script).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        t!("manual.defaultTitle", stem, made.case)
    } else {
        notes.title.trim().to_string()
    };
    out.push_str(&format!("# {title}\n\n"));
    if !notes.intro.trim().is_empty() {
        out.push_str(&format!("{}\n\n", user_text(&notes.intro)));
    }
    out.push_str(&format!("## {}\n\n", t!("manual.stepsHead")));
    let mut n = 0;
    for s in &made.steps {
        let note = notes.steps.get(&s.id).cloned().unwrap_or_default();
        if note.hide {
            continue;
        }
        n += 1;
        out.push_str(&format!("### {n}. {}\n\n", s.heading));
        let (lead, items): (Vec<&String>, Vec<&String>) = s.text.iter().partition(|l| !l.starts_with("- "));
        for l in lead {
            out.push_str(&format!("{l}\n"));
        }
        out.push('\n');
        if !items.is_empty() {
            for l in items {
                out.push_str(&format!("{l}\n"));
            }
            out.push('\n');
        }
        if !note.note.trim().is_empty() {
            out.push_str(&format!("{}\n\n", user_text(&note.note)));
        }
        for img in &s.images {
            out.push_str(&format!("![{}]({img})\n\n", s.heading.replace(['[', ']'], "")));
        }
    }
    if !made.results.is_empty() {
        out.push_str(&format!("## {}\n\n", t!("manual.resultsHead")));
        for l in &made.results {
            out.push_str(&format!("{l}\n"));
        }
        out.push('\n');
    }
    if !notes.outro.trim().is_empty() {
        out.push_str(&format!("{}\n", user_text(&notes.outro)));
    }
    out.truncate(out.trim_end().len());
    out
}

/// Writes the Markdown file of a case's manual from its steps and notes
/// (after the notes changed); returns the file.
pub fn write_markdown(case_dir: &Path) -> Result<PathBuf> {
    let made = load_made(case_dir).ok_or_else(|| err!("manual.notMade"))?;
    let notes = load_notes(case_dir)?;
    let path = markdown_path(case_dir);
    write(&path, &(markdown(&made, &notes) + "\n"))?;
    Ok(path)
}

/// The case named name of the script or folder at path.
pub fn find_case(path: &str, name: &str) -> Result<Case> {
    let (root, _) = target(path)?;
    cases(&root)?.into_iter().find(|c| c.name == name).ok_or_else(|| err!("test.noSuchCase", name))
}

/// Makes the manual of the case named name (of the script or folder at
/// path): replays its own answers, takes the pictures with shooter (none:
/// no pictures) and writes the steps and the Markdown file.
pub fn make(path: &str, name: &str, o: &RunOptions, shooter: Option<Arc<dyn Ui>>) -> Result<(Made, PathBuf)> {
    let case = find_case(path, name)?;
    let plan = case.plans().remove(0);
    let events = super::run::run_with(&case, &plan, o.limit.unwrap_or(super::DEFAULT_LIMIT), &o.version, true)?;
    let (found, results) = find(&events);
    let dir = folder(&case.dir);
    let images = dir.join(IMAGES);
    let _ = std::fs::remove_dir_all(&images);
    let mut made = Made { case: case.name.clone(), script: case.script.clone(), results, ..Default::default() };
    // The pictures: step id (and a number for the second and later ones).
    let mut jobs = Vec::new();
    let mut names: Vec<Vec<String>> = Vec::new();
    for f in &found {
        let mut list = Vec::new();
        for (k, s) in f.screens.iter().enumerate() {
            let file = if k == 0 { format!("{}.png", f.step.id) } else { format!("{}-{}.png", f.step.id, k + 1) };
            if let Some(job) = shot_job(s, &images.join(&file)) {
                jobs.push(job);
                list.push(format!("{IMAGES}/{file}"));
            }
        }
        names.push(list);
    }
    let mut taken: Vec<String> = Vec::new();
    match shooter {
        Some(u) if !jobs.is_empty() => {
            std::fs::create_dir_all(&images).map_err(|e| Error::path("mkdir", images.display().to_string(), e))?;
            for (p, r) in take(u, jobs) {
                match r {
                    Ok(()) => taken.push(p),
                    Err(e) => made.failed.push(format!("{}: {}", Path::new(&p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), crate::i18n::error(&e))),
                }
            }
        }
        None if !jobs.is_empty() => made.failed.push(t!("manual.noShots")),
        _ => {}
    }
    for (f, list) in found.into_iter().zip(names) {
        let mut step = f.step;
        step.images = list.into_iter().filter(|rel| taken.iter().any(|p| p.replace('\\', "/").ends_with(rel.as_str()))).collect();
        made.steps.push(step);
    }
    write(&dir.join(STEPS), &(serde_json::to_string_pretty(&made)? + "\n"))?;
    let md = write_markdown(&case.dir)?;
    Ok((made, md))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(v: Value) -> Value {
        v
    }

    #[test]
    fn steps_of_a_run() {
        let _g = crate::testutil::lock_lang("ja");
        let events = vec![
            ev(json!({"type": "form", "title": "受注入力", "submit": "登録", "buttons": ["下書き"]})),
            ev(json!({"type": "field", "name": "customer", "label": "得意先", "kind": "text"})),
            ev(json!({"type": "field", "name": "kind", "label": "区分", "kind": "select", "options": [["a", "通常"], ["b", "至急"]]})),
            ev(json!({"type": "field", "name": "mail", "label": "メールで送る", "kind": "checkbox"})),
            ev(json!({"type": "submit", "values": {"customer": "山田商店", "kind": "b", "mail": true}})),
            ev(json!({"type": "screen", "page": "form", "init": {"type": "init"}, "mark": {"type": "mark"}, "width": 600, "height": 400})),
            ev(json!({"type": "confirm", "text": "登録しますか？", "answer": true})),
            ev(json!({"type": "screen", "page": "dialog", "kind": "confirm", "title": "受注", "text": "登録しますか？", "answer": true})),
            ev(json!({"type": "openFiles", "multiple": false, "answer": ["input/a.xlsx"]})),
            ev(json!({"type": "out", "text": "done"})),
            ev(json!({"type": "submit", "values": {"$button": 0}})),
            ev(json!({"type": "close"})),
            ev(json!({"type": "end", "code": 0})),
            ev(json!({"type": "file", "file": "out.xlsx", "kind": "book"})),
        ];
        let (found, results) = find(&events);
        let ids: Vec<&str> = found.iter().map(|f| f.step.id.as_str()).collect();
        assert_eq!(ids, ["form-1", "confirm-1", "openFiles-1", "form-2", "form-3"]);
        let s = &found[0].step;
        assert_eq!(s.heading, "「受注入力」画面");
        assert_eq!(s.text, ["次のように入力して［登録］を押します。", "- **得意先**: 「山田商店」", "- **区分**: 「至急」", "- **メールで送る**: オンにする"]);
        assert_eq!(found[0].screens.len(), 1);
        assert_eq!(found[1].step.text, ["「登録しますか？」と確認されたら［OK］を押します。"]);
        assert_eq!(found[1].screens.len(), 1);
        assert!(found[2].step.text[0].contains("`input/a.xlsx`"));
        assert_eq!(found[3].step.text, ["［下書き］を押します。"]);
        assert_eq!(found[4].step.heading, "「受注入力」画面を閉じる");
        assert_eq!(results, ["- `out.xlsx` ができます（すでにあれば書き換わります）。"]);
    }

    #[test]
    fn markdown_with_notes() {
        let _g = crate::testutil::lock_lang("ja");
        let made = Made {
            case: "月次".into(),
            script: "集計.js".into(),
            steps: vec![
                Step { id: "form-1".into(), heading: "「集計」画面".into(), text: vec!["次のように入力して［OK］を押します。".into(), "- **月**: 「4」".into()], images: vec!["images/form-1.png".into()] },
                Step { id: "notify-1".into(), heading: "通知".into(), text: vec!["x".into()], images: vec![] },
                Step { id: "alert-1".into(), heading: "メッセージ".into(), text: vec!["「完了」と表示されたら［OK］を押します。".into()], images: vec![] },
            ],
            results: vec!["- `out.xlsx` ができます。".into()],
            failed: vec![],
        };
        let mut notes = Notes::default();
        let md = markdown(&made, &notes);
        assert!(md.starts_with("# 集計の手順（月次）\n\n## 手順\n\n### 1. 「集計」画面\n\n次のように入力して［OK］を押します。\n\n- **月**: 「4」\n\n![「集計」画面](images/form-1.png)\n\n### 2. 通知"), "{md}");
        assert!(md.ends_with("## できるファイル\n\n- `out.xlsx` ができます。"), "{md}");
        notes.title = "月次集計のしかた".into();
        notes.intro = "毎月 1 日に行います。".into();
        notes.outro = "以上です。\n不明な点は\n\n山田まで。".into();
        notes.steps.insert("form-1".into(), StepNote { note: "月は半角で入力します。".into(), hide: false });
        notes.steps.insert("notify-1".into(), StepNote { note: String::new(), hide: true });
        let md = markdown(&made, &notes);
        assert!(md.starts_with("# 月次集計のしかた\n\n毎月 1 日に行います。\n\n## 手順\n\n"), "{md}");
        assert!(md.contains("- **月**: 「4」\n\n月は半角で入力します。\n\n![「集計」画面](images/form-1.png)"), "{md}");
        assert!(!md.contains("通知") && md.contains("### 2. メッセージ"), "{md}");
        assert!(md.ends_with("\n\n以上です。  \n不明な点は\n\n山田まで。"), "{md}");
    }

    #[test]
    fn notes_round_trip() {
        let dir = crate::testutil::tempdir().join("case");
        assert_eq!(load_notes(&dir).unwrap(), Notes::default());
        let mut n = Notes { title: "t".into(), ..Default::default() };
        n.steps.insert("form-1".into(), StepNote { note: "メモ".into(), hide: false });
        save_notes(&dir, &n).unwrap();
        assert_eq!(load_notes(&dir).unwrap(), n);
        let text = std::fs::read_to_string(dir.join(NOTES)).unwrap();
        assert!(!text.contains("hide") && !text.contains("intro"), "{text}");
        std::fs::write(dir.join(NOTES), "{").unwrap();
        assert!(load_notes(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dialog_pictures() {
        let _g = crate::testutil::lock_lang("ja");
        let j = shot_job(&json!({"type": "screen", "page": "dialog", "kind": "prompt", "title": "t", "text": "名前は？", "answer": "山田"}), Path::new("a.png")).unwrap();
        assert_eq!(j.page, "shot.html?shot");
        let init: Value = serde_json::from_str(&j.init).unwrap();
        assert_eq!(init["buttons"], json!(["OK", "キャンセル"]));
        assert_eq!(init["value"], "山田");
        let mark: Value = serde_json::from_str(&j.mark).unwrap();
        assert_eq!(mark, json!({"type": "mark", "button": 0, "input": true}));
        let j = shot_job(&json!({"type": "screen", "page": "form", "init": {"type": "init"}, "mark": {"type": "mark", "fields": ["a"]}, "width": 640, "height": 300}), Path::new("b.png")).unwrap();
        assert_eq!((j.page.as_str(), j.width, j.height), ("form.html?shot", 640, 300 + TITLE_BAR));
        assert!(shot_job(&json!({"type": "screen", "page": "x"}), Path::new("c.png")).is_none());
    }
}
