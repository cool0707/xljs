//! The UI of a test run: answers from the case's queues, shows nothing and
//! writes everything the script shows to the transcript.

use std::collections::{HashMap, VecDeque};
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Map, Value, json};

use crate::Result;
use crate::ui::mock::LogSink;
use crate::ui::*;

/// What a run of a script showed and wrote, in order: one JSON object per
/// event (see docs/design/test-design.md).
#[derive(Clone, Default)]
pub struct Transcript(Arc<Mutex<Vec<Value>>>);

impl Transcript {
    pub fn push(&self, v: Value) {
        self.0.lock().unwrap().push(v);
    }

    pub fn take(&self) -> Vec<Value> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }

    /// Something that makes the run useless as a test.
    pub fn problem(&self, kind: &str, mut more: Value) {
        let mut m = Map::new();
        m.insert("type".into(), "problem".into());
        m.insert("kind".into(), kind.into());
        if let Some(o) = more.as_object_mut() {
            m.append(o);
        }
        self.push(Value::Object(m));
    }
}

/// The script's standard output or error: every line is an event.
pub struct Lines {
    t: Transcript,
    kind: &'static str,
    buf: Vec<u8>,
}

impl Lines {
    pub fn new(t: &Transcript, kind: &'static str) -> Arc<Mutex<Lines>> {
        Arc::new(Mutex::new(Lines { t: t.clone(), kind, buf: Vec::new() }))
    }

    fn line(&self, l: &[u8]) {
        let text = String::from_utf8_lossy(l);
        self.t.push(json!({ "type": self.kind, "text": text.trim_end_matches('\r') }));
    }

    /// Writes out a last line without a newline.
    pub fn finish(&mut self) {
        if !self.buf.is_empty() {
            let b = std::mem::take(&mut self.buf);
            self.line(&b);
        }
    }
}

