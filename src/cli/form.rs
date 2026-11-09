//! Forms of the commands that ask before they work (no script): the
//! built-in form page (form.html) in a window of its own where the UI has
//! HTML windows, else Ui::form (the console, the mock).

use std::sync::{Arc, mpsc};

use serde_json::{Map, Value, json};

use crate::ui::{ASSET_ORIGIN, BUILTIN_PREFIX, FormOptions, Ui, WindowOptions};
use crate::{Result, t};

/// Returns the field and the message of a value that cannot be used.
pub(super) type Check = fn(&Map<String, Value>) -> Option<(String, String)>;

/// Asks the form (simple fields: no pickers, tables or charts) and returns
/// the values, None when cancelled. size is that of the window's page.
pub(super) fn ask(
    u: &Arc<dyn Ui>,
    o: &FormOptions,
    size: (i64, i64),
    check: Check,
) -> Option<Map<String, Value>> {
    if u.supports_windows() {
        let o2 = o.clone();
        if let Ok(r) = crate::ui::run(u.clone(), move |su| window(&*su, &o2, size, check)) {
            return r;
        }
    }
    let mut o = o.clone();
    loop {
        let v = u.form(&o)?;
        let Some((field, msg)) = check(&v) else {
            return Some(v);
        };
        for f in &mut o.fields {
            f.error = if f.name == field {
                msg.clone()
            } else {
                String::new()
            };
            if let Some(x) = v.get(&f.name) {
                f.value = x.clone();
            }
        }
    }
}

/// The texts of the form page (the prelude's formTexts).
fn texts() -> Value {
    let mut m = Map::new();
    for (k, key) in [
        ("ok", "form.ok"),
        ("cancel", "form.cancel"),
        ("close", "form.close"),
        ("browse", "form.browse"),
        ("clear", "form.clear"),
        ("abort", "form.abort"),
        ("aborting", "form.aborting"),
        ("running", "form.running"),
        ("done", "form.done"),
        ("aborted", "form.aborted"),
        ("failed", "form.failed"),
        ("openFolder", "form.openFolder"),
    ] {
        m.insert(k.into(), Value::String(t!(key)));
    }
    Value::Object(m)
}

fn window(
    u: &dyn Ui,
    o: &FormOptions,
    (width, height): (i64, i64),
    check: Check,
) -> Result<Option<Map<String, Value>>> {
    // Some(message) from the page, None once the window has closed.
    let (tx, rx) = mpsc::channel::<Option<String>>();
    let (t1, t2) = (tx.clone(), tx);
    let w = u.open_window(WindowOptions {
        title: o.title.clone(),
        width,
        height,
        fixed_size: true,
        url: format!("{ASSET_ORIGIN}{BUILTIN_PREFIX}form.html"),
        assets: Some(Arc::new(super::launch::LaunchAssets)),
        on_message: Some(Arc::new(move |m, _| {
            let _ = t1.send(Some(m));
        })),
        on_closed: Some(Arc::new(move || {
            let _ = t2.send(None);
        })),
        ..Default::default()
    })?;
    let init = json!({
        "type": "init",
        "description": o.description,
        "texts": texts(),
        "submit": o.submit,
        "tabs": [], "groups": [], "buttons": [],
        "fields": o.fields.iter().map(|f| json!({
            "name": f.name, "label": f.label, "type": f.kind, "options": f.options,
            "required": f.required, "help": f.help, "value": f.value, "tab": -1, "group": -1,
        })).collect::<Vec<_>>(),
        "lang": crate::i18n::lang(),
    })
    .to_string();
    let mut result = None;
    while let Ok(Some(m)) = rx.recv() {
        let v: Value = serde_json::from_str(&m).unwrap_or_default();
        match v["type"].as_str() {
            Some("ready") => w.post(init.clone()),
            Some("submit") => {
                let values = v["values"].as_object().cloned().unwrap_or_default();
                if let Some((field, msg)) = check(&values) {
                    w.post(
                        json!({ "type": "errors", "errors": { field: msg }, "error": "" })
                            .to_string(),
                    );
                    continue;
                }
                result = Some(values);
                w.close();
            }
            Some("cancel" | "close") => w.close(),
            _ => {}
        }
    }
    Ok(result)
}
