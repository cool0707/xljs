//! Explorer shortcuts: shell links (.lnk) and Internet shortcuts (.url),
//! read and written by shell.readShortcutLink / shell.writeShortcutLink.
//!
//! A .lnk goes through IShellLinkW on Windows (`win.rs`), so that Windows
//! itself builds the item ID list and the link information; elsewhere (the
//! tests on Linux) `lnk.rs` reads and writes the format (MS-SHLLINK)
//! directly. A .url is a small INI file, handled here on every platform.

#[cfg(any(not(windows), test))]
pub(crate) mod lnk;
#[cfg(windows)]
mod win;

use serde::{Deserialize, Serialize};

use crate::{Error, Result, err};

/// A shortcut as scripts see it (the ShortcutDetails of Electron, with
/// windowState, hotkey and runAsAdmin added).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Details {
    pub target: String,
    pub args: String,
    pub cwd: String,
    pub description: String,
    pub icon: String,
    pub icon_index: i32,
    pub window_state: String,
    pub hotkey: String,
    pub run_as_admin: bool,
    /// The file that was read (with ".lnk" / ".url" when it was added).
    pub path: String,
}

/// The fields writeShortcutLink was given (None = not given).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Changes {
    pub target: Option<String>,
    pub args: Option<String>,
    pub cwd: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub icon_index: Option<i32>,
    pub window_state: Option<String>,
    pub hotkey: Option<String>,
    pub run_as_admin: Option<bool>,
}

/// A shell link in the terms of the format (and of IShellLinkW).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Link {
    pub target: String,
    pub args: String,
    pub cwd: String,
    pub description: String,
    pub icon: String,
    pub icon_index: i32,
    /// SW_SHOWNORMAL (1), SW_SHOWMAXIMIZED (3) or SW_SHOWMINNOACTIVE (7).
    pub show: u32,
    /// Virtual key in the low byte, HOTKEYF_* in the high byte.
    pub hotkey: u16,
    pub run_as_admin: bool,
}

pub const SW_SHOWNORMAL: u32 = 1;
pub const SW_SHOWMAXIMIZED: u32 = 3;
pub const SW_SHOWMINNOACTIVE: u32 = 7;

/// How writeShortcutLink treats an existing file (Electron's operation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    /// A new shortcut with the given fields only (an existing file is replaced).
    Create,
    /// Changes the given fields of an existing shortcut, keeping the others.
    Update,
    /// Like Create, but the shortcut must exist.
    Replace,
}

impl Operation {
    pub fn parse(s: &str) -> Result<Operation> {
        Ok(match s.trim().to_lowercase().as_str() {
            "" | "create" => Operation::Create,
            "update" => Operation::Update,
            "replace" => Operation::Replace,
            _ => return Err(err!("shell.shortcutOperation", s)),
        })
    }
}

