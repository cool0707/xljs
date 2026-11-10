//! The development mode (xljs dev): a window with an AI chat (a web
//! browser pane: Copilot, Gemini, ChatGPT...) beside a canvas for the
//! script (web/dev.html: the code, the differences of an AI's answer, the
//! form or app preview, earlier versions and the output of runs).
//!
//! Files the chat downloads are matched to the script's files and shown as
//! differences; when the user accepts, the file is replaced and its old
//! content kept as a version (versions.rs). Runs are processes of their
//! own (runner.rs) with the endless loop check on and fs.remove asking
//! before deleting much; their errors come back with where they happened
//! and a request to paste into the chat (report.rs).
//!
//! The page and this side talk in JSON: the page sends {op, ...}, this
//! side answers with {type, ...} events.

pub mod notify;
pub mod project;
pub mod report;
pub mod runner;
pub mod versions;
pub mod wizard;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::meta::AssetFs;
use crate::ui::{
    ASSET_ORIGIN, BUILTIN_PREFIX, BrowserCommand, BrowserEvent, BrowserOptions, BuiltinAssets, FileFilter, PickerOptions, Ui, Window, WindowOptions,
};
use crate::{Error, Result, err, i18n, t};
use project::{Project, read_text, write_text};
use runner::{Run, RunEvent};

/// The instructions for AI chats (docs/ai/prompt.md).
pub fn ai_prompt() -> &'static str {
    crate::embed::AI_PROMPT.text()
}

/// The page of the development window.
pub const DEV_PAGE: &str = "dev.html";

/// The path under ASSET_ORIGIN that serves the script's folder to the app
/// preview.
const PROJECT_PREFIX: &str = "_project/";

/// The AI chats offered on the start of the chat pane: (name, URL, icon).
/// The icon is a class of the bundled Font Awesome brands (dev.css colors
/// it per service).
pub const SERVICES: &[(&str, &str, &str)] = &[
    ("Microsoft Copilot", "https://copilot.microsoft.com/", "copilot"),
    ("Microsoft 365 Copilot", "https://m365.cloud.microsoft/chat", "m365"),
    ("Gemini", "https://gemini.google.com/app", "gemini"),
    ("ChatGPT", "https://chatgpt.com/", "openai"),
    ("Claude", "https://claude.ai/new", "claude"),
];

/// How the development mode starts.
#[derive(Clone, Default)]
pub struct DevOptions {
    /// The script to open ("" = none; a folder = choose in it).
    pub path: String,
    /// The exe that runs scripts (this one).
    pub exe: PathBuf,
    /// Arguments for every run before the script (--lang=...).
    pub run_args: Vec<String>,
    /// Where the development mode keeps what is not a script's (the
    /// recent scripts).
    pub settings: PathBuf,
    /// The settings distributed with the exe (xljs.json next to it; ""
    /// = none): the default chat and more chats to choose from.
    pub exe_settings: PathBuf,
}

/// The name of the settings file read from the folder of the exe.
pub const EXE_SETTINGS: &str = "xljs.json";

/// The settings file next to exe.
pub fn exe_settings(exe: &Path) -> PathBuf {
    exe.parent().map(|d| d.join(EXE_SETTINGS)).unwrap_or_default()
}

/// The settings distributed with the exe (xljs.json):
/// `{"dev": {"chat": URL, "services": [{name, url, icon}], "builtinServices": bool}}`
/// (and `"userAgent"`, read by net::agent).
#[derive(Deserialize, Default)]
#[serde(default)]
struct ExeSettings {
    dev: DevDefaults,
}

#[derive(Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct DevDefaults {
    /// The chat opened when a script has none of its own, and at the start.
    chat: String,
    /// Chats offered before the built-in ones.
    services: Vec<Service>,
    /// Whether the built-in chats (SERVICES) are offered too.
    builtin_services: bool,
}

impl Default for DevDefaults {
    fn default() -> Self {
        DevDefaults { chat: String::new(), services: Vec::new(), builtin_services: true }
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Service {
    name: String,
    url: String,
    icon: String,
}

fn is_web(url: &str) -> bool {
    url.starts_with("https://") || url.starts_with("http://")
}

/// Reads the exe's settings; a missing file is the defaults.
fn load_exe_settings(p: &Path) -> Result<DevDefaults> {
    if p.as_os_str().is_empty() || !p.is_file() {
        return Ok(DevDefaults::default());
    }
    let text = read_text(p)?;
    let s: ExeSettings = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|e| err!("dev.exeSettingsInvalid", p.display(), e))?;
    let mut d = s.dev;
    d.chat = d.chat.trim().to_string();
    if !is_web(&d.chat) {
        d.chat.clear();
    }
    d.services.retain(|s| is_web(s.url.trim()));
    for s in &mut d.services {
        s.url = s.url.trim().to_string();
        if s.name.trim().is_empty() {
            s.name = s.url.clone();
        }
    }
    Ok(d)
}

/// The settings file of the development mode in the user's profile.
pub fn default_settings() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("xljs").join("dev.json")
}

