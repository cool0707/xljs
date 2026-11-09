//! The User-Agent of fetch and of the AI chat pane of the development mode:
//! that of the Edge or the Chrome installed on the computer (its version
//! from the registry), so that sites treat the requests as theirs.
//!
//! The choice is the environment variable XLJS_USER_AGENT, else
//! `"userAgent"` of xljs.json next to the exe: "edge" (the default),
//! "chrome", "xljs" (xljs/<version>, what fetch sent before) or any other
//! text, sent as it is. Chrome falls back to Edge when it is not installed,
//! Edge to the WebView2 runtime (the same engine); with neither (not
//! Windows) fetch sends xljs/<version> and the pane keeps its own.

use std::path::Path;
use std::sync::OnceLock;

use serde_json::{Value, json};

/// The environment variable that chooses the User-Agent (before xljs.json).
pub const ENV: &str = "XLJS_USER_AGENT";

/// A browser whose User-Agent is sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Brand {
    Edge,
    Chrome,
}

/// A browser installed on the computer.
#[derive(Debug, Clone, PartialEq)]
pub struct Browser {
    pub brand: Brand,
    /// The full version ("142.0.3595.53").
    pub version: String,
}

impl Browser {
    fn major(&self) -> u32 {
        self.version.split('.').next().and_then(|m| m.parse().ok()).unwrap_or(0)
    }

    /// The User-Agent the browser sends (the reduced form: the minor
    /// version and the OS are fixed).
    pub fn user_agent(&self) -> String {
        let m = self.major();
        let base = format!("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{m}.0.0.0 Safari/537.36");
        match self.brand {
            Brand::Edge => format!("{base} Edg/{m}.0.0.0"),
            Brand::Chrome => base,
        }
    }

    /// The brands of the client hints (Sec-CH-UA, navigator.userAgentData)
    /// as Chromium makes them: the browser, Chromium and a "GREASE" brand
    /// in an order that depends on the major version. With full, the full
    /// versions (Sec-CH-UA-Full-Version-List).
    fn brands(&self, full: bool) -> Vec<Value> {
        const CHARS: [&str; 11] = [" ", "(", ":", "-", ".", "/", ")", ";", "=", "?", "_"];
        const VERSIONS: [&str; 3] = ["8", "99", "24"];
        const ORDERS: [[usize; 3]; 6] = [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]];
        let m = self.major() as usize;
        let name = match self.brand {
            Brand::Edge => "Microsoft Edge",
            Brand::Chrome => "Google Chrome",
        };
        let grease = format!("Not{}A{}Brand", CHARS[m % 11], CHARS[(m + 1) % 11]);
        let gv = VERSIONS[m % 3];
        let (gv, v) = if full { (format!("{gv}.0.0.0"), self.version.clone()) } else { (gv.to_string(), m.to_string()) };
        let all = [(grease, gv), ("Chromium".to_string(), v.clone()), (name.to_string(), v)];
        let order = ORDERS[m % 6];
        let mut out = vec![Value::Null; 3];
        for (i, (b, v)) in all.into_iter().enumerate() {
            out[order[i]] = json!({"brand": b, "version": v});
        }
        out
    }

    /// The parameters of the DevTools protocol's
    /// Emulation.setUserAgentOverride: the User-Agent with the client hints
    /// of the browser (else the pane's would name WebView2).
    pub fn override_params(&self, os: &Os) -> String {
        json!({
            "userAgent": self.user_agent(),
            "userAgentMetadata": {
                "brands": self.brands(false),
                "fullVersionList": self.brands(true),
                "fullVersion": self.version,
                "platform": "Windows",
                "platformVersion": os.platform_version,
                "architecture": os.architecture,
                "model": "",
                "mobile": false,
                "bitness": os.bitness,
                "wow64": false,
            },
        })
        .to_string()
    }
}

/// What the client hints tell about Windows.
#[derive(Debug, Clone, PartialEq)]
pub struct Os {
    /// "15.0.0" (Windows 11 22H2): the major version of the Windows
    /// UniversalApiContract, as Chromium gives it.
    pub platform_version: String,
    /// "x86" or "arm".
    pub architecture: String,
    /// "64" or "32".
    pub bitness: String,
}

