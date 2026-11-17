//! The error report shown when a run of a script fails. The person who sees
//! the error is usually not the one who wrote the script, so the report
//! carries what the author needs to find the cause (the run, the computer,
//! the code at the error and the last output), and the message says whom
//! to ask (the script's @author / @contact).

use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use regex::Regex;

use crate::engine::Out;
use crate::meta::Meta;
use crate::ui::ErrorReport;
use crate::t;

/// The bytes of output kept for the report.
const TAIL_BYTES: usize = 16 * 1024;
/// The lines of output put in the report.
const TAIL_LINES: usize = 40;
/// The longest mailto: link made (mail programs cut longer ones).
const MAX_MAILTO: usize = 1800;

/// What is known about the run of a script.
#[derive(Clone, Default)]
pub struct RunInfo {
    pub script_path: String,
    pub args: Vec<String>,
    pub meta: Meta,
    /// The script is embedded in the exe (its source is not next to it).
    pub packed: bool,
    /// The last output of the script (console.log and errors).
    pub tail: Arc<Mutex<VecDeque<u8>>>,
}

impl RunInfo {
    pub fn new(script_path: &str, args: &[String], meta: &Meta, packed: bool) -> RunInfo {
        RunInfo { script_path: script_path.to_string(), args: args.to_vec(), meta: meta.clone(), packed, tail: Arc::default() }
    }

    /// An output that writes to out and keeps the last of it for the report.
    pub fn tee(&self, out: Out) -> Out {
        Arc::new(Mutex::new(Tee { out, tail: self.tail.clone() }))
    }

    fn script_name(&self) -> String {
        Path::new(&self.script_path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
    }

    /// The name of the tool for people: its @title, else the file name.
    fn tool_name(&self) -> String {
        if self.meta.title.is_empty() { self.script_name() } else { self.meta.title.clone() }
    }

    fn output_lines(&self) -> Vec<String> {
        let t = self.tail.lock().unwrap();
        let (a, b) = t.as_slices();
        let bytes = [a, b].concat();
        let text = String::from_utf8_lossy(&bytes);
        let lines: Vec<&str> = text.lines().collect();
        // The first line may be cut.
        let skip = usize::from(t.len() >= TAIL_BYTES);
        let lines = &lines[skip.min(lines.len())..];
        lines[lines.len().saturating_sub(TAIL_LINES)..].iter().map(|l| l.trim_end().to_string()).collect()
    }
}

struct Tee {
    out: Out,
    tail: Arc<Mutex<VecDeque<u8>>>,
}

impl Write for Tee {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        {
            let mut t = self.tail.lock().unwrap();
            t.extend(buf);
            let over = t.len().saturating_sub(TAIL_BYTES);
            t.drain(..over);
        }
        // Output that cannot be written (no console) is still kept.
        let _ = self.out.lock().unwrap().write_all(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let _ = self.out.lock().unwrap().flush();
        Ok(())
    }
}

/// The sentence telling the user whom to ask, None when the script names
/// no author or contact.
pub fn ask_author(m: &Meta) -> Option<String> {
    match (m.author.is_empty(), m.contact.is_empty()) {
        (false, false) => Some(t!("report.askAuthorBoth", m.author, m.contact)),
        (false, true) => Some(t!("report.askAuthor", m.author)),
        (true, false) => Some(t!("report.askContact", m.contact)),
        (true, true) => None,
    }
}

static MAIL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\w.+-]+@[\w-]+(?:\.[\w-]+)+").unwrap());
static URL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"https?://[^\s<>()（）、。]+").unwrap());
static FRAME: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*at\s+(?:[^(]*\()?([^()]+?):(\d+):(\d+)\)?\s*$").unwrap());

/// The mail address in a contact ("山田 yamada@example.com 内線 12").
fn mail_address(contact: &str) -> Option<&str> {
    MAIL.find(contact).map(|m| m.as_str())
}

/// Builds the report of the error message msg (as shown to the user).
pub fn build(info: &RunInfo, msg: &str) -> ErrorReport {
    let now = chrono::Local::now();
    let text = report_text(info, msg, &now.format("%Y-%m-%d %H:%M:%S (%:z)").to_string());
    let stem = Path::new(&info.script_path).file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "xljs".into());
    let file_name = format!("{stem}_{}_{}.txt", t!("report.fileName"), now.format("%Y%m%d-%H%M%S"));
    let (contact_link, contact_mail) = match mail_address(&info.meta.contact) {
        Some(addr) => (Some(mailto(addr, info, msg)), true),
        None => (URL.find(&info.meta.contact).map(|u| u.as_str().to_string()), false),
    };
    ErrorReport { text, file_name, contact_link, contact_mail }
}

