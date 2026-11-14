//! The .xljs file type: a script file (TypeScript syntax, so plain JavaScript
//! too) that Explorer runs with xljs like .bat or .vbs files.
//!
//! The association is written for the current user only
//! (HKCU\Software\Classes, no administrator rights): double-click runs the
//! script ("open"), the context menu edits it in the development mode
//! ("edit") or packs it into an exe ("pack"), and files dropped onto a
//! .xljs file are passed to the script as arguments (the drop handler of
//! executables, which hands Explorer's "open" command the dropped paths,
//! long names, after %1). The icon is the exe's icon resource FILE_ICON
//! (assets/xljs-file.ico, see src/cli/xljs.rc).

use std::path::{Path, PathBuf};

use crate::{Result, t};

/// The extension of xljs script files.
pub const EXT: &str = ".xljs";
/// The ProgID the extension points to.
pub const PROG_ID: &str = "xljs.script";
/// The shell's drop handler for executables: the dropped files become
/// arguments of the "open" command.
const DROP_HANDLER: &str = "{86C86720-42A0-1069-A2E8-08002B30309D}";
/// The resource id of the file icon in the exe (src/cli/xljs.rc).
const FILE_ICON: i32 = 2;
/// Where per-user file types live, under HKEY_CURRENT_USER.
const CLASSES: &str = r"Software\Classes";

/// A registry value under HKEY_CURRENT_USER: the key, the value's name (""
/// for the key's default value) and its text.
#[derive(Debug, Clone, PartialEq)]
pub struct Value {
    pub key: String,
    pub name: String,
    pub data: String,
}

/// The values that associate .xljs files with exe (the messages in the
/// current language).
pub fn entries(exe: &str) -> Vec<Value> {
    let ext = format!(r"{CLASSES}\{EXT}");
    let prog = format!(r"{CLASSES}\{PROG_ID}");
    let v = |key: &str, name: &str, data: &str| Value { key: key.to_string(), name: name.to_string(), data: data.to_string() };
    let q = format!("\"{exe}\"");
    vec![
        v(&ext, "", PROG_ID),
        v(&format!(r"{ext}\OpenWithProgids"), PROG_ID, ""),
        v(&prog, "", &t!("assoc.typeName")),
        v(&format!(r"{prog}\DefaultIcon"), "", &format!("{q},-{FILE_ICON}")),
        v(&format!(r"{prog}\shell"), "", "open"),
        v(&format!(r"{prog}\shell\open"), "", &t!("assoc.verbRun")),
        v(&format!(r"{prog}\shell\open\command"), "", &format!("{q} -- \"%1\" %*")),
        v(&format!(r"{prog}\shell\edit"), "", &t!("assoc.verbEdit")),
        v(&format!(r"{prog}\shell\edit\command"), "", &format!("{q} dev \"%1\"")),
        v(&format!(r"{prog}\shell\pack"), "", &t!("assoc.verbPack")),
        v(&format!(r"{prog}\shell\pack\command"), "", &format!("{q} pack --dialog \"%1\"")),
        v(&format!(r"{prog}\shellex\DropHandler"), "", DROP_HANDLER),
    ]
}

/// The program of a command line: its first, possibly quoted, word.
pub fn command_program(cmd: &str) -> Option<String> {
    let cmd = cmd.trim_start();
    let p = match cmd.strip_prefix('"') {
        Some(rest) => &rest[..rest.find('"')?],
        None => cmd.split_whitespace().next()?,
    };
    (!p.is_empty()).then(|| p.to_string())
}

/// What .xljs files are associated with.
#[derive(Debug, PartialEq)]
pub enum Status {
    /// Nothing (or not with xljs).
    None,
    /// The exe asked about.
    This,
    /// Another xljs exe (moved, or another copy), with its path.
    Other(String),
}

/// The exe to associate: the running one, except for the console build
/// (xljsc.exe), which leaves the GUI build next to it (xljs.exe) the
/// files when there is one, so that no console window opens with them.
pub fn target_exe() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_else(|_| "xljs.exe".into());
    gui_sibling(&exe).unwrap_or(exe)
}

fn gui_sibling(exe: &Path) -> Option<PathBuf> {
    let stem = exe.file_stem()?.to_string_lossy().to_lowercase();
    let base = stem.strip_suffix('c')?;
    let ext = exe.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let gui = exe.with_file_name(format!("{base}{ext}"));
    gui.is_file().then_some(gui)
}

fn same_path(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.replace('/', "\\").trim_end_matches('\\').to_lowercase();
    norm(a) == norm(b)
}

/// What .xljs files are associated with, compared with exe.
pub fn status(exe: &Path) -> Status {
    if reg::get(&format!(r"{CLASSES}\{EXT}"), "").as_deref() != Some(PROG_ID) {
        return Status::None;
    }
    match reg::get(&format!(r"{CLASSES}\{PROG_ID}\shell\open\command"), "").as_deref().and_then(command_program) {
        Some(p) if same_path(&p, &exe.to_string_lossy()) => Status::This,
        Some(p) => Status::Other(p),
        None => Status::None,
    }
}

/// Associates .xljs files with exe for the current user.
pub fn register(exe: &Path) -> Result<()> {
    // Start afresh, so that nothing of an older registration is left.
    reg::delete(&format!(r"{CLASSES}\{PROG_ID}"))?;
    for v in entries(&exe.to_string_lossy()) {
        reg::set(&v.key, &v.name, &v.data)?;
    }
    reg::changed();
    Ok(())
}

/// Removes the association (only when .xljs points to xljs).
pub fn unregister() -> Result<()> {
    let ext = format!(r"{CLASSES}\{EXT}");
    if reg::get(&ext, "").as_deref() == Some(PROG_ID) {
        reg::delete(&ext)?;
    }
    reg::delete(&format!(r"{CLASSES}\{PROG_ID}"))?;
    reg::changed();
    Ok(())
}

