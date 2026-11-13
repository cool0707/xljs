use std::path::{Path, PathBuf};
use std::time::Duration;

use super::*;
use crate::testutil::{lock_lang, tempdir};

/// A script folder with the script and one case.
fn project(script: &str, case: &str) -> PathBuf {
    let root = tempdir();
    std::fs::write(root.join("main.js"), script).unwrap();
    let dir = root.join(TESTS_DIR).join("基本");
    std::fs::create_dir_all(dir.join("input")).unwrap();
    std::fs::write(dir.join("case.json"), case).unwrap();
    root
}

fn options(approve: bool) -> RunOptions {
    RunOptions { approve, limit: Some(Duration::from_secs(20)), version: "test".into(), ..Default::default() }
}

fn run_dir(root: &Path, approve: bool) -> Vec<Outcome> {
    run_all(&root.to_string_lossy(), &options(approve), |_| {}).unwrap()
}

fn described(o: &Outcome) -> String {
    o.transcript.iter().map(describe).collect::<Vec<_>>().join("\n")
}

fn parse_error(case: &str) -> String {
    let root = project("", case);
    let e = cases(&root).unwrap_err();
    let _ = std::fs::remove_dir_all(&root);
    crate::i18n::error(&e)
}

#[test]
fn case_mistakes_are_told() {
    let _g = lock_lang("ja");
    let e = parse_error("{\n  \"script\": \"main.js\",\n  \"answers\": { \"form\": [ }\n}");
    assert!(e.contains("case.json: 3 行 26 列: expected value") && !e.contains("at line"), "{e}");
    let e = parse_error(r#"{"script": "main.js", "ansers": {}}"#);
    assert!(e.contains("「ansers」はここでは使えません。「answers」ですか？"), "{e}");
    let e = parse_error(r#"{"script": "main.js", "answers": {"comfirm": [true]}}"#);
    assert!(e.contains("「confirm」ですか？"), "{e}");
    let e = parse_error(r#"{"script": "main.js", "answers": {"form": {}}}"#);
    assert!(e.contains("「form」は [...] の形で"), "{e}");
    let e = parse_error(r#"{"script": "main.js", "variants": {"a/b": {}}}"#);
    assert!(e.contains("パターン名「a/b」は使えません"), "{e}");
    let e = parse_error(r#"{"script": "main.js", "variants": {"x": {"zzzzzzzz": 1}}}"#);
    assert!(e.contains("「zzzzzzzz」はここでは使えません（使えるもの: now, args, confirm"), "{e}");
    let e = parse_error(r#"{"script": "main.js", "now": "4/10"}"#);
    assert!(e.contains("「4/10」は日時として読めません"), "{e}");
    let e = parse_error(r#"{"answers": {}}"#);
    assert!(e.contains("「script」（テストするスクリプト）がありません"), "{e}");
    let e = parse_error(r#"{"script": "none.js"}"#);
    assert!(e.contains("スクリプト「none.js」がありません"), "{e}");
}

#[test]
fn variants_lie_over_the_answers() {
    let _g = lock_lang("ja");
    let root = project(
        "",
        r#"{"script": "main.js", "now": "2026-04-10", "args": ["a"],
            "answers": {"form": [{"a": 1, "b": 2}, {"c": 3}], "confirm": [true]},
            "variants": {
              "x": {"form": [{"b": 9}], "confirm": [false, true], "prompt": ["p"]},
              "y": {"form": [{}, null], "now": "2026-05-01 12:30", "args": []}
            }}"#,
    );
    let c = cases(&root).unwrap().remove(0);
    let _ = std::fs::remove_dir_all(&root);
    let p = c.plans();
    assert_eq!(p.iter().map(|x| x.variant.as_str()).collect::<Vec<_>>(), ["", "x", "y"]);
    assert_eq!(p[0].now, "2026-04-10T00:00:00");
    assert_eq!(serde_json::json!(p[1].answers), serde_json::json!({"form": [{"a": 1, "b": 9}, {"c": 3}], "confirm": [false, true], "prompt": ["p"]}));
    assert_eq!(serde_json::json!(p[2].answers), serde_json::json!({"form": [{"a": 1, "b": 2}, null], "confirm": [true]}));
    assert_eq!((p[2].now.as_str(), p[2].args.len()), ("2026-05-01T12:30:00", 0));
    assert_eq!(p[1].args, ["a"]);
}

const SCRIPT: &str = r#"
const v = await ui.form({
  title: "集計",
  fields: [
    { name: "月", label: "対象月", type: "month", required: true },
    { name: "倍", label: "倍率", type: "number", default: 2, min: 1, max: 9 },
  ],
});
if (!v) exit();
if (!confirm("保存しますか？")) { console.log("やめました"); exit(); }
const rows = fs.readText("input/data.csv").trim().split("\n").slice(1).map((l) => l.split(","));
const out = ["名前,金額"].concat(rows.map(([n, a]) => `${n},${Number(a) * v.倍}`));
fs.writeText("out.csv", out.join("\n") + "\n");
fs.writeText(path.join(xljs.scriptDir, "log.txt"), `${dayjs().format("YYYY-MM-DD HH:mm")} ${new Date().getFullYear()} ${xljs.userName}@${xljs.computerName}\n`);
const b = workbooks.create();
b.addSheet("集計").write(rows.map(([n, a]) => ({ 名前: n, 金額: Number(a) * v.倍 })));
b.save("out.xlsx");
alert(`${rows.length} 件を ${v.月} で保存しました: ${path.resolve("out.csv")}`);
"#;

const CASE: &str = r#"{
  "script": "main.js",
  "now": "2026-04-10T09:30:00",
  "answers": { "form": [{ "月": "2026-04" }], "confirm": [true] },
  "variants": {
    "倍率が上限超え": { "form": [{ "倍": 10 }] },
    "保存しない": { "confirm": [false] }
  }
}"#;

fn sample() -> PathBuf {
    let root = project(SCRIPT, CASE);
    std::fs::write(root.join(TESTS_DIR).join("基本/input/data.csv"), "名前,金額\n山田,100\n鈴木,250\n").unwrap();
    root
}

#[test]
fn replay_approve_and_compare() {
    let _g = lock_lang("ja");
    let root = sample();
    let cwd = std::env::current_dir().unwrap();

    let r = run_dir(&root, false);
    assert_eq!(r.iter().map(|o| (o.name(), o.status)).collect::<Vec<_>>(), [
        ("基本".to_string(), Status::New),
        ("基本/倍率が上限超え".to_string(), Status::New),
        ("基本/保存しない".to_string(), Status::New),
    ]);
    let base = described(&r[0]);
    for want in [
        "form \"集計\"",
        "  field 月 (month) \"対象月\" {\"required\":true}",
        "  field 倍 (number) \"倍率\" = \"2\"",
        "  submit {\"月\":\"2026-04\"}",
        "confirm \"保存しますか？\" -> true",
        "alert \"2 件を 2026-04 で保存しました: out.csv\"",
        "end 0",
        "file log.txt (text)",
        "  log.txt: 2026-04-10 09:30 2026 test@TEST-PC",
        "file out.csv (text)",
        "  out.csv: 山田,200",
        "file out.xlsx (book)",
        "  out.xlsx 集計!B3 = \"500\"",
    ] {
        assert!(base.contains(want), "missing {want:?} in:\n{base}");
    }
    // The form is asked again with the error; nothing is left to answer, so it closes.
    let over = described(&r[1]);
    assert!(over.contains("form error: 倍:") && over.contains("  close") && !over.contains("file "), "{over}");
    assert!(over.contains("unused answers: confirm x1"), "{over}");
    let no = described(&r[2]);
    assert!(no.contains("confirm \"保存しますか？\" -> false") && no.contains("out: やめました") && !no.contains("file "), "{no}");

    // Nothing is written to the real folder, and the process is where it was.
    assert!(!root.join("log.txt").exists() && !root.join("out.csv").exists());
    assert_eq!(std::env::current_dir().unwrap(), cwd);
    assert!(!root.join(TESTS_DIR).join("基本/expected").exists());

    let r = run_dir(&root, true);
    assert!(r.iter().all(|o| o.approved && o.ok()));
    assert!(root.join(TESTS_DIR).join("基本/expected/_base.jsonl").is_file());
    assert!(root.join(TESTS_DIR).join("基本/expected/保存しない.jsonl").is_file());
    let r = run_dir(&root, false);
    assert!(r.iter().all(|o| o.status == Status::Pass && o.lines.is_empty()), "{:?}", r.iter().map(|o| &o.lines).collect::<Vec<_>>());

    // A change of the script shows as the difference.
    std::fs::write(root.join("main.js"), SCRIPT.replace("Number(a) * v.倍}`", "Number(a) * v.倍 + 1}`")).unwrap();
    let r = run_dir(&root, false);
    assert_eq!(r.iter().map(|o| o.status).collect::<Vec<_>>(), [Status::Fail, Status::Pass, Status::Pass]);
    assert!(!r[0].ok());
    let diff = r[0].lines.join("\n");
    assert!(diff.contains("-   out.csv: 山田,200\n-   out.csv: 鈴木,500\n+   out.csv: 山田,201\n+   out.csv: 鈴木,501"), "{diff}");
    assert!(diff.contains("    out.csv: 名前,金額\n-"), "{diff}");
    assert!(!diff.contains("out.xlsx"), "{diff}");

    // Only one run.
    let o = RunOptions { only: "基本/保存しない".into(), ..options(false) };
    let r = run_all(&root.join("main.js").to_string_lossy(), &o, |_| {}).unwrap();
    assert_eq!(r.len(), 1);
    let o = RunOptions { only: "ない".into(), ..options(false) };
    assert!(run_all(&root.to_string_lossy(), &o, |_| {}).is_err());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn problems_are_never_approved() {
    let _g = lock_lang("ja");
    // A question without an answer stops the script right there.
    let root = project(
        "if (!confirm('続けますか？')) { fs.writeText('no.txt', 'x'); }\nfs.writeText('yes.txt', 'x');\n",
        r#"{"script": "main.js"}"#,
    );
    let r = run_dir(&root, true);
    assert_eq!((r[0].status, r[0].approved, r[0].ok()), (Status::Problem, false, false));
    assert_eq!(r[0].problems, ["confirm が出ましたが、応答が記録されていません: \"続けますか？\""]);
    let text = described(&r[0]);
    assert!(text.contains("end 125") && !text.contains("file "), "{text}");
    assert!(!root.join(TESTS_DIR).join("基本/expected").exists());

    // A recorded null is the user cancelling: no problem.
    std::fs::write(root.join(TESTS_DIR).join("基本/case.json"), r#"{"script": "main.js", "answers": {"confirm": [null]}}"#).unwrap();
    let r = run_dir(&root, false);
    assert_eq!(r[0].status, Status::New);
    assert!(described(&r[0]).contains("file no.txt (text)"));

    // Answers nobody asked for are part of the run.
    std::fs::write(root.join(TESTS_DIR).join("基本/case.json"), r#"{"script": "main.js", "answers": {"confirm": [true, true], "prompt": ["x"]}}"#).unwrap();
    let r = run_dir(&root, false);
    assert_eq!(r[0].status, Status::New);
    let text = described(&r[0]);
    assert!(text.contains("unused answers: confirm x1\nunused answers: prompt x1\nend 0"), "{text}");
    let _ = std::fs::remove_dir_all(&root);

    // A field the form does not have.
    let root = project(
        "await ui.form({ title: 't', fields: [{ name: '対象月', label: '月' }] });\n",
        r#"{"script": "main.js", "answers": {"form": [{"対象付き": "x"}]}}"#,
    );
    let r = run_dir(&root, false);
    assert_eq!(r[0].problems, ["フォームに項目「対象付き」はありません。「対象月」ですか？"]);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn endless_loop_is_stopped() {
    let _g = lock_lang("ja");
    let root = project("for (;;) {}\n", r#"{"script": "main.js"}"#);
    let o = RunOptions { limit: Some(Duration::from_millis(300)), version: "test".into(), ..Default::default() };
    let r = run_all(&root.to_string_lossy(), &o, |_| {}).unwrap();
    assert_eq!(r[0].status, Status::Problem);
    assert!(r[0].problems[0].contains("無限ループ"), "{:?}", r[0].problems);
    assert!(described(&r[0]).contains("end 124"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn errors_and_new_cases() {
    let _g = lock_lang("ja");
    let root = project("function f() { throw new Error('だめです'); }\nf();\n", r#"{"script": "main.js"}"#);
    let r = run_dir(&root, false);
    let text = described(&r[0]);
    assert!(text.contains("end 1: ") && text.contains("だめです") && !text.contains("    at "), "{text}");

    std::fs::write(root.join("form.js"), "await ui.form({ title: 't', fields: [] });\n").unwrap();
    let p = new_case(&root.join("form.js").to_string_lossy(), "新規", "2026-04-10").unwrap();
    let c = load(&root, p.parent().unwrap()).unwrap();
    assert_eq!((c.script.as_str(), c.now.as_str()), ("form.js", "2026-04-10T09:00:00"));
    assert_eq!(serde_json::json!(c.answers), serde_json::json!({"form": [{}]}));
    assert!(p.parent().unwrap().join("input").is_dir());
    assert!(new_case(&root.join("form.js").to_string_lossy(), "新規", "2026-04-10").is_err());
    assert!(new_case(&root.join("form.js").to_string_lossy(), "a/b", "2026-04-10").is_err());
    assert!(new_case(&root.to_string_lossy(), "x", "2026-04-10").is_err());
    let _ = std::fs::remove_dir_all(&root);
}

/// The files a user would pick, outside the script folder.
fn outside() -> PathBuf {
    let d = tempdir();
    std::fs::write(d.join("a.txt"), "A\n").unwrap();
    std::fs::create_dir_all(d.join("dir")).unwrap();
    std::fs::write(d.join("dir/b.txt"), "B\n").unwrap();
    d
}

fn case_json(root: &Path, name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(root.join(TESTS_DIR).join(name).join("case.json")).unwrap()).unwrap()
}

const RECORDED: &str = r#"
if (!confirm("始めますか？")) exit();
const name = prompt("名前", "x");
const [f] = await showOpenFilePicker();
const dir = await showDirectoryPicker();
const dest = await showSaveFilePicker({ suggestedName: "out.txt" });
const picked = await ui.table([{ a: 1 }, { a: 2 }], { selectable: true });
await ui.form({
  title: "t",
  fields: [{ name: "src", label: "入力", type: "file" }, { name: "memo", label: "メモ" }, { name: "pw", label: "pw", type: "password" }],
  async onSubmit(v, ctx) { ctx.log(fs.readText(v.src).trim() + ":" + v.memo); },
});
const second = await ui.form({ title: "second", fields: [{ name: "x", label: "x" }] });
fs.writeText(dest, [name, fs.readText(f).trim(), dir.walk("*.txt").toArray().length, picked.length, second.x, xljs.args.length].join(",") + "\n");
fs.writeText(path.join(dir.path, "made.txt"), "x");
"#;

#[test]
fn record_then_replay() {
    let _g = lock_lang("ja");
    let root = tempdir();
    let script = root.join("main.js");
    std::fs::write(&script, RECORDED).unwrap();
    let out = outside();
    let p = |rel: &str| out.join(rel).to_string_lossy().into_owned();
    let answers = serde_json::json!({
        "confirm": [true],
        "prompt": ["山田"],
        "showOpenFilePicker": [[p("a.txt")]],
        "showDirectoryPicker": [p("dir")],
        "showSaveFilePicker": [p("dir/out.txt")],
        "table": [[1]],
        "form": [{ "src": p("a.txt"), "memo": "m", "pw": "secret" }, { "x": "y" }],
    });
    let u = std::sync::Arc::new(crate::ui::Mock::from_json(&answers.to_string(), None).unwrap());
    let quiet = || -> crate::engine::Out { std::sync::Arc::new(std::sync::Mutex::new(std::io::sink())) };
    let o = crate::engine::Options { stdout: Some(quiet()), stderr: Some(quiet()), ..Default::default() };
    let args = vec![p("a.txt"), "--flag".to_string()];
    let r = record(u, &script.to_string_lossy(), "記録", &args, "2026-04-10T09:00:00", o).unwrap();
    assert!(matches!(r.result, Ok(0)) && r.failed.is_empty(), "{:?} {:?}", r.result.as_ref().map_err(|e| e.to_string()), r.failed);

    // The paths name the copies; the password is not written; the form
    // with onSubmit is closed (null) before the second one.
    assert_eq!(case_json(&root, "記録"), serde_json::json!({
        "script": "main.js",
        "now": "2026-04-10T09:00:00",
        "args": ["input/a.txt", "--flag"],
        "answers": {
            "confirm": [true],
            "form": [{ "src": "input/a.txt", "memo": "m" }, null, { "x": "y" }],
            "prompt": ["山田"],
            "showDirectoryPicker": ["input/dir"],
            "showOpenFilePicker": [["input/a.txt"]],
            "showSaveFilePicker": ["input/dir/out.txt"],
            "table": [[1]],
        },
        "variants": {},
    }));
    // Copied when picked: without what the script wrote afterwards.
    let input = root.join(TESTS_DIR).join("記録/input");
    assert!(input.join("a.txt").is_file() && input.join("dir/b.txt").is_file());
    assert!(!input.join("dir/made.txt").exists() && !input.join("dir/out.txt").exists());
    assert!(out.join("dir/made.txt").is_file());
    // The same name again is refused.
    let u = std::sync::Arc::new(crate::ui::Mock::from_json("", None).unwrap());
    assert!(record(u, &script.to_string_lossy(), "記録", &[], "2026-04-10T09:00:00", Default::default()).is_err());

    // The password is not recorded, so the run of the case has none: the rest is the same.
    let r = run_dir(&root, false);
    assert_eq!((r[0].status, &r[0].problems), (Status::New, &Vec::new()));
    let text = described(&r[0]);
    for want in ["out: A:m", "form \"second\"", "  input/dir/out.txt: 山田,A,1,1,y,2", "file input/dir/made.txt (text)", "end 0"] {
        assert!(text.contains(want), "missing {want:?} in:\n{text}");
    }
    assert!(!text.contains("unused"), "{text}");
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&out);
}

/// A UI with windows whose pages are played: every window sends the next
/// list of messages (as the page would), then is closed by the user.
struct Pages {
    m: crate::ui::Mock,
    pages: std::sync::Mutex<std::collections::VecDeque<Vec<Value>>>,
}

struct Page {
    o: crate::ui::WindowOptions,
    closed: std::sync::atomic::AtomicBool,
}

impl crate::ui::Window for Page {
    fn post(&self, _: String) {}
    fn close(&self) {
        if !self.closed.swap(true, std::sync::atomic::Ordering::SeqCst)
            && let Some(f) = &self.o.on_closed
        {
            f();
        }
    }
}

impl crate::ui::Ui for Pages {
    fn name(&self) -> String { "native".into() }
    fn alert(&self, a: &str, b: &str) { self.m.alert(a, b) }
    fn confirm(&self, a: &str, b: &str) -> bool { self.m.confirm(a, b) }
    fn prompt(&self, a: &str, b: &str, c: &str) -> Option<String> { self.m.prompt(a, b, c) }
    fn open_files(&self, o: &crate::ui::PickerOptions) -> Vec<String> { self.m.open_files(o) }
    fn save_file(&self, o: &crate::ui::PickerOptions) -> Option<String> { self.m.save_file(o) }
    fn pick_directory(&self, o: &crate::ui::PickerOptions) -> Option<String> { self.m.pick_directory(o) }
    fn progress(&self, o: &crate::ui::ProgressOptions) -> std::sync::Arc<dyn crate::ui::Progress> { self.m.progress(o) }
    fn show_error(&self, a: &str, b: &str) { self.m.show_error(a, b) }
    fn show_in_folder(&self, p: &[String]) -> crate::Result<()> { self.m.show_in_folder(p) }
    fn open_path(&self, t: &str) -> crate::Result<()> { self.m.open_path(t) }
    fn supports_windows(&self) -> bool { true }
    fn form(&self, o: &crate::ui::FormOptions) -> Option<Map<String, Value>> { self.m.form(o) }
    fn table(&self, o: &crate::ui::TableOptions) -> Option<Vec<i64>> { self.m.table(o) }
    fn open_window(&self, o: crate::ui::WindowOptions) -> crate::Result<std::sync::Arc<dyn crate::ui::Window>> {
        let messages = self.pages.lock().unwrap().pop_front().unwrap_or_default();
        let w = std::sync::Arc::new(Page { o, closed: Default::default() });
        let w2 = w.clone();
        std::thread::spawn(move || {
            use crate::ui::Window;
            for m in messages {
                std::thread::sleep(Duration::from_millis(150));
                if w2.closed.load(std::sync::atomic::Ordering::SeqCst) {
                    return;
                }
                if let Some(f) = &w2.o.on_message {
                    f(m.to_string(), Vec::new());
                }
            }
            std::thread::sleep(Duration::from_millis(150));
            w2.close();
        });
        Ok(w)
    }
}

#[test]
fn record_form_windows() {
    let _g = lock_lang("ja");
    let root = tempdir();
    let script = root.join("main.js");
    std::fs::write(
        &script,
        r#"
await ui.form({
  title: "w",
  fields: [{ name: "n", label: "数", type: "number", min: 1, default: 1 }, { name: "f", label: "ファイル", type: "file" }],
  buttons: [{ label: "確認", onClick(v, ctx) { ctx.log("確認 " + v.n); } }],
  async onSubmit(v, ctx) { ctx.log("実行 " + v.n + " " + fs.readText(v.f).trim()); },
});
const picked = await ui.table([{ a: 1 }, { a: 2 }, { a: 3 }], { selectable: true });
const v = await ui.form({ title: "second", fields: [{ name: "x", label: "x" }] });
console.log("終わり", v.x, picked.map((r) => r.a).join("+"));
"#,
    )
    .unwrap();
    let out = outside();
    let file = out.join("a.txt").to_string_lossy().into_owned();
    let ready = serde_json::json!({ "type": "ready" });
    let pages = vec![
        vec![
            ready.clone(),
            // Opening the picker of the field is not a question of its own.
            serde_json::json!({ "type": "pick", "field": "f", "value": "" }),
            serde_json::json!({ "type": "submit", "values": { "n": "0", "f": file } }),
            serde_json::json!({ "type": "submit", "values": { "n": "5", "f": file }, "button": 0 }),
            serde_json::json!({ "type": "submit", "values": { "n": "5", "f": file } }),
        ],
        vec![ready.clone(), serde_json::json!({ "type": "ok", "indices": [0, 2] })],
        vec![ready, serde_json::json!({ "type": "submit", "values": { "x": "y" } })],
    ];
    let answers = serde_json::json!({ "showOpenFilePicker": [[file]] });
    let u = std::sync::Arc::new(Pages { m: crate::ui::Mock::from_json(&answers.to_string(), None).unwrap(), pages: std::sync::Mutex::new(pages.into()) });
    let quiet = || -> crate::engine::Out { std::sync::Arc::new(std::sync::Mutex::new(std::io::sink())) };
    let o = crate::engine::Options { stdout: Some(quiet()), stderr: Some(quiet()), ..Default::default() };
    let r = record(u, &script.to_string_lossy(), "窓", &[], "2026-04-10T09:00:00", o).unwrap();
    assert!(matches!(r.result, Ok(0)), "{:?}", r.result.as_ref().map_err(|e| e.to_string()));
    assert_eq!(case_json(&root, "窓")["answers"], serde_json::json!({
        "form": [{ "n": "0", "f": "input/a.txt" }, { "n": "5", "$button": "確認" }, {}, null, { "x": "y" }],
        "table": [[0, 2]],
    }));

    // The run of the case does what the user did in the windows.
    let r = run_dir(&root, false);
    assert_eq!((r[0].status, &r[0].problems), (Status::New, &Vec::new()));
    let text = described(&r[0]);
    for want in ["form error: n:", "out: 確認 5", "out: 実行 5 A", "  select [0,2]", "form \"second\"", "out: 終わり y 1+3", "end 0"] {
        assert!(text.contains(want), "missing {want:?} in:\n{text}");
    }
    assert!(!text.contains("unused"), "{text}");
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&out);
}

#[test]
fn suggest_and_remove_variants() {
    let _g = lock_lang("ja");
    let root = project(
        r#"await ui.form({
  title: "登録",
  fields: [
    { name: "名前", label: "名前", required: true, maxLength: 3 },
    { name: "数", label: "数量", type: "number", min: 1, max: 9, default: 3 },
    { name: "区分", label: "区分", type: "select", options: ["通常", "至急"], default: "通常" },
    { name: "控え", label: "控え/出す", type: "checkbox" },
  ],
  onSubmit(v, ctx) { ctx.log(`${v.名前} x${v.数} ${v.区分} ${v.控え}`); },
});
"#,
        r#"{"script": "main.js", "answers": {"form": [{"名前": "山田", "数": 9}]}, "variants": {"手書き": {"form": [{"数": 5}]}}}"#,
    );
    let path = root.to_string_lossy().into_owned();
    let added = suggest(&path, "基本", &options(false)).unwrap();
    // The value of the case's own run (数 = 9, 区分 = 通常) is not suggested again.
    assert_eq!(added, ["名前が空", "名前が3文字", "名前が4文字", "数量が下限（1）", "数量が下限未満（0）", "数量が上限超え（10）", "区分＝至急", "控え_出す＝true"]);
    // What was there stays; a second time adds nothing.
    let c = cases(&root).unwrap().remove(0);
    assert_eq!(c.variants.len(), 9);
    assert_eq!((c.variants[0].name.as_str(), serde_json::json!(c.variants[0].answers)), ("手書き", serde_json::json!({"form": [{"数": 5}]})));
    assert_eq!(serde_json::json!(c.variants[5].answers), serde_json::json!({"form": [{"数": 0}]}));
    assert!(suggest(&path, "基本", &options(false)).unwrap().is_empty());
    assert!(suggest(&path, "ない", &options(false)).is_err());

    // The suggestions run: inside the limits the script runs, outside the form refuses.
    let r = run_dir(&root, true);
    assert!(r.iter().all(|o| o.problems.is_empty() && o.approved), "{:?}", r.iter().map(|o| (&o.variant, &o.problems)).collect::<Vec<_>>());
    let of = |v: &str| described(r.iter().find(|o| o.variant == v).unwrap());
    assert!(of("数量が下限（1）").contains("out: 山田 x1 通常 false"));
    assert!(of("数量が下限未満（0）").contains("form error: 数:"));
    assert!(of("名前が4文字").contains("form error: 名前:"));
    assert!(of("区分＝至急").contains("out: 山田 x9 至急 false"));

    // Removing a variant takes its approved result along; removing the case, its folder.
    let expected = c.expected_path("区分＝至急");
    assert!(expected.is_file());
    remove(&path, "基本", "区分＝至急").unwrap();
    assert!(!expected.exists() && cases(&root).unwrap()[0].variants.len() == 8);
    assert!(remove(&path, "基本", "区分＝至急").is_err());
    remove(&path, "基本", "").unwrap();
    assert!(!c.dir.exists());
    let _ = std::fs::remove_dir_all(&root);
}