/// What reaches the session.
pub enum Event {
    Page(String, Vec<String>),
    Browser(BrowserEvent),
    Run(RunEvent),
    Closed,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct GlobalSettings {
    recent: Vec<String>,
}

/// The settings of a script's folder (.xljs/dev.json).
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct ProjectSettings {
    /// The AI chat used for each script (entry name → URL).
    chat: HashMap<String, String>,
    /// Scripts whose name is temporary (the wizard's "later" and "up to
    /// the AI"): the first answer of the AI may rename them.
    unnamed: Vec<String>,
    /// Where each script was last packed into an exe (entry name → path).
    exe: HashMap<String, String>,
}

fn load<T: for<'a> Deserialize<'a> + Default>(p: &Path) -> T {
    std::fs::read(p).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn store<T: Serialize>(p: &Path, v: &T) {
    if let Ok(s) = serde_json::to_string_pretty(v) {
        let _ = write_text(p, &s);
    }
}

/// Serves the built-in pages and, for the app preview, the script's folder.
struct DevAssets {
    dir: Arc<Mutex<Option<PathBuf>>>,
}

impl AssetFs for DevAssets {
    fn read(&self, name: &str) -> std::io::Result<Vec<u8>> {
        if let Some(rest) = name.strip_prefix(BUILTIN_PREFIX) {
            return BuiltinAssets.read(rest);
        }
        if let Some(rest) = name.strip_prefix(PROJECT_PREFIX)
            && let Some(dir) = self.dir.lock().unwrap().clone()
            && crate::meta::assets_valid_path(rest)
            && !rest.split('/').any(|p| p.starts_with('.'))
        {
            return std::fs::read(dir.join(rest));
        }
        Err(std::io::Error::new(std::io::ErrorKind::NotFound, name.to_string()))
    }
    fn files(&self, _root: &str) -> std::io::Result<Vec<String>> {
        Ok(Vec::new())
    }
}

/// What a run is for.
#[derive(Clone, Copy, PartialEq, Debug)]
enum RunKind {
    Run,
    Debug,
    Preview,
}

struct Running {
    run: Run,
    kind: RunKind,
    stderr: String,
    preview: Option<PathBuf>,
}

/// Opens the development window and serves it until it is closed.
pub fn run(u: Arc<dyn Ui>, o: DevOptions) -> Result<i32> {
    if !u.supports_windows() {
        return Err(err!("dev.needsWindows"));
    }
    let (tx, rx) = mpsc::channel::<Event>();
    let downloads = std::env::temp_dir().join("xljs-dev").join(format!("{}", std::process::id()));
    let _ = std::fs::create_dir_all(&downloads);
    let dir = Arc::new(Mutex::new(None));
    let (t1, t2, t3) = (tx.clone(), tx.clone(), tx.clone());
    let w = u.open_window(WindowOptions {
        title: t!("dev.title"),
        width: 1280,
        height: 820,
        // The page draws the bar (dev.html #titlebar) with the file's name.
        custom_title_bar: true,
        url: format!("{ASSET_ORIGIN}{BUILTIN_PREFIX}{DEV_PAGE}"),
        assets: Some(Arc::new(DevAssets { dir: dir.clone() })),
        debug: std::env::var("XLJS_DEV_DEBUG").is_ok_and(|v| !v.is_empty()),
        on_message: Some(Arc::new(move |m, files| {
            let _ = t1.send(Event::Page(m, files));
        })),
        on_closed: Some(Arc::new(move || {
            let _ = t2.send(Event::Closed);
        })),
        browser: Some(BrowserOptions {
            downloads: downloads.to_string_lossy().into_owned(),
            // The answers of the chat are only read and compared.
            trusted_extensions: project::TEXT_EXTS.iter().map(|e| e.to_string()).collect(),
            agent: crate::net::agent::for_pane(&o.exe_settings),
            on_event: Arc::new(move |e| {
                let _ = t3.send(Event::Browser(e));
            }),
        }),
        ..Default::default()
    })?;
    let mut s = Session::new(u, w, o, tx, dir);
    while let Ok(ev) = rx.recv() {
        if !s.event(ev) {
            break;
        }
    }
    s.shutdown();
    let _ = std::fs::remove_dir_all(&downloads);
    Ok(0)
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Request {
    op: String,
    path: String,
    file: String,
    text: String,
    note: String,
    url: String,
    /// The template of a new script (new): "script", "form" or "app".
    template: String,
    debug: bool,
    n: u32,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    scale: f64,
    /// The parts of the browser pane under a menu of the page (show).
    holes: Vec<[f64; 4]>,
    /// The new name of the file (apply).
    rename: String,
    /// What to copy (intro): "prompt" (the instructions alone), "script"
    /// (the current script alone) or "" (both).
    mode: String,
    /// The wizard's answers.
    answers: wizard::Answers,
    /// Shown by itself (preview): only when the script opens a form.
    auto: bool,
    /// The elements picked in the preview (pick).
    elements: Vec<report::Picked>,
    /// With the whole file (pick).
    full: bool,
}

/// The state of the development window.
pub struct Session {
    u: Arc<dyn Ui>,
    w: Arc<dyn Window>,
    o: DevOptions,
    tx: Sender<Event>,
    dir: Arc<Mutex<Option<PathBuf>>>,
    project: Option<Project>,
    /// The file being edited (relative).
    current: String,
    /// The files saved in this session (the first save keeps a version).
    saved: Vec<String>,
    running: Option<Running>,
    next_run: u64,
    ready: bool,
    /// The page the chat pane shows ("" = none).
    chat_url: String,
    /// The settings distributed with the exe.
    defaults: DevDefaults,
}

impl Session {
    pub fn new(u: Arc<dyn Ui>, w: Arc<dyn Window>, o: DevOptions, tx: Sender<Event>, dir: Arc<Mutex<Option<PathBuf>>>) -> Session {
        Session { u, w, o, tx, dir, project: None, current: String::new(), saved: Vec::new(), running: None, next_run: 1, ready: false, chat_url: String::new(), defaults: DevDefaults::default() }
    }

    fn post(&self, v: Value) {
        self.w.post(v.to_string());
    }

    fn fail(&self, e: &Error) {
        self.post(json!({"type": "error", "message": i18n::error(e)}));
    }

    /// Handles an event; false when the window has closed.
    pub fn event(&mut self, ev: Event) -> bool {
        match ev {
            Event::Closed => return false,
            Event::Page(m, files) => {
                let r: Request = match serde_json::from_str(&m) {
                    Ok(r) => r,
                    Err(_) => return true,
                };
                if let Err(e) = self.request(r, files) {
                    self.fail(&e);
                }
            }
            Event::Browser(b) => self.browser_event(b),
            Event::Run(r) => self.run_event(r),
        }
        true
    }

    fn project(&self) -> Result<&Project> {
        self.project.as_ref().ok_or_else(|| err!("dev.noScript"))
    }

    fn request(&mut self, r: Request, files: Vec<String>) -> Result<()> {
        match r.op.as_str() {
            "ready" => {
                self.ready = true;
                let failed = match load_exe_settings(&self.o.exe_settings) {
                    Ok(d) => {
                        self.defaults = d;
                        None
                    }
                    Err(e) => Some(e),
                };
                self.post(self.init());
                if let Some(e) = &failed {
                    self.fail(e);
                }
                let path = std::mem::take(&mut self.o.path);
                if !path.is_empty() {
                    if Path::new(&path).is_dir() {
                        self.open_dialog(&path);
                    } else {
                        self.open(&path)?;
                    }
                }
                // Nothing to show yet: the default chat.
                if self.project.is_none() && self.chat_url.is_empty() && !self.defaults.chat.is_empty() {
                    self.chat_url = self.defaults.chat.clone();
                    self.w.browser(BrowserCommand::Navigate(self.chat_url.clone()));
                    self.post(json!({"type": "url", "url": self.chat_url}));
                }
            }
            "open" => {
                if r.path.is_empty() {
                    let start = self.project.as_ref().map(|p| p.dir.to_string_lossy().into_owned()).unwrap_or_default();
                    self.open_dialog(&start);
                } else {
                    self.open(&r.path)?;
                }
            }
            "drop" => self.drop(&files)?,
            "new" => self.create(Template::parse(&r.template))?,
            "read" => {
                let p = self.project()?;
                let path = p.path(&r.file)?;
                let exists = path.is_file();
                let text = if exists { read_text(&path)? } else { String::new() };
                self.post(json!({"type": "file", "file": r.file, "text": text, "exists": exists}));
            }
            "edit" => {
                // The page shows this file now (for downloads and the chat).
                self.current = r.file;
            }
            "save" => {
                let p = self.project()?.clone();
                let path = p.path(&r.file)?;
                if !self.saved.contains(&r.file) {
                    versions::keep(&p.dir, &r.file, &t!("dev.noteEdited"))?;
                    self.saved.push(r.file.clone());
                }
                write_text(&path, &r.text)?;
                self.post(json!({"type": "saved", "file": r.file}));
                if r.file == p.entry_name() {
                    self.reload_meta();
                }
            }
            "apply" => {
                let p = self.project()?.clone();
                p.path(&r.file)?;
                let note = if r.note.is_empty() { t!("dev.noteApplied") } else { t!("dev.noteAppliedFrom", r.note) };
                versions::replace(&p.dir, &r.file, &r.text, &note)?;
                // The AI's answer: what it knows of the file now.
                notify::remember(&p.dir, &r.file, &r.text)?;
                if !r.rename.is_empty() && r.rename != r.file && r.file == p.entry_name() {
                    return self.rename_entry(&p, &r.rename);
                }
                self.post(json!({"type": "applied", "file": r.file, "text": r.text}));
                self.post_versions(&r.file);
                self.post_files();
                if r.file == p.entry_name() {
                    self.reload_meta();
                }
            }
            "versions" => self.post_versions(&r.file),
            "version" => {
                let p = self.project()?;
                p.path(&r.file)?;
                let text = versions::read(&p.dir, &r.file, r.n)?;
                self.post(json!({"type": "version", "file": r.file, "n": r.n, "text": text}));
            }
            "restore" => {
                let p = self.project()?.clone();
                p.path(&r.file)?;
                let text = versions::restore(&p.dir, &r.file, r.n, &t!("dev.noteRestored", r.n))?;
                self.post(json!({"type": "applied", "file": r.file, "text": text}));
                self.post_versions(&r.file);
            }
            "run" => self.start_run(if r.debug { RunKind::Debug } else { RunKind::Run })?,
            "preview" => {
                if r.auto {
                    // Shown by itself: not while something runs, and only
                    // for a script that opens a form.
                    let busy = self.running.as_ref().is_some_and(|r| r.run.running());
                    if busy || !self.project()?.opens_form() {
                        self.post(json!({"type": "previewSkipped", "busy": busy}));
                        return Ok(());
                    }
                }
                self.start_run(RunKind::Preview)?
            }
            "pick" => {
                let p = self.project()?;
                if r.elements.is_empty() {
                    return Ok(());
                }
                let text = report::pick(p, &r.elements, &r.text, r.full);
                self.post(json!({"type": "copy", "what": "pick", "text": text}));
                if r.full {
                    let file = report::pick_file(p);
                    if let Ok(text) = read_text(&p.dir.join(&file)) {
                        notify::remember(&p.dir, &file, &text)?;
                    }
                }
            }
            "stop" => {
                if let Some(r) = &self.running {
                    r.run.kill();
                }
            }
            "intro" => {
                let (prompt, script) = match r.mode.as_str() {
                    "prompt" => (true, false),
                    "script" => (false, true),
                    _ => (true, true),
                };
                let text = report::intro(self.project.as_ref(), prompt, script);
                if text.is_empty() {
                    self.post(json!({"type": "info", "message": t!("dev.introNoScript")}));
                    return Ok(());
                }
                let what = if prompt { "intro" } else { "script" };
                self.post(json!({"type": "copy", "what": what, "text": text}));
                if script
                    && let Some(p) = &self.project
                    && let Ok(text) = read_text(&p.entry)
                {
                    notify::remember(&p.dir, &p.entry_name(), &text)?;
                }
            }
            "notify" => {
                // The user's own changes since the AI's answers.
                let p = self.project()?;
                let current = if r.file.is_empty() { self.current.clone() } else { r.file.clone() };
                let changes = notify::changes(p, &current);
                if changes.is_empty() {
                    self.post(json!({"type": "info", "message": t!("dev.notifyNone")}));
                    return Ok(());
                }
                self.post(json!({"type": "copy", "what": "notify", "text": notify::prompt(&changes)}));
                for c in &changes {
                    notify::remember(&p.dir, &c.file, &c.text)?;
                }
            }
            "dragFile" => {
                // A file of the project dragged out of the page (to the chat,
                // as from Explorer).
                let p = self.project()?;
                let path = p.path(&r.file)?;
                if path.is_file() {
                    self.w.browser(BrowserCommand::DragFiles(vec![path.to_string_lossy().into_owned()]));
                }
            }
            "pack" => self.pack()?,
            "showFolder" => {
                let p = self.project()?;
                self.u.show_in_folder(&[p.entry.to_string_lossy().into_owned()])?;
            }
            "openUrl" => {
                if is_web(&r.url) {
                    self.u.open_path(&r.url)?;
                }
            }
            "chat" => {
                // The page chose a chat (or none: back to the start).
                if !r.url.is_empty() && !is_web(&r.url) {
                    return Ok(());
                }
                if let Some(p) = &self.project {
                    let f = p.dir.join(project::DATA_DIR).join("dev.json");
                    let mut ps: ProjectSettings = load(&f);
                    if r.url.is_empty() {
                        ps.chat.remove(&p.entry_name());
                    } else {
                        ps.chat.insert(p.entry_name(), r.url.clone());
                    }
                    store(&f, &ps);
                }
                if r.url.is_empty() {
                    self.chat_url.clear();
                    self.w.browser(BrowserCommand::Navigate("about:blank".into()));
                    self.w.browser(BrowserCommand::Hide);
                } else {
                    self.w.browser(BrowserCommand::Navigate(r.url));
                }
            }
            "wizardBooks" => {
                let paths = self.u.open_files(&PickerOptions {
                    title: t!("wizard.selectBooks"),
                    multiple: true,
                    filters: vec![FileFilter {
                        description: t!("wizard.books"),
                        patterns: ["*.xlsm", "*.xlsb", "*.xls", "*.xlsx", "*.xltm", "*.xlam"].iter().map(|s| s.to_string()).collect(),
                    }],
                    ..Default::default()
                });
                if !paths.is_empty() {
                    let dir = Path::new(&paths[0]).parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
                    self.post(json!({"type": "wizardBooks", "books": wizard::books(&paths), "dir": dir}));
                }
            }
            "wizardFolder" => {
                if let Some(d) = self.u.pick_directory(&PickerOptions { title: t!("wizard.folderTitle"), start_in: r.path, ..Default::default() }) {
                    self.post(json!({"type": "wizardFolder", "path": d}));
                }
            }
            "wizardPrompt" => {
                let text = wizard::prompt(&r.answers);
                self.post(json!({"type": "wizardPrompt", "text": text, "file": wizard::file_name(&r.answers), "attachment": wizard::attachment_name(&r.answers)}));
            }
            "wizardSave" => {
                let first = r.answers.books.first().cloned().unwrap_or_default();
                let p = Path::new(&first);
                let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                if let Some(out) = self.u.save_file(&PickerOptions {
                    title: t!("wizard.saveTitle"),
                    start_in: p.parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default(),
                    suggested_name: format!("{stem}_xljs.md"),
                    filters: vec![FileFilter { description: t!("wizard.textFiles"), patterns: vec!["*.md".into(), "*.txt".into()] }],
                    ..Default::default()
                }) {
                    wizard::save_text(&r.answers, Path::new(&out))?;
                    self.post(json!({"type": "info", "message": t!("wizard.savedText", out)}));
                }
            }
            "wizardCreate" => self.wizard_create(&r.answers, &r.path, &r.text)?,
            "browser" => {
                let cmd = match r.path.as_str() {
                    "show" => BrowserCommand::Show { x: r.x, y: r.y, width: r.width, height: r.height, scale: if r.scale > 0.0 { r.scale } else { 1.0 }, holes: r.holes.clone() },
                    "hide" => BrowserCommand::Hide,
                    "back" => BrowserCommand::Back,
                    "forward" => BrowserCommand::Forward,
                    "reload" => BrowserCommand::Reload,
                    _ => return Ok(()),
                };
                self.w.browser(cmd);
            }
            _ => {}
        }
        Ok(())
    }

    /// The first message to the page: its texts and the choices.
    fn init(&self) -> Value {
        let gs: GlobalSettings = load(&self.o.settings);
        let recent: Vec<Value> = gs
            .recent
            .iter()
            .filter(|p| Path::new(p).is_file())
            .map(|p| json!({"path": p, "name": Path::new(p).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()}))
            .collect();
        json!({
            "type": "init",
            "lang": i18n::lang(),
            "texts": texts(),
            // For the wizard's preview of the look (form.html).
            "formTexts": texts_under("form."),
            "services": self.services(),
            "recent": recent,
            "home": default_dir(&gs),
            // For the editor's completion (dev-types.js).
            "dts": crate::dts(),
        })
    }

    /// The chats offered on the start of the chat pane: those of the exe's
    /// settings, then the built-in ones.
    fn services(&self) -> Vec<Value> {
        let d = &self.defaults;
        let own = d.services.iter().map(|s| json!({"name": s.name, "url": s.url, "icon": s.icon}));
        let builtin = SERVICES.iter().filter(|_| d.builtin_services).map(|(n, u, i)| json!({"name": n, "url": u, "icon": i}));
        let mut v: Vec<Value> = own.chain(builtin).collect();
        // The default chat is always among them (to come back to it).
        if !d.chat.is_empty() && !v.iter().any(|s| s["url"] == d.chat.as_str()) {
            v.insert(0, json!({"name": d.chat, "url": d.chat, "icon": ""}));
        }
        v
    }

    fn open_dialog(&mut self, start: &str) {
        let paths = self.u.open_files(&PickerOptions {
            title: t!("cli.selectScript"),
            start_in: start.to_string(),
            filters: vec![FileFilter { description: t!("cli.scripts"), patterns: vec!["*.js".into(), "*.mjs".into(), "*.ts".into()] }],
            ..Default::default()
        });
        if let Some(p) = paths.into_iter().next()
            && let Err(e) = self.open(&p)
        {
            self.fail(&e);
        }
    }

    /// Files dropped on the window: the first script is opened (a folder:
    /// choose a script in it).
    fn drop(&mut self, files: &[String]) -> Result<()> {
        let files: Vec<&String> = files.iter().filter(|f| !f.is_empty()).collect();
        if let Some(d) = files.iter().find(|f| Path::new(f.as_str()).is_dir()) {
            self.open_dialog(d);
            return Ok(());
        }
        match files.iter().find(|f| is_script(f)) {
            Some(f) => self.open(f),
            None => match files.first() {
                Some(f) => Err(err!("dev.dropNotScript", Path::new(f.as_str()).file_name().map(|n| n.to_string_lossy()).unwrap_or_default())),
                None => Ok(()),
            },
        }
    }

    /// Opens a script: the page gets its files and the chat it was
    /// developed with.
    pub fn open(&mut self, path: &str) -> Result<()> {
        let p = Project::open(path)?;
        let text = read_text(&p.entry)?;
        let settings = p.dir.join(project::DATA_DIR).join("dev.json");
        let mut ps: ProjectSettings = load(&settings);
        // The folder's settings may come with the script: only web pages.
        let mut chat = ps.chat.get(&p.entry_name()).filter(|u| is_web(u)).cloned().unwrap_or_default();
        // A script without a chat of its own takes the one open now, else
        // the default chat (remembered once the chat shows it).
        let keep_chat = chat.is_empty() && !self.chat_url.is_empty();
        if keep_chat {
            chat = self.chat_url.clone();
            ps.chat.insert(p.entry_name(), chat.clone());
            store(&settings, &ps);
        } else if chat.is_empty() {
            chat = self.defaults.chat.clone();
        }
        // The recent scripts, newest first.
        let mut gs: GlobalSettings = load(&self.o.settings);
        let abs = p.entry.to_string_lossy().into_owned();
        gs.recent.retain(|r| r != &abs);
        gs.recent.insert(0, abs.clone());
        gs.recent.truncate(10);
        store(&self.o.settings, &gs);
        *self.dir.lock().unwrap() = Some(p.dir.clone());
        self.current = p.entry_name();
        self.saved.clear();
        self.post(json!({
            "type": "project",
            "path": abs,
            "dir": p.dir.to_string_lossy(),
            "entry": p.entry_name(),
            "text": text,
            "app": p.meta.is_app(),
            "html": p.meta.html,
            "title": p.meta.title,
            "files": p.files(),
            "chat": chat,
        }));
        let entry = p.entry_name();
        self.project = Some(p);
        self.post_versions(&entry);
        if chat.is_empty() {
            self.w.browser(BrowserCommand::Hide);
        } else if !keep_chat {
            self.w.browser(BrowserCommand::Navigate(chat));
        }
        Ok(())
    }

    fn reload_meta(&mut self) {
        let Some(p) = &self.project else { return };
        if let Ok(np) = Project::open(&p.entry.to_string_lossy()) {
            self.post(json!({"type": "meta", "app": np.meta.is_app(), "html": np.meta.html, "title": np.meta.title}));
            self.project = Some(np);
        }
    }

    fn post_files(&self) {
        if let Some(p) = &self.project {
            self.post(json!({"type": "files", "files": p.files()}));
        }
    }

    fn post_versions(&self, file: &str) {
        let Some(p) = &self.project else { return };
        let list = versions::list(&p.dir, file);
        self.post(json!({"type": "versions", "file": file, "versions": list}));
    }

    /// Creates a new script from a template (and for an app its page) and
    /// opens it.
    fn create(&mut self, tpl: Template) -> Result<()> {
        let name = match tpl {
            Template::Script => t!("dev.newScriptName"),
            Template::Form => t!("dev.newFormName"),
            Template::App => t!("dev.newAppName"),
        };
        let Some(path) = self.u.save_file(&PickerOptions {
            title: t!("dev.newTitle"),
            suggested_name: format!("{name}.js"),
            filters: vec![FileFilter { description: t!("cli.scripts"), patterns: vec!["*.js".into(), "*.ts".into()] }],
            ..Default::default()
        }) else {
            return Ok(());
        };
        write_template(Path::new(&path), tpl)?;
        self.open(&path)
    }

    /// Creates the script the wizard was answered for in folder, opens it
    /// and copies the first message for the chat. The macros and outlines
    /// of a port go to a file next to it, for the page to offer to drag
    /// to the chat.
    fn wizard_create(&mut self, a: &wizard::Answers, folder: &str, text: &str) -> Result<()> {
        let dir = Path::new(folder);
        if folder.is_empty() || !dir.is_dir() {
            return Err(err!("wizard.noFolder"));
        }
        let path = wizard::free_path(dir, &wizard::file_name(a));
        if a.mode == "app" {
            write_template(&path, Template::App)?;
        } else {
            write_text(&path, &wizard::placeholder(&path_title(&path)))?;
        }
        let entry = path.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        if a.name_mode != "name" || a.name.trim().is_empty() {
            let f = dir.join(project::DATA_DIR).join("dev.json");
            let mut ps: ProjectSettings = load(&f);
            ps.unnamed.push(entry);
            store(&f, &ps);
        }
        let attachment = wizard::write_attachment(a, dir)?;
        self.open(&path.to_string_lossy())?;
        match attachment.as_deref().and_then(|p| p.file_name()) {
            Some(name) => {
                self.post(json!({"type": "wizardAttach", "file": name.to_string_lossy()}));
                self.post(json!({"type": "copy", "what": "wizardAttach", "text": text}));
            }
            None => self.post(json!({"type": "copy", "what": "wizard", "text": text})),
        }
        Ok(())
    }

    /// Packs the script into an exe (as xljs pack): asks where to save it,
    /// remembers the place for the next time and offers to show it.
    fn pack(&mut self) -> Result<()> {
        let p = self.project()?.clone();
        let script = p.entry.to_string_lossy().into_owned();
        let f = settings_path(&p);
        let mut ps: ProjectSettings = load(&f);
        let last = ps.exe.get(&p.entry_name()).cloned().unwrap_or_else(|| crate::pack::default_output(&script));
        let last = Path::new(&last);
        let Some(out) = self.u.save_file(&PickerOptions {
            title: t!("cli.packSaveTitle"),
            start_in: last.parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default(),
            suggested_name: last.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
            filters: vec![FileFilter { description: t!("cli.programs"), patterns: vec!["*.exe".into()] }],
            ..Default::default()
        }) else {
            return Ok(());
        };
        self.post(json!({"type": "info", "message": t!("dev.packing")}));
        let warnings = crate::pack::pack_script(&self.o.exe, &script, Path::new(&out))?;
        ps.exe.insert(p.entry_name(), out.clone());
        store(&f, &ps);
        self.post(json!({"type": "packed", "path": out, "warnings": warnings}));
        if self.u.confirm("xljs", &t!("cli.packed", out)) {
            self.u.show_in_folder(&[out])?;
        }
        Ok(())
    }

    /// Renames the entry script (with its versions and its chat) after the
    /// AI's first answer named it.
    fn rename_entry(&mut self, p: &Project, to: &str) -> Result<()> {
        let from = p.entry_name();
        let new = p.path(to)?;
        if new.exists() {
            return Err(Error::path("rename", new.display().to_string(), std::io::Error::from(std::io::ErrorKind::AlreadyExists)));
        }
        std::fs::rename(&p.entry, &new).map_err(|e| Error::Link { op: "rename".into(), old: p.entry.display().to_string(), new: new.display().to_string(), source: e })?;
        let (vo, vn) = (versions::folder(&p.dir, &from), versions::folder(&p.dir, to));
        if vo.is_dir() && !vn.exists() {
            let _ = std::fs::rename(vo, vn);
        }
        notify::rename(&p.dir, &from, to);
        let f = settings_path(p);
        let mut ps: ProjectSettings = load(&f);
        ps.unnamed.retain(|u| u != &from);
        if let Some(url) = ps.chat.remove(&from) {
            ps.chat.insert(to.to_string(), url);
        }
        if let Some(exe) = ps.exe.remove(&from) {
            ps.exe.insert(to.to_string(), exe);
        }
        store(&f, &ps);
        let mut gs: GlobalSettings = load(&self.o.settings);
        let old = p.entry.to_string_lossy().into_owned();
        gs.recent.retain(|r| r != &old);
        store(&self.o.settings, &gs);
        self.open(&new.to_string_lossy())?;
        self.post(json!({"type": "info", "message": t!("wizard.renamed", from, to)}));
        Ok(())
    }
}

/// The templates of a new script.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Template {
    Script,
    Form,
    App,
}

impl Template {
    fn parse(s: &str) -> Template {
        match s {
            "form" => Template::Form,
            "app" => Template::App,
            _ => Template::Script,
        }
    }
}

/// The title of a new script: its file name without the extension.
fn path_title(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Writes a new script at p from a template (an app also gets ui/index.html).
fn write_template(p: &Path, tpl: Template) -> Result<()> {
    let title = path_title(p);
    match tpl {
        Template::App => {
            write_text(p, &format!(
                "// ==xljs==\n// @mode   app\n// @html   ui/index.html\n// @assets ui/\n// @title  {title}\n// ==/xljs==\n\n// {}\nexport function hello() {{\n  return \"{}\";\n}}\n",
                t!("dev.templateAppComment"),
                t!("dev.templateHello"),
            ))?;
            let html = p.parent().unwrap_or(Path::new(".")).join("ui").join("index.html");
            if !html.exists() {
                write_text(&html, &format!(
                    "<!doctype html>\n<html lang=\"{}\">\n<head>\n<meta charset=\"utf-8\">\n<title>{title}</title>\n</head>\n<body>\n  <h1>{title}</h1>\n  <button data-action=\"hello\" data-result=\"message\">OK</button>\n  <p data-bind=\"message\"></p>\n</body>\n</html>\n",
                    i18n::lang()
                ))?;
            }
        }
        Template::Form => {
            let js_title = serde_json::to_string(&title).unwrap_or_default();
            write_text(p, &format!(
                "// ==xljs==\n// @title {title}\n// ==/xljs==\n\n// {}\nconst values = await ui.form({{\n  title: {js_title},\n  fields: [\n    {{ name: \"name\", label: {}, required: true }},\n  ],\n  submit: \"OK\",\n}});\nif (values) {{\n  alert({});\n}}\n",
                t!("dev.templateFormComment"),
                serde_json::to_string(&t!("dev.templateFormLabel")).unwrap_or_default(),
                format!("`{}`", t!("dev.templateFormHello", "${values.name}")),
            ))?;
        }
        Template::Script => {
            write_text(p, &format!("// ==xljs==\n// @title {title}\n// ==/xljs==\n\nalert(\"{}\");\n", t!("dev.templateHello")))?;
        }
    }
    Ok(())
}

impl Session {

    fn browser_event(&mut self, b: BrowserEvent) {
        match b {
            BrowserEvent::Url(url) => {
                let web = is_web(&url);
                self.chat_url = if web { url.clone() } else { String::new() };
                if let Some(p) = &self.project
                    && web
                {
                    let f = p.dir.join(project::DATA_DIR).join("dev.json");
                    let mut ps: ProjectSettings = load(&f);
                    if ps.chat.get(&p.entry_name()) != Some(&url) {
                        ps.chat.insert(p.entry_name(), url.clone());
                        store(&f, &ps);
                    }
                }
                self.post(json!({"type": "url", "url": url}));
            }
            BrowserEvent::Downloaded { path, name } => {
                if let Err(e) = self.downloaded(&path, &name) {
                    self.fail(&e);
                }
            }
            BrowserEvent::DownloadFailed { name } => self.post(json!({"type": "error", "message": t!("dev.downloadFailed", name)})),
            BrowserEvent::Failed(e) => self.post(json!({"type": "browserFailed", "message": e})),
        }
    }

    /// A file from the chat: which file it replaces, and both texts.
    pub fn downloaded(&mut self, path: &str, name: &str) -> Result<()> {
        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let text = if size > 4 << 20 { None } else { Some(read_text(Path::new(path))) };
        let _ = std::fs::remove_file(path);
        let text = match text {
            Some(t) => t?,
            None => String::new(),
        };
        if size > 4 << 20 || text.contains('\0') {
            self.post(json!({"type": "error", "message": t!("dev.downloadNotText", name)}));
            return Ok(());
        }
        let Some(p) = &self.project else {
            self.post(json!({"type": "error", "message": t!("dev.downloadNoScript", name)}));
            return Ok(());
        };
        let target = p.match_download(name, &self.current);
        let old = match &target {
            Some(t) => read_text(&p.path(t)?).ok(),
            None => None,
        };
        // A script with a temporary name takes the name of the AI's file.
        let ps: ProjectSettings = load(&settings_path(p));
        let entry = p.entry_name();
        let rename = (target.as_deref() == Some(entry.as_str()) && ps.unnamed.contains(&entry) && project::is_script(name))
            .then(|| {
                let ext = project::ext_of(name);
                format!("{}.{ext}", project::base_stem(name).trim())
            })
            .filter(|n| n != &entry && !n.starts_with('.') && !p.dir.join(n).exists() && p.path(n).is_ok());
        self.post(json!({
            "type": "download",
            "name": name,
            "text": text,
            "target": target,
            "old": old,
            "rename": rename,
            "files": p.files(),
        }));
        Ok(())
    }

    fn start_run(&mut self, kind: RunKind) -> Result<()> {
        if self.running.as_ref().is_some_and(|r| r.run.running()) {
            return Err(err!("dev.alreadyRunning"));
        }
        let p = self.project()?.clone();
        let mut args = self.o.run_args.clone();
        if kind == RunKind::Debug {
            args.push("--debug".into());
        }
        args.push(p.entry.to_string_lossy().into_owned());
        let mut env = vec![(runner::DEV_ENV.to_string(), "1".to_string())];
        if std::env::var("XLJS_WATCHDOG").unwrap_or_default().is_empty() {
            env.push(("XLJS_WATCHDOG".into(), "on".into()));
        }
        if std::env::var("XLJS_STOP_KEY").unwrap_or_default().is_empty() {
            env.push(("XLJS_STOP_KEY".into(), "on".into()));
        }
        let preview = (kind == RunKind::Preview).then(|| std::env::temp_dir().join(format!("xljs-preview-{}-{}.json", std::process::id(), self.next_run)));
        if let Some(f) = &preview {
            let _ = std::fs::remove_file(f);
            env.push((runner::PREVIEW_ENV.into(), f.to_string_lossy().into_owned()));
        }
        let id = self.next_run;
        self.next_run += 1;
        let run = Run::start(&self.o.exe, &args, &p.dir, &env, id, self.tx.clone(), Event::Run)?;
        self.running = Some(Running { run, kind, stderr: String::new(), preview });
        self.post(json!({"type": "runStarted", "kind": format!("{kind:?}").to_lowercase()}));
        Ok(())
    }

    fn run_event(&mut self, e: RunEvent) {
        match e {
            RunEvent::Output { id, stderr, text } => {
                let Some(r) = &mut self.running else { return };
                if r.run.id != id {
                    return;
                }
                if stderr {
                    r.stderr.push_str(&text);
                    if r.stderr.len() > 1 << 20 {
                        let cut = r.stderr.len() - (1 << 19);
                        let cut = (cut..r.stderr.len()).find(|&i| r.stderr.is_char_boundary(i)).unwrap_or(0);
                        r.stderr.drain(..cut);
                    }
                }
                self.post(json!({"type": "output", "stream": if stderr { "stderr" } else { "stdout" }, "text": text}));
            }
            RunEvent::Exited { id, code } => {
                let Some(r) = self.running.take_if(|r| r.run.id == id) else { return };
                self.finished(r, code);
            }
        }
    }

    /// Reports the end of a run.
    fn finished(&mut self, r: Running, code: Option<i32>) {
        let Some(p) = self.project.clone() else { return };
        if let Some(f) = &r.preview {
            let form = std::fs::read_to_string(f).ok().and_then(|s| serde_json::from_str::<Value>(&s).ok());
            let _ = std::fs::remove_file(f);
            match form {
                Some(form) => self.post(json!({"type": "previewForm", "form": form})),
                None => {
                    let message = if code == Some(0) { t!("dev.previewNoForm") } else { report::last_error(&r.stderr) };
                    self.post(json!({"type": "previewFailed", "message": message}));
                }
            }
            self.post(json!({"type": "runEnded", "kind": "preview", "code": code}));
            return;
        }
        use crate::engine::watchdog::{STOPPED_CODE, STUCK_CODE};
        // Stopped by the user (the stop key, the debugger window closed):
        // not a failure to ask the AI about.
        let stopped = code == Some(STOPPED_CODE);
        let stuck = code == Some(STUCK_CODE);
        let failed = code != Some(0) && !stopped;
        let mut ev = json!({"type": "runEnded", "kind": format!("{:?}", r.kind).to_lowercase(), "code": code, "stopped": stopped, "stuck": stuck});
        if failed {
            let message = report::last_error(&r.stderr);
            let loc = report::find_location(&message, &p.dir).or_else(|| report::find_location(&r.stderr, &p.dir));
            ev["message"] = json!(message);
            ev["location"] = json!(loc);
            ev["prompt"] = json!(report::prompt(&p, stuck, &message, loc.as_ref()));
        }
        self.post(ev);
    }

    /// Ends what is left when the window closes.
    pub fn shutdown(&mut self) {
        if let Some(r) = self.running.take() {
            r.run.kill();
        }
    }
}

/// Where the wizard proposes to create scripts: the folder of the last
/// script, else the documents folder.
fn default_dir(gs: &GlobalSettings) -> String {
    if let Some(d) = gs.recent.iter().filter_map(|r| Path::new(r).parent()).find(|d| d.is_dir()) {
        return d.to_string_lossy().into_owned();
    }
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_default();
    let docs = home.join("Documents");
    if docs.is_dir() { docs } else { home }.to_string_lossy().into_owned()
}

fn settings_path(p: &Project) -> PathBuf {
    p.dir.join(project::DATA_DIR).join("dev.json")
}

/// Whether a path names a script that can be opened (.js, .mjs, .ts).
fn is_script(path: &str) -> bool {
    let ext = Path::new(path).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    matches!(ext.as_str(), "js" | "mjs" | "ts")
}

/// The texts of the page (the keys under "devPage." without the prefix).
fn texts() -> Value {
    texts_under("devPage.")
}

/// The messages whose keys start with prefix, without it.
fn texts_under(prefix: &str) -> Value {
    let m: serde_json::Map<String, Value> =
        i18n::messages().into_iter().filter_map(|(k, v)| k.strip_prefix(prefix).map(|k| (k.to_string(), json!(v)))).collect();
    Value::Object(m)
}

#[cfg(test)]
mod tests;