impl Write for Lines {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.buf.extend_from_slice(data);
        while let Some(i) = self.buf.iter().position(|b| *b == b'\n') {
            let rest = self.buf.split_off(i + 1);
            let line = std::mem::replace(&mut self.buf, rest);
            self.line(&line[..i]);
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The next answer of a queue.
enum Next {
    Answer(Value),
    /// A recorded null: the user cancelled.
    Cancel,
    /// Nothing recorded.
    None,
}

pub struct Replay {
    t: Transcript,
    answers: Mutex<HashMap<String, VecDeque<Value>>>,
    abort: Arc<AtomicBool>,
}

impl Replay {
    pub fn new(t: &Transcript, answers: &super::Answers, abort: Arc<AtomicBool>) -> Replay {
        let answers = answers.iter().map(|(k, v)| (k.clone(), v.iter().cloned().collect())).collect();
        Replay { t: t.clone(), answers: Mutex::new(answers), abort }
    }

    fn next(&self, kind: &str) -> Next {
        match self.answers.lock().unwrap().get_mut(kind).and_then(|q| q.pop_front()) {
            Some(Value::Null) => Next::Cancel,
            Some(v) => Next::Answer(v),
            None => Next::None,
        }
    }

    /// A question without a recorded answer: the script stops here.
    fn unanswered(&self, kind: &str, text: &str) {
        self.t.problem("unanswered", json!({ "what": kind, "text": text }));
        self.abort.store(true, Ordering::SeqCst);
    }

    fn invalid(&self, kind: &str, answer: &Value) {
        self.t.problem("invalidAnswer", json!({ "what": kind, "answer": answer }));
    }

    /// Tells the answers nothing asked for (at the end of the run). A
    /// variant whose values do not pass the form's checks leaves the
    /// answers of the later questions: it is part of the run, not a mistake.
    pub fn unused(&self) {
        let a = self.answers.lock().unwrap();
        let mut kinds: Vec<&String> = a.keys().collect();
        kinds.sort();
        for k in kinds {
            if !a[k].is_empty() {
                self.t.push(json!({ "type": "unused", "what": k, "count": a[k].len() }));
            }
        }
    }

    fn log_sink(&self) -> LogSink {
        let t = self.t.clone();
        Arc::new(move |s: &str| t.push(json!({ "type": "window", "text": s.strip_prefix("[mock] ").unwrap_or(s) })))
    }
}

fn options(list: &[FormOption]) -> Value {
    Value::Array(list.iter().map(|o| if o.label.is_empty() || o.label == o.value { json!(o.value) } else { json!([o.value, o.label]) }).collect())
}

fn field_event(f: &FormField) -> Value {
    let mut m = Map::new();
    m.insert("type".into(), "field".into());
    m.insert("name".into(), f.name.clone().into());
    m.insert("label".into(), f.label.clone().into());
    m.insert("kind".into(), f.kind.clone().into());
    if f.required {
        m.insert("required".into(), true.into());
    }
    if f.custom {
        m.insert("custom".into(), true.into());
    }
    for (k, v) in [("placeholder", &f.placeholder), ("help", &f.help), ("tab", &f.tab), ("group", &f.group), ("groupHelp", &f.group_help), ("select", &f.select)] {
        if !v.is_empty() {
            m.insert(k.into(), v.clone().into());
        }
    }
    if !f.options.is_empty() {
        m.insert("options".into(), options(&f.options));
    }
    if let Some(x) = f.min {
        m.insert("min".into(), json!(x));
    }
    if let Some(x) = f.max {
        m.insert("max".into(), json!(x));
    }
    if f.max_length > 0 {
        m.insert("maxLength".into(), f.max_length.into());
    }
    if !f.accept.is_empty() {
        m.insert("accept".into(), json!(f.accept));
    }
    let empty = match &f.value {
        Value::Null => true,
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        _ => false,
    };
    if !empty {
        m.insert("value".into(), f.value.clone());
    }
    if !f.columns.is_empty() {
        m.insert("columns".into(), json!(f.columns));
    }
    if !f.editable.is_empty() {
        let e: Vec<Value> = f.editable.iter().map(|c| {
            let mut x = Map::new();
            x.insert("column".into(), c.index.into());
            x.insert("kind".into(), c.kind.clone().into());
            if !c.options.is_empty() {
                x.insert("options".into(), options(&c.options));
            }
            Value::Object(x)
        }).collect();
        m.insert("editable".into(), e.into());
    }
    Value::Object(m)
}

/// The name in names closest to name, when it is close enough to be a
/// typing mistake.
pub(crate) fn closest<'a>(name: &str, names: impl IntoIterator<Item = &'a String>) -> Option<&'a String> {
    fn distance(a: &[char], b: &[char]) -> usize {
        let mut row: Vec<usize> = (0..=b.len()).collect();
        for (i, ca) in a.iter().enumerate() {
            let mut prev = row[0];
            row[0] = i + 1;
            for (j, cb) in b.iter().enumerate() {
                let cur = row[j + 1];
                row[j + 1] = if ca == cb { prev } else { 1 + prev.min(cur).min(row[j]) };
                prev = cur;
            }
        }
        row[b.len()]
    }
    let a: Vec<char> = name.to_lowercase().chars().collect();
    names
        .into_iter()
        .map(|n| (distance(&a, &n.to_lowercase().chars().collect::<Vec<_>>()), n))
        .filter(|(d, n)| *d <= (a.len().max(n.chars().count()) / 2).max(1))
        .min_by_key(|(d, _)| *d)
        .map(|(_, n)| n)
}

impl Ui for Replay {
    /// The prelude treats the run like one with the mock UI.
    fn name(&self) -> String {
        "mock".into()
    }

    fn alert(&self, _title: &str, message: &str) {
        self.t.push(json!({ "type": "alert", "text": message }));
    }

    fn confirm(&self, _title: &str, message: &str) -> bool {
        let v = match self.next("confirm") {
            Next::Answer(Value::Bool(b)) => b,
            Next::Answer(v) => {
                self.invalid("confirm", &v);
                false
            }
            Next::Cancel => false,
            Next::None => {
                self.unanswered("confirm", message);
                return false;
            }
        };
        self.t.push(json!({ "type": "confirm", "text": message, "answer": v }));
        v
    }

    fn prompt(&self, _title: &str, message: &str, def: &str) -> Option<String> {
        let v = match self.next("prompt") {
            Next::Answer(Value::String(s)) => Some(s),
            Next::Answer(v) => {
                self.invalid("prompt", &v);
                None
            }
            Next::Cancel => None,
            Next::None => {
                self.unanswered("prompt", message);
                return None;
            }
        };
        let mut e = json!({ "type": "prompt", "text": message });
        if !def.is_empty() {
            e["default"] = def.into();
        }
        e["answer"] = json!(v);
        self.t.push(e);
        v
    }

