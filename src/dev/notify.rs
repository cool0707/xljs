//! Telling the AI chat what the user changed by hand: after an AI's
//! answer is accepted, a later request to the AI starts from its own
//! answer and undoes the user's fixes. So the development mode keeps what
//! the AI last saw of each file (<folder>/.xljs/ai/<file>: the accepted
//! answer, the script sent with the introduction, what was told last) and
//! makes a message with the differences (a unified diff) to paste into the
//! chat.

use std::path::{Path, PathBuf};

use super::project::{DATA_DIR, Project, read_text, write_text};
use crate::{Result, t};

/// Where what the AI last saw of the file rel (slashes) is kept.
pub fn seen_path(dir: &Path, rel: &str) -> PathBuf {
    let mut p = dir.join(DATA_DIR).join("ai");
    for part in rel.split('/') {
        p.push(part);
    }
    p
}

/// What the AI last saw of the file; None when it has not seen it.
pub fn seen(dir: &Path, rel: &str) -> Option<String> {
    let p = seen_path(dir, rel);
    p.is_file().then(|| read_text(&p).ok()).flatten()
}

/// Remembers that the AI has seen text as the file.
pub fn remember(dir: &Path, rel: &str, text: &str) -> Result<()> {
    write_text(&seen_path(dir, rel), text)
}

/// Moves what the AI saw of a file renamed from → to.
pub fn rename(dir: &Path, from: &str, to: &str) {
    let (a, b) = (seen_path(dir, from), seen_path(dir, to));
    if a.is_file() && !b.exists() {
        let _ = std::fs::rename(a, b);
    }
}

/// A change of a file to tell the AI.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub file: String,
    /// The unified diff from what the AI saw; None = the AI has not seen
    /// the file (or the diff is longer than the file): the whole text.
    pub diff: Option<String>,
    pub text: String,
}

fn lines(s: &str) -> Vec<&str> {
    s.split_inclusive('\n').collect()
}

/// Edits beyond which a diff is not worth it (the whole text is told).
const MAX_EDITS: isize = 1000;

/// The edit script of a and b (Myers): for each line, 0 = in both,
/// -1 = only in a, 1 = only in b, in order. None for more than MAX_EDITS
/// edits.
pub(crate) fn edits(a: &[&str], b: &[&str]) -> Option<Vec<(i8, usize)>> {
    let (n, m) = (a.len() as isize, b.len() as isize);
    let max = n + m;
    let off = max + 1;
    let mut v = vec![0isize; (2 * max + 3) as usize];
    // v before each round d, for k in -d-1..=d+1 (what the way back reads).
    let mut trace: Vec<Vec<isize>> = Vec::new();
    'outer: for d in 0..=max {
        if d > MAX_EDITS {
            return None;
        }
        trace.push(v[(off - d - 1) as usize..=(off + d + 1) as usize].to_vec());
        let mut k = -d;
        while k <= d {
            let i = (k + off) as usize;
            let mut x = if k == -d || (k != d && v[i - 1] < v[i + 1]) { v[i + 1] } else { v[i - 1] + 1 };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[i] = x;
            if x >= n && y >= m {
                break 'outer;
            }
            k += 2;
        }
    }
    let mut out = Vec::new();
    let (mut x, mut y) = (n, m);
    for (d, v) in trace.iter().enumerate().rev() {
        let d = d as isize;
        let at = |k: isize| v[(k + d + 1) as usize];
        let k = x - y;
        let pk = if k == -d || (k != d && at(k - 1) < at(k + 1)) { k + 1 } else { k - 1 };
        let px = at(pk);
        let py = px - pk;
        while x > px && y > py {
            x -= 1;
            y -= 1;
            out.push((0, x as usize));
        }
        if d > 0 {
            if x == px {
                y -= 1;
                out.push((1, y as usize));
            } else {
                x -= 1;
                out.push((-1, x as usize));
            }
        }
    }
    out.reverse();
    Some(out)
}

/// The unified diff (3 lines of context) of old and new named file; ""
/// when they are the same, None when they differ too much.
pub fn unified(file: &str, old: &str, new: &str) -> Option<String> {
    let (a, b) = (lines(old), lines(new));
    let ed = edits(&a, &b)?;
    // Line numbers (0-based) in a and b before each edit.
    let mut pos = Vec::with_capacity(ed.len() + 1);
    let (mut i, mut j) = (0usize, 0usize);
    for &(op, _) in &ed {
        pos.push((i, j));
        match op {
            0 => {
                i += 1;
                j += 1;
            }
            -1 => i += 1,
            _ => j += 1,
        }
    }
    pos.push((i, j));
    const CTX: usize = 3;
    let changed: Vec<usize> = ed.iter().enumerate().filter(|(_, e)| e.0 != 0).map(|(k, _)| k).collect();
    if changed.is_empty() {
        return Some(String::new());
    }
    let mut out = format!("--- {file}\n+++ {file}\n");
    let mut k = 0;
    while k < changed.len() {
        let start = changed[k].saturating_sub(CTX);
        let mut end = changed[k];
        while k + 1 < changed.len() && changed[k + 1] <= end + 2 * CTX + 1 {
            k += 1;
            end = changed[k];
        }
        let end = (end + CTX + 1).min(ed.len());
        let (ai, bj) = pos[start];
        let (ae, be) = pos[end];
        let range = |s: usize, e: usize| if e - s == 1 { format!("{}", s + 1) } else { format!("{},{}", if e == s { s } else { s + 1 }, e - s) };
        out.push_str(&format!("@@ -{} +{} @@\n", range(ai, ae), range(bj, be)));
        for &(op, l) in &ed[start..end] {
            let (mark, text) = match op {
                0 => (' ', a[l]),
                -1 => ('-', a[l]),
                _ => ('+', b[l]),
            };
            out.push(mark);
            out.push_str(text);
            if !text.ends_with('\n') {
                out.push_str("\n\\ No newline at end of file\n");
            }
        }
        k += 1;
    }
    Some(out)
}

/// The changes to tell the AI: the files whose text differs from what the
/// AI last saw, and current (the file being edited) whole when the AI has
/// not seen it.
pub fn changes(p: &Project, current: &str) -> Vec<Change> {
    let mut out = Vec::new();
    for file in p.files() {
        let Ok(text) = p.path(&file).and_then(|f| read_text(&f)) else { continue };
        match seen(&p.dir, &file) {
            Some(old) if old != text => {
                let diff = unified(&file, &old, &text).filter(|d| d.len() < text.len());
                out.push(Change { file, diff, text });
            }
            Some(_) => {}
            None if file == current && !text.trim().is_empty() => out.push(Change { file, diff: None, text }),
            None => {}
        }
    }
    out
}

/// The message for the AI chat.
pub fn prompt(changes: &[Change]) -> String {
    let mut s = t!("dev.notifyLead");
    s.push_str("\n\n");
    for c in changes {
        match &c.diff {
            Some(d) => s.push_str(&format!("{}\n```diff\n{}```\n\n", t!("dev.notifyDiff", c.file), d)),
            None => s.push_str(&format!("{}\n```{}\n{}\n```\n\n", t!("dev.notifyWhole", c.file), super::report::fence(&c.file), c.text.trim_end())),
        }
    }
    s.push_str(&t!("dev.notifyAsk"));
    s
}
