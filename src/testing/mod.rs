//! Tests of scripts (`xljs test`): a case is a recorded way of using a
//! script (answers to its dialogs and forms, input files); a run replays it
//! without showing anything and compares what the script showed and wrote
//! with what was approved before. See docs/design/test-design.md.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use serde_json::{Map, Value};

use crate::{Error, Result, err, t};

mod dump;
mod record;
pub mod replay;
mod run;

pub use record::Recorder;
pub use run::run;

/// The folder of the cases, in the folder of the script.
pub const TESTS_DIR: &str = ".xljs/tests";

/// The name of the expected transcript of a case's own answers.
const BASE: &str = "_base";

/// How long a script may run JavaScript without waiting.
pub const DEFAULT_LIMIT: Duration = Duration::from_secs(30);

/// The kinds of answers (the keys of the mock UI's answers file).
const KINDS: &[&str] =
    &["confirm", "prompt", "showOpenFilePicker", "showSaveFilePicker", "showDirectoryPicker", "form", "table", "progress", "app", "credentials"];

/// Kind -> the answers in the order they are given.
pub type Answers = BTreeMap<String, Vec<Value>>;

/// A test case (`<script folder>/.xljs/tests/<name>/case.json`).
#[derive(Debug, Clone, Default)]
pub struct Case {
    pub name: String,
    /// The folder of the case and that of the script.
    pub dir: PathBuf,
    pub root: PathBuf,
    /// The script, relative to root.
    pub script: String,
    /// "" = the real time.
    pub now: String,
    pub user_name: String,
    pub computer_name: String,
    pub args: Vec<String>,
    pub answers: Answers,
    pub variants: Vec<Variant>,
}

/// A variation of a case: what differs from its answers.
#[derive(Debug, Clone, Default)]
pub struct Variant {
    pub name: String,
    pub now: Option<String>,
    pub args: Option<Vec<String>>,
    pub answers: Answers,
}

/// What one run uses: the case's own answers (variant "") or those of a
/// variant laid over them.
#[derive(Debug, Clone, Default)]
pub struct Plan {
    pub variant: String,
    pub now: String,
    pub args: Vec<String>,
    pub answers: Answers,
}

fn unknown_key(file: &str, key: &str, allowed: &[&str]) -> Error {
    let names: Vec<String> = allowed.iter().map(|s| s.to_string()).collect();
    match replay::closest(key, &names) {
        Some(n) => err!("test.unknownKeyHint", file, key, n),
        None => err!("test.unknownKey", file, key, allowed.join(", ")),
    }
}

fn bad_type(file: &str, key: &str, want: &str) -> Error {
    err!("test.badType", file, key, want)
}

