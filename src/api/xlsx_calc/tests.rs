use super::*;
use crate::testutil::lock_lang;

/// testdata/xlsx/make_calc.py
fn book() -> Book {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/xlsx/calc.xlsx");
    super::super::xlsx::open_bytes(&std::fs::read(path).unwrap(), path.into()).unwrap()
}

fn raw(b: &Book, sheet: &str, r: &str) -> String {
    let (col, row) = parse_ref(r).unwrap();
    b.sheet(sheet).unwrap().cell((col, row)).map(|c| c.raw_value().to_string()).unwrap_or_default()
}

fn num(b: &Book, sheet: &str, r: &str) -> f64 {
    raw(b, sheet, r).parse().unwrap_or(f64::NAN)
}

fn set(b: &mut Book, sheet: &str, r: &str, v: f64) {
    let (col, row) = parse_ref(r).unwrap();
    b.cell_mut(sheet, col, row).unwrap().set_value_number(v);
}

fn formula(b: &mut Book, sheet: &str, r: &str, f: &str) {
    let (col, row) = parse_ref(r).unwrap();
    let c = b.cell_mut(sheet, col, row).unwrap();
    c.cell_value_mut().set_blank();
    c.set_formula(formula_to_file(f));
}

/// Every formula of the 数式 sheet gives the value beside it (the ones
/// Excel gives for the same workbook).
#[test]
fn formulas_give_the_values_of_excel() {
    let _l = lock_lang("ja");
    let mut b = book();
    let sum = calculate(&mut b);
    assert!(sum.formulas > 200, "{} formulas", sum.formulas);
    let ws = b.sheet("数式").unwrap();
    let mut checked = 0;
    for row in 1..=ws.highest_row() {
        let text = raw(&b, "数式", &format!("A{row}"));
        let (got, want) = (ws.cell((2, row)).unwrap().raw_value(), ws.cell((3, row)).unwrap().raw_value());
        let same = match (got, want) {
            (CellRawValue::Numeric(a), CellRawValue::Numeric(b)) => (a - b).abs() <= 1e-9 * b.abs().max(1.0),
            (a, b) => a.to_string() == b.to_string(),
        };
        assert!(same, "row {row}: {text} gave {got:?}, not {want:?}");
        checked += 1;
    }
    assert!(checked >= 139, "{checked} formulas checked");
}

#[test]
fn names_tables_and_shared_formulas() {
    let _l = lock_lang("ja");
    let mut b = book();
    calculate(&mut b);
    // A name of the sheet, read on that sheet.
    assert_eq!(num(&b, "データ", "I1"), 281.0);
    // The calculated column and the formulas that name the table's columns.
    assert_eq!(num(&b, "表", "D2"), 100.0);
    assert_eq!(num(&b, "表", "D6"), 2500.0);
    assert_eq!(num(&b, "表", "F1"), 5500.0);
    assert_eq!(num(&b, "表", "F2"), 3.0);
    assert_eq!(num(&b, "表", "F3"), 4.0);
    assert_eq!(num(&b, "表", "F4"), 5.0);
    // A shared formula moves with each of its cells; absolute rows stay.
    for (i, want) in [110.0, 440.0, 990.0, 1760.0, 2750.0].iter().enumerate() {
        assert!((num(&b, "共有", &format!("C{}", i + 2)) - want).abs() < 1e-9, "C{}", i + 2);
    }
    assert!((num(&b, "共有", "D6") - 6050.0).abs() < 1e-9);
}

