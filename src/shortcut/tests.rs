use super::*;

/// Wine's IShellLinkW keeps neither the show command nor the data flags
/// (and writes ANSI paths), so the checks of those are left to Windows.
fn wine() -> bool {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
        if let Ok(m) = GetModuleHandleW(windows::core::w!("ntdll.dll")) {
            return GetProcAddress(m, windows::core::s!("wine_get_version")).is_some();
        }
    }
    false
}

#[test]
fn hotkeys() {
    let _g = crate::test_lang("en");
    for (s, h, back) in [
        ("Ctrl+Alt+K", 0x0600 | 0x4B, "Ctrl+Alt+K"),
        ("alt+ctrl+k", 0x0600 | 0x4B, "Ctrl+Alt+K"),
        ("Ctrl+Shift+F5", 0x0300 | 0x74, "Ctrl+Shift+F5"),
        ("F12", 0x7B, "F12"),
        ("Ctrl+Alt+Num3", 0x0600 | 0x63, "Ctrl+Alt+Num3"),
        ("Ctrl+Alt+Del", 0x0600 | 0x2E, "Ctrl+Alt+Delete"),
        ("Ctrl+Alt+0x6A", 0x0600 | 0x6A, "Ctrl+Alt+0x6A"),
        ("", 0, ""),
    ] {
        assert_eq!(parse_hotkey(s).unwrap(), h, "{s}");
        assert_eq!(format_hotkey(h), back, "{s}");
    }
    for bad in ["Ctrl+", "Win+K", "Ctrl+Alt+KK", "F25"] {
        let e = parse_hotkey(bad).unwrap_err();
        assert_eq!(e.key(), Some("shell.shortcutHotkey"), "{bad}");
    }
}

#[test]
fn window_states() {
    let _g = crate::test_lang("en");
    assert_eq!(parse_window_state("Maximized").unwrap(), SW_SHOWMAXIMIZED);
    assert_eq!(parse_window_state("minimized").unwrap(), SW_SHOWMINNOACTIVE);
    assert_eq!(window_state_name(SW_SHOWMINNOACTIVE), "minimized");
    assert_eq!(window_state_name(0), "normal");
    assert!(parse_window_state("hidden").is_err());
}

#[test]
fn urls() {
    assert!(is_url("https://example.com/"));
    assert!(is_url("mailto:a@example.com"));
    assert!(!is_url("C:\\Windows\\notepad.exe"));
    assert!(!is_url("shell:Downloads"));
    assert!(!is_url("notes.txt"));
}

fn sample() -> Link {
    Link {
        target: "C:\\データ\\売上 2026.xlsx".into(),
        args: "/r \"a b\"".into(),
        cwd: "C:\\データ".into(),
        description: "売上の集計".into(),
        icon: "C:\\Windows\\System32\\shell32.dll".into(),
        icon_index: 21,
        show: SW_SHOWMAXIMIZED,
        hotkey: 0x0600 | b'U' as u16,
        run_as_admin: true,
    }
}

#[test]
fn lnk_round_trip() {
    let l = sample();
    assert_eq!(lnk::parse(&lnk::build(&l)).unwrap(), l);

    let unc = Link { target: "\\\\server\\共有\\部署\\一覧.xlsx".into(), show: SW_SHOWNORMAL, ..Default::default() };
    assert_eq!(lnk::parse(&lnk::build(&unc)).unwrap(), unc);
    let share = Link { target: "\\\\server\\share".into(), show: SW_SHOWNORMAL, ..Default::default() };
    assert_eq!(lnk::parse(&lnk::build(&share)).unwrap().target, "\\\\server\\share");

    // Environment variables are kept in the file and expanded when read.
    let env = Link { target: "%XLJS_LNK_TEST%\\notepad.exe".into(), icon: "%XLJS_LNK_TEST%\\a.ico".into(), show: 1, ..Default::default() };
    let b = lnk::build(&env);
    let _g = crate::test_lang("en");
    unsafe { std::env::set_var("XLJS_LNK_TEST", "C:\\Win") };
    let got = lnk::parse(&b).unwrap();
    unsafe { std::env::remove_var("XLJS_LNK_TEST") };
    assert_eq!(got.target, "C:\\Win\\notepad.exe");
    assert_eq!(got.icon, "C:\\Win\\a.ico");
    assert_eq!(lnk::parse(&b).unwrap().target, "%XLJS_LNK_TEST%\\notepad.exe");

    assert!(lnk::parse(b"[InternetShortcut]\r\n").is_none());
    assert!(lnk::parse(&[]).is_none());
}