/// "2026-04-10T09:00:00" for a date, or a date and a time (also with a
/// space, without seconds).
fn parse_now(file: &str, s: &str) -> Result<String> {
    use chrono::{NaiveDate, NaiveDateTime};
    let x = s.trim().replace(' ', "T");
    let t = NaiveDateTime::parse_from_str(&x, "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(&x, "%Y-%m-%dT%H:%M"))
        .or_else(|_| NaiveDate::parse_from_str(&x, "%Y-%m-%d").map(|d| d.and_hms_opt(0, 0, 0).unwrap()))
        .map_err(|_| err!("test.badNow", file, s))?;
    Ok(t.format("%Y-%m-%dT%H:%M:%S").to_string())
}

fn string(file: &str, key: &str, v: &Value) -> Result<String> {
    v.as_str().map(str::to_string).ok_or_else(|| bad_type(file, key, "\"...\""))
}

fn strings(file: &str, key: &str, v: &Value) -> Result<Vec<String>> {
    let list = v.as_array().ok_or_else(|| bad_type(file, key, "[\"...\"]"))?;
    list.iter().map(|x| string(file, key, x).map_err(|_| bad_type(file, key, "[\"...\"]"))).collect()
}

fn answer_list(file: &str, key: &str, v: &Value) -> Result<Vec<Value>> {
    v.as_array().cloned().ok_or_else(|| bad_type(file, key, "[...]"))
}

fn parse_answers(file: &str, v: &Value) -> Result<Answers> {
    let m = v.as_object().ok_or_else(|| bad_type(file, "answers", "{...}"))?;
    let mut a = Answers::new();
    for (k, x) in m {
        if !KINDS.contains(&k.as_str()) {
            return Err(unknown_key(file, k, KINDS));
        }
        a.insert(k.clone(), answer_list(file, k, x)?);
    }
    Ok(a)
}

/// Whether a case or variant cannot have the name (it names a folder or a file).
fn bad_name(name: &str) -> bool {
    name.is_empty() || name.starts_with(['_', '.']) || name.ends_with(['.', ' ']) || name.chars().any(|c| c.is_control() || "\\/:*?\"<>|".contains(c))
}

fn parse_variant(file: &str, name: &str, v: &Value) -> Result<Variant> {
    if bad_name(name) {
        return Err(err!("test.badVariantName", file, name));
    }
    let m = v.as_object().ok_or_else(|| bad_type(file, name, "{...}"))?;
    let mut x = Variant { name: name.to_string(), ..Default::default() };
    for (k, val) in m {
        match k.as_str() {
            "now" => x.now = Some(parse_now(file, &string(file, k, val)?)?),
            "args" => x.args = Some(strings(file, k, val)?),
            k if KINDS.contains(&k) => {
                x.answers.insert(k.to_string(), answer_list(file, k, val)?);
            }
            _ => {
                let mut allowed = vec!["now", "args"];
                allowed.extend_from_slice(KINDS);
                return Err(unknown_key(file, k, &allowed));
            }
        }
    }
    Ok(x)
}

/// Reads and checks the case in dir (root: the folder of the scripts).
/// Mistakes are errors that tell where: nothing is ignored silently.
pub fn load(root: &Path, dir: &Path) -> Result<Case> {
    let path = dir.join("case.json");
    let file = path.display().to_string();
    let text = std::fs::read_to_string(&path).map_err(|e| Error::path("open", file.clone(), e))?;
    parse(root, dir, &file, &text)
}

fn parse(root: &Path, dir: &Path, file: &str, text: &str) -> Result<Case> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let v: Value = serde_json::from_str(text).map_err(|e| {
        let msg = e.to_string();
        let msg = msg.split(" at line ").next().unwrap_or(&msg).to_string();
        err!("test.badJson", file, e.line(), e.column(), msg)
    })?;
    let m = v.as_object().ok_or_else(|| bad_type(file, "case.json", "{...}"))?;
    let mut c = Case {
        name: dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        dir: dir.to_path_buf(),
        root: root.to_path_buf(),
        ..Default::default()
    };
    for (k, val) in m {
        match k.as_str() {
            "script" => c.script = string(file, k, val)?,
            "now" => c.now = parse_now(file, &string(file, k, val)?)?,
            "args" => c.args = strings(file, k, val)?,
            "user" => {
                let u = val.as_object().ok_or_else(|| bad_type(file, k, "{...}"))?;
                for (uk, uv) in u {
                    match uk.as_str() {
                        "userName" => c.user_name = string(file, uk, uv)?,
                        "computerName" => c.computer_name = string(file, uk, uv)?,
                        _ => return Err(unknown_key(file, uk, &["userName", "computerName"])),
                    }
                }
            }
            "answers" => c.answers = parse_answers(file, val)?,
            "variants" => {
                let vs = val.as_object().ok_or_else(|| bad_type(file, k, "{...}"))?;
                for (name, x) in vs {
                    c.variants.push(parse_variant(file, name, x)?);
                }
            }
            _ => return Err(unknown_key(file, k, &["script", "now", "user", "args", "answers", "variants"])),
        }
    }
    if c.script.is_empty() {
        return Err(err!("test.noScript", file));
    }
    if !root.join(&c.script).is_file() {
        return Err(err!("test.scriptMissing", file, c.script));
    }
    Ok(c)
}

/// Lays the answers of a variant over those of the case: per kind, the
/// i-th answer over the i-th one (objects key by key, anything else
/// replaces it); more answers are added.
fn overlay(base: &Answers, over: &Answers) -> Answers {
    let mut out = base.clone();
    for (kind, list) in over {
        let to = out.entry(kind.clone()).or_default();
        for (i, v) in list.iter().enumerate() {
            match (to.get_mut(i), v) {
                (Some(Value::Object(a)), Value::Object(b)) => {
                    for (k, x) in b {
                        a.insert(k.clone(), x.clone());
                    }
                }
                (Some(a), _) => *a = v.clone(),
                (None, _) => to.push(v.clone()),
            }
        }
    }
    out
}

