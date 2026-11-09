use std::sync::{Arc, mpsc};

use super::{ConsoleState, pack_output, pack_script, start};
use crate::meta::AssetFs;
use crate::ui::{ASSET_ORIGIN, BUILTIN_PREFIX, BuiltinAssets, Choice, FileFilter, PickerOptions, Ui, WindowOptions};
use crate::{Result, t};

// What the start screen offers, in its order.
const LAUNCH_RUN: i64 = 0;
const LAUNCH_DEBUG: i64 = 1;
const LAUNCH_DEV: i64 = 2;
const LAUNCH_PACK: i64 = 3;
const LAUNCH_VBA: i64 = 4;

// Icons of the choices in the dialog without HTML windows (drawn by
// tools/make_launch_icons.py) and in the page (web/icons.css).
const ICON_RUN: &[u8] = include_bytes!("icons/run.ico");
const ICON_DEBUG: &[u8] = include_bytes!("icons/debug.ico");
const ICON_DEV: &[u8] = include_bytes!("icons/dev.ico");
const ICON_PACK: &[u8] = include_bytes!("icons/pack.ico");
const ICON_VBA: &[u8] = include_bytes!("icons/vba.ico");

/// The start screen of xljs started without a script (double-clicked): it
/// asks whether to run, debug or pack a script, to open the development
/// mode, or to extract the VBA code of macro workbooks, and asks for the
/// files.
pub(super) fn launch(con: &ConsoleState, u: Arc<dyn Ui>, explicit_lang: bool, stop: &super::Stop) -> i32 {
    let choice = |label: &str, note: &str, icon: &[u8]| Choice { label: t!(label), note: t!(note), icon: icon.to_vec() };
    let choices = [
        choice("cli.launchRun", "cli.launchRunNote", ICON_RUN),
        choice("cli.launchDebug", "cli.launchDebugNote", ICON_DEBUG),
        choice("cli.launchDev", "cli.launchDevNote", ICON_DEV),
        choice("cli.launchPack", "cli.launchPackNote", ICON_PACK),
        choice("cli.launchVBA", "cli.launchVBANote", ICON_VBA),
    ];
    let Some(c) = ask(&u, &choices) else {
        eprint!("{}", super::usage());
        return 2;
    };
    match c {
        LAUNCH_RUN | LAUNCH_DEBUG => {
            let Some(script) = pick_script(&*u, &t!("cli.selectScript")) else { return 0 };
            let debug = if c == LAUNCH_DEBUG { "script" } else { "" };
            start(con, u, vec![script], explicit_lang, debug, stop)
        }
        LAUNCH_DEV => super::dev(con, u, String::new(), explicit_lang),
        LAUNCH_PACK => {
            let Some(script) = pick_script(&*u, &t!("cli.selectScriptToPack")) else { return 0 };
            launch_pack_script(con, &*u, &script)
        }
        LAUNCH_VBA => {
            let books = u.open_files(&PickerOptions {
                title: t!("cli.selectMacroBooks"),
                multiple: true,
                filters: vec![FileFilter {
                    description: t!("cli.macroBooks"),
                    patterns: super::vba::MACRO_PATTERNS.iter().map(|s| s.to_string()).collect(),
                }],
                ..Default::default()
            });
            if books.is_empty() {
                return 0;
            }
            super::vba::drop_vba(con, u, &books)
        }
        _ => 0,
    }
}

/// The icons of the choices in the page (web/icons.css), in their order.
const PAGE_ICONS: [&str; 5] = ["play", "bug", "sparkles", "package", "scroll-text"];

/// The size of the start screen's page (below its title bar).
const PAGE_WIDTH: i64 = 600;
const PAGE_HEIGHT: i64 = 424;