impl Os {
    /// From the build number of Windows and the processor's architecture
    /// (PROCESSOR_ARCHITECTURE: "AMD64", "ARM64", "x86").
    pub fn new(build: u32, processor: &str) -> Os {
        const CONTRACTS: [(u32, u32); 14] = [
            (26100, 19),
            (22621, 15),
            (22000, 14),
            (20348, 12),
            (19041, 10),
            (18362, 8),
            (17763, 7),
            (17134, 6),
            (16299, 5),
            (15063, 4),
            (14393, 3),
            (10586, 2),
            (10240, 1),
            (0, 0),
        ];
        let contract = CONTRACTS.iter().find(|(b, _)| build >= *b).map(|(_, c)| *c).unwrap_or(0);
        let p = processor.to_ascii_uppercase();
        let architecture = if p.starts_with("ARM") { "arm" } else { "x86" };
        let bitness = if p == "X86" { "32" } else { "64" };
        Os { platform_version: format!("{contract}.0.0"), architecture: architecture.into(), bitness: bitness.into() }
    }
}

/// What the chat pane is set to.
#[derive(Debug, Clone, PartialEq)]
pub struct PaneAgent {
    pub user_agent: String,
    /// The parameters of Emulation.setUserAgentOverride (a browser's,
    /// with its client hints); none for a text of the settings.
    pub devtools: Option<String>,
}

/// The User-Agent chosen and found.
#[derive(Debug, Clone, PartialEq)]
pub enum Agent {
    Browser(Browser),
    /// A text of the settings.
    Text(String),
    /// xljs/<version>: chosen, or no browser found.
    Xljs,
}

impl Agent {
    /// The User-Agent of fetch.
    pub fn fetch(&self, version: &str) -> String {
        match self {
            Agent::Browser(b) => b.user_agent(),
            Agent::Text(t) => t.clone(),
            Agent::Xljs => format!("xljs/{version}"),
        }
    }

    /// What the chat pane is set to; none keeps WebView2's own.
    pub fn pane(&self, os: &Os) -> Option<PaneAgent> {
        match self {
            Agent::Browser(b) => Some(PaneAgent { user_agent: b.user_agent(), devtools: Some(b.override_params(os)) }),
            Agent::Text(t) => Some(PaneAgent { user_agent: t.clone(), devtools: None }),
            Agent::Xljs => None,
        }
    }
}

/// The installed versions to choose from (Installed::find on Windows).
pub trait Versions {
    fn version(&self, brand: Brand) -> Option<String>;
    /// The WebView2 runtime's: Edge's engine when Edge is not installed.
    fn webview2(&self) -> Option<String>;
}

/// Resolves a choice of the settings ("" = edge).
pub fn resolve(choice: &str, v: &dyn Versions) -> Agent {
    let choice = choice.trim();
    let edge = || v.version(Brand::Edge).or_else(|| v.webview2()).map(|version| Agent::Browser(Browser { brand: Brand::Edge, version }));
    match choice.to_ascii_lowercase().as_str() {
        "" | "edge" => edge().unwrap_or(Agent::Xljs),
        "chrome" => v.version(Brand::Chrome).map(|version| Agent::Browser(Browser { brand: Brand::Chrome, version })).or_else(edge).unwrap_or(Agent::Xljs),
        "xljs" => Agent::Xljs,
        _ => Agent::Text(choice.to_string()),
    }
}

/// The choice: XLJS_USER_AGENT, else "userAgent" of the settings file
/// (xljs.json); "" when neither says.
pub fn choice_in(settings: &Path) -> String {
    if let Ok(v) = std::env::var(ENV)
        && !v.trim().is_empty()
    {
        return v;
    }
    if settings.as_os_str().is_empty() {
        return String::new();
    }
    std::fs::read(settings)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(strip_bom(&b)).ok())
        .and_then(|j| j.get("userAgent").and_then(|v| v.as_str()).map(str::to_string))
        .unwrap_or_default()
}

fn strip_bom(b: &[u8]) -> &[u8] {
    b.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(b)
}

/// The settings file next to the running exe.
fn exe_settings() -> std::path::PathBuf {
    std::env::current_exe().map(|e| crate::dev::exe_settings(&e)).unwrap_or_default()
}

/// The User-Agent of fetch (version: xljs's).
pub fn for_fetch(version: &str) -> String {
    resolve(&choice_in(&exe_settings()), installed()).fetch(version)
}