impl Case {
    /// The runs of the case: its own answers, then each variant.
    pub fn plans(&self) -> Vec<Plan> {
        let mut v = vec![Plan { variant: String::new(), now: self.now.clone(), args: self.args.clone(), answers: self.answers.clone() }];
        for x in &self.variants {
            v.push(Plan {
                variant: x.name.clone(),
                now: x.now.clone().unwrap_or_else(|| self.now.clone()),
                args: x.args.clone().unwrap_or_else(|| self.args.clone()),
                answers: overlay(&self.answers, &x.answers),
            });
        }
        v
    }

    /// The file of the approved transcript of a variant ("" = the case's own answers).
    pub fn expected_path(&self, variant: &str) -> PathBuf {
        self.dir.join("expected").join(format!("{}.jsonl", if variant.is_empty() { BASE } else { variant }))
    }
}

/// The cases of the scripts in root, by name. A case that cannot be read
/// is an error.
pub fn cases(root: &Path) -> Result<Vec<Case>> {
    let dir = root.join(TESTS_DIR);
    let mut names: Vec<PathBuf> = match std::fs::read_dir(&dir) {
        Ok(rd) => rd.flatten().map(|e| e.path()).filter(|p| p.join("case.json").is_file()).collect(),
        Err(_) => Vec::new(),
    };
    names.sort();
    names.iter().map(|d| load(root, d)).collect()
}

// ---- transcripts ----

fn quote(v: &Value) -> String {
    match v {
        Value::String(s) => crate::ui::quote(s),
        other => other.to_string(),
    }
}

fn get<'a>(e: &'a Value, k: &str) -> &'a Value {
    e.get(k).unwrap_or(&Value::Null)
}

fn text<'a>(e: &'a Value, k: &str) -> &'a str {
    get(e, k).as_str().unwrap_or("")
}