/// Shows the start screen: a window of its own (web/launch.html) where the
/// UI has HTML windows, else Ui::choose (also when the window cannot open,
/// e.g. without the WebView2 runtime). The index of the choice, None when
/// cancelled.
fn ask(u: &Arc<dyn Ui>, choices: &[Choice]) -> Option<i64> {
    if u.supports_windows() {
        let data = serde_json::json!({
            "message": t!("cli.launchMessage"),
            "version": super::version(),
            "choices": choices.iter().zip(PAGE_ICONS).map(|(c, icon)| serde_json::json!({
                "label": c.label, "note": c.note, "icon": icon,
            })).collect::<Vec<_>>(),
        })
        .to_string();
        if let Ok(r) = crate::ui::run(u.clone(), move |su| ask_window(&*su, data)) {
            return r;
        }
    }
    u.choose("xljs", &t!("cli.launchMessage"), choices)
}

/// Opens the start screen's window with data (the texts) and waits for the
/// choice.
fn ask_window(u: &dyn Ui, data: String) -> Result<Option<i64>> {
    // Some(message) from the page, None once the window has closed.
    let (tx, rx) = mpsc::channel::<Option<String>>();
    let (t1, t2) = (tx.clone(), tx);
    let w = u.open_window(WindowOptions {
        title: "xljs".into(),
        width: PAGE_WIDTH,
        height: PAGE_HEIGHT,
        fixed_size: true,
        custom_title_bar: true,
        url: format!("{ASSET_ORIGIN}{BUILTIN_PREFIX}launch.html"),
        assets: Some(Arc::new(LaunchAssets)),
        on_message: Some(Arc::new(move |m, _| {
            let _ = t1.send(Some(m));
        })),
        on_closed: Some(Arc::new(move || {
            let _ = t2.send(None);
        })),
        ..Default::default()
    })?;
    let mut result = None;
    // Whether the window is still open (the loop ends on the choice, Esc or
    // the window closing).
    let mut open = false;
    while let Ok(Some(m)) = rx.recv() {
        let v: serde_json::Value = serde_json::from_str(&m).unwrap_or_default();
        match v["op"].as_str() {
            Some("ready") => {
                w.post(data.clone());
                continue;
            }
            Some("choose") => result = v["index"].as_i64(),
            Some("cancel") => {}
            _ => continue,
        }
        open = true;
        break;
    }
    if open {
        // Wait until the window is gone, so that it is not left behind the
        // dialogs that follow.
        w.close();
        while let Ok(Some(_)) = rx.recv() {}
    }
    Ok(result)
}

/// Serves the built-in files under BUILTIN_PREFIX.
struct LaunchAssets;

impl AssetFs for LaunchAssets {
    fn read(&self, name: &str) -> std::io::Result<Vec<u8>> {
        match name.strip_prefix(BUILTIN_PREFIX) {
            Some(rest) => BuiltinAssets.read(rest),
            None => Err(std::io::Error::new(std::io::ErrorKind::NotFound, name.to_string())),
        }
    }
    fn files(&self, _: &str) -> std::io::Result<Vec<String>> {
        Ok(Vec::new())
    }
}

/// Asks for a script; None when cancelled.
fn pick_script(u: &dyn Ui, title: &str) -> Option<String> {
    let paths = u.open_files(&PickerOptions {
        title: title.to_string(),
        filters: vec![FileFilter { description: t!("cli.scripts"), patterns: vec!["*.js".into(), "*.mjs".into(), "*.ts".into()] }],
        ..Default::default()
    });
    paths.into_iter().next()
}

/// Asks where to save the exe for the script, creates it and offers to
/// show it in its folder.
fn launch_pack_script(con: &ConsoleState, u: &dyn Ui, script: &str) -> i32 {
    let out = pack_output(script);
    let abs = std::path::absolute(&out).unwrap_or_else(|_| out.clone().into());
    let Some(out) = u.save_file(&PickerOptions {
        title: t!("cli.packSaveTitle"),
        start_in: abs.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(),
        suggested_name: abs.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
        filters: vec![FileFilter { description: t!("cli.programs"), patterns: vec!["*.exe".into()] }],
        ..Default::default()
    }) else {
        return 0;
    };
    if let Err(e) = pack_script(script, &out) {
        return con.fail(Some(u), &e);
    }
    if u.confirm("xljs", &t!("cli.packed", out))
        && let Err(e) = u.show_in_folder(&[out])
    {
        return con.fail(Some(u), &e);
    }
    0
}
