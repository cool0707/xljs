use super::*;

fn book() -> Book {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/xlsx/conditional.xlsx");
    super::super::xlsx::open_bytes(&std::fs::read(path).unwrap(), path.into()).unwrap()
}

fn fx() -> HashMap<(u32, u32), Fx> {
    fx_of(&book())
}

fn fx_of(b: &Book) -> HashMap<(u32, u32), Fx> {
    let theme = b.wb.theme();
    let css = |c: &Color| {
        let s = c.argb_with_theme(theme);
        (s.len() >= 6).then(|| format!("#{}", &s[s.len() - 6..]))
    };
    let today = chrono::NaiveDate::from_ymd_opt(2026, 4, 1).unwrap();
    effects(b, "条件", &sheet_rules(b.sheet("条件").unwrap()), &css, today)
}

#[test]
fn highlight_rules() {
    let f = fx();
    // A8:A10 > $H$1 (7): fill and font, and stop there (no top 3, no underline).
    let a8 = &f[&(1, 8)];
    assert_eq!(a8.fill.as_deref(), Some("#FFC7CE"));
    assert_eq!(a8.color.as_deref(), Some("#9C0006"));
    assert_eq!(a8.bold, Some(true));
    assert_eq!(a8.underline, None);
    assert!(!f.contains_key(&(1, 7)));
    // MOD($A1,2)=0 moves with the row.
    assert_eq!(f[&(1, 2)].underline, Some(true));
    assert_eq!(f[&(1, 6)].underline, Some(true));
    assert!(!f.contains_key(&(1, 3)));
    // Duplicates and text.
    assert_eq!(f[&(7, 1)].italic, Some(true));
    assert_eq!(f[&(7, 3)].italic, Some(true));
    assert!(!f.contains_key(&(7, 2)));
    assert_eq!(f[&(7, 4)].strike, Some(true));
}

#[test]
fn color_scale() {
    let f = fx();
    assert_eq!(f[&(2, 1)].fill.as_deref(), Some("#F8696B"));
    assert_eq!(f[&(2, 10)].fill.as_deref(), Some("#63BE7B"));
    // 50 between the minimum 10 and the median 55.
    assert_eq!(f[&(2, 5)].fill.as_deref(), Some(mix("#F8696B", "#FFEB84", 40.0 / 45.0).as_str()));
}

#[test]
fn data_bars() {
    let f = fx();
    let c1 = f[&(3, 1)].bar.as_ref().unwrap();
    assert_eq!((c1.start, c1.len, c1.fill.as_str(), c1.gradient), (0.0, 0.1, "#638EC6", true));
    assert!((f[&(3, 10)].bar.as_ref().unwrap().len - 0.9).abs() < 1e-9);
    // Excel 2010: from the axis at 4/9, negative values red to the left.
    let e1 = f[&(5, 1)].bar.as_ref().unwrap();
    assert!((e1.start).abs() < 1e-9 && (e1.len - 4.0 / 9.0).abs() < 1e-9);
    assert_eq!((e1.fill.as_str(), e1.leftward, e1.gradient), ("#FF0000", true, false));
    let e10 = f[&(5, 10)].bar.as_ref().unwrap();
    assert!((e10.start - 4.0 / 9.0).abs() < 1e-9 && (e10.len - 5.0 / 9.0).abs() < 1e-9);
    assert_eq!((e10.fill.as_str(), e10.border.as_deref()), ("#63C384", Some("#63C384")));
    assert_eq!(e10.axis.as_ref().map(|a| a.1.as_str()), Some("#000000"));
}

#[test]
fn icon_sets() {
    let f = fx();
    let icon = |c, r| f.get(&(c, r)).and_then(|x| x.icon.clone()).map(|(s, i)| format!("{s}{i}"));
    assert_eq!(icon(4, 1).as_deref(), Some("3Arrows0"));
    assert_eq!(icon(4, 4).as_deref(), Some("3Arrows1"));
    assert_eq!(icon(4, 8).as_deref(), Some("3Arrows2"));
    // 3Stars (Excel 2010): > 8 for the last, icons only.
    assert_eq!(icon(6, 4).as_deref(), Some("3Stars1"));
    assert_eq!(icon(6, 8).as_deref(), Some("3Stars1"));
    assert_eq!(icon(6, 9).as_deref(), Some("3Stars2"));
    assert!(f[&(6, 9)].hide_value);
    for set in ["3Arrows", "3ArrowsGray", "3Flags", "3TrafficLights1", "3TrafficLights2", "3Signs", "3Symbols", "3Symbols2", "3Stars", "3Triangles"] {
        assert!(icon_svg(set, 2).is_some(), "{set}");
    }
    for set in ["4Arrows", "4ArrowsGray", "4RedToBlack", "4Rating", "4TrafficLights"] {
        assert!(icon_svg(set, 3).is_some(), "{set}");
    }
    for set in ["5Arrows", "5ArrowsGray", "5Rating", "5Quarters", "5Boxes"] {
        assert!(icon_svg(set, 4).is_some(), "{set}");
    }
}

#[test]
fn periods() {
    let d = |m, day| chrono::NaiveDate::from_ymd_opt(2026, m, day).unwrap();
    let today = d(4, 1); // Wednesday
    assert!(in_period(d(3, 31), today, "yesterday"));
    assert!(in_period(d(3, 26), today, "last7Days") && !in_period(d(3, 25), today, "last7Days"));
    assert!(in_period(d(3, 29), today, "thisWeek") && in_period(d(4, 4), today, "thisWeek"));
    assert!(in_period(d(3, 28), today, "lastWeek"));
    assert!(in_period(d(3, 1), today, "lastMonth") && in_period(d(5, 31), today, "nextMonth"));
}

#[test]
fn html() {
    let b = book();
    let out = super::super::xlsx_print::workbook_html(&b, &Default::default()).unwrap();
    assert!(out.html.contains("class=\"xljs-bar\""));
    assert!(out.html.contains("class=\"xljs-icon\""));
    assert!(out.html.contains("background-color:#F8696B"));
}

#[test]
fn saved_book_keeps_the_rules() {
    // Saving writes the rules back (icon sets with their kind, the Excel 2010 rules).
    let data = book().to_bytes().unwrap();
    let again = super::super::xlsx::open_bytes(&data, "again.xlsx".into()).unwrap();
    assert_eq!(fx_of(&again), fx());
}
