use std::sync::Arc;

use super::*;
use crate::book::Include;
use crate::testutil::{SharedBuf, lock_lang, repo_path, tempdir};

fn launch_with(answers: serde_json::Value) -> (i32, String) {
    let log = SharedBuf::default();
    let u = Arc::new(ui::Mock::from_json(&answers.to_string(), Some(log.sink())).unwrap());
    let code = launch::launch(
        &ConsoleState {
            has: true,
            own: false,
        },
        u,
        false,
        &Stop::default(),
    );
    (code, log.text())
}

#[test]
fn launch_screen() {
    let _l = lock_lang("ja");
    let dir = tempdir();
    let script = dir.join("hello.js");
    std::fs::write(&script, r#"ui.alert("こんにちは");"#).unwrap();
    let s = script.to_string_lossy().into_owned();

    let (code, log) = launch_with(serde_json::json!({}));
    assert!(code == 0 && log.contains("-> -1"), "cancel: {code}\n{log}");

    let (code, log) = launch_with(serde_json::json!({"choose": [0], "showOpenFilePicker": [[s]]}));
    assert!(
        code == 0 && log.contains("こんにちは"),
        "run: {code}\n{log}"
    );

    // The development mode needs WebView2 windows, which the mock has not.
    let (code, log) = launch_with(serde_json::json!({"choose": [2]}));
    assert_eq!(code, 1, "dev\n{log}");

    let out = dir.join("hello.exe");
    let (code, log) = launch_with(
        serde_json::json!({"choose": [3], "showOpenFilePicker": [[s]], "showSaveFilePicker": [out.to_string_lossy()]}),
    );
    assert_eq!(code, 0, "pack\n{log}");
    let p = pack::read(&out).unwrap().unwrap();
    assert_eq!(p.name, "hello.js");
    std::fs::remove_file(&out).ok();

    let book = dir.join("売上.xlsm");
    std::fs::copy(repo_path("testdata/scripts/macro.xlsm"), &book).unwrap();
    let (code, log) = launch_with(
        serde_json::json!({"choose": [4], "showOpenFilePicker": [[book.to_string_lossy()]], "form": [{"include": ["vba"]}]}),
    );
    assert_eq!(code, 0, "vba\n{log}");
    assert!(dir.join("売上_VBA.txt").exists());
    // Cancelled when asked what to extract.
    std::fs::remove_file(dir.join("売上_VBA.txt")).unwrap();
    let (code, log) = launch_with(
        serde_json::json!({"choose": [4], "showOpenFilePicker": [[book.to_string_lossy()]]}),
    );
    assert!(
        code == 0 && !dir.join("売上_VBA.txt").exists(),
        "vba cancel\n{log}"
    );
}

#[test]
fn drop_vba() {
    let _l = lock_lang("ja");
    let dir = tempdir();
    let (a, b) = (dir.join("売上.xlsm"), dir.join("b.xlsm"));
    std::fs::copy(repo_path("testdata/scripts/macro.xlsm"), &a).unwrap();
    std::fs::copy(repo_path("testdata/scripts/macro.xlsm"), &b).unwrap();
    let (a, b) = (
        a.to_string_lossy().into_owned(),
        b.to_string_lossy().into_owned(),
    );
    assert!(vba::is_workbook(&a) && vba::is_workbook("x.XLS") && !vba::is_workbook("x.ts"));
    let con = ConsoleState {
        has: true,
        own: false,
    };
    let log = SharedBuf::default();
    let u = Arc::new(
        ui::Mock::from_json(
            r#"{"form": [{"include": []}, {"include": ["vba"]}]}"#,
            Some(log.sink()),
        )
        .unwrap(),
    );
    assert_eq!(vba::drop_vba(&con, u, &[a.clone(), b]), 0);
    // Nothing chosen: asked again.
    assert!(
        log.text()
            .contains("form error: include: 1 つ以上選んでください"),
        "{}",
        log.text()
    );
    let text = std::fs::read_to_string(dir.join("売上_VBA.txt")).unwrap();
    assert!(
        text.starts_with("# ") && text.matches("\n# ").count() == 1,
        "want two workbooks:\n{text}"
    );
    assert!(text.contains("Sub 売上集計()"));

    let plain = dir.join("plain.xlsx");
    std::fs::copy(repo_path("testdata/book/rules.xlsx"), &plain).unwrap();
    let plain = plain.to_string_lossy().into_owned();
    let e = vba::book_text(&[a.clone(), plain.clone()], &Include::VBA).unwrap_err();
    assert_eq!(e.key(), Some("cli.vbaNone"));
    assert!(
        vba::book_text(
            &[dir.join("missing.xlsm").to_string_lossy().into_owned()],
            &Include::ALL
        )
        .is_err()
    );

    // With more than the VBA code, a workbook without macros says so.
    let (text, modules) = vba::book_text(&[a.clone(), plain.clone()], &Include::ALL).unwrap();
    assert!(modules > 0);
    for want in [
        "# 売上.xlsm の VBA コード",
        "# 売上.xlsm の数式\n\n数式はありません。",
        "# plain.xlsx の VBA コード\n\nマクロ（VBA）は含まれていません。",
        "- D2:D11: `=B2*C2`",
    ] {
        assert!(text.contains(want), "want {want:?} in\n{text}");
    }
    // A workbook without macros is offered without the VBA code checked.
    let log = SharedBuf::default();
    let u = Arc::new(
        ui::Mock::from_json(
            r#"{"form": [{"include": ["formulas", "names"]}]}"#,
            Some(log.sink()),
        )
        .unwrap(),
    );
    assert_eq!(vba::drop_vba(&con, u, &[plain.clone()]), 0);
    let text = std::fs::read_to_string(dir.join("plain_数式.txt")).unwrap();
    assert!(
        text.starts_with("# plain.xlsx の数式")
            && text.contains("# plain.xlsx の名前の定義")
            && !text.contains("VBA"),
        "{text}"
    );

    // xljs vba --include
    let out = dir.join("out.txt").to_string_lossy().into_owned();
    assert_eq!(
        vba::cmd_vba(
            &con,
            &[
                "--include=formulas,conditional".into(),
                plain.clone(),
                "-o".into(),
                out.clone()
            ]
        ),
        0
    );
    let text = std::fs::read_to_string(&out).unwrap();
    assert!(
        text.contains("# plain.xlsx の条件付き書式") && !text.contains("入力規則"),
        "{text}"
    );
    assert_eq!(
        vba::cmd_vba(
            &con,
            &["--all".into(), plain.clone(), "-o".into(), out.clone()]
        ),
        0
    );
    assert!(
        std::fs::read_to_string(&out)
            .unwrap()
            .contains("# plain.xlsx の入力規則")
    );
    assert_eq!(
        vba::cmd_vba(&con, &["--include=nope".into(), plain, "-o".into(), out]),
        1
    );
}

#[test]
fn flags() {
    let args: Vec<String> = [
        "--ui=mock",
        "-lang",
        "en",
        "--debug",
        "s.js",
        "--debug",
        "x",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let (f, rest) = parse_flags(&args).unwrap();
    assert_eq!(f.ui.as_deref(), Some("mock"));
    assert_eq!(f.lang.as_deref(), Some("en"));
    assert_eq!(f.debug.as_deref(), Some("script"));
    assert_eq!(rest, vec!["s.js", "--debug", "x"]);
    assert!(parse_flags(&["--nope".to_string()]).is_err());
    let (f, rest) = parse_flags(&["--dev".to_string(), "a.js".to_string()]).unwrap();
    assert!(f.dev && rest == vec!["a.js".to_string()]);
    let (out, rest) = split_out(&["a.js".into(), "-o".into(), "b.exe".into()]).unwrap();
    assert_eq!(
        (out.as_deref(), rest),
        (Some("b.exe"), vec!["a.js".to_string()])
    );
}

#[test]
fn watchdog_flag() {
    let s = |v: f64| Some(Duration::from_secs_f64(v));
    let def = Some(watchdog::DEFAULT_LIMIT);
    for (v, native, want) in [
        ("", true, def),
        ("", false, None),
        ("on", false, def),
        ("off", true, None),
        ("0", true, None),
        ("30", false, s(30.0)),
        ("2.5", true, s(2.5)),
    ] {
        assert_eq!(
            parse_watchdog(v, native).unwrap(),
            want,
            "{v:?} native={native}"
        );
    }
    for v in ["-1", "abc", "inf"] {
        assert!(parse_watchdog(v, true).is_err(), "{v:?}");
    }
    let (f, rest) = parse_flags(&[
        "--watchdog=off".into(),
        "--stop-key".into(),
        "f12".into(),
        "a.js".into(),
    ])
    .unwrap();
    assert_eq!(
        (f.watchdog.as_deref(), f.stop_key.as_deref(), rest),
        (Some("off"), Some("f12"), vec!["a.js".to_string()])
    );
}

#[test]
fn memory_floor_value() {
    let def = Some(crate::engine::memory::DEFAULT_FLOOR);
    for (v, want) in [
        ("", def),
        ("on", def),
        ("off", None),
        ("0", None),
        ("200", Some(200 << 20)),
    ] {
        assert_eq!(parse_memory_floor(v).unwrap(), want, "{v:?}");
    }
    for v in ["-1", "abc", "1.5"] {
        assert!(parse_memory_floor(v).is_err(), "{v:?}");
    }
}

#[test]
fn stop_key_flag() {
    let key = |s: &str| Some(watchdog::StopKey::parse(s).unwrap());
    for (v, native, want) in [
        ("", true, key("esc")),
        ("", false, None),
        ("on", false, key("esc")),
        ("off", true, None),
        ("F12", false, key("f12")),
        ("ctrl+shift+q", true, key("ctrl+shift+q")),
    ] {
        assert_eq!(
            parse_stop_key(v, native).unwrap(),
            want,
            "{v:?} native={native}"
        );
    }
    for v in ["hoge", "ctrl", "q+w"] {
        assert!(parse_stop_key(v, true).is_err(), "{v:?}");
    }
}
