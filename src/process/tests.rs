use super::*;

fn sh(line: &str) -> Spec {
    Spec {
        command: line.into(),
        shell: true,
        ..Default::default()
    }
}

#[test]
fn splits_lines() {
    let mut s = LineSplitter::default();
    assert_eq!(s.push(b"a\r\nb\rc"), vec![b"a".to_vec(), b"b".to_vec()]);
    assert_eq!(s.push(b"\nd\n"), vec![b"c".to_vec(), b"d".to_vec()]);
    assert_eq!(s.push(b"\r"), vec![Vec::<u8>::new()]);
    assert_eq!(s.push(b"\ne"), Vec::<Vec<u8>>::new());
    assert_eq!(s.finish(), Some(b"e".to_vec()));
}

#[test]
fn decodes_cp932_or_utf8() {
    assert_eq!(decode_output("日本".as_bytes(), "auto"), "日本");
    assert_eq!(decode_output(&[0x93, 0xfa, 0x96, 0x7b], "auto"), "日本");
    assert_eq!(decode_output(&[0x93, 0xfa, 0x96, 0x7b], ""), "日本");
}

#[cfg(not(windows))]
#[test]
fn runs_and_captures() {
    let d = Running::start(&sh("echo out; echo err >&2; exit 3"), None, None)
        .unwrap()
        .wait();
    assert_eq!(d.code, Some(3));
    assert_eq!(d.stdout, b"out\n");
    assert_eq!(d.stderr, b"err\n");
    assert!(!d.timed_out);
}

#[cfg(not(windows))]
#[test]
fn input_args_env_cwd() {
    let dir = std::env::temp_dir();
    // The arguments are quoted onto the shell's command line.
    let mut s = sh("cat; printf '%s|' \"$XLJS_T\"; pwd; echo");
    s.args = vec!["a b".into(), "it's".into()];
    s.env.insert("XLJS_T".into(), Some("v".into()));
    s.cwd = Some(dir.to_string_lossy().into_owned());
    let d = Running::start(&s, Some(b"in\n".to_vec()), None)
        .unwrap()
        .wait();
    let out = String::from_utf8(d.stdout).unwrap();
    let real = std::fs::canonicalize(&dir).unwrap();
    assert!(out.starts_with("in\nv|"), "{out}");
    assert!(
        out.contains(&*real.to_string_lossy()) || out.contains(&*dir.to_string_lossy()),
        "{out}"
    );
    assert!(out.trim_end().ends_with("a b it's"), "{out}");

    let direct = Spec {
        command: "printf".into(),
        args: vec!["%s,".into(), "a b".into(), "c".into()],
        ..Default::default()
    };
    assert_eq!(
        Running::start(&direct, None, None).unwrap().wait().stdout,
        b"a b,c,"
    );
}

#[cfg(not(windows))]
#[test]
fn timeout_kills() {
    let mut s = sh("sleep 5");
    s.timeout = 100.0;
    let t = Instant::now();
    let d = Running::start(&s, None, None).unwrap().wait();
    assert!(d.timed_out);
    assert!(t.elapsed() < Duration::from_secs(3));
}

#[cfg(not(windows))]
#[test]
fn streams_lines_and_stdin() {
    let got = Arc::new(Mutex::new(Vec::new()));
    let g = got.clone();
    let cb: OnLines = Arc::new(move |s, l| g.lock().unwrap().extend(l.into_iter().map(|x| (s, x))));
    let s = Spec {
        command: "cat".into(),
        stdin: Io::Pipe,
        ..Default::default()
    };
    let r = Running::start(&s, None, Some(cb)).unwrap();
    assert!(r.running());
    r.write(b"one\ntwo").unwrap();
    r.end();
    let d = r.wait();
    assert_eq!(d.code, Some(0));
    assert!(!r.running());
    assert_eq!(
        *got.lock().unwrap(),
        vec![
            (Stream::Stdout, "one".to_string()),
            (Stream::Stdout, "two".to_string())
        ]
    );
    assert!(r.write(b"x").is_err());
}

#[test]
fn missing_program_and_folder() {
    let _l = crate::test_lang("ja");
    let e = Running::start(
        &Spec {
            command: "xljs-no-such-program".into(),
            ..Default::default()
        },
        None,
        None,
    )
    .err()
    .unwrap();
    assert!(
        crate::i18n::error(&e).contains("xljs-no-such-program"),
        "{}",
        crate::i18n::error(&e)
    );
    let e = Running::start(
        &Spec {
            command: "x".into(),
            cwd: Some("/no/such/dir".into()),
            ..Default::default()
        },
        None,
        None,
    )
    .err()
    .unwrap();
    assert!(crate::i18n::error(&e).contains("/no/such/dir"));
    assert!(Running::start(&Spec::default(), None, None).is_err());
}

#[cfg(windows)]
#[test]
fn windows_command_lines() {
    let mut o = String::new();
    win::quote_arg(r#"a "b" c\"#, &mut o);
    assert_eq!(o, r#""a \"b\" c\\""#);
    let s = Spec {
        command: "x.bat".into(),
        args: vec!["a b".into(), "c&d".into()],
        ..Default::default()
    };
    assert!(
        win::command_line(&s)
            .unwrap()
            .ends_with(r#" /d /s /c "x.bat "a b" "c&d"""#)
    );
    assert!(win::shell_quote("a\"b").is_err());
}

#[cfg(windows)]
#[test]
fn windows_runs_cmd() {
    let d = Running::start(&sh("echo out& echo err 1>&2& exit /b 3"), None, None)
        .unwrap()
        .wait();
    assert_eq!(d.code, Some(3));
    assert_eq!(String::from_utf8_lossy(&d.stdout).trim(), "out");
    assert_eq!(String::from_utf8_lossy(&d.stderr).trim(), "err");
    let mut s = sh("ping -n 30 127.0.0.1 >nul");
    s.timeout = 200.0;
    let t = Instant::now();
    let d = Running::start(&s, None, None).unwrap().wait();
    assert!(d.timed_out && t.elapsed() < Duration::from_secs(10));
}