#[cfg(windows)]
mod reg {
    use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, WIN32_ERROR};
    use windows::Win32::System::Registry::*;
    use windows::Win32::UI::Shell::{SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify};
    use windows::core::HSTRING;

    use crate::{Error, Result};

    fn check(r: WIN32_ERROR, op: &str, key: &str) -> Result<()> {
        if r.is_ok() {
            return Ok(());
        }
        Err(Error::path(op, format!(r"HKEY_CURRENT_USER\{key}"), std::io::Error::from_raw_os_error(r.0 as i32)))
    }

    pub fn set(key: &str, name: &str, data: &str) -> Result<()> {
        unsafe {
            let mut h = HKEY::default();
            check(RegCreateKeyExW(HKEY_CURRENT_USER, &HSTRING::from(key), None, None, REG_OPTION_NON_VOLATILE, KEY_WRITE, None, &mut h, None), "write", key)?;
            let w: Vec<u16> = data.encode_utf16().chain(Some(0)).collect();
            let bytes = std::slice::from_raw_parts(w.as_ptr() as *const u8, w.len() * 2);
            let r = RegSetValueExW(h, &HSTRING::from(name), None, REG_SZ, Some(bytes));
            let _ = RegCloseKey(h);
            check(r, "write", key)
        }
    }

    pub fn get(key: &str, name: &str) -> Option<String> {
        unsafe {
            let (k, n) = (HSTRING::from(key), HSTRING::from(name));
            let mut size = 0u32;
            if RegGetValueW(HKEY_CURRENT_USER, &k, &n, RRF_RT_REG_SZ, None, None, Some(&mut size)).is_err() {
                return None;
            }
            let mut buf = vec![0u16; size as usize / 2 + 1];
            let mut size = (buf.len() * 2) as u32;
            if RegGetValueW(HKEY_CURRENT_USER, &k, &n, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr().cast()), Some(&mut size)).is_err() {
                return None;
            }
            let end = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
            Some(String::from_utf16_lossy(&buf[..end]))
        }
    }

    /// Deletes the key with everything under it (nothing when missing).
    pub fn delete(key: &str) -> Result<()> {
        let r = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(key)) };
        if r == ERROR_FILE_NOT_FOUND {
            return Ok(());
        }
        check(r, "remove", key)
    }

    /// Tells Explorer that file types changed (icons, menus).
    pub fn changed() {
        unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None) };
    }
}

#[cfg(not(windows))]
mod reg {
    use crate::{Result, err};

    pub fn set(_key: &str, _name: &str, _data: &str) -> Result<()> {
        Err(err!("assoc.windowsOnly"))
    }
    pub fn get(_key: &str, _name: &str) -> Option<String> {
        None
    }
    pub fn delete(_key: &str) -> Result<()> {
        Err(err!("assoc.windowsOnly"))
    }
    pub fn changed() {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_for_exe() {
        let _l = crate::testutil::lock_lang("ja");
        let e = entries(r"C:\Tools\xljs.exe");
        let get = |key: &str, name: &str| e.iter().find(|v| v.key == format!(r"Software\Classes\{key}") && v.name == name).map(|v| v.data.clone());
        assert_eq!(get(".xljs", "").as_deref(), Some("xljs.script"));
        assert_eq!(get(r".xljs\OpenWithProgids", "xljs.script").as_deref(), Some(""));
        assert_eq!(get(r"xljs.script\DefaultIcon", "").as_deref(), Some(r#""C:\Tools\xljs.exe",-2"#));
        assert_eq!(get(r"xljs.script\shell\open\command", "").as_deref(), Some(r#""C:\Tools\xljs.exe" -- "%1" %*"#));
        assert_eq!(get(r"xljs.script\shell\edit\command", "").as_deref(), Some(r#""C:\Tools\xljs.exe" dev "%1""#));
        assert_eq!(get(r"xljs.script\shell\pack\command", "").as_deref(), Some(r#""C:\Tools\xljs.exe" pack --dialog "%1""#));
        assert_eq!(get(r"xljs.script\shellex\DropHandler", "").as_deref(), Some(DROP_HANDLER));
        assert_eq!(get(r"xljs.script\shell\edit", "").as_deref(), Some(t!("assoc.verbEdit").as_str()));
        // The commands name the exe that status() compares.
        assert_eq!(command_program(&get(r"xljs.script\shell\open\command", "").unwrap()).as_deref(), Some(r"C:\Tools\xljs.exe"));
    }

    #[test]
    fn command_programs() {
        assert_eq!(command_program(r#""C:\Program Files\xljs\xljs.exe" -- "%1" %*"#).as_deref(), Some(r"C:\Program Files\xljs\xljs.exe"));
        assert_eq!(command_program(r#"C:\xljs.exe "%1""#).as_deref(), Some(r"C:\xljs.exe"));
        assert_eq!(command_program(r#""unterminated"#), None);
        assert_eq!(command_program("  "), None);
        assert!(same_path(r"C:\Tools\XLJS.exe", "c:/tools/xljs.exe"));
    }

    #[test]
    fn console_build_leaves_files_to_the_gui_build() {
        let dir = crate::testutil::tempdir();
        let con = dir.join("xljsc.exe");
        std::fs::write(&con, "").unwrap();
        assert_eq!(gui_sibling(&con), None);
        let gui = dir.join("xljs.exe");
        std::fs::write(&gui, "").unwrap();
        assert_eq!(gui_sibling(&con), Some(gui.clone()));
        assert_eq!(gui_sibling(&gui), None);
    }
}