fn report_text(info: &RunInfo, msg: &str, time: &str) -> String {
    let m = &info.meta;
    let mut r = String::new();
    let mut line = |k: &str, v: &str| r.push_str(&format!("{k}: {v}\n"));
    line(&t!("report.time"), time);
    let script = if m.title.is_empty() { info.script_name() } else { format!("{}（{}）", m.title, info.script_name()) };
    line(&t!("report.script"), &script);
    let path = if info.packed { format!("{} {}", exe_path(), t!("report.packed")) } else { info.script_path.clone() };
    line(&t!("report.path"), &path);
    for (k, v) in [("report.scriptVersion", &m.version), ("report.author", &m.author), ("report.contact", &m.contact)] {
        if !v.is_empty() {
            line(&t!(k), v);
        }
    }
    let args = if info.args.is_empty() { t!("report.none") } else { info.args.iter().map(|a| format!("\"{a}\"")).collect::<Vec<_>>().join(" ") };
    line(&t!("report.args"), &args);
    let cwd = std::env::current_dir().map(|d| d.display().to_string()).unwrap_or_default();
    line(&t!("report.cwd"), &cwd);
    line("xljs", &format!("{} ({})", super::version(), std::env::consts::ARCH));
    line("OS", &crate::net::agent::os_description());
    let wv = crate::ui::webview2_version();
    if !wv.is_empty() {
        line("WebView2", &wv);
    }
    line(&t!("report.user"), &format!("{} @ {}", crate::api::user_name(), crate::api::computer_name()));
    line(&t!("report.lang"), &crate::i18n::lang());

    let mut out = format!("{}\n{}\n{r}\n■ {}\n{}\n", t!("report.title"), "=".repeat(40), t!("report.error"), msg.trim_end());
    if !info.packed
        && let Some((file, code)) = source_at(info, msg)
    {
        out.push_str(&format!("\n■ {}\n{code}", t!("report.source", file)));
    }
    let lines = info.output_lines();
    if !lines.is_empty() {
        out.push_str(&format!("\n■ {}\n", t!("report.output", lines.len())));
        for l in lines {
            out.push_str(&l);
            out.push('\n');
        }
    }
    out
}

fn exe_path() -> String {
    std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default()
}

/// The lines around the first place of the stack in msg that is a file of
/// the script: ("main.ts:13:29", text).
fn source_at(info: &RunInfo, msg: &str) -> Option<(String, String)> {
    let dir = Path::new(&info.script_path).parent().map(Path::to_path_buf).unwrap_or_default();
    for l in msg.lines() {
        let Some(c) = FRAME.captures(l) else { continue };
        let file = c[1].trim();
        let Ok(line) = c[2].parse::<usize>() else { continue };
        if line == 0 || file.contains("node_modules") {
            continue;
        }
        let p = if Path::new(file).is_absolute() { PathBuf::from(file) } else { dir.join(file) };
        if !std::fs::metadata(&p).is_ok_and(|m| m.is_file() && m.len() < 8 << 20) {
            continue;
        }
        let Ok(src) = std::fs::read(&p) else { continue };
        let src = String::from_utf8_lossy(src.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&src)).into_owned();
        let lines: Vec<&str> = src.lines().collect();
        if line > lines.len() {
            continue;
        }
        let (from, to) = (line.saturating_sub(3).max(1), (line + 2).min(lines.len()));
        let w = to.to_string().len();
        let mut text = String::new();
        for n in from..=to {
            let mark = if n == line { ">" } else { " " };
            text.push_str(&format!("{mark} {n:>w$} | {}\n", lines[n - 1].trim_end()));
        }
        return Some((format!("{file}:{}:{}", &c[2], &c[3]), text));
    }
    None
}

/// A mailto: link to addr with a subject and a short body: the full
/// report goes onto the clipboard to be pasted (links have to be short).
fn mailto(addr: &str, info: &RunInfo, msg: &str) -> String {
    let subject = t!("report.mailSubject", info.tool_name());
    let mut chars: Vec<char> = msg.trim().chars().collect();
    loop {
        let excerpt: String = chars.iter().collect();
        let excerpt = if excerpt.len() < msg.trim().len() { format!("{excerpt}…") } else { excerpt };
        let body = t!("report.mailBody", info.tool_name(), excerpt);
        let link = format!("mailto:{}?subject={}&body={}", addr, encode(&subject), encode(&body));
        if link.len() <= MAX_MAILTO || chars.is_empty() {
            return link;
        }
        let keep = chars.len().saturating_sub(chars.len() / 4 + 1);
        chars.truncate(keep);
    }
}

