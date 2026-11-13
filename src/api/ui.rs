use std::sync::Arc;

use rquickjs::function::Rest;
use rquickjs::{Function, Object, Value};

use super::js::{self, elems, get, has, nullish, to_bool, to_str};
use super::N;
use crate::ui::{FileFilter, FormOptions, PickerOptions, ProgressOptions, TableOptions};
use crate::{Error, i18n};

fn opt_str(v: &Value, def: &str) -> String {
    if nullish(v) { def.to_string() } else { to_str(v) }
}

pub(crate) fn picker_options(v: &Value) -> PickerOptions {
    let mut o = PickerOptions::default();
    if nullish(v) {
        return o;
    }
    o.title = opt_str(&get(v, "title"), "");
    o.multiple = has(v, "multiple") && to_bool(&get(v, "multiple"));
    o.start_in = opt_str(&get(v, "startIn"), "");
    o.suggested_name = opt_str(&get(v, "suggestedName"), "");
    for f in elems(&get(v, "filters")) {
        o.filters.push(FileFilter { description: opt_str(&get(&f, "description"), ""), patterns: js::strings(&get(&f, "patterns")) });
    }
    o
}

/// The folder for remembered settings (%APPDATA%\xljs), "" if unknown.
pub(crate) fn config_dir() -> String {
    if let Ok(d) = std::env::var("XLJS_CONFIG_DIR")
        && !d.is_empty()
    {
        return d;
    }
    let base = if cfg!(windows) {
        std::env::var("APPDATA").ok()
    } else if cfg!(target_os = "macos") {
        std::env::var("HOME").ok().map(|h| format!("{h}/Library/Application Support"))
    } else {
        std::env::var("XDG_CONFIG_HOME").ok().filter(|s| !s.is_empty()).or_else(|| std::env::var("HOME").ok().map(|h| format!("{h}/.config")))
    };
    match base {
        Some(b) if !b.is_empty() => std::path::Path::new(&b).join("xljs").to_string_lossy().into_owned(),
        _ => String::new(),
    }
}

/// Ends the script (like exit(), with engine::ABORT_CODE) when the UI of a
/// test run asked for it during the call that has just returned: a
/// question nobody answers.
fn aborted<'js>(h: &super::Host, a: &js::A<'js>) -> rquickjs::Result<()> {
    let stop = h.env.test.as_ref().is_some_and(|t| t.abort.load(std::sync::atomic::Ordering::SeqCst));
    if stop {
        h.set_exit(crate::engine::ABORT_CODE);
        return Err(js::throw_uncatchable(&a.ctx));
    }
    Ok(())
}