fn ext_of(p: &str) -> String {
    std::path::Path::new(p).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// Whether s is a URL ("https://...", "mailto:" is not a file shortcut target either).
pub fn is_url(s: &str) -> bool {
    let Some(i) = s.find(':') else { return false };
    let scheme = &s[..i];
    // "C:\..." is a drive, not a scheme.
    scheme.len() > 1
        && scheme.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
        && !scheme.eq_ignore_ascii_case("shell")
}

fn exists(p: &str) -> bool {
    std::fs::metadata(p).is_ok_and(|m| m.is_file())
}

/// The file a shortcut path names: ".lnk" (or ".url" for a URL, or when
/// only that file exists) is added unless the name already ends in one.
fn shortcut_path(p: &str, url_target: bool) -> String {
    if matches!(ext_of(p).as_str(), "lnk" | "url") {
        return p.to_string();
    }
    let (lnk, url) = (format!("{p}.lnk"), format!("{p}.url"));
    if exists(&lnk) {
        lnk
    } else if exists(&url) || url_target {
        url
    } else {
        lnk
    }
}

fn is_url_file(p: &str) -> bool {
    ext_of(p) == "url"
}

/// Reads the shortcut at path (".lnk" / ".url" may be left out).
pub fn read(path: &str) -> Result<Details> {
    let p = if exists(path) { path.to_string() } else { shortcut_path(path, false) };
    std::fs::metadata(&p).map_err(|e| Error::path("open", p.clone(), e))?;
    let mut d = if is_url_file(&p) || looks_like_url_file(&p) {
        let u = read_url(&p)?;
        Details { target: u.url, icon: u.icon, icon_index: u.icon_index, window_state: "normal".into(), ..Default::default() }
    } else {
        to_details(&load_lnk(&p)?)
    };
    d.path = std::path::absolute(&p).map(|a| a.to_string_lossy().into_owned()).unwrap_or(p);
    Ok(d)
}

/// Writes the shortcut at path and returns the file written (".lnk" or
/// ".url" is added when the name has neither).
pub fn write(path: &str, op: Operation, c: &Changes) -> Result<String> {
    let url_target = c.target.as_deref().is_some_and(is_url);
    let p = shortcut_path(path, url_target);
    let p = std::path::absolute(&p).map(|a| a.to_string_lossy().into_owned()).unwrap_or(p);
    let found = exists(&p);
    if op != Operation::Create && !found {
        return Err(Error::path("open", p, std::io::Error::from(std::io::ErrorKind::NotFound)));
    }
    if op != Operation::Update && c.target.as_deref().is_none_or(|t| t.trim().is_empty()) {
        return Err(err!("shell.shortcutTarget"));
    }
    if let Some(dir) = std::path::Path::new(&p).parent()
        && !dir.as_os_str().is_empty()
        && !dir.is_dir()
    {
        return Err(Error::path("open", dir.to_string_lossy(), std::io::Error::from(std::io::ErrorKind::NotFound)));
    }
    if is_url_file(&p) {
        let prev = if op == Operation::Update { Some(read_url(&p)?) } else { None };
        write_url(&p, prev.as_ref(), c)?;
        return Ok(p);
    }
    if url_target {
        return Err(err!("shell.shortcutURLTarget", p));
    }
    let prev = if op == Operation::Update { Some(load_lnk(&p)?) } else { None };
    let mut link = prev.clone().unwrap_or(Link { show: SW_SHOWNORMAL, ..Default::default() });
    apply(&mut link, c)?;
    save_lnk(&p, &link, prev.as_ref())?;
    Ok(p)
}

/// Relative paths are taken from the current directory; environment
/// variables ("%SystemRoot%\...") and shell names ("shell:...", "::{...}")
/// are kept as they are.
fn abs_path(s: &str) -> String {
    let s = s.trim();
    let windows_abs = s.as_bytes().get(1) == Some(&b':') || s.starts_with("\\\\");
    if s.is_empty() || windows_abs || s.starts_with('%') || s.starts_with("::") || s.to_lowercase().starts_with("shell:") {
        return s.to_string();
    }
    std::path::absolute(s).map(|a| a.to_string_lossy().into_owned()).unwrap_or_else(|_| s.to_string())
}

fn apply(l: &mut Link, c: &Changes) -> Result<()> {
    if let Some(t) = &c.target {
        l.target = abs_path(t);
    }
    if let Some(v) = &c.args {
        l.args = v.clone();
    }
    if let Some(v) = &c.cwd {
        l.cwd = abs_path(v);
    }
    if let Some(v) = &c.description {
        l.description = v.clone();
    }
    if let Some(v) = &c.icon {
        l.icon = abs_path(v);
    }
    if let Some(v) = c.icon_index {
        l.icon_index = v;
    }
    if let Some(v) = &c.window_state {
        l.show = parse_window_state(v)?;
    }
    if let Some(v) = &c.hotkey {
        l.hotkey = parse_hotkey(v)?;
    }
    if let Some(v) = c.run_as_admin {
        l.run_as_admin = v;
    }
    Ok(())
}

fn to_details(l: &Link) -> Details {
    Details {
        target: l.target.clone(),
        args: l.args.clone(),
        cwd: l.cwd.clone(),
        description: l.description.clone(),
        icon: l.icon.clone(),
        icon_index: l.icon_index,
        window_state: window_state_name(l.show).into(),
        hotkey: format_hotkey(l.hotkey),
        run_as_admin: l.run_as_admin,
        path: String::new(),
    }
}

#[cfg(windows)]
fn load_lnk(p: &str) -> Result<Link> {
    win::load(p)
}
#[cfg(windows)]
fn save_lnk(p: &str, l: &Link, prev: Option<&Link>) -> Result<()> {
    win::save(p, l, prev)
}
#[cfg(not(windows))]
fn load_lnk(p: &str) -> Result<Link> {
    let b = std::fs::read(p).map_err(|e| Error::path("open", p, e))?;
    lnk::parse(&b).ok_or_else(|| err!("shell.notShortcut", p))
}
#[cfg(not(windows))]
fn save_lnk(p: &str, l: &Link, _prev: Option<&Link>) -> Result<()> {
    std::fs::write(p, lnk::build(l)).map_err(|e| Error::path("open", p, e))
}

// ---------------------------------------------------------------- window state

pub fn parse_window_state(s: &str) -> Result<u32> {
    Ok(match s.trim().to_lowercase().as_str() {
        "" | "normal" => SW_SHOWNORMAL,
        "maximized" | "max" => SW_SHOWMAXIMIZED,
        "minimized" | "min" => SW_SHOWMINNOACTIVE,
        _ => return Err(err!("shell.shortcutWindowState", s)),
    })
}

pub fn window_state_name(show: u32) -> &'static str {
    match show {
        3 => "maximized",
        2 | 6 | 7 => "minimized",
        _ => "normal",
    }
}