#[test]
fn array_formulas_fill_their_cells() {
    let _l = lock_lang("ja");
    let mut b = book();
    calculate(&mut b);
    // Entered over C1:C5 with Ctrl+Shift+Enter.
    assert_eq!((1..=5).map(|r| num(&b, "配列", &format!("C{r}"))).collect::<Vec<_>>(), [10.0, 40.0, 90.0, 160.0, 250.0]);
    // FILTER spills three rows, SORT five.
    assert_eq!((1..=3).map(|r| num(&b, "配列", &format!("E{r}"))).collect::<Vec<_>>(), [3.0, 4.0, 5.0]);
    assert_eq!(raw(&b, "配列", "E4"), "");
    assert_eq!((1..=5).map(|r| num(&b, "配列", &format!("G{r}"))).collect::<Vec<_>>(), [50.0, 40.0, 30.0, 20.0, 10.0]);
    assert_eq!(num(&b, "配列", "I1"), 550.0);
    assert_eq!(num(&b, "配列", "I2"), 12.0);
    // Saved without dynamic-array metadata: the value of the formula's own row.
    assert_eq!(num(&b, "配列", "I3"), 3.0);
    assert_eq!(num(&b, "配列", "I4"), 150.0);

    // A smaller result takes back the cells it no longer fills.
    set(&mut b, "配列", "B3", 5.0);
    calculate(&mut b);
    assert_eq!((1..=3).map(|r| raw(&b, "配列", &format!("E{r}"))).collect::<Vec<_>>(), ["4", "5", ""]);
    assert_eq!(num(&b, "配列", "I2"), 9.0);
    assert_eq!(num(&b, "配列", "C3"), 15.0);
}

/// What the engine cannot work out keeps the result Excel saved, and so does
/// what depends on it until something else it reads changes.
#[test]
fn formulas_it_cannot_calculate_keep_their_values() {
    let _l = lock_lang("ja");
    let mut b = book();
    let sum = calculate(&mut b);
    let reasons: Vec<(String, &str, Option<&str>)> = sum.skipped.iter().filter(|s| s.sheet == "保存値").map(|s| (s.address.clone(), s.skip.reason(), s.skip.name())).collect();
    assert_eq!(reasons, [("B1".to_string(), "function", Some("MYFUNC")), ("B2".into(), "external", None), ("B3".into(), "name", Some("未定義の名前")), ("B4".into(), "function", Some("PHONETIC"))]);
    assert_eq!(sum.skipped.iter().find(|s| s.address == "B1").map(|s| (s.formula.as_str(), s.skip.message())), Some(("=MYFUNC(A1)", "関数 MYFUNC は計算できません".to_string())));
    assert_eq!([raw(&b, "保存値", "B1"), raw(&b, "保存値", "B2"), raw(&b, "保存値", "B3"), raw(&b, "保存値", "B4")], ["123", "42", "7", "ゴ"]);
    // The formulas are still there.
    assert!(b.sheet("保存値").unwrap().cell((2, 1)).unwrap().is_formula());
    // What reads them is calculated from the saved values.
    assert_eq!(num(&b, "保存値", "C1"), 128.0);
    assert_eq!(num(&b, "保存値", "C2"), 84.0);
    // calculate() works every formula out: also the one whose saved result was off.
    assert_eq!(num(&b, "保存値", "D1"), 50.0);
    // A name of another sheet is not seen from here.
    assert!(sum.skipped.iter().all(|s| s.sheet != "データ"));
}

/// Without calculate(), only what depends on the cells that were written is
/// calculated again (and what has no saved result).
#[test]
fn only_what_changed_is_calculated_again() {
    let _l = lock_lang("ja");
    let mut b = book();
    // Nothing written yet: the saved result stands, right or not.
    refresh(&mut b);
    assert_eq!(num(&b, "保存値", "D1"), 999.0);
    assert_eq!(num(&b, "保存値", "C1"), 128.0);
    // The formulas saved without a result were worked out.
    assert_eq!(num(&b, "数式", "B1"), 64050.0);
    set(&mut b, "保存値", "A1", 7.0);
    assert_eq!(calc_cell(&b, "保存値", "D1").unwrap(), "70");
    assert_eq!(calc_cell(&b, "保存値", "C1").unwrap(), "130");
    // calc() stores nothing; the pages' refresh does.
    assert_eq!(num(&b, "保存値", "D1"), 999.0);
    refresh(&mut b);
    assert_eq!(num(&b, "保存値", "D1"), 70.0);
    assert_eq!(num(&b, "保存値", "C1"), 130.0);
    assert_eq!(num(&b, "保存値", "C2"), 84.0);
    // A formula written by a script, on the cells as they are now.
    formula(&mut b, "保存値", "E1", "=SUM(A1,D1)&\"円\"");
    assert_eq!(calc_cell(&b, "保存値", "E1").unwrap(), "77円");
    formula(&mut b, "保存値", "E2", "=NOSUCH(1)");
    assert_eq!(calc_cell(&b, "保存値", "E2").unwrap(), "#NAME?");
    formula(&mut b, "保存値", "E3", "=1+");
    assert!(calc_cell(&b, "保存値", "E3").is_err());
    // Other changes than the content of cells: everything is calculated again.
    b.sheet_mut("保存値").unwrap();
    assert_eq!(calc_cell(&b, "保存値", "C1").unwrap(), "130");
    refresh(&mut b);
    assert_eq!(num(&b, "数式", "B1"), 64050.0);
}

