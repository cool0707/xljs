//! One run of a test case: the script runs in a copy of its folder with the
//! replay UI, and what it showed and wrote comes back as the transcript.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use serde_json::{Value, json};

use super::replay::{Lines, Replay, Transcript};
use super::{Case, Plan, dump};
use crate::engine::{self, Options, TestRun, watchdog};
use crate::ui::Ui;
use crate::{Error, Result, err, i18n};

/// The script folder is not copied when it is larger than this.
const MAX_COPY: u64 = 200 << 20;

/// The names a script sees when the case does not give any.
pub const USER_NAME: &str = "test";
pub const COMPUTER_NAME: &str = "TEST-PC";

fn temp(what: &str) -> Result<PathBuf> {
    static N: AtomicU64 = AtomicU64::new(0);
    let p = std::env::temp_dir().join(format!("xljs-{what}-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).map_err(|e| Error::path("mkdir", p.display().to_string(), e))?;
    Ok(p)
}

/// Copies the files of src into dst (see dump::skipped; no exe files).
fn copy_tree(src: &Path, dst: &Path, budget: &mut u64) -> Result<()> {
    let entries = std::fs::read_dir(src).map_err(|e| Error::path("open", src.display().to_string(), e))?;
    for e in entries.flatten() {
        let name = e.file_name();
        let Ok(kind) = e.file_type() else { continue };
        let n = name.to_string_lossy();
        if dump::skipped(&n, kind.is_dir()) {
            continue;
        }
        let to = dst.join(&name);
        if kind.is_dir() {
            std::fs::create_dir_all(&to).map_err(|e| Error::path("mkdir", to.display().to_string(), e))?;
            copy_tree(&e.path(), &to, budget)?;
        } else if kind.is_file() && !n.to_lowercase().ends_with(".exe") {
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            if size > *budget {
                return Err(err!("test.tooLarge", MAX_COPY >> 20));
            }
            *budget -= size;
            std::fs::copy(e.path(), &to).map_err(|e| Error::path("open", to.display().to_string(), e))?;
        }
    }
    Ok(())
}

/// Puts the working folder and the settings folder back.
struct Restore {
    cwd: Option<PathBuf>,
    config: Option<std::ffi::OsString>,
    dirs: Vec<PathBuf>,
}

impl Drop for Restore {
    fn drop(&mut self) {
        if let Some(d) = &self.cwd {
            let _ = std::env::set_current_dir(d);
        }
        // The process is single-threaded here but for the UI thread, which waits.
        match &self.config {
            Some(v) => unsafe { std::env::set_var("XLJS_CONFIG_DIR", v) },
            None => unsafe { std::env::remove_var("XLJS_CONFIG_DIR") },
        }
        for d in &self.dirs {
            let _ = std::fs::remove_dir_all(d);
        }
    }
}

/// Removes the path of the work folder from the strings of v: paths in it
/// become relative ones.
fn strip_paths(v: &mut Value, prefixes: &[String]) {
    match v {
        Value::String(s) => {
            for p in prefixes {
                if s.contains(p.as_str()) {
                    *s = s.replace(&format!("{p}/"), "").replace(&format!("{p}\\"), "").replace(p.as_str(), ".");
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| strip_paths(x, prefixes)),
        Value::Object(m) => m.values_mut().for_each(|x| strip_paths(x, prefixes)),
        _ => {}
    }
}

/// The message of an error without the stack (the line numbers change with
/// every edit of the script).
fn error_text(e: &Error) -> String {
    i18n::error(e).lines().filter(|l| !l.trim_start().starts_with("at ")).collect::<Vec<_>>().join("\n").trim_end().to_string()
}

/// Runs the script of case with the answers of plan and returns the
/// transcript. limit: how long the script may run JavaScript without
/// waiting before it is stopped.
pub fn run(case: &Case, plan: &Plan, limit: Duration, version: &str) -> Result<Vec<Value>> {
    let work = temp("case")?;
    let config = temp("config")?;
    let mut restore = Restore { cwd: None, config: std::env::var_os("XLJS_CONFIG_DIR"), dirs: vec![work.clone(), config.clone()] };
    let mut budget = MAX_COPY;
    copy_tree(&case.root, &work, &mut budget)?;
    let input = case.dir.join("input");
    if input.is_dir() {
        let to = work.join("input");
        std::fs::create_dir_all(&to).map_err(|e| Error::path("mkdir", to.display().to_string(), e))?;
        copy_tree(&input, &to, &mut budget)?;
    }
    let before = dump::snapshot(&work);

    restore.cwd = std::env::current_dir().ok();
    std::env::set_current_dir(&work).map_err(|e| Error::path("chdir", work.display().to_string(), e))?;
    unsafe { std::env::set_var("XLJS_CONFIG_DIR", &config) };

    let t = Transcript::default();
    let abort = Arc::new(AtomicBool::new(false));
    let replay = Arc::new(Replay::new(&t, &plan.answers, abort.clone()));
    let (out, errs) = (Lines::new(&t, "out"), Lines::new(&t, "err"));
    let o = Options {
        script_path: work.join(&case.script).to_string_lossy().into_owned(),
        args: plan.args.clone(),
        stdout: Some(out.clone()),
        stderr: Some(errs.clone()),
        version: version.to_string(),
        watchdog: Some(limit),
        memory_floor: Some(engine::memory::DEFAULT_FLOOR),
        test: Some(TestRun {
            now: plan.now.clone(),
            user_name: if case.user_name.is_empty() { USER_NAME.into() } else { case.user_name.clone() },
            computer_name: if case.computer_name.is_empty() { COMPUTER_NAME.into() } else { case.computer_name.clone() },
            abort: abort.clone(),
        }),
        ..Default::default()
    };
    let u: Arc<dyn Ui> = replay.clone();
    let r = crate::ui::run(u, move |su| {
        let mut o = o;
        o.ui = Some(su);
        engine::run(o)
    });
    out.lock().unwrap().finish();
    errs.lock().unwrap().finish();
    // Answers left over by a script that was stopped tell nothing.
    if !abort.load(Ordering::SeqCst) && !matches!(r, Ok(watchdog::STUCK_CODE)) {
        replay.unused();
    }
    match &r {
        Ok(code) => {
            if *code == watchdog::STUCK_CODE {
                t.problem("timeout", json!({ "seconds": limit.as_secs_f64() }));
            }
            t.push(json!({ "type": "end", "code": code }));
        }
        Err(e) => t.push(json!({ "type": "end", "code": 1, "error": error_text(e) })),
    }
    let mut events = t.take();
    events.extend(dump::changes(&work, &before));

    let mut prefixes = vec![work.to_string_lossy().into_owned()];
    if let Ok(c) = work.canonicalize() {
        prefixes.push(c.to_string_lossy().into_owned());
    }
    for p in prefixes.clone() {
        prefixes.push(p.replace('\\', "/"));
    }
    prefixes.sort_by_key(|p| std::cmp::Reverse(p.len()));
    prefixes.dedup();
    events.iter_mut().for_each(|e| strip_paths(e, &prefixes));
    drop(restore);
    Ok(events)
}