// ---------------------------------------------------------------- hotkeys

const HOTKEYF_SHIFT: u16 = 1;
const HOTKEYF_CONTROL: u16 = 2;
const HOTKEYF_ALT: u16 = 4;

const KEY_NAMES: &[(&str, u8)] = &[
    ("Space", 0x20),
    ("PageUp", 0x21),
    ("PageDown", 0x22),
    ("End", 0x23),
    ("Home", 0x24),
    ("Left", 0x25),
    ("Up", 0x26),
    ("Right", 0x27),
    ("Down", 0x28),
    ("Insert", 0x2D),
    ("Delete", 0x2E),
    ("Pause", 0x13),
    ("NumLock", 0x90),
    ("ScrollLock", 0x91),
];

fn key_name(vk: u8) -> String {
    match vk {
        b'0'..=b'9' | b'A'..=b'Z' => (vk as char).to_string(),
        0x60..=0x69 => format!("Num{}", vk - 0x60),
        0x70..=0x87 => format!("F{}", vk - 0x6F),
        _ => match KEY_NAMES.iter().find(|(_, v)| *v == vk) {
            Some((n, _)) => n.to_string(),
            None => format!("0x{vk:02X}"),
        },
    }
}

fn key_code(name: &str) -> Option<u8> {
    let up = name.to_uppercase();
    if up.len() == 1 {
        let c = up.as_bytes()[0];
        return c.is_ascii_alphanumeric().then_some(c);
    }
    if let Some(n) = up.strip_prefix('F').and_then(|n| n.parse::<u8>().ok()) {
        return (1..=24).contains(&n).then(|| 0x6F + n);
    }
    if let Some(n) = up.strip_prefix("NUM").and_then(|n| n.parse::<u8>().ok()) {
        return (n <= 9).then(|| 0x60 + n);
    }
    if let Some(h) = up.strip_prefix("0X") {
        return u8::from_str_radix(h, 16).ok().filter(|&v| v != 0);
    }
    let alias = match up.as_str() {
        "DEL" => "Delete",
        "INS" => "Insert",
        "PGUP" => "PageUp",
        "PGDN" => "PageDown",
        _ => name,
    };
    KEY_NAMES.iter().find(|(n, _)| n.eq_ignore_ascii_case(alias)).map(|(_, v)| *v)
}

/// "Ctrl+Alt+K" -> the hotkey word ("" = none).
pub fn parse_hotkey(s: &str) -> Result<u16> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(0);
    }
    let parts: Vec<&str> = s.split('+').map(str::trim).collect();
    let (key, mods) = parts.split_last().unwrap();
    let mut flags = 0;
    for m in mods {
        flags |= match m.to_lowercase().as_str() {
            "ctrl" | "control" => HOTKEYF_CONTROL,
            "alt" => HOTKEYF_ALT,
            "shift" => HOTKEYF_SHIFT,
            _ => return Err(err!("shell.shortcutHotkey", s)),
        };
    }
    let vk = key_code(key).ok_or_else(|| err!("shell.shortcutHotkey", s))?;
    Ok(flags << 8 | vk as u16)
}

/// The hotkey word -> "Ctrl+Alt+K" ("" = none).
pub fn format_hotkey(h: u16) -> String {
    let vk = (h & 0xFF) as u8;
    if vk == 0 {
        return String::new();
    }
    let f = h >> 8;
    let mut s = String::new();
    for (bit, name) in [(HOTKEYF_CONTROL, "Ctrl+"), (HOTKEYF_SHIFT, "Shift+"), (HOTKEYF_ALT, "Alt+")] {
        if f & bit != 0 {
            s.push_str(name);
        }
    }
    s + &key_name(vk)
}