/// Percent-encodes s for a mailto: field (RFC 6068): line breaks as CRLF.
fn encode(s: &str) -> String {
    let s = s.replace("\r\n", "\n").replace('\n', "\r\n");
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Whether the error is reported with the author's contact: not for runs
/// started by the development mode (the author is the one running it).
pub fn wanted() -> bool {
    std::env::var(crate::dev::runner::DEV_ENV).map(|v| v.is_empty()).unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(dir: &Path) -> RunInfo {
        let meta = Meta { title: "印刷ツール".into(), author: "山田太郎".into(), contact: "山田 yamada@example.com 内線 12".into(), version: "1.2".into(), ..Default::default() };
        RunInfo::new(&dir.join("tool.xljs").to_string_lossy(), &["a.xlsx".into()], &meta, false)
    }

    #[test]
    fn report_has_run_source_and_output() {
        let _l = crate::testutil::lock_lang("ja");
        let dir = crate::testutil::tempdir();
        std::fs::write(dir.join("tool.xljs"), "// 1\n// 2\nconst a = 1;\nconst b = null;\nb.x();\n// 6\n// 7\n// 8\n").unwrap();
        let i = info(&dir);
        let out = i.tee(Arc::new(Mutex::new(std::io::sink())));
        for n in 0..50 {
            writeln!(out.lock().unwrap(), "行 {n}").unwrap();
        }
        let msg = "TypeError: cannot read property 'x' of null\n    at <anonymous> (tool.xljs:5:3)";
        let r = build(&i, msg);
        let t = &r.text;
        for want in ["印刷ツール（tool.xljs）", "山田太郎", "\"a.xlsx\"", "1.2", msg, "tool.xljs:5:3", "> 5 | b.x();", "  2 | // 2", "  7 | // 7", "行 49", "行 10"] {
            assert!(t.contains(want), "{want:?} in\n{t}");
        }
        assert!(!t.contains("行 9\n") && !t.contains("| // 1") && !t.contains("| // 8"), "{t}");
        assert!(r.file_name.starts_with("tool_エラーレポート_") && r.file_name.ends_with(".txt"), "{}", r.file_name);
        assert!(r.contact_mail);
        let link = r.contact_link.unwrap();
        assert!(link.starts_with("mailto:yamada@example.com?subject=") && link.contains("&body=") && link.len() <= MAX_MAILTO, "{link}");
        assert!(link.contains("%0D%0A") && !link.contains(' '), "{link}");
    }

    #[test]
    fn long_messages_make_short_links() {
        let _l = crate::testutil::lock_lang("ja");
        let dir = crate::testutil::tempdir();
        let msg = "エラー".repeat(2000);
        let r = build(&info(&dir), &msg);
        let link = r.contact_link.unwrap();
        assert!(link.len() <= MAX_MAILTO && link.contains("%E2%80%A6"), "{}", link.len()); // …
        assert!(r.text.contains(&msg));
    }

    #[test]
    fn contacts() {
        let _l = crate::testutil::lock_lang("ja");
        let dir = crate::testutil::tempdir();
        let mut i = info(&dir);
        i.meta.contact = "https://example.com/help（社内ポータル）".into();
        let r = build(&i, "x");
        assert_eq!((r.contact_link.as_deref(), r.contact_mail), (Some("https://example.com/help"), false));
        i.meta.contact = "内線 1234".into();
        assert_eq!(build(&i, "x").contact_link, None);
        assert_eq!(ask_author(&i.meta).unwrap(), t!("report.askAuthorBoth", "山田太郎", "内線 1234"));
        i.meta.author.clear();
        i.meta.contact.clear();
        assert_eq!(ask_author(&i.meta), None);
    }

    #[test]
    fn packed_scripts_have_no_source() {
        let _l = crate::testutil::lock_lang("ja");
        let dir = crate::testutil::tempdir();
        std::fs::write(dir.join("tool.xljs"), "x();\n").unwrap();
        let mut i = info(&dir);
        i.packed = true;
        let r = build(&i, "Error: x\n    at tool.xljs:1:1");
        assert!(!r.text.contains("| x();") && r.text.contains(&t!("report.packed")), "{}", r.text);
    }
}