    fn open_files(&self, o: &PickerOptions) -> Vec<String> {
        let v: Vec<String> = match self.next("showOpenFilePicker") {
            Next::Answer(v) => match serde_json::from_value(v.clone()) {
                Ok(list) => list,
                Err(_) => {
                    self.invalid("showOpenFilePicker", &v);
                    Vec::new()
                }
            },
            Next::Cancel => Vec::new(),
            Next::None => {
                self.unanswered("showOpenFilePicker", &o.title);
                return Vec::new();
            }
        };
        self.t.push(json!({ "type": "openFiles", "multiple": o.multiple, "answer": v }));
        v
    }

    fn save_file(&self, o: &PickerOptions) -> Option<String> {
        let v = match self.next("showSaveFilePicker") {
            Next::Answer(Value::String(s)) => Some(s),
            Next::Answer(v) => {
                self.invalid("showSaveFilePicker", &v);
                None
            }
            Next::Cancel => None,
            Next::None => {
                self.unanswered("showSaveFilePicker", &o.suggested_name);
                return None;
            }
        };
        self.t.push(json!({ "type": "saveFile", "suggestedName": o.suggested_name, "answer": v }));
        v
    }

    fn pick_directory(&self, o: &PickerOptions) -> Option<String> {
        let v = match self.next("showDirectoryPicker") {
            Next::Answer(Value::String(s)) => Some(s),
            Next::Answer(v) => {
                self.invalid("showDirectoryPicker", &v);
                None
            }
            Next::Cancel => None,
            Next::None => {
                self.unanswered("showDirectoryPicker", &o.title);
                return None;
            }
        };
        self.t.push(json!({ "type": "directory", "answer": v }));
        v
    }

    fn show_error(&self, _title: &str, message: &str) {
        self.t.push(json!({ "type": "error", "text": message }));
    }

    fn notify(&self, title: &str, message: &str) {
        self.t.push(json!({ "type": "notify", "title": title, "text": message }));
    }

    fn credentials(&self, p: &crate::net::auth::LoginPrompt) -> Option<crate::net::auth::Login> {
        #[derive(serde::Deserialize)]
        struct A {
            user: String,
            password: String,
        }
        let a: Option<A> = match self.next("credentials") {
            Next::Answer(v) => match serde_json::from_value(v.clone()) {
                Ok(a) => Some(a),
                Err(_) => {
                    self.invalid("credentials", &v);
                    None
                }
            },
            Next::Cancel => None,
            Next::None => {
                self.unanswered("credentials", &p.realm);
                return None;
            }
        };
        self.t.push(json!({ "type": "credentials", "realm": p.realm, "proxy": p.proxy, "failed": p.failed, "user": a.as_ref().map(|a| a.user.clone()) }));
        a.map(|a| crate::net::auth::Login { user: a.user, password: a.password })
    }

    fn print_pdf(&self, job: PdfJob, done: PdfDone) {
        // Writes nothing (there is no browser to print with).
        let pages = job.html.matches("class=\"xljs-page\"").count();
        self.t.push(json!({ "type": "pdf", "file": job.path, "pages": pages }));
        done(Ok(()))
    }

    fn progress(&self, o: &ProgressOptions) -> Arc<dyn Progress> {
        let cancel_after = match self.next("progress") {
            Next::Answer(v) => v.get("cancelAfter").and_then(Value::as_i64).unwrap_or(0),
            _ => 0,
        };
        self.t.push(json!({ "type": "progress", "title": o.title, "total": o.total }));
        progress_opened();
        Arc::new(ReplayProgress { t: self.t.clone(), cancel_after, st: Mutex::new((0, 0, o.total, false)) })
    }

    fn show_in_folder(&self, paths: &[String]) -> Result<()> {
        self.t.push(json!({ "type": "showInFolder", "files": paths }));
        shell::check_paths(paths)
    }

    fn open_path(&self, target: &str) -> Result<()> {
        self.t.push(json!({ "type": "openPath", "target": target }));
        Ok(())
    }

    fn supports_windows(&self) -> bool {
        false
    }