// ---------------------------------------------------------------- .url

struct UrlFile {
    url: String,
    icon: String,
    icon_index: i32,
    lines: Vec<String>,
}

fn looks_like_url_file(p: &str) -> bool {
    ext_of(p) != "lnk"
        && std::fs::read(p).is_ok_and(|b| {
            let s = crate::api::decode(&b, "auto").unwrap_or_default();
            s.lines().any(|l| l.trim().eq_ignore_ascii_case("[InternetShortcut]"))
        })
}

fn read_url(p: &str) -> Result<UrlFile> {
    let b = std::fs::read(p).map_err(|e| Error::path("open", p, e))?;
    let text = crate::api::decode(&b, "auto")?;
    let lines: Vec<String> = text.lines().map(|l| l.trim_start_matches('\u{feff}').to_string()).collect();
    let mut u = UrlFile { url: String::new(), icon: String::new(), icon_index: 0, lines };
    let mut section = String::new();
    let mut found = false;
    for l in &u.lines {
        let t = l.trim();
        if t.starts_with('[') && t.ends_with(']') {
            section = t[1..t.len() - 1].to_string();
            found |= section.eq_ignore_ascii_case("InternetShortcut");
            continue;
        }
        if !section.eq_ignore_ascii_case("InternetShortcut") {
            continue;
        }
        let Some((k, v)) = t.split_once('=') else { continue };
        match k.trim().to_lowercase().as_str() {
            "url" => u.url = v.trim().to_string(),
            "iconfile" => u.icon = v.trim().to_string(),
            "iconindex" => u.icon_index = v.trim().parse().unwrap_or(0),
            _ => {}
        }
    }
    if !found {
        return Err(err!("shell.notShortcut", p));
    }
    Ok(u)
}

fn write_url(p: &str, prev: Option<&UrlFile>, c: &Changes) -> Result<()> {
    let mut url = prev.map(|u| u.url.clone()).unwrap_or_default();
    let mut icon = prev.map(|u| u.icon.clone()).unwrap_or_default();
    let mut index = prev.map(|u| u.icon_index).unwrap_or(0);
    if let Some(t) = &c.target {
        let t = t.trim();
        if !is_url(t) {
            return Err(err!("shell.expectedURL", t));
        }
        url = t.to_string();
    }
    if let Some(v) = &c.icon {
        icon = abs_path(v);
    }
    if let Some(v) = c.icon_index {
        index = v;
    }
    // The keys of [InternetShortcut] are rewritten; other lines and
    // sections (Windows keeps its own there) stay.
    let mut out: Vec<String> = Vec::new();
    let mut section = String::new();
    let mut ours: Vec<String> = vec![format!("URL={url}")];
    if !icon.is_empty() {
        ours.push(format!("IconFile={icon}"));
        ours.push(format!("IconIndex={index}"));
    }
    let mut placed = false;
    for l in prev.map(|u| u.lines.as_slice()).unwrap_or(&[]) {
        let t = l.trim();
        if t.starts_with('[') && t.ends_with(']') {
            section = t[1..t.len() - 1].to_string();
            out.push(l.clone());
            if section.eq_ignore_ascii_case("InternetShortcut") && !placed {
                out.append(&mut ours);
                placed = true;
            }
            continue;
        }
        if section.eq_ignore_ascii_case("InternetShortcut")
            && let Some((k, _)) = t.split_once('=')
            && matches!(k.trim().to_lowercase().as_str(), "url" | "iconfile" | "iconindex")
        {
            continue;
        }
        out.push(l.clone());
    }
    if !placed {
        let mut head = vec!["[InternetShortcut]".to_string()];
        head.append(&mut ours);
        head.append(&mut out);
        out = head;
    }
    let text = out.join("\r\n") + "\r\n";
    // .url files are read as ANSI (CP932) unless they start with a UTF-16 BOM.
    let bytes = if text.is_ascii() {
        text.into_bytes()
    } else {
        let (b, _, bad) = encoding_rs::SHIFT_JIS.encode(&text);
        if bad { crate::api::encode(&text, "utf16le")? } else { b.into_owned() }
    };
    std::fs::write(p, bytes).map_err(|e| Error::path("open", p, e))
}

#[cfg(test)]
mod tests;
