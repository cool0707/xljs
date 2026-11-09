use std::path::Path;
use std::sync::Arc;

use serde_json::{Map, Value};

use super::ConsoleState;
use crate::book::Include;
use crate::ui::{FileFilter, FormField, FormOption, FormOptions, PickerOptions, Ui};
use crate::{Error, Result, book, err, t, vba};

/// The workbooks offered by the picker: those with macros first in the
/// list, and the others.
pub(crate) const BOOK_PATTERNS: &[&str] = &["*.xlsm", "*.xlsx", "*.xlsb", "*.xlam", "*.xltm", "*.xltx", "*.xls"];

/// Whether path is an Excel workbook (with or without macros) rather than a
/// script.
pub(crate) fn is_workbook(path: &str) -> bool {
    let ext = Path::new(path).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    matches!(ext.as_str(), "xlsm" | "xlsb" | "xlam" | "xltm" | "xls" | "xla" | "xlt" | "xlsx" | "xltx")
}

fn base(p: &str) -> String {
    Path::new(p).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

/// What inc asks for of the workbooks as one text (Markdown), and the
/// number of VBA modules. With nothing but the VBA code every workbook
/// must have macros; with more, a workbook without them (or that cannot
/// be read) says so in its text.
pub(crate) fn book_text(books: &[String], inc: &Include) -> Result<(String, usize)> {
    let mut parts = Vec::new();
    let mut modules = 0;
    for b in books {
        let name = base(b);
        let data = std::fs::read(b).map_err(|e| Error::path("open", b, e))?;
        let mut s = String::new();
        if inc.vba {
            match vba::read(&data).map_err(|e| e.context(b)) {
                Ok(Some(p)) => {
                    s.push_str(&vba::text(&name, &p));
                    modules += p.modules.len();
                }
                Ok(None) if !inc.sheets() => return Err(err!("cli.vbaNone", name)),
                Err(e) if !inc.sheets() => return Err(e),
                Ok(None) => s.push_str(&format!("{}\n\n{}\n", t!("vba.textTitle", name), t!("book.noVBA"))),
                Err(e) => s.push_str(&format!("{}\n\n{}\n", t!("vba.textTitle", name), crate::i18n::error(&e))),
            }
        }
        if inc.sheets() {
            if !s.is_empty() {
                s.push('\n');
            }
            match book::read(&data) {
                Ok(wb) => s.push_str(&book::text(&name, &wb, inc)),
                Err(e) => s.push_str(&book::error_text(&name, inc, &e)),
            }
            s = s.trim_end().to_string() + "\n";
        }
        parts.push(s);
    }
    Ok((parts.join("\n"), modules))
}

/// The default text file for the workbook: 売上.xlsm → 売上_VBA.txt (売上_数式.txt
/// without the VBA code).
pub(crate) fn text_output(book: &str, inc: &Include) -> String {
    let p = Path::new(book);
    let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let suffix = if inc.vba { "VBA".to_string() } else { t!("cli.bookTextSuffix") };
    p.with_file_name(format!("{stem}_{suffix}.txt")).to_string_lossy().into_owned()
}

/// Takes --include=vba,formulas,... (or --include vba,...) and --all out
/// of the arguments: what to write (the VBA code by default) and the rest.
fn split_include(args: &[String]) -> Result<(Include, Vec<String>)> {
    let mut keys: Vec<String> = Vec::new();
    let mut rest = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        let v = if a == "--include" || a == "-include" {
            i += 1;
            Some(args.get(i).cloned().ok_or_else(|| Error::msg("flag needs an argument: --include"))?)
        } else {
            a.strip_prefix("--include=").or_else(|| a.strip_prefix("-include=")).map(str::to_string)
        };
        match v {
            Some(v) => keys.extend(v.split(',').map(str::to_string)),
            None if a == "--all" || a == "-all" => keys.push("all".into()),
            None => rest.push(args[i].clone()),
        }
        i += 1;
    }
    let inc = if keys.is_empty() { Include::VBA } else { Include::parse(&keys)? };
    if inc.is_empty() {
        return Err(Error::msg("flag needs an argument: --include"));
    }
    Ok((inc, rest))
}