#[test]
fn the_japanese_functions_follow_the_language() {
    let check = |f: &str, want: &str| {
        let mut b = book();
        formula(&mut b, "保存値", "G1", f);
        assert_eq!(calc_cell(&b, "保存値", "G1").unwrap(), want, "{f}");
    };
    {
        let _l = lock_lang("ja");
        check("=LENB(\"東京都abc\")", "9");
        check("=LEFTB(\"東京都\",4)", "東京");
        check("=TEXT(DATE(2026,4,1),\"ggge年m月d日(aaa)\")", "令和8年4月1日(水)");
        check("=ASC(\"ＡＢＣ　カタカナ\")", "ABC ｶﾀｶﾅ");
        check("=JIS(\"ﾊﾞﾅﾅ\")", "バナナ");
        check("=YEN(1234.5)", "¥1,235");
    }
    let _l = lock_lang("en");
    check("=LENB(\"東京都abc\")", "6");
}

#[test]
fn now_and_today_are_local_and_can_be_fixed() {
    let _l = lock_lang("ja");
    set_fixed_now(Some("2026-04-10T09:30:00"));
    let mut b = book();
    formula(&mut b, "保存値", "G1", "=TEXT(NOW(),\"yyyy/mm/dd hh:mm\")");
    formula(&mut b, "保存値", "G2", "=TEXT(TODAY(),\"yyyy/m/d\")");
    formula(&mut b, "保存値", "G3", "=RAND()");
    let (now, today, rand) = (calc_cell(&b, "保存値", "G1").unwrap(), calc_cell(&b, "保存値", "G2").unwrap(), calc_cell(&b, "保存値", "G3").unwrap());
    calculate(&mut b);
    let stored = raw(&b, "保存値", "G1");
    set_fixed_now(None);
    assert_eq!((now.as_str(), today.as_str(), stored.as_str()), ("2026/04/10 09:30", "2026/4/10", "2026/04/10 09:30"));
    assert!(rand.parse::<f64>().is_ok_and(|r| (0.0..1.0).contains(&r)), "{rand}");
    // The clock is the computer's local time otherwise.
    let mut b = book();
    formula(&mut b, "保存値", "G1", "=TEXT(NOW(),\"yyyy/mm/dd hh\")");
    let near = [chrono::Local::now(), chrono::Local::now() + chrono::Duration::seconds(5)].map(|t| t.format("%Y/%m/%d %H").to_string());
    assert!(near.contains(&calc_cell(&b, "保存値", "G1").unwrap()));
}

#[test]
fn formulas_get_the_prefixes_files_need() {
    assert_eq!(formula_to_file("=SUM(A1:A3)"), "SUM(A1:A3)");
    assert_eq!(formula_to_file("= sum( a1 , 2 )"), " sum( a1 , 2 )");
    assert_eq!(formula_to_file("=XLOOKUP(1,A:A,B:B)"), "_xlfn.XLOOKUP(1,A:A,B:B)");
    assert_eq!(formula_to_file("=_xlfn.XLOOKUP(1,A:A,B:B)"), "_xlfn.XLOOKUP(1,A:A,B:B)");
    assert_eq!(formula_to_file("=IFS(A1>1,\"a\",TRUE,\"b\")&TEXTJOIN(\",\",TRUE,A1:A3)"), "_xlfn.IFS(A1>1,\"a\",TRUE,\"b\")&_xlfn.TEXTJOIN(\",\",TRUE,A1:A3)");
    assert_eq!(formula_to_file("=FILTER(A1:A9,B1:B9>0)"), "_xlfn._xlws.FILTER(A1:A9,B1:B9>0)");
    assert_eq!(formula_to_file("=LET(x,1,x+1)"), "_xlfn.LET(_xlpm.x,1,_xlpm.x+1)");
    // What cannot be read is kept as it is.
    assert_eq!(formula_to_file("=1+"), "1+");
    assert_eq!(formula_from_file("_xlfn.XLOOKUP(1,A:A,B:B)"), "XLOOKUP(1,A:A,B:B)");
    assert_eq!(formula_from_file("_xlfn.LET(_xlpm.x,1,_xlpm.x+1)"), "LET(x,1,x+1)");
    assert_eq!(formula_from_file(" sum( a1 , 2 )"), " sum( a1 , 2 )");
}