/// The other properties of an event, as JSON ("" for none).
fn rest(e: &Value, shown: &[&str]) -> String {
    let m: Map<String, Value> = e.as_object().map(|m| m.iter().filter(|(k, _)| !shown.contains(&k.as_str())).map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default();
    if m.is_empty() { String::new() } else { format!(" {}", Value::Object(m)) }
}

/// An event as a line for people (the same in every language, like the
/// log of the mock UI).
pub fn describe(e: &Value) -> String {
    let kind = text(e, "type");
    match kind {
        "alert" | "error" => format!("{kind} {}", quote(get(e, "text"))),
        "notify" => format!("notify {} {}", quote(get(e, "title")), quote(get(e, "text"))),
        "confirm" | "prompt" => format!("{kind} {} -> {}", quote(get(e, "text")), quote(get(e, "answer"))),
        "openFiles" => format!("showOpenFilePicker -> {}", get(e, "answer")),
        "saveFile" => format!("showSaveFilePicker {} -> {}", quote(get(e, "suggestedName")), quote(get(e, "answer"))),
        "directory" => format!("showDirectoryPicker -> {}", quote(get(e, "answer"))),
        "form" => format!("form {}{}", quote(get(e, "title")), rest(e, &["type", "title"])),
        "field" => {
            let value = match e.get("value") {
                Some(v) => format!(" = {}", quote(v)),
                None => String::new(),
            };
            format!("  field {} ({}) {}{value}{}", text(e, "name"), text(e, "kind"), quote(get(e, "label")), rest(e, &["type", "name", "kind", "label", "value"]))
        }
        "formError" => {
            let at = match (e.get("field"), e.get("row")) {
                (Some(f), Some(r)) => format!("{} [{r}, {}]: ", f.as_str().unwrap_or(""), get(e, "column")),
                (Some(f), None) => format!("{}: ", f.as_str().unwrap_or("")),
                _ => String::new(),
            };
            format!("  form error: {at}{}", text(e, "text"))
        }
        "submit" => format!("  submit {}", get(e, "values")),
        "close" => "  close".into(),
        "table" => format!("table {} {}{}", quote(get(e, "title")), get(e, "columns"), rest(e, &["type", "title", "columns"])),
        "row" => format!("    row {}", get(e, "cells")),
        "select" => format!("  select {}", get(e, "rows")),
        "out" => format!("out: {}", text(e, "text")),
        "err" => format!("err: {}", text(e, "text")),
        "window" => format!("window: {}", text(e, "text")),
        "end" => match e.get("error") {
            Some(m) => format!("end {}: {}", get(e, "code"), m.as_str().unwrap_or("")),
            None => format!("end {}", get(e, "code")),
        },
        "file" => format!("file {} ({}){}", text(e, "file"), text(e, "kind"), rest(e, &["type", "file", "kind"])),
        "removed" => format!("removed {}", text(e, "file")),
        "cell" => {
            let formula = match e.get("formula") {
                Some(f) => format!("  (={})", f.as_str().unwrap_or("")),
                None => String::new(),
            };
            format!("  {} {}!{} = {}{formula}", text(e, "file"), text(e, "sheet"), text(e, "cell"), quote(get(e, "text")))
        }
        "line" => format!("  {}: {}", text(e, "file"), text(e, "text")),
        "unused" => format!("unused answers: {} x{}", text(e, "what"), get(e, "count")),
        "problem" => format!("problem: {}", problem_text(e)),
        _ => format!("{kind}{}", rest(e, &["type"])),
    }
}

/// What is wrong with a run (a "problem" event), in the user's language.
pub fn problem_text(e: &Value) -> String {
    match text(e, "kind") {
        "unanswered" => t!("test.unanswered", text(e, "what"), quote(get(e, "text"))),
        "unknownField" => match e.get("hint") {
            Some(h) => t!("test.unknownFieldHint", text(e, "field"), h.as_str().unwrap_or("")),
            None => t!("test.unknownField", text(e, "field")),
        },
        "invalidAnswer" => t!("test.invalidAnswer", text(e, "what"), get(e, "answer")),
        "timeout" => t!("test.timeout", get(e, "seconds")),
        other => other.to_string(),
    }
}

/// The lines of the difference between two transcripts: "- " what was
/// expected, "+ " what the run gave, with up to two lines before each.
fn difference(expected: &[String], actual: &[String]) -> Vec<String> {
    const CONTEXT: usize = 2;
    let show = |l: &String| serde_json::from_str::<Value>(l).map(|v| describe(&v)).unwrap_or_else(|_| l.clone());
    let (a, b): (Vec<&str>, Vec<&str>) = (expected.iter().map(String::as_str).collect(), actual.iter().map(String::as_str).collect());
    let Some(edits) = crate::dev::notify::edits(&a, &b) else {
        return vec![t!("test.tooDifferent")];
    };
    let mut out = Vec::new();
    let mut shown = 0; // edits[..shown] are told
    for (i, (op, at)) in edits.iter().enumerate() {
        if *op == 0 {
            continue;
        }
        let from = i.saturating_sub(CONTEXT).max(shown);
        if from > shown && !out.is_empty() {
            out.push("  ...".into());
        }
        for (op, at) in &edits[from..i] {
            if *op == 0 {
                out.push(format!("  {}", show(&expected[*at])));
            }
        }
        out.push(if *op < 0 { format!("- {}", show(&expected[*at])) } else { format!("+ {}", show(&actual[*at])) });
        shown = i + 1;
    }
    out
}

// ---- running the cases ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// The same as approved.
    Pass,
    /// Differs from what was approved.
    Fail,
    /// Nothing approved yet.
    New,
    /// The run is not a valid test (see the "problem" events).
    Problem,
}

/// The result of one run.
#[derive(Debug, Clone, Serialize)]
pub struct Outcome {
    pub case: String,
    /// "" = the case's own answers.
    pub variant: String,
    pub status: Status,
    /// The run was saved as the expected one (Report::approve).
    pub approved: bool,
    pub problems: Vec<String>,
    /// For Fail: the difference. For New and Problem: the whole run.
    pub lines: Vec<String>,
    /// The events of the run.
    pub transcript: Vec<Value>,
}

impl Outcome {
    /// "case" or "case/variant".
    pub fn name(&self) -> String {
        if self.variant.is_empty() { self.case.clone() } else { format!("{}/{}", self.case, self.variant) }
    }

    pub fn ok(&self) -> bool {
        self.approved || matches!(self.status, Status::Pass | Status::New)
    }
}

#[derive(Default)]
pub struct RunOptions {
    /// Saves the runs that are new or differ as the expected ones.
    pub approve: bool,
    /// Only this case ("case") or run ("case/variant", "case/" for the case's own answers).
    pub only: String,
    pub limit: Option<Duration>,
    pub version: String,
}

fn to_lines(events: &[Value]) -> Vec<String> {
    events.iter().map(Value::to_string).collect()
}