/// xljs vba [--include=vba,formulas,validation,names,conditional | --all]
/// <book>... [-o out.txt]: the VBA code (and what else is asked for) of
/// the workbooks in one text file (next to the first workbook by default,
/// - for stdout).
pub(crate) fn cmd_vba(con: &ConsoleState, args: &[String]) -> i32 {
    let parsed = split_include(args).and_then(|(inc, rest)| super::split_out(&rest).map(|(out, books)| (inc, out, books)));
    let (inc, out, books) = match parsed {
        Ok(x) => x,
        Err(e) => return con.fail(None, &e),
    };
    if books.is_empty() {
        eprintln!("{}", t!("cli.usageVBA"));
        return 2;
    }
    let (text, _) = match book_text(&books, &inc) {
        Ok(x) => x,
        Err(e) => return con.fail(None, &e),
    };
    if out.as_deref() == Some("-") {
        print!("{text}");
        return 0;
    }
    let out = out.unwrap_or_else(|| text_output(&books[0], &inc));
    if let Err(e) = std::fs::write(&out, text) {
        return con.fail(None, &Error::path("open", out, e));
    }
    println!("{}", t!("cli.created", out));
    0
}

/// The size of the form's window (its page).
const FORM_SIZE: (i64, i64) = (520, 300);

/// Asks what to write of the workbooks; None when cancelled. The VBA code
/// is checked when one of them has macros.
fn ask_include(u: &Arc<dyn Ui>, books: &[String]) -> Option<Include> {
    let has_vba = books.iter().any(|b| matches!(vba::read_file(b), Ok(Some(_))));
    let labels = ["cli.bookVBA", "cli.bookFormulas", "cli.bookValidation", "cli.bookNames", "cli.bookConditional"];
    // The workbooks' names (three of them and how many more).
    let mut names = books.iter().take(3).map(|b| base(b)).collect::<Vec<_>>().join(&t!("vba.separator"));
    if books.len() > 3 {
        names.push_str(&t!("cli.bookFormMore", books.len() - 3));
    }
    let value: Vec<Value> = Include::KEYS.iter().filter(|k| has_vba || **k != "vba").map(|k| Value::String(k.to_string())).collect();
    let o = FormOptions {
        title: t!("cli.bookFormTitle"),
        description: t!("cli.bookFormDescription", names),
        submit: t!("cli.bookFormSubmit"),
        fields: vec![FormField {
            name: "include".into(),
            label: t!("cli.bookFormInclude"),
            kind: "checkboxes".into(),
            options: Include::KEYS.iter().zip(labels).map(|(k, l)| FormOption { value: k.to_string(), label: t!(l) }).collect(),
            help: t!("cli.bookFormHelp"),
            value: Value::Array(value),
            ..Default::default()
        }],
        ..Default::default()
    };
    fn check(v: &Map<String, Value>) -> Option<(String, String)> {
        let none = v.get("include").and_then(Value::as_array).is_none_or(|a| a.is_empty());
        none.then(|| ("include".to_string(), t!("cli.bookFormNone")))
    }
    let v = super::form::ask(u, &o, FORM_SIZE, check)?;
    let keys: Vec<String> = v.get("include").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
    Include::parse(&keys).ok().filter(|i| !i.is_empty())
}

/// Handles workbooks given instead of a script (dropped onto xljs.exe or
/// chosen in its picker): asks what to write of them (the VBA code, the
/// formulas, the input rules, the names, the conditional formats) and
/// where to save it as one text file, and offers to open it. Without the
/// native UI the text is written next to the first workbook.
pub(crate) fn drop_vba(con: &ConsoleState, u: Arc<dyn Ui>, books: &[String]) -> i32 {
    let Some(inc) = ask_include(&u, books) else { return 0 };
    let (text, modules) = match book_text(books, &inc) {
        Ok(x) => x,
        Err(e) => return con.fail(Some(&*u), &e),
    };
    let mut out = text_output(&books[0], &inc);
    let native = u.name() == "native";
    if native {
        let abs = std::path::absolute(&out).unwrap_or_else(|_| out.clone().into());
        let p = u.save_file(&PickerOptions {
            title: t!("cli.vbaSaveTitle"),
            start_in: abs.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(),
            suggested_name: abs.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
            filters: vec![FileFilter { description: t!("cli.vbaText"), patterns: vec!["*.txt".into()] }],
            ..Default::default()
        });
        let Some(p) = p else { return 0 };
        out = p;
    }
    if let Err(e) = std::fs::write(&out, text) {
        return con.fail(Some(&*u), &Error::path("open", out, e));
    }
    if !native {
        println!("{}", t!("cli.created", out));
        return 0;
    }
    let saved = if inc == Include::VBA { t!("cli.vbaSaved", modules, out) } else { t!("cli.bookSaved", out) };
    if u.confirm("xljs", &saved)
        && let Err(e) = u.open_path(&out)
    {
        return con.fail(Some(&*u), &e);
    }
    0
}
