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