fn write_expected(path: &Path, lines: &[String]) -> Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| Error::path("mkdir", d.display().to_string(), e))?;
    }
    let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
    std::fs::write(path, text).map_err(|e| Error::path("open", path.display().to_string(), e))
}

/// Runs one plan of a case and compares it with what was approved.
pub fn check(case: &Case, plan: &Plan, o: &RunOptions) -> Result<Outcome> {
    let events = run(case, plan, o.limit.unwrap_or(DEFAULT_LIMIT), &o.version)?;
    let actual = to_lines(&events);
    let problems: Vec<String> = events.iter().filter(|e| text(e, "type") == "problem").map(problem_text).collect();
    let path = case.expected_path(&plan.variant);
    let expected: Option<Vec<String>> = std::fs::read_to_string(&path).ok().map(|s| s.lines().filter(|l| !l.trim().is_empty()).map(str::to_string).collect());
    let whole = || events.iter().map(describe).collect();
    let (status, lines) = if !problems.is_empty() {
        (Status::Problem, whole())
    } else {
        match &expected {
            None => (Status::New, whole()),
            Some(e) if *e == actual => (Status::Pass, Vec::new()),
            Some(e) => (Status::Fail, difference(e, &actual)),
        }
    };
    let mut out = Outcome { case: case.name.clone(), variant: plan.variant.clone(), status, approved: false, problems, lines, transcript: events };
    if o.approve && matches!(status, Status::New | Status::Fail) {
        write_expected(&path, &actual)?;
        out.approved = true;
    }
    Ok(out)
}

/// The folder of the scripts and, for a script, its path in it.
pub fn target(path: &str) -> Result<(PathBuf, Option<String>)> {
    let p = std::path::absolute(path).unwrap_or_else(|_| PathBuf::from(path));
    let meta = std::fs::metadata(&p).map_err(|e| Error::path("open", path, e))?;
    if meta.is_dir() {
        return Ok((p, None));
    }
    let root = p.parent().map(Path::to_path_buf).unwrap_or_default();
    Ok((root, p.file_name().map(|n| n.to_string_lossy().into_owned())))
}

fn same_script(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.replace('\\', "/").trim_start_matches("./").to_string();
    norm(a) == norm(b)
}

/// Runs the cases of the script or folder at path, each outcome going to
/// each as soon as it is there.
pub fn run_all(path: &str, o: &RunOptions, mut each: impl FnMut(&Outcome)) -> Result<Vec<Outcome>> {
    let (root, script) = target(path)?;
    let all = cases(&root)?;
    if all.is_empty() {
        return Err(err!("test.noCases", root.join(TESTS_DIR).display()));
    }
    let (only_case, only_variant) = match o.only.split_once('/') {
        Some((c, v)) => (c, Some(v)),
        None => (o.only.as_str(), None),
    };
    let mut out = Vec::new();
    for c in &all {
        if script.as_ref().is_some_and(|s| !same_script(s, &c.script)) || (!only_case.is_empty() && c.name != only_case) {
            continue;
        }
        for p in c.plans() {
            if only_variant.is_some_and(|v| v != p.variant) {
                continue;
            }
            let r = check(c, &p, o)?;
            each(&r);
            out.push(r);
        }
    }
    if out.is_empty() {
        return Err(if o.only.is_empty() { err!("test.noCases", root.join(TESTS_DIR).display()) } else { err!("test.noSuchCase", o.only) });
    }
    Ok(out)
}

/// The folder and the case.json of a case to create for a script.
fn new_dir(root: &Path, file: Option<&str>, script: &str, name: &str) -> Result<(PathBuf, PathBuf)> {
    if file.is_none() {
        return Err(err!("test.needsScript", script));
    }
    if bad_name(name) {
        return Err(err!("test.badCaseName", name));
    }
    let dir = root.join(TESTS_DIR).join(name);
    let path = dir.join("case.json");
    if path.exists() {
        return Err(err!("test.caseExists", path.display()));
    }
    Ok((dir, path))
}

/// Checks that a case of the name can be created for the script.
pub fn check_new(script: &str, name: &str) -> Result<()> {
    let (root, file) = target(script)?;
    new_dir(&root, file.as_deref(), script, name).map(|_| ())
}

/// The outcome of record.
pub struct Recorded {
    /// The case.json written.
    pub path: PathBuf,
    /// How the script ended.
    pub result: Result<i32>,
    /// The input files that could not be copied (messages).
    pub failed: Vec<String>,
}

