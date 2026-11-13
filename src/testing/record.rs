//! Recording a test case: the script runs as usual with the real UI, and
//! the answers the user gives go to the case's case.json. The files and
//! folders the user picks are copied to the case's input folder at the
//! moment they are picked (before the script changes them), and the
//! answers name the copies.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::{Map, Value, json};

use super::Answers;
use crate::{Error, Result, err};

/// The files of a case's input folder may take this much together.
const MAX_INPUT: u64 = 200 << 20;

#[derive(Default)]
struct State {
    answers: Answers,
    /// Where the paths the user gave are in the run of the case: the
    /// absolute path -> the path relative to the work folder.
    places: Vec<(PathBuf, String)>,
    budget: u64,
    /// What could not be copied.
    failed: Vec<String>,
}

pub struct Recorder {
    /// The folder of the case.
    dir: PathBuf,
    st: Mutex<State>,
}

fn absolute(p: &str) -> PathBuf {
    std::path::absolute(p).unwrap_or_else(|_| PathBuf::from(p))
}

fn slashes(p: &Path) -> String {
    p.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/")
}

impl Recorder {
    pub fn new(dir: &Path) -> Recorder {
        Recorder { dir: dir.to_path_buf(), st: Mutex::new(State { budget: MAX_INPUT, ..Default::default() }) }
    }

    /// Adds the answer to a question of the kind (a key of the answers).
    pub fn answer(&self, kind: &str, v: Value) {
        self.st.lock().unwrap().answers.entry(kind.to_string()).or_default().push(v);
    }

    /// Takes back the last answer of the kind: the question was one a run
    /// of the case does not ask (a picker opened from a field of a form
    /// window, whose value is part of the form's answer).
    pub fn forget(&self, kind: &str) {
        let mut st = self.st.lock().unwrap();
        if let Some(list) = st.answers.get_mut(kind) {
            list.pop();
            if list.is_empty() {
                st.answers.remove(kind);
            }
        }
    }

    /// The path of a run of the case for a path the user gave. An existing
    /// file or folder is copied into the input folder (input: something
    /// the script reads); a file to write gets its name in the work folder,
    /// or its place in a folder that was copied.
    fn place(&self, path: &str, input: bool) -> String {
        if path.is_empty() {
            return String::new();
        }
        let abs = absolute(path);
        let mut st = self.st.lock().unwrap();
        for (from, to) in &st.places {
            if abs == *from {
                return to.clone();
            }
            if let Ok(rest) = abs.strip_prefix(from) {
                return format!("{to}/{}", slashes(rest));
            }
        }
        let name = abs.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "input".into());
        if !input || !abs.exists() {
            st.places.push((abs, name.clone()));
            return name;
        }
        // A name that is free in the input folder.
        let dir = self.dir.join("input");
        let (stem, ext) = match name.rsplit_once('.') {
            Some((s, e)) if !s.is_empty() && abs.is_file() => (s.to_string(), format!(".{e}")),
            _ => (name.clone(), String::new()),
        };
        let mut free = name.clone();
        let mut n = 2;
        while dir.join(&free).exists() {
            free = format!("{stem}-{n}{ext}");
            n += 1;
        }
        let to = dir.join(&free);
        let copied = (|| -> Result<()> {
            if abs.is_dir() {
                std::fs::create_dir_all(&to).map_err(|e| Error::path("mkdir", to.display().to_string(), e))?;
                return super::run::copy_tree(&abs, &to, &mut st.budget);
            }
            std::fs::create_dir_all(&dir).map_err(|e| Error::path("mkdir", dir.display().to_string(), e))?;
            let size = std::fs::metadata(&abs).map(|m| m.len()).unwrap_or(0);
            if size > st.budget {
                return Err(err!("test.tooLarge", MAX_INPUT >> 20));
            }
            st.budget -= size;
            std::fs::copy(&abs, &to).map(|_| ()).map_err(|e| Error::path("open", abs.display().to_string(), e))
        })();
        if let Err(e) = copied {
            st.failed.push(crate::i18n::error(&e));
        }
        let rel = format!("input/{free}");
        st.places.push((abs, rel.clone()));
        rel
    }

    pub fn input(&self, path: &str) -> String {
        self.place(path, true)
    }

    pub fn output(&self, path: &str) -> String {
        self.place(path, false)
    }

    /// The arguments of the script: those naming files become inputs.
    pub fn args(&self, args: &[String]) -> Vec<String> {
        args.iter().map(|a| if Path::new(a).exists() { self.input(a) } else { a.clone() }).collect()
    }

    /// Adds the answer to a form (None: closed). paths names the fields
    /// that hold paths, with their types (file, files, folder, saveFile).
    pub fn form(&self, answer: Option<Map<String, Value>>, paths: &Map<String, Value>) {
        let Some(mut a) = answer else { return self.answer("form", Value::Null) };
        for (name, kind) in paths {
            let Some(v) = a.get_mut(name) else { continue };
            let input = kind.as_str() != Some("saveFile");
            match v {
                Value::String(s) => *s = self.place(s, input),
                Value::Array(list) => {
                    for x in list {
                        if let Value::String(s) = x {
                            *s = self.place(s, input);
                        }
                    }
                }
                _ => {}
            }
        }
        self.answer("form", Value::Object(a));
    }

    /// Writes the case.json of the recording (script: relative to the
    /// script folder; now: the time the run started) and returns what
    /// could not be copied.
    pub fn save(&self, script: &str, now: &str, args: &[String]) -> Result<Vec<String>> {
        let st = self.st.lock().unwrap();
        let mut doc = Map::new();
        doc.insert("script".into(), script.into());
        doc.insert("now".into(), now.into());
        if !args.is_empty() {
            doc.insert("args".into(), json!(args));
        }
        doc.insert("answers".into(), json!(st.answers));
        doc.insert("variants".into(), json!({}));
        let path = self.dir.join("case.json");
        let input = self.dir.join("input");
        std::fs::create_dir_all(&input).map_err(|e| Error::path("mkdir", input.display().to_string(), e))?;
        let text = serde_json::to_string_pretty(&Value::Object(doc)).unwrap_or_default() + "\n";
        std::fs::write(&path, text).map_err(|e| Error::path("open", path.display().to_string(), e))?;
        Ok(st.failed.clone())
    }
}