/// An item ID list as Windows writes it: My Computer, the drive and file
/// entries with their long names in the 0xBEEF0004 extension block.
#[test]
fn lnk_id_list() {
    fn item(body: &[u8]) -> Vec<u8> {
        let mut v = ((body.len() + 2) as u16).to_le_bytes().to_vec();
        v.extend(body);
        v
    }
    fn file_entry(kind: u8, short: &str, long: &str) -> Vec<u8> {
        let mut b = vec![kind, 0];
        b.extend([0u8; 4 + 4 + 2]);
        b.extend(short.as_bytes());
        b.push(0);
        if b.len() % 2 == 1 {
            b.push(0);
        }
        // Version 9 block: 46 bytes before the name.
        let mut ext = vec![0u8; 46];
        ext[2..4].copy_from_slice(&9u16.to_le_bytes());
        ext[4..8].copy_from_slice(&0xBEEF_0004u32.to_le_bytes());
        ext.extend(long.encode_utf16().flat_map(u16::to_le_bytes));
        ext.extend([0, 0, 0, 0]);
        let n = ext.len() as u16;
        ext[0..2].copy_from_slice(&n.to_le_bytes());
        b.extend(ext);
        item(&b)
    }
    let mut list = item(&[0x1F, 0x50, 0xE0, 0x4F, 0xD0, 0x20, 0xEA, 0x3A, 0x69, 0x10, 0xA2, 0xD8, 0x08, 0x00, 0x2B, 0x30, 0x30, 0x9D]);
    let mut drive = vec![0x2F];
    drive.extend(b"C:\\");
    drive.resize(23, 0);
    list.extend(item(&drive));
    list.extend(file_entry(0x31, "DATA~1", "データ"));
    list.extend(file_entry(0x32, "URIAGE~1.XLS", "売上.xlsx"));
    list.extend([0, 0]);

    let mut b = lnk::build(&Link { args: "x".into(), show: 1, ..Default::default() });
    b[20] |= 1; // HasLinkTargetIDList
    let mut with = b[..0x4C].to_vec();
    with.extend((list.len() as u16).to_le_bytes());
    with.extend(&list);
    with.extend(&b[0x4C..]);
    let l = lnk::parse(&with).unwrap();
    assert_eq!(l.target, "C:\\データ\\売上.xlsx");
    assert_eq!(l.args, "x");
}

/// The temporary directory with its long name: Windows gives the targets
/// of shortcuts with long names (the CI's TEMP is "C:\Users\RUNNER~1\...").
fn tempdir() -> std::path::PathBuf {
    let dir = crate::testutil::tempdir();
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Storage::FileSystem::GetLongPathNameW;
        let short = windows::core::HSTRING::from(dir.as_os_str());
        let mut buf = vec![0u16; 1024];
        let n = GetLongPathNameW(&short, Some(&mut buf)) as usize;
        if n > 0 && n < buf.len() {
            return std::path::PathBuf::from(String::from_utf16_lossy(&buf[..n]));
        }
    }
    dir
}

fn cd(dir: &std::path::Path) -> impl Drop {
    struct Back(std::path::PathBuf);
    impl Drop for Back {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.0);
        }
    }
    let back = Back(std::env::current_dir().unwrap());
    std::env::set_current_dir(dir).unwrap();
    back
}