/// Runs the script as usual with the UI u (o: how; its script, arguments
/// and UI are set here) and writes the answers the user gives as the case
/// name. now: the time of the run ("2026-04-10T09:00:00").
pub fn record(u: std::sync::Arc<dyn crate::ui::Ui>, script: &str, name: &str, args: &[String], now: &str, mut o: crate::engine::Options) -> Result<Recorded> {
    let (root, file) = target(script)?;
    let (dir, path) = new_dir(&root, file.as_deref(), script, name)?;
    let rec = std::sync::Arc::new(Recorder::new(&dir));
    // Before the script runs: it may change the files.
    let case_args = rec.args(args);
    o.script_path = script.to_string();
    o.args = args.to_vec();
    o.record = Some(rec.clone());
    let result = crate::ui::run(u, move |su| {
        o.ui = Some(su);
        crate::engine::run(o)
    });
    let failed = rec.save(&file.unwrap_or_default(), now, &case_args)?;
    Ok(Recorded { path, result, failed })
}

// ---- variants from the fields of the form ----

fn number(x: f64) -> Value {
    if x == x.trunc() && x.abs() < 1e15 { serde_json::json!(x as i64) } else { serde_json::json!(x) }
}

/// The variants worth trying for a field of a form (an event of the
/// transcript), as (name, value): the edges of what it accepts and just
/// beyond, the other choices. base is the value of the case's own run.
fn field_variants(f: &Value, base: &Value) -> Vec<(String, Value)> {
    let label = match text(f, "label") {
        "" => text(f, "name"),
        l => l,
    };
    let kind = text(f, "kind");
    let mut out = Vec::new();
    let required = f.get("required").and_then(Value::as_bool).unwrap_or(false);
    if required && !matches!(kind, "checkbox" | "table" | "note") {
        out.push((t!("test.suggestEmpty", label), if kind == "files" || kind == "tags" || kind == "checkboxes" { serde_json::json!([]) } else { "".into() }));
    }
    match kind {
        "number" => {
            let (min, max) = (f.get("min").and_then(Value::as_f64), f.get("max").and_then(Value::as_f64));
            if let Some(m) = min {
                out.push((t!("test.suggestMin", label, number(m)), number(m)));
                out.push((t!("test.suggestBelowMin", label, number(m - 1.0)), number(m - 1.0)));
            }
            if let Some(m) = max {
                out.push((t!("test.suggestMax", label, number(m)), number(m)));
                out.push((t!("test.suggestAboveMax", label, number(m + 1.0)), number(m + 1.0)));
            }
            if min.is_none() && max.is_none() {
                out.push((t!("test.suggestValue", label, 0), number(0.0)));
                out.push((t!("test.suggestValue", label, -1), number(-1.0)));
            }
        }
        "checkbox" => {
            let on = base.as_bool().unwrap_or(false);
            out.push((t!("test.suggestValue", label, !on), (!on).into()));
        }
        "select" | "radio" => {
            for o in f.get("options").and_then(Value::as_array).into_iter().flatten().take(8) {
                // "value" or ["value", "label"]
                let (v, shown) = match o {
                    Value::Array(a) => (a.first().cloned().unwrap_or_default(), a.get(1).and_then(Value::as_str).unwrap_or("").to_string()),
                    v => (v.clone(), v.as_str().unwrap_or("").to_string()),
                };
                out.push((t!("test.suggestValue", label, shown), v));
            }
        }
        _ => {}
    }
    if let Some(n) = f.get("maxLength").and_then(Value::as_u64).filter(|n| *n > 0 && *n <= 2000) {
        out.push((t!("test.suggestLength", label, n), "あ".repeat(n as usize).into()));
        out.push((t!("test.suggestLength", label, n + 1), "あ".repeat(n as usize + 1).into()));
    }
    // A number of the form's values is a string.
    let same = |a: &Value, b: &Value| a == b || quote(a).trim_matches('"') == quote(b).trim_matches('"');
    out.retain(|(_, v)| !same(v, base));
    out
}