pub(super) fn install(n: &N) {
    n.set("uiAlert", |h, a| {
        h.env.ui.alert(&a.str(0), &a.str(1));
        a.undefined()
    });
    n.set("uiConfirm", |h, a| {
        let v = h.env.ui.confirm(&a.str(0), &a.str(1));
        aborted(h, a)?;
        if let Some(r) = &h.env.record {
            r.answer("confirm", v.into());
        }
        a.ret(v)
    });
    // testRecord(kind, answerJSON, pathsJSON): an answer the prelude saw (a
    // form: pathsJSON names its fields that hold paths, with their types).
    n.set("testRecord", |h, a| {
        if let Some(r) = &h.env.record {
            let v: serde_json::Value = serde_json::from_str(&a.str(1)).unwrap_or_default();
            if a.str(0) == "form" {
                let paths: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&a.str(2)).unwrap_or_default();
                let answer = match v {
                    serde_json::Value::Object(m) => Some(m),
                    _ => None,
                };
                r.form(answer, &paths);
            } else {
                r.answer(&a.str(0), v);
            }
        }
        a.undefined()
    });
    // testForget(kind): the last recorded answer of the kind is not one a
    // run of the case will be asked for.
    n.set("testForget", |h, a| {
        if let Some(r) = &h.env.record {
            r.forget(&a.str(0));
        }
        a.undefined()
    });
    n.set("uiPrompt", |h, a| {
        let v = h.env.ui.prompt(&a.str(0), &a.str(1), &a.str(2));
        aborted(h, a)?;
        if let Some(r) = &h.env.record {
            r.answer("prompt", serde_json::json!(v));
        }
        match v {
            Some(s) => a.ret(s),
            None => a.null(),
        }
    });
    n.set("uiOpen", |h, a| {
        let v = h.env.ui.open_files(&picker_options(&a.arg(0)));
        aborted(h, a)?;
        if let Some(r) = &h.env.record {
            let list: Vec<String> = v.iter().map(|p| r.input(p)).collect();
            r.answer("showOpenFilePicker", if list.is_empty() { serde_json::Value::Null } else { serde_json::json!(list) });
        }
        a.ret(v)
    });
    n.set("uiSave", |h, a| {
        let v = h.env.ui.save_file(&picker_options(&a.arg(0)));
        aborted(h, a)?;
        if let Some(r) = &h.env.record {
            r.answer("showSaveFilePicker", serde_json::json!(v.as_ref().map(|p| r.output(p))));
        }
        match v {
            Some(p) => a.ret(p),
            None => a.null(),
        }
    });
    n.set("uiDirectory", |h, a| {
        let v = h.env.ui.pick_directory(&picker_options(&a.arg(0)));
        aborted(h, a)?;
        if let Some(r) = &h.env.record {
            r.answer("showDirectoryPicker", serde_json::json!(v.as_ref().map(|p| r.input(p))));
        }
        match v {
            Some(p) => a.ret(p),
            None => a.null(),
        }
    });
    n.set("uiError", |h, a| {
        h.env.ui.show_error(&a.str(0), &a.str(1));
        a.undefined()
    });
    n.set("uiNotify", |h, a| {
        h.env.ui.notify(&a.str(0), &a.str(1));
        a.undefined()
    });
    n.set("uiAutoNotify", |_, a| {
        crate::ui::set_auto_notify(a.bool(0));
        a.undefined()
    });
    n.set("shellShowInFolder", |h, a| {
        h.env.ui.show_in_folder(&a.strings(0)).map_err(|e| a.throw(&e))?;
        a.undefined()
    });
    n.set("shellOpen", |h, a| {
        h.env.ui.open_path(&a.str(0)).map_err(|e| a.throw(&e))?;
        a.undefined()
    });
    // shellReadShortcut(path) -> detailsJSON
    n.set("shellReadShortcut", |_, a| {
        let d = crate::shortcut::read(&a.str(0)).map_err(|e| a.throw(&e))?;
        a.ret(serde_json::to_string(&d).map_err(|e| a.throw(&Error::from(e)))?)
    });
    // shellWriteShortcut(path, operation, changesJSON) -> the file written
    n.set("shellWriteShortcut", |_, a| {
        let op = crate::shortcut::Operation::parse(&a.str(1)).map_err(|e| a.throw(&e))?;
        let c: crate::shortcut::Changes = serde_json::from_str(&a.str(2)).map_err(|e| a.throw(&Error::from(e)))?;
        a.ret(crate::shortcut::write(&a.str(0), op, &c).map_err(|e| a.throw(&e))?)
    });
    // uiForm(optionsJSON) -> valuesJSON | null (UIs without windows)
    n.set("uiForm", |h, a| {
        let o: FormOptions = serde_json::from_str(&a.str(0)).map_err(|e| a.throw(&Error::from(e)))?;
        let v = h.env.ui.form(&o);
        aborted(h, a)?;
        match v {
            Some(v) => a.ret(serde_json::Value::Object(v).to_string()),
            None => a.null(),
        }
    });
    // uiTable(optionsJSON) -> selected indices | null (UIs without windows)
    n.set("uiTable", |h, a| {
        let o: TableOptions = serde_json::from_str(&a.str(0)).map_err(|e| a.throw(&Error::from(e)))?;
        let v = h.env.ui.table(&o);
        if let Some(r) = h.env.record.as_ref().filter(|_| o.select != crate::ui::SELECT_NONE) {
            r.answer("table", serde_json::json!(v));
        }
        match v {
            Some(sel) => a.ret(sel.into_iter().map(|i| i as f64).collect::<Vec<f64>>()),
            None => a.null(),
        }
    });
    // pdfPrint(html, chartsJSON, textsJSON, path) -> Promise<string>: "" when
    // the PDF is written, else the message of the error.
    n.set("pdfPrint", |h, a| {
        let ctx = a.ctx.clone();
        let (promise, resolve, _) = ctx.promise()?;
        let id = h.keep_fn(&ctx, resolve);
        let poster = h.poster();
        // The job keeps the event loop alive until it is done.
        poster.add_pending();
        let job = crate::ui::PdfJob { html: a.str(0), charts: a.str(1), texts: a.str(2), path: a.str(3) };
        h.env.ui.print_pdf(
            job,
            Box::new(move |r| {
                let msg = match r {
                    Ok(()) => String::new(),
                    Err(e) => i18n::error(&e),
                };
                poster.post(Box::new(move |ctx, host| {
                    if let Some(f) = host.kept_fn(ctx, id) {
                        f.call::<_, Value>((msg,))?;
                    }
                    host.drop_fn(id);
                    Ok(())
                }));
            }),
        );
        Ok(promise.into_value())
    });
    n.set("configDir", |_, a| a.ret(config_dir()));
    // displayText(s) translates library messages for display.
    n.set("displayText", |_, a| a.ret(i18n::text(&a.str(0))));
    // uiProgress(title, total, message, cancellable) -> {update, cancelled, close}
    n.set("uiProgress", |h, a| {
        let p = h.env.ui.progress(&ProgressOptions {
            title: a.str(0),
            total: a.int(1),
            message: a.str(2),
            cancellable: nullish(&a.arg(3)) || a.bool(3),
        });
        let pc = p.clone();
        h.on_cleanup(move || pc.close());
        let obj = Object::new(a.ctx.clone())?;
        let p1: Arc<dyn crate::ui::Progress> = p.clone();
        obj.set(
            "update",
            Function::new(a.ctx.clone(), move |args: Rest<Value>| {
                let get = |i: usize| args.0.get(i).cloned();
                let int = |i: usize| get(i).map(|v| js::to_int(&v)).unwrap_or(0);
                let msg = get(2).filter(|v| !nullish(v)).map(|v| to_str(&v)).unwrap_or_default();
                p1.update(int(0), int(1), &msg);
            })?,
        )?;
        let p2 = p.clone();
        obj.set("cancelled", Function::new(a.ctx.clone(), move || p2.cancelled())?)?;
        let p3 = p.clone();
        obj.set("close", Function::new(a.ctx.clone(), move || p3.close())?)?;
        Ok(obj.into_value())
    });
}