#[test]
fn print_areas_given_as_formulas() {
    let _l = lock_lang("ja");
    let b = book();
    assert_eq!(eval_area(&b, "データ", "データ!$A$1:$G$41"), Some((1, 1, 7, 41)));
    assert_eq!(eval_area(&b, "データ", "OFFSET(データ!$A$1,0,0,COUNTA(データ!$A:$A),7)"), Some((1, 1, 7, 41)));
    assert_eq!(eval_area(&b, "データ", "OFFSET($B$2,1,1,MAX(数量列),2)"), Some((3, 3, 4, 15)));
    assert_eq!(eval_area(&b, "データ", "SUM(1,2)"), None);
    assert_eq!(eval_area(&b, "データ", "データ!$A:$C"), None);
}

/// Formulas for many cells (conditional formatting): the relative references move.
#[test]
fn prepared_formulas_move_with_the_cell() {
    let _l = lock_lang("ja");
    let b = book();
    let f = prepare("=$D2>=10").unwrap();
    let hits: Vec<u32> = (2..=41).filter(|r| eval_at(&b, "データ", &f, (1, *r), 0, *r as i64 - 2) == CalcVal::Bool(true)).collect();
    assert_eq!(hits.len(), 12);
    assert!(hits.iter().all(|r| value_at(&b, "データ", 4, *r) == CalcVal::Num(raw(&b, "データ", &format!("D{r}")).parse().unwrap())));
    assert_eq!(eval_at(&b, "データ", &prepare("=ROW()+COLUMN()").unwrap(), (3, 5), 0, 0), CalcVal::Num(8.0));
    // A formula without a saved result is worked out.
    assert_eq!(value_at(&b, "データ", 6, 2), CalcVal::Num(1200.0));
    assert!(prepare("=1+").is_none());
}

/// A circular reference is not calculated: it and what reads it keep their values.
#[test]
fn circular_references_keep_their_values() {
    let _l = lock_lang("ja");
    let mut b = book();
    formula(&mut b, "保存値", "H1", "=H2+1");
    formula(&mut b, "保存値", "H2", "=H1+A1");
    formula(&mut b, "保存値", "H3", "=H1*2");
    formula(&mut b, "保存値", "H4", "=A1*2");
    b.cell_mut("保存値", 8, 3).unwrap().set_formula_result_number(8.0);
    assert_eq!(calc_cell(&b, "保存値", "H1").unwrap(), "#VALUE!");
    assert_eq!(calc_cell(&b, "保存値", "H3").unwrap(), "8", "the saved value");
    let sum = calculate(&mut b);
    let loops: Vec<(&str, &str)> = sum.skipped.iter().filter(|s| s.skip == Skip::Circular).map(|s| (s.address.as_str(), s.formula.as_str())).collect();
    assert_eq!(loops, [("H1", "=H2+1"), ("H2", "=H1+A1"), ("H3", "=H1*2")]);
    assert_eq!([raw(&b, "保存値", "H1"), raw(&b, "保存値", "H3"), raw(&b, "保存値", "H4")], ["", "8", "10"]);
    assert_eq!(Skip::Circular.message(), "循環参照です（数式が自分自身の結果を参照しています）");
}

// ---- formulas as scripts write them, array formulas in files ----

/// A formula as a script writes it (cell.formula = ...).
fn write(b: &mut Book, sheet: &str, r: &str, f: &str) {
    let (col, row) = parse_ref(r).unwrap();
    set_formula(b, sheet, col, row, f).unwrap();
}