/// Adds variants to the case named name (of the script or folder at path)
/// that try the edges of the fields of its first form, and returns their
/// names. Nothing that is there is changed: only variants are added, and
/// they are not approved.
pub fn suggest(path: &str, name: &str, o: &RunOptions) -> Result<Vec<String>> {
    let (root, _) = target(path)?;
    let case = cases(&root)?.into_iter().find(|c| c.name == name).ok_or_else(|| err!("test.noSuchCase", name))?;
    let plan = case.plans().remove(0);
    let events = run(&case, &plan, o.limit.unwrap_or(DEFAULT_LIMIT), &o.version)?;
    let fields: Vec<&Value> = events.iter().skip_while(|e| text(e, "type") != "form").skip(1).take_while(|e| matches!(text(e, "type"), "field" | "row")).filter(|e| text(e, "type") == "field").collect();
    let answer = plan.answers.get("form").and_then(|l| l.first()).cloned().unwrap_or_default();
    let file = case.dir.join("case.json");
    let source = std::fs::read_to_string(&file).map_err(|e| Error::path("open", file.display().to_string(), e))?;
    let mut doc: Value = serde_json::from_str(source.strip_prefix('\u{feff}').unwrap_or(&source))?;
    let Some(top) = doc.as_object_mut() else { return Ok(Vec::new()) };
    let variants = top.entry("variants").or_insert_with(|| serde_json::json!({}));
    let Some(variants) = variants.as_object_mut() else { return Ok(Vec::new()) };
    let mut added = Vec::new();
    for f in fields {
        let field = text(f, "name");
        let base = answer.get(field).or_else(|| f.get("value")).cloned().unwrap_or_default();
        for (label, value) in field_variants(f, &base) {
            let label: String = label.chars().map(|c| if c.is_control() || "\\/:*?\"<>|".contains(c) { '_' } else { c }).collect();
            let label = label.trim_start_matches(['_', '.']).trim_end_matches(['.', ' ']).to_string();
            if label.is_empty() || variants.contains_key(&label) {
                continue;
            }
            variants.insert(label.clone(), serde_json::json!({ "form": [{ field: value }] }));
            added.push(label);
        }
    }
    if !added.is_empty() {
        let out = serde_json::to_string_pretty(&doc).unwrap_or_default() + "\n";
        std::fs::write(&file, out).map_err(|e| Error::path("open", file.display().to_string(), e))?;
    }
    Ok(added)
}

/// Removes a variant of a case with its approved result, or (variant "")
/// the whole case with its input files.
pub fn remove(path: &str, name: &str, variant: &str) -> Result<()> {
    let (root, _) = target(path)?;
    let case = cases(&root)?.into_iter().find(|c| c.name == name).ok_or_else(|| err!("test.noSuchCase", name))?;
    if variant.is_empty() {
        return std::fs::remove_dir_all(&case.dir).map_err(|e| Error::path("remove", case.dir.display().to_string(), e));
    }
    let file = case.dir.join("case.json");
    let source = std::fs::read_to_string(&file).map_err(|e| Error::path("open", file.display().to_string(), e))?;
    let mut doc: Value = serde_json::from_str(source.strip_prefix('\u{feff}').unwrap_or(&source))?;
    let gone = doc.get_mut("variants").and_then(Value::as_object_mut).and_then(|v| v.shift_remove(variant)).is_some();
    if !gone {
        return Err(err!("test.noSuchCase", format!("{name}/{variant}")));
    }
    let out = serde_json::to_string_pretty(&doc).unwrap_or_default() + "\n";
    std::fs::write(&file, out).map_err(|e| Error::path("open", file.display().to_string(), e))?;
    let _ = std::fs::remove_file(case.expected_path(variant));
    Ok(())
}

/// Creates a case for script with an empty input folder and a case.json
/// to fill in. Returns the file.
pub fn new_case(script: &str, name: &str, today: &str) -> Result<PathBuf> {
    let (root, file) = target(script)?;
    let (dir, path) = new_dir(&root, file.as_deref(), script, name)?;
    let file = file.unwrap_or_default();
    let input = dir.join("input");
    std::fs::create_dir_all(&input).map_err(|e| Error::path("mkdir", input.display().to_string(), e))?;
    let source = std::fs::read_to_string(root.join(&file)).unwrap_or_default();
    let mut answers = Map::new();
    if source.contains("ui.form(") {
        answers.insert("form".into(), serde_json::json!([{}]));
    }
    let doc = serde_json::json!({ "script": file, "now": format!("{today}T09:00:00"), "answers": answers, "variants": {} });
    let text = serde_json::to_string_pretty(&doc).unwrap_or_default() + "\n";
    std::fs::write(&path, text).map_err(|e| Error::path("open", path.display().to_string(), e))?;
    Ok(path)
}

#[cfg(test)]
mod tests;