/// What the chat pane is set to, by the settings file settings.
pub fn for_pane(settings: &Path) -> Option<PaneAgent> {
    resolve(&choice_in(settings), installed()).pane(&os())
}

/// The versions installed on the computer (read once).
fn installed() -> &'static Installed {
    static I: OnceLock<Installed> = OnceLock::new();
    I.get_or_init(Installed::find)
}

#[derive(Default)]
struct Installed {
    edge: Option<String>,
    chrome: Option<String>,
    webview2: Option<String>,
}

impl Versions for Installed {
    fn version(&self, brand: Brand) -> Option<String> {
        match brand {
            Brand::Edge => self.edge.clone(),
            Brand::Chrome => self.chrome.clone(),
        }
    }
    fn webview2(&self) -> Option<String> {
        self.webview2.clone()
    }
}

/// A version as the update services write it ("142.0.3595.53").
fn valid(v: &str) -> Option<String> {
    let v = v.trim();
    let parts: Vec<&str> = v.split('.').collect();
    let ok = parts.len() == 4 && parts.iter().all(|p| !p.is_empty() && p.bytes().all(|c| c.is_ascii_digit())) && parts[0] != "0";
    ok.then(|| v.to_string())
}

#[cfg(windows)]
mod win {
    use windows::Win32::System::Registry::*;
    use windows::core::HSTRING;

    pub fn reg_string(root: HKEY, sub: &str, name: &str) -> Option<String> {
        unsafe {
            let mut key = HKEY::default();
            // The update services write to the 32-bit view.
            if RegOpenKeyExW(root, &HSTRING::from(sub), Some(0), KEY_READ | KEY_WOW64_32KEY, &mut key).is_err() {
                return None;
            }
            let name = HSTRING::from(name);
            let mut size = 0u32;
            let mut out = None;
            if RegQueryValueExW(key, &name, None, None, None, Some(&mut size)).is_ok() && size > 0 {
                let mut buf = vec![0u16; size as usize / 2 + 1];
                let mut size2 = size;
                if RegQueryValueExW(key, &name, None, None, Some(buf.as_mut_ptr() as *mut u8), Some(&mut size2)).is_ok() {
                    let n = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
                    out = Some(String::from_utf16_lossy(&buf[..n]));
                }
            }
            let _ = RegCloseKey(key);
            out
        }
    }

    /// The version of a product of EdgeUpdate / Google Update (its client
    /// id), for all users or this one, else the last one run (BLBeacon).
    pub fn version(update: &str, id: &str, beacon: Option<&str>) -> Option<String> {
        let sub = format!("Software\\{update}\\Clients\\{id}");
        [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER]
            .into_iter()
            .find_map(|root| reg_string(root, &sub, "pv").and_then(|v| super::valid(&v)))
            .or_else(|| beacon.and_then(|b| reg_string(HKEY_CURRENT_USER, b, "version")).and_then(|v| super::valid(&v)))
    }
}

impl Installed {
    #[cfg(windows)]
    fn find() -> Installed {
        Installed {
            edge: win::version("Microsoft\\EdgeUpdate", "{56EB18F8-B008-4CBD-B6D2-8C97FE7E9062}", Some("Software\\Microsoft\\Edge\\BLBeacon")),
            chrome: win::version("Google\\Update", "{8A69D345-D564-463c-AFF1-A69D9E530F96}", Some("Software\\Google\\Chrome\\BLBeacon")),
            webview2: win::version("Microsoft\\EdgeUpdate", "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}", None),
        }
    }

    #[cfg(not(windows))]
    fn find() -> Installed {
        Installed::default()
    }
}

/// What the client hints tell about this Windows.
fn os() -> Os {
    #[cfg(windows)]
    let build = win::reg_string(windows::Win32::System::Registry::HKEY_LOCAL_MACHINE, "Software\\Microsoft\\Windows NT\\CurrentVersion", "CurrentBuildNumber")
        .and_then(|b| b.trim().parse().ok())
        .unwrap_or(0);
    #[cfg(not(windows))]
    let build = 0;
    // A 32-bit process on 64-bit Windows sees x86 here, the system's in
    // PROCESSOR_ARCHITEW6432.
    let p = std::env::var("PROCESSOR_ARCHITEW6432").or_else(|_| std::env::var("PROCESSOR_ARCHITECTURE")).unwrap_or_else(|_| "AMD64".into());
    Os::new(build, &p)
}

#[cfg(test)]
mod tests;