#[test]
fn write_and_read() {
    let _g = crate::test_lang("en");
    let dir = tempdir();
    let _back = cd(&dir);
    std::fs::write("a.txt", "x").unwrap();
    let abs = |p: &str| std::path::absolute(p).unwrap().to_string_lossy().into_owned();

    let c = Changes {
        target: Some("a.txt".into()),
        args: Some("--x".into()),
        description: Some("メモ".into()),
        hotkey: Some("Ctrl+Alt+M".into()),
        window_state: Some("minimized".into()),
        ..Default::default()
    };
    let p = write("メモ", Operation::Create, &c).unwrap();
    assert_eq!(p, abs("メモ.lnk"));
    let d = read("メモ").unwrap();
    assert_eq!(d.target, abs("a.txt"));
    assert_eq!(d.args, "--x");
    assert_eq!(d.description, "メモ");
    assert_eq!(d.hotkey, "Ctrl+Alt+M");
    if !wine() {
        assert_eq!(d.window_state, "minimized");
    }
    assert_eq!(d.path, abs("メモ.lnk"));
    assert!(!d.run_as_admin);

    // update keeps what is not given; create and replace start over.
    write("メモ.lnk", Operation::Update, &Changes { args: Some("--y".into()), run_as_admin: Some(true), ..Default::default() }).unwrap();
    let d = read("メモ.lnk").unwrap();
    assert_eq!((d.args.as_str(), d.description.as_str(), d.target.as_str()), ("--y", "メモ", abs("a.txt").as_str()));
    assert!(d.run_as_admin || wine());
    write("メモ.lnk", Operation::Replace, &Changes { target: Some("a.txt".into()), ..Default::default() }).unwrap();
    let d = read("メモ.lnk").unwrap();
    assert_eq!((d.args.as_str(), d.description.as_str(), d.hotkey.as_str(), d.window_state.as_str()), ("", "", "", "normal"));

    // Errors.
    let e = write("none", Operation::Update, &Changes::default()).unwrap_err();
    assert!(e.is_not_found(), "{e}");
    let e = write("none", Operation::Replace, &c).unwrap_err();
    assert!(e.is_not_found(), "{e}");
    assert_eq!(write("x", Operation::Create, &Changes::default()).unwrap_err().key(), Some("shell.shortcutTarget"));
    let e = write("x.lnk", Operation::Create, &Changes { target: Some("https://example.com/".into()), ..Default::default() }).unwrap_err();
    assert_eq!(e.key(), Some("shell.shortcutURLTarget"));
    let e = write("x", Operation::Create, &Changes { target: Some("a.txt".into()), hotkey: Some("Win+K".into()), ..Default::default() }).unwrap_err();
    assert_eq!(e.key(), Some("shell.shortcutHotkey"));
    let e = write("no/dir/x", Operation::Create, &Changes { target: Some("a.txt".into()), ..Default::default() }).unwrap_err();
    assert!(e.is_not_found(), "{e}");
    assert!(read("none").unwrap_err().is_not_found());
    assert_eq!(read("a.txt").unwrap_err().key(), Some("shell.notShortcut"));
    assert_eq!(Operation::parse("move").unwrap_err().key(), Some("shell.shortcutOperation"));
    drop(_back);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn url_files() {
    let _g = crate::test_lang("en");
    let dir = tempdir();
    let _back = cd(&dir);
    let c = Changes { target: Some("https://example.com/a?b=1".into()), ..Default::default() };
    let p = write("サイト", Operation::Create, &c).unwrap();
    assert!(p.ends_with("サイト.url"), "{p}");
    assert_eq!(std::fs::read_to_string(&p).unwrap(), "[InternetShortcut]\r\nURL=https://example.com/a?b=1\r\n");
    let d = read("サイト").unwrap();
    assert_eq!((d.target.as_str(), d.icon.as_str(), d.icon_index), ("https://example.com/a?b=1", "", 0));

    // Windows' own keys and sections stay on update.
    std::fs::write(&p, "[{000214A0-0000-0000-C000-000000000046}]\r\nProp3=19,11\r\n[InternetShortcut]\r\nIDList=\r\nURL=https://old.example/\r\nHotKey=0\r\n").unwrap();
    write(&p, Operation::Update, &Changes { icon: Some("C:\\アイコン\\a.ico".into()), icon_index: Some(2), ..Default::default() }).unwrap();
    let b = std::fs::read(&p).unwrap();
    let text = encoding_rs::SHIFT_JIS.decode(&b).0;
    assert_eq!(
        text,
        "[{000214A0-0000-0000-C000-000000000046}]\r\nProp3=19,11\r\n[InternetShortcut]\r\nURL=https://old.example/\r\nIconFile=C:\\アイコン\\a.ico\r\nIconIndex=2\r\nIDList=\r\nHotKey=0\r\n"
    );
    let d = read(&p).unwrap();
    assert_eq!((d.target.as_str(), d.icon.as_str(), d.icon_index), ("https://old.example/", "C:\\アイコン\\a.ico", 2));
    let e = write(&p, Operation::Update, &Changes { target: Some("a.txt".into()), ..Default::default() }).unwrap_err();
    assert_eq!(e.key(), Some("shell.expectedURL"));
    drop(_back);
    let _ = std::fs::remove_dir_all(&dir);
}

/// IShellLinkW and the format code agree: each reads what the other wrote.
#[cfg(windows)]
#[test]
fn windows_agrees_with_the_format() {
    if wine() {
        return;
    }
    let dir = tempdir();
    let target = dir.join("売上 2026.xlsx");
    std::fs::write(&target, "x").unwrap();
    let l = Link { target: target.to_string_lossy().into_owned(), cwd: dir.to_string_lossy().into_owned(), ..sample() };

    let p = dir.join("com.lnk").to_string_lossy().into_owned();
    win::save(&p, &l, None).unwrap();
    assert_eq!(win::load(&p).unwrap(), l);
    assert_eq!(lnk::parse(&std::fs::read(&p).unwrap()).unwrap(), l);

    let p = dir.join("format.lnk").to_string_lossy().into_owned();
    std::fs::write(&p, lnk::build(&l)).unwrap();
    assert_eq!(win::load(&p).unwrap(), l);

    // update keeps the rest of the file.
    let prev = win::load(&p).unwrap();
    let next = Link { args: "--other".into(), ..prev.clone() };
    win::save(&p, &next, Some(&prev)).unwrap();
    assert_eq!(win::load(&p).unwrap(), next);

    let env = Link { target: "%SystemRoot%\\notepad.exe".into(), show: SW_SHOWNORMAL, ..Default::default() };
    let p = dir.join("env.lnk").to_string_lossy().into_owned();
    win::save(&p, &env, None).unwrap();
    let windir = std::env::var("SystemRoot").unwrap();
    assert!(win::load(&p).unwrap().target.eq_ignore_ascii_case(&format!("{windir}\\notepad.exe")));
    assert!(lnk::parse(&std::fs::read(&p).unwrap()).unwrap().target.eq_ignore_ascii_case(&format!("{windir}\\notepad.exe")));
    let _ = std::fs::remove_dir_all(&dir);
}
