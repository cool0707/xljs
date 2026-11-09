use super::*;
use crate::testutil::{lock_lang, repo_path};

#[test]
fn rules_book() {
    let _l = lock_lang("ja");
    let wb = read_file(&repo_path("testdata/book/rules.xlsx").to_string_lossy()).unwrap();
    let s = text("rules.xlsx", &wb, &Include::ALL);
    for want in [
        "# rules.xlsx の数式",
        "## 売上\n",
        "- D2:D11: `=B2*C2`\n",
        "- E2:E11: `=D2*(1+税率)`\n",
        "- D12:E12: `=SUM(D2:D11)`\n",
        "- G2:I5: `=SUM($B$2:B2)&\"件\"`\n",
        "- D13: `=XLOOKUP(A2,A:A,D:D)`\n",
        "# rules.xlsx の入力規則",
        "- A2:A11: ユーザー設定 `=COUNTIF($A:$A,A2)=1`（空白は不可、エラー（注意））\n",
        "- C2:C11: 整数 1 〜 100（入力時メッセージ「数量: 1〜100」、エラー（停止）「数量: 1〜100 の整数」）\n",
        "- F2:F11: リスト「東京,大阪,名古屋」（エラーを表示しない）\n",
        "- J2:J11: 日付 >= 2025/01/01（空白は不可、エラーを表示しない）\n",
        "# rules.xlsx の名前の定義",
        "- 税率: `=設定!$B$1`\n",
        "- 対象（シート「売上」のみ）: `=売上!$A$2:$A$11`\n",
        "- 印刷範囲（シート「売上」のみ）: `='売上'!$A$1:$E$12`\n",
        "# rules.xlsx の条件付き書式",
        "- D2:D11: 数式 `=D2>$K$1` → 太字、文字色 #9C0006、塗りつぶし #FFC7CE\n",
        "- C2:C11: セルの値 1 〜 5 → 斜体\n",
        "- B2:B11: カラースケール 最小値 #F8696B → 最大値 #63BE7B\n",
        "- E2:E11: データバー #638EC6（最小値 〜 最大値）\n",
        "- H2:H5: アイコンセット 3TrafficLights1（しきい値: 33%、67%）\n",
    ] {
        assert!(s.contains(want), "want {want:?} in\n{s}");
    }
    // The second sheet has nothing but values and an empty heading is not written.
    assert!(!s.contains("## 設定"), "{s}");
    // The rules are in their order.
    assert!(s.find("- D2:D11: 数式").unwrap() < s.find("- C2:C11: セルの値").unwrap());

    let s = text("rules.xlsx", &wb, &Include::parse(&["names"]).unwrap());
    assert!(s.starts_with("# rules.xlsx の名前の定義") && !s.contains("数式"), "{s}");
}

/// Shared formulas (written once with the range they fill), array
/// formulas and the rules of Excel 2010 in extLst.
#[test]
fn sheet_parts() {
    let _l = lock_lang("ja");
    let xml = r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:x14="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main" xmlns:xm="http://schemas.microsoft.com/office/excel/2006/main"><sheetData>
<row r="2"><c r="C2"><f t="shared" ref="C2:C4" si="0">A2&amp;B2</f><v>1</v></c><c r="D2"><f t="array" ref="D2:D4">TRANSPOSE(F1:H1)</f></c></row>
<row r="3"><c r="C3"><f t="shared" si="0"/><v>1</v></c></row>
<row r="4"><c r="C4"><f t="shared" si="0"/></c><c r="E4"><f>C4&amp;D4</f></c></row>
</sheetData>
<conditionalFormatting sqref="A1:A5"><cfRule type="dataBar" priority="1"><dataBar><cfvo type="min"/><cfvo type="max"/><color rgb="FF638EC6"/></dataBar><extLst><ext uri="{B025F937-C7B1-47D3-B67F-A62EFF666E3E}"><x14:id>{AAA}</x14:id></ext></extLst></cfRule></conditionalFormatting>
<extLst><ext uri="{78C0D931-6437-407d-A8EE-F0AAD7539E65}"><x14:conditionalFormattings>
<x14:conditionalFormatting><x14:cfRule type="dataBar" id="{AAA}"><x14:dataBar/></x14:cfRule><xm:sqref>A1:A5</xm:sqref></x14:conditionalFormatting>
<x14:conditionalFormatting><x14:cfRule type="expression" priority="2" id="{BBB}"><xm:f>B1=他!$A$1</xm:f><x14:dxf><font><b/></font><fill><patternFill><bgColor theme="4" tint="0.79998168889431442"/></patternFill></fill></x14:dxf></x14:cfRule><xm:sqref>B1:B3 B4:B9</xm:sqref></x14:conditionalFormatting>
</x14:conditionalFormattings></ext>
<ext uri="{CCE6A557-97BC-4b89-ADB6-D9C93CAAB3DF}"><x14:dataValidations count="1"><x14:dataValidation type="list" allowBlank="1" showErrorMessage="1"><x14:formula1><xm:f>他!$A$1:$A$3</xm:f></x14:formula1><xm:sqref>B1:B9</xm:sqref></x14:dataValidation></x14:dataValidations></ext></extLst></worksheet>"#;
    let mut sh = Sheet { name: "S".into(), ..Default::default() };
    read_sheet(xml.as_bytes(), &mut sh);
    let wb = Workbook { sheets: vec![sh], theme: ["FFFFFF", "000000", "E7E6E6", "44546A", "4472C4"].map(String::from).to_vec(), ..Default::default() };
    let s = text("b.xlsx", &wb, &Include::ALL);
    for want in [
        "- C2:C4, E4: `=A2&B2`\n",
        "- D2:D4: `=TRANSPOSE(F1:H1)`（配列数式）\n",
        "- B1:B9: リスト `=他!$A$1:$A$3`（エラー（停止））\n",
        "- A1:A5: データバー #638EC6（最小値 〜 最大値）\n",
        "- B1:B9: 数式 `=B1=他!$A$1` → 太字、塗りつぶし #DAE3F3\n",
    ] {
        assert!(s.contains(want), "want {want:?} in\n{s}");
    }
    assert_eq!(s.matches("データバー").count(), 1, "{s}");
}

#[test]
fn unreadable() {
    let _l = lock_lang("ja");
    let e = read(&std::fs::read(repo_path("testdata/vba/macro.xls")).unwrap()).unwrap_err();
    assert_eq!(e.key(), Some("book.oldFormat"));
    assert_eq!(read(b"not a workbook").unwrap_err().key(), Some("book.notWorkbook"));
    let s = error_text("a.xls", &Include::ALL, &e);
    assert!(s.starts_with("# a.xls の数式\n\n") && s.contains("# a.xls の条件付き書式"), "{s}");
    assert!(Include::parse(&["vba", "nope"]).is_err());
    assert_eq!(Include::parse(&["formulas,names".split(',').collect::<Vec<_>>()].concat()).unwrap().keys(), ["formulas", "names"]);
}
