use super::*;

#[test]
fn formulas_follow_the_cells() {
    let ins = Lines { columns: false, at: 3, count: 2, insert: true }.edit();
    let a = |host: Option<&str>, f: &str| adjusted(host, f, "Data", &ins);
    assert_eq!(a(Some("Data"), "SUM(A1:A5)").as_deref(), Some("SUM(A1:A7)"));
    assert_eq!(a(Some("Data"), "$B$3*2").as_deref(), Some("$B$5*2"));
    assert_eq!(a(Some("Data"), "A1+B2"), None);
    assert_eq!(a(Some("Other"), "A5"), None);
    assert_eq!(a(Some("Other"), "data!A5").as_deref(), Some("data!A7"));
    assert_eq!(a(None, "'Data'!$A$3:$A$10").as_deref(), Some("Data!$A$5:$A$12"));
    assert_eq!(a(Some("Data"), "_xlfn.XLOOKUP(1,A3:A4,B3:B4)").as_deref(), Some("_xlfn.XLOOKUP(1,A5:A6,B5:B6)"));
    assert_eq!(a(Some("Data"), "A:A"), None);
    let del = Lines { columns: true, at: 2, count: 1, insert: false }.edit();
    let d = |f: &str| adjusted(Some("Data"), f, "Data", &del);
    assert_eq!(d("B1").as_deref(), Some("#REF!"));
    assert_eq!(d("SUM(A1:C1)").as_deref(), Some("SUM(A1:B1)"));
    assert_eq!(d("D1&\"B1\"").as_deref(), Some("C1&\"B1\""));
}

#[test]
fn tables_renamed_in_formulas_of_a_copy() {
    let r = vec![("Sales".to_string(), "Sales_2".to_string())];
    assert_eq!(rename_tables("SUM(Sales[Amount])", &r).as_deref(), Some("SUM(Sales_2[Amount])"));
    assert_eq!(rename_tables("SUM(Other[Amount])", &r), None);
    assert_eq!(rename_tables("A1", &r), None);
}

#[test]
fn what_is_cut_apart() {
    let ins = Lines { columns: false, at: 3, count: 1, insert: true };
    assert!(ins.cuts(2, 5));
    assert!(!ins.cuts(3, 5));
    assert!(!ins.cuts(1, 2));
    let del = Lines { columns: false, at: 3, count: 2, insert: false };
    assert!(del.cuts(4, 8));
    assert!(!del.cuts(3, 4));
    assert!(del.cuts(2, 5));
    assert!(!del.cuts(5, 8));
    assert!(del.cuts(1, 3));
    assert_eq!(del.span("B2:C9"), Some((2, 9)));
    assert_eq!(del.span("B:C"), Some((1, MAX_ROWS)));
}