/// A formula as a script reads it.
fn shown(b: &Book, sheet: &str, r: &str) -> Option<String> {
    let (col, row) = parse_ref(r).unwrap();
    formula_shown(b, sheet, col, row)
}

/// How the cell holds its formula: (text, range, array formula, dynamic).
fn held(b: &Book, sheet: &str, r: &str) -> Option<(String, String, bool, bool)> {
    let (col, row) = parse_ref(r).unwrap();
    let f = b.sheet(sheet).unwrap().cell((col, row))?.formula_obj()?;
    Some((f.text().to_string(), f.reference().to_string(), *f.formula_type() == CellFormulaValues::Array, f.dynamic_array()))
}

fn column(b: &Book, sheet: &str, col: &str, rows: std::ops::RangeInclusive<u32>) -> Vec<String> {
    rows.map(|r| raw(b, sheet, &format!("{col}{r}"))).collect()
}

/// The workbook saved and opened again.
fn reopened(b: &Book) -> Book {
    super::super::xlsx::open_bytes(&b.to_bytes().unwrap(), "saved.xlsx".into()).unwrap()
}

fn part(b: &Book, name: &str) -> Option<String> {
    super::super::xlsx::zip_part(&b.to_bytes().unwrap(), name).map(|x| String::from_utf8(x).unwrap())
}

