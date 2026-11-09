use super::*;
use serde_json::Value;

struct Fake {
    edge: Option<&'static str>,
    chrome: Option<&'static str>,
    webview2: Option<&'static str>,
}

impl Versions for Fake {
    fn version(&self, brand: Brand) -> Option<String> {
        match brand {
            Brand::Edge => self.edge.map(str::to_string),
            Brand::Chrome => self.chrome.map(str::to_string),
        }
    }
    fn webview2(&self) -> Option<String> {
        self.webview2.map(str::to_string)
    }
}

const ALL: Fake = Fake {
    edge: Some("142.0.3595.53"),
    chrome: Some("141.0.7390.123"),
    webview2: Some("142.0.3595.40"),
};

fn edge(v: &str) -> Agent {
    Agent::Browser(Browser {
        brand: Brand::Edge,
        version: v.into(),
    })
}

#[test]
fn resolves_the_choice() {
    assert_eq!(resolve("", &ALL), edge("142.0.3595.53"));
    assert_eq!(resolve(" Edge ", &ALL), edge("142.0.3595.53"));
    assert_eq!(
        resolve("chrome", &ALL),
        Agent::Browser(Browser {
            brand: Brand::Chrome,
            version: "141.0.7390.123".into()
        })
    );
    assert_eq!(resolve("XLJS", &ALL), Agent::Xljs);
    assert_eq!(
        resolve(" MyAgent/1.0 ", &ALL),
        Agent::Text("MyAgent/1.0".into())
    );
    // Chrome not installed: Edge; Edge not installed: the WebView2 runtime.
    let no_chrome = Fake {
        chrome: None,
        ..ALL
    };
    assert_eq!(resolve("chrome", &no_chrome), edge("142.0.3595.53"));
    let runtime = Fake {
        edge: None,
        chrome: None,
        ..ALL
    };
    assert_eq!(resolve("chrome", &runtime), edge("142.0.3595.40"));
    // Nothing (not Windows): xljs's.
    let none = Fake {
        edge: None,
        chrome: None,
        webview2: None,
    };
    assert_eq!(resolve("edge", &none), Agent::Xljs);
    assert_eq!(resolve("edge", &none).fetch("1.2.3"), "xljs/1.2.3");
    assert_eq!(resolve("edge", &none).pane(&Os::new(22631, "AMD64")), None);
}

#[test]
fn user_agents() {
    assert_eq!(
        resolve("edge", &ALL).fetch("1"),
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36 Edg/142.0.0.0"
    );
    assert_eq!(
        resolve("chrome", &ALL).fetch("1"),
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36"
    );
    assert_eq!(resolve("MyAgent/1.0", &ALL).fetch("1"), "MyAgent/1.0");
    // A text of the settings goes to the pane without client hints.
    assert_eq!(
        resolve("MyAgent/1.0", &ALL).pane(&Os::new(0, "AMD64")),
        Some(PaneAgent {
            user_agent: "MyAgent/1.0".into(),
            devtools: None
        })
    );
}

fn brands(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|b| {
            format!(
                "{}={}",
                b["brand"].as_str().unwrap(),
                b["version"].as_str().unwrap()
            )
        })
        .collect()
}

#[test]
fn client_hints_as_chromium() {
    // Sec-CH-UA of Chrome 130, 131 and 133 as the browsers send it.
    let chrome = |v: &str| Browser {
        brand: Brand::Chrome,
        version: v.into(),
    };
    assert_eq!(
        brands(&Value::Array(chrome("130.0.6723.70").brands(false))),
        ["Chromium=130", "Google Chrome=130", "Not?A_Brand=99"]
    );
    assert_eq!(
        brands(&Value::Array(chrome("131.0.6778.86").brands(false))),
        ["Google Chrome=131", "Chromium=131", "Not_A Brand=24"]
    );
    assert_eq!(
        brands(&Value::Array(chrome("133.0.6943.54").brands(false))),
        ["Not(A:Brand=99", "Google Chrome=133", "Chromium=133"]
    );

    let pane = resolve("edge", &ALL)
        .pane(&Os::new(22631, "AMD64"))
        .unwrap();
    assert!(pane.user_agent.ends_with("Edg/142.0.0.0"));
    let p: Value = serde_json::from_str(pane.devtools.as_deref().unwrap()).unwrap();
    assert_eq!(p["userAgent"], pane.user_agent);
    let m = &p["userAgentMetadata"];
    let b = brands(&m["brands"]);
    assert!(
        b.contains(&"Microsoft Edge=142".to_string()) && b.contains(&"Chromium=142".to_string()),
        "{b:?}"
    );
    let f = brands(&m["fullVersionList"]);
    assert!(
        f.contains(&"Microsoft Edge=142.0.3595.53".to_string()),
        "{f:?}"
    );
    assert!(
        f.iter()
            .any(|x| x.starts_with("Not") && x.ends_with(".0.0.0")),
        "{f:?}"
    );
    assert_eq!(m["fullVersion"], "142.0.3595.53");
    assert_eq!(m["platform"], "Windows");
    assert_eq!(m["platformVersion"], "15.0.0");
    assert_eq!(m["architecture"], "x86");
    assert_eq!(m["bitness"], "64");
    assert_eq!(m["mobile"], false);
}

#[test]
fn os_hints() {
    assert_eq!(
        Os::new(19045, "AMD64"),
        Os {
            platform_version: "10.0.0".into(),
            architecture: "x86".into(),
            bitness: "64".into()
        }
    );
    assert_eq!(Os::new(22000, "ARM64").platform_version, "14.0.0");
    assert_eq!(Os::new(22000, "ARM64").architecture, "arm");
    assert_eq!(Os::new(26100, "AMD64").platform_version, "19.0.0");
    assert_eq!(
        Os::new(9600, "x86"),
        Os {
            platform_version: "0.0.0".into(),
            architecture: "x86".into(),
            bitness: "32".into()
        }
    );
}

#[test]
fn versions_from_the_registry_are_checked() {
    assert_eq!(valid(" 142.0.3595.53 "), Some("142.0.3595.53".into()));
    assert_eq!(valid("142.0"), None);
    assert_eq!(valid("0.0.0.0"), None);
    assert_eq!(valid("1.2.3.x"), None);
}

#[test]
fn choice_from_the_settings() {
    let _l = crate::testutil::lock_lang("ja");
    let f = crate::testutil::tempdir().join("xljs.json");
    unsafe { std::env::remove_var(ENV) };
    assert_eq!(choice_in(&f), "");
    std::fs::write(&f, "\u{feff}{\"userAgent\": \"chrome\", \"dev\": {}}").unwrap();
    assert_eq!(choice_in(&f), "chrome");
    std::fs::write(&f, "{ broken").unwrap();
    assert_eq!(choice_in(&f), "");
    // The environment variable comes first.
    std::fs::write(&f, "{\"userAgent\": \"chrome\"}").unwrap();
    unsafe { std::env::set_var(ENV, "xljs") };
    assert_eq!(choice_in(&f), "xljs");
    unsafe { std::env::remove_var(ENV) };
}