    fn form(&self, o: &FormOptions) -> Option<Map<String, Value>> {
        let mut e = json!({ "type": "form", "title": o.title });
        if !o.description.is_empty() {
            e["description"] = o.description.clone().into();
        }
        if !o.submit.is_empty() {
            e["submit"] = o.submit.clone().into();
        }
        if !o.buttons.is_empty() {
            e["buttons"] = json!(o.buttons);
        }
        self.t.push(e);
        for f in &o.fields {
            self.t.push(field_event(f));
            for r in &f.rows {
                self.t.push(json!({ "type": "row", "field": f.name, "cells": r }));
            }
        }
        if !o.error.is_empty() {
            self.t.push(json!({ "type": "formError", "text": o.error }));
        }
        for f in &o.fields {
            if !f.error.is_empty() {
                self.t.push(json!({ "type": "formError", "field": f.name, "text": f.error }));
            }
            for c in &f.cell_errors {
                self.t.push(json!({ "type": "formError", "field": f.name, "row": c.row, "column": c.col, "text": c.message }));
            }
        }
        let v = match self.next("form") {
            Next::Answer(Value::Object(m)) => m,
            Next::Answer(v) => {
                self.invalid("form", &v);
                self.t.push(json!({ "type": "close" }));
                return None;
            }
            Next::Cancel | Next::None => {
                self.t.push(json!({ "type": "close" }));
                return None;
            }
        };
        if !o.names.is_empty() {
            for k in v.keys().filter(|k| !k.starts_with('$') && !o.names.contains(k)) {
                let mut more = json!({ "field": k });
                if let Some(n) = closest(k, &o.names) {
                    more["hint"] = n.clone().into();
                }
                self.t.problem("unknownField", more);
            }
        }
        self.t.push(json!({ "type": "submit", "values": v }));
        Some(v)
    }

    fn table(&self, o: &TableOptions) -> Option<Vec<i64>> {
        let mut e = json!({ "type": "table", "title": o.title, "columns": o.columns });
        if o.select != SELECT_NONE {
            e["select"] = o.select.clone().into();
        }
        self.t.push(e);
        for r in &o.rows {
            self.t.push(json!({ "type": "row", "cells": r }));
        }
        if o.select == SELECT_NONE {
            return Some(Vec::new());
        }
        let mut v: Vec<i64> = match self.next("table") {
            Next::Answer(v) => match serde_json::from_value(v.clone()) {
                Ok(list) => list,
                Err(_) => {
                    self.invalid("table", &v);
                    self.t.push(json!({ "type": "close" }));
                    return None;
                }
            },
            Next::Cancel | Next::None => {
                self.t.push(json!({ "type": "close" }));
                return None;
            }
        };
        if o.select == SELECT_SINGLE {
            v.truncate(1);
        }
        self.t.push(json!({ "type": "select", "rows": v }));
        Some(v)
    }

    /// A window plays the next "app" scenario like the mock UI; without
    /// one it is closed at once.
    fn open_window(&self, o: WindowOptions) -> Result<Arc<dyn Window>> {
        self.t.push(json!({ "type": "window", "text": format!("open({}, {})", quote(&o.title), quote(&o.url)) }));
        if let Next::Answer(v) = self.next("app") {
            match serde_json::from_value::<Vec<AppStep>>(v.clone()) {
                Ok(steps) => return Ok(mock_app::AppWindow::start(Some(self.log_sink()), o, steps)),
                Err(_) => self.invalid("app", &v),
            }
        }
        let w = Arc::new(ReplayWindow { t: self.t.clone(), o, closed: AtomicBool::new(false) });
        w.close();
        Ok(w)
    }
}

/// (updates, done, total, closed)
struct ReplayProgress {
    t: Transcript,
    cancel_after: i64,
    st: Mutex<(i64, i64, i64, bool)>,
}

impl Progress for ReplayProgress {
    fn update(&self, done: i64, total: i64, _message: &str) {
        let mut s = self.st.lock().unwrap();
        s.0 += 1;
        s.1 = done;
        if total > 0 {
            s.2 = total;
        }
    }

    fn cancelled(&self) -> bool {
        let s = self.st.lock().unwrap();
        self.cancel_after > 0 && s.0 >= self.cancel_after
    }

    fn close(&self) {
        let mut s = self.st.lock().unwrap();
        if s.3 {
            return;
        }
        s.3 = true;
        progress_closed();
        self.t.push(json!({ "type": "progressEnd", "done": s.1, "total": s.2 }));
    }
}

struct ReplayWindow {
    t: Transcript,
    o: WindowOptions,
    closed: AtomicBool,
}

impl Window for ReplayWindow {
    fn post(&self, _msg: String) {}

    fn close(&self) {
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        self.t.push(json!({ "type": "window", "text": format!("closed({})", quote(&self.o.title)) }));
        if let Some(f) = &self.o.on_closed {
            f();
        }
    }
}