/// The array formulas of a file are array formulas in the file that is saved:
/// the one entered over a range, and the ones that spill with their metadata.
#[test]
fn array_formulas_of_a_file_are_saved_as_they_were() {
    let _l = lock_lang("ja");
    let mut b = book();
    assert_eq!(held(&b, "配列", "C1"), Some(("A1:A5*B1:B5".into(), "C1:C5".into(), true, false)));
    assert_eq!(held(&b, "配列", "E1"), Some(("_xlfn._xlws.FILTER(A1:A5,B1:B5>20)".into(), "E1".into(), true, true)));
    assert_eq!(held(&b, "配列", "I3"), Some(("A1:A5".into(), String::new(), false, false)));

    let sheet = part(&b, "xl/worksheets/sheet3.xml").unwrap();
    assert!(sheet.contains(r#"<c r="C1" t="str"><f t="array" ref="C1:C5">A1:A5*B1:B5</f>"#), "{sheet}");
    assert!(sheet.contains(r#"cm="1"><f t="array" ref="E1">_xlfn._xlws.FILTER(A1:A5,B1:B5&gt;20)</f>"#), "{sheet}");
    assert!(sheet.contains(r#"cm="1"><f t="array" ref="G1:G5">_xlfn._xlws.SORT(B1:B5,1,-1)</f>"#), "{sheet}");
    assert!(sheet.contains("<f>A1:A5</f>"), "{sheet}");
    let metadata = part(&b, "xl/metadata.xml").unwrap();
    assert!(metadata.contains(r#"<xda:dynamicArrayProperties fDynamic="1" fCollapsed="0"/>"#), "{metadata}");
    assert!(part(&b, "xl/_rels/workbook.xml.rels").unwrap().contains("relationships/sheetMetadata\" Target=\"metadata.xml\""));
    assert!(part(&b, "[Content_Types].xml").unwrap().contains("<Override PartName=\"/xl/metadata.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheetMetadata+xml\"/>"));

    // Calculated: the formulas say which cells they fill, and the values are in them.
    calculate(&mut b);
    let sheet = part(&b, "xl/worksheets/sheet3.xml").unwrap();
    assert!(sheet.contains(r#"<c r="E1" cm="1"><f t="array" ref="E1:E3">_xlfn._xlws.FILTER(A1:A5,B1:B5&gt;20)</f><v>3</v></c>"#), "{sheet}");
    assert!(sheet.contains(r#"<c r="E2"><v>4</v></c>"#), "{sheet}");

    // Opened again, it is the same workbook: nothing changes when it is calculated.
    let mut again = reopened(&b);
    assert_eq!(held(&again, "配列", "C1"), Some(("A1:A5*B1:B5".into(), "C1:C5".into(), true, false)));
    assert_eq!(held(&again, "配列", "E1").map(|h| (h.1, h.2, h.3)), Some(("E1:E3".into(), true, true)));
    assert_eq!(held(&again, "配列", "E2"), None);
    assert_eq!(calculate(&mut again).changed, 0);
    // What depends on the data follows it, and the range with the result.
    set(&mut again, "配列", "B2", 99.0);
    calculate(&mut again);
    assert_eq!(column(&again, "配列", "E", 1..=5), ["2", "3", "4", "5", ""]);
    assert_eq!(held(&again, "配列", "E1").map(|h| h.1), Some("E1:E4".into()));
    assert_eq!(column(&again, "配列", "C", 1..=2), ["10", "198"]);
}

/// A formula a script writes is one typed into Excel: it spills when it gives
/// several values, and the file holds it as Excel saves it.
#[test]
fn formulas_scripts_write_spill() {
    let _l = lock_lang("ja");
    let mut b = book();
    write(&mut b, "配列", "K1", "=FILTER(A1:A5,B1:B5>20)");
    write(&mut b, "配列", "M1", "=A1:A5*2");
    write(&mut b, "配列", "O1", "=SUM(A1:A5*B1:B5)");
    write(&mut b, "配列", "O2", "=SUM(A1:A5)");
    write(&mut b, "配列", "O3", "=@A1:A5");
    write(&mut b, "配列", "Q1", "=TRANSPOSE(SORT(A1:A3,1,-1))");
    // An array formula with the metadata where the old evaluation would differ.
    assert_eq!(held(&b, "配列", "K1"), Some(("_xlfn._xlws.FILTER(A1:A5,B1:B5>20)".into(), String::new(), true, true)));
    assert_eq!(held(&b, "配列", "M1"), Some(("A1:A5*2".into(), String::new(), true, true)));
    assert_eq!(held(&b, "配列", "O1"), Some(("SUM(A1:A5*B1:B5)".into(), String::new(), true, true)));
    // A plain formula where it gives one value either way, and for an `@`.
    assert_eq!(held(&b, "配列", "O2"), Some(("SUM(A1:A5)".into(), String::new(), false, false)));
    assert_eq!(held(&b, "配列", "O3"), Some(("A1:A5".into(), String::new(), false, false)));
    // Read as written.
    assert_eq!(shown(&b, "配列", "K1").as_deref(), Some("FILTER(A1:A5,B1:B5>20)"));
    assert_eq!(shown(&b, "配列", "O3").as_deref(), Some("@A1:A5"));
    assert_eq!(shown(&b, "配列", "O2").as_deref(), Some("SUM(A1:A5)"));
    assert_eq!(shown(&b, "配列", "P9"), None);
    // The formula in its own cell before it is calculated; cell.calc() gives its first value.
    assert_eq!(raw(&b, "配列", "K2"), "");
    assert_eq!(calc_cell(&b, "配列", "K1").unwrap(), "3");
    assert_eq!(calc_cell(&b, "配列", "O1").unwrap(), "550");

    calculate(&mut b);
    assert_eq!(column(&b, "配列", "K", 1..=4), ["3", "4", "5", ""]);
    assert_eq!(column(&b, "配列", "M", 1..=6), ["2", "4", "6", "8", "10", ""]);
    assert_eq!([raw(&b, "配列", "O1"), raw(&b, "配列", "O2"), raw(&b, "配列", "O3")], ["550", "15", "3"]);
    assert_eq!([raw(&b, "配列", "Q1"), raw(&b, "配列", "R1"), raw(&b, "配列", "S1"), raw(&b, "配列", "Q2")], ["3", "2", "1", ""]);
    assert_eq!(held(&b, "配列", "K1").map(|h| h.1), Some("K1:K3".into()));
    assert_eq!(held(&b, "配列", "Q1").map(|h| h.1), Some("Q1:S1".into()));
    // One value: its own cell, which is what the file says of a formula without a range.
    assert_eq!(held(&b, "配列", "O1").map(|h| h.1), Some(String::new()));
    // Excel calculates when it opens the file.
    assert!(part(&b, "xl/workbook.xml").unwrap().contains("fullCalcOnLoad=\"1\""));
    let sheet = part(&b, "xl/worksheets/sheet3.xml").unwrap();
    assert!(sheet.contains(r#"<c r="M1" cm="1"><f t="array" ref="M1:M5">A1:A5*2</f><v>2</v></c>"#), "{sheet}");
    assert!(sheet.contains(r#"<c r="O1" cm="1"><f t="array" ref="O1">SUM(A1:A5*B1:B5)</f><v>550</v></c>"#), "{sheet}");
    assert!(sheet.contains(r#"<c r="O3"><f>A1:A5</f><v>3</v></c>"#), "{sheet}");

    // Saved and opened: the same formulas, the same values.
    let mut again = reopened(&b);
    assert_eq!(shown(&again, "配列", "K1").as_deref(), Some("FILTER(A1:A5,B1:B5>20)"));
    assert_eq!(held(&again, "配列", "M1"), Some(("A1:A5*2".into(), "M1:M5".into(), true, true)));
    assert_eq!(shown(&again, "配列", "O3").as_deref(), Some("@A1:A5"));
    assert_eq!(column(&again, "配列", "M", 1..=6), ["2", "4", "6", "8", "10", ""]);
    assert_eq!(calculate(&mut again).changed, 0);
}

/// A formula saved without dynamic-array metadata reads with the `@` Excel
/// shows, so that writing the text into another cell gives the same formula.
#[test]
fn formulas_read_as_excel_shows_them() {
    let _l = lock_lang("ja");
    let mut b = book();
    // The file's own: one value of the range, in the formula's row.
    assert_eq!(shown(&b, "配列", "I3").as_deref(), Some("@A1:A5"));
    assert_eq!(shown(&b, "配列", "I1").as_deref(), Some("SUM(C1:C5)"));
    assert_eq!(shown(&b, "配列", "I4").as_deref(), Some("SUM(G1#)"));
    // A column of the table in the row of the formula is one value: no `@` (the text is the file's).
    assert_eq!(shown(&b, "表", "D3").as_deref(), Some("[[#This Row],数量]*[[#This Row],単価]"));
    assert_eq!(shown(&b, "表", "F1").as_deref(), Some("SUM(売上表[金額])"));
    // Array formulas work with every value as they are written.
    assert_eq!(shown(&b, "配列", "C1").as_deref(), Some("A1:A5*B1:B5"));
    assert_eq!(shown(&b, "配列", "E1").as_deref(), Some("FILTER(A1:A5,B1:B5>20)"));
    // A name of a range.
    write(&mut b, "データ", "L5", "=@金額列*2");
    assert_eq!(held(&b, "データ", "L5"), Some(("金額列*2".into(), String::new(), false, false)));
    assert_eq!(shown(&b, "データ", "L5").as_deref(), Some("@金額列*2"));
    assert_eq!(calc_cell(&b, "データ", "L5").unwrap(), "1800");
    // Copied as a script copies a formula: the same formula in the other cell.
    let text = shown(&b, "配列", "I3").unwrap();
    write(&mut b, "配列", "J4", &format!("={text}"));
    assert_eq!(held(&b, "配列", "J4"), Some(("A1:A5".into(), String::new(), false, false)));
    assert_eq!(calc_cell(&b, "配列", "J4").unwrap(), "4");
    // The prefixes of the file never show, and an empty text removes the formula.
    write(&mut b, "配列", "J5", "=_xlfn.XLOOKUP(3,A1:A5,B1:B5)");
    assert_eq!(held(&b, "配列", "J5").map(|h| (h.0, h.3)), Some(("_xlfn.XLOOKUP(3,A1:A5,B1:B5)".into(), false)));
    assert_eq!(shown(&b, "配列", "J5").as_deref(), Some("XLOOKUP(3,A1:A5,B1:B5)"));
    write(&mut b, "配列", "J5", "");
    assert_eq!(shown(&b, "配列", "J5"), None);
    // What cannot be read is kept as it is.
    write(&mut b, "配列", "J6", "=SUM(1,");
    assert_eq!(shown(&b, "配列", "J6").as_deref(), Some("SUM(1,"));
}

/// The cells an array formula fills are the formula's: a value written into
/// one is in its way, and writing over the formula takes its values away.
#[test]
fn writing_into_the_cells_of_an_array_formula() {
    let _l = lock_lang("ja");
    let mut b = book();
    calculate(&mut b);
    assert_eq!(column(&b, "配列", "G", 1..=5), ["50", "40", "30", "20", "10"]);
    // Over a spilled value: the others go, the formula stays in its own cell.
    set(&mut b, "配列", "G3", 7.0);
    assert_eq!(column(&b, "配列", "G", 2..=5), ["", "7", "", ""]);
    assert_eq!(held(&b, "配列", "G1").map(|h| (h.1, h.2, h.3)), Some(("G1".into(), true, true)));
    let saved = part(&b, "xl/worksheets/sheet3.xml").unwrap();
    assert!(saved.contains(r#"<f t="array" ref="G1">_xlfn._xlws.SORT(B1:B5,1,-1)</f>"#), "{saved}");
    // Calculated, it cannot spill (#SPILL!, which files hold as #VALUE!), and what reads it follows.
    calculate(&mut b);
    assert_eq!(column(&b, "配列", "G", 1..=5), ["#VALUE!", "", "7", "", ""]);
    assert_eq!(calc_cell(&b, "配列", "G1").unwrap(), "#SPILL!");
    assert!(raw(&b, "配列", "I4").starts_with('#'), "=SUM(G1#): {}", raw(&b, "配列", "I4"));
    // The cell emptied: it spills again.
    let (col, row) = parse_ref("G3").unwrap();
    b.cell_mut("配列", col, row).unwrap().cell_value_mut().set_blank();
    calculate(&mut b);
    assert_eq!(column(&b, "配列", "G", 1..=5), ["50", "40", "30", "20", "10"]);
    assert_eq!(held(&b, "配列", "G1").map(|h| h.1), Some("G1:G5".into()));
    assert_eq!(raw(&b, "配列", "I4"), "150");

    // Over the formula itself, without calculating: its values go with it.
    set(&mut b, "配列", "G1", 1.0);
    assert_eq!(column(&b, "配列", "G", 1..=5), ["1", "", "", "", ""]);
    // An array formula entered over a range (Ctrl+Shift+Enter) gives its cells up the same way.
    set(&mut b, "配列", "C4", 0.0);
    assert_eq!(column(&b, "配列", "C", 2..=5), ["", "", "0", ""]);
    assert_eq!(held(&b, "配列", "C1"), Some(("A1:A5*B1:B5".into(), "C1".into(), true, false)));
    // A new formula over a spilled value of a workbook that was not calculated here.
    let mut b = reopened(&{
        let mut b = book();
        calculate(&mut b);
        b
    });
    write(&mut b, "配列", "E2", "=A1+100");
    assert_eq!(column(&b, "配列", "E", 1..=3), ["3", "", ""]);
    calculate(&mut b);
    assert_eq!(column(&b, "配列", "E", 1..=3), ["#VALUE!", "101", ""]);
    assert_eq!(raw(&b, "配列", "I2"), "#VALUE!");
}

/// File > Options > Formulas > "Enable iterative calculation": the circular
/// references of such a workbook are calculated, as many times as it says.
#[test]
fn iterative_calculation_follows_the_workbook() {
    let _l = lock_lang("ja");
    let mut b = book();
    b.wb.calculation_properties_mut().set_iterate(true).set_iterate_count(10).set_iterate_delta(0.001);
    b.calc_reset();
    // Counts the iterations; settles on 2 * A1 (A1 is 5).
    write(&mut b, "保存値", "H1", "=H1+1");
    write(&mut b, "保存値", "H2", "=(H2+A1*2)/2");
    write(&mut b, "保存値", "H3", "=H1*2");
    let sum = calculate(&mut b);
    assert!(sum.skipped.iter().all(|s| s.skip != Skip::Circular), "nothing is left out as circular");
    assert_eq!(num(&b, "保存値", "H1"), 10.0);
    assert!((num(&b, "保存値", "H2") - 10.0).abs() < 0.02, "{}", raw(&b, "保存値", "H2"));
    assert_eq!(num(&b, "保存値", "H3"), 20.0);
    // The setting stays in the file, and the next calculation goes on from the values.
    let workbook = part(&b, "xl/workbook.xml").unwrap();
    assert!(workbook.contains("iterate=\"1\"") && workbook.contains("iterateCount=\"10\""), "{workbook}");
    let mut again = reopened(&b);
    calculate(&mut again);
    assert_eq!(num(&again, "保存値", "H1"), 20.0);
}
