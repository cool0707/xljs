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
        "# rules.xlsx のグラフ\n\n## 売上\n\n### Chart 1（L2 から）\n\n- 種類: 集合縦棒、折れ線\n- タイトル「商品別の金額」\n",
        "- 系列 `='売上'!D1`（集合縦棒）: 項目 `='売上'!$A$2:$A$11`、値 `='売上'!$D$2:$D$11`\n",
        "- 系列 `='売上'!C1`（折れ線、第 2 軸）: 値 `='売上'!$C$2:$C$11`\n",
        "- 横（項目）軸: タイトル「商品」\n- 縦（値）軸: タイトル「金額」、最小値 0\n- 第 2 縦（値）軸: タイトル「数量」\n- 凡例: 下\n",
        "## 設定\n\n### Chart 1（D2 から）\n\n- 種類: 円\n- タイトル「単価の割合」\n- 系列（名前なし）: 項目 `='売上'!$A$2:$A$6`、値 `='売上'!$B$2:$B$6`\n- 凡例: 右\n",
    ] {
        assert!(s.contains(want), "want {want:?} in\n{s}");
    }
    // The second sheet has nothing but values and a chart: its heading is only under the charts.
    assert_eq!(s.matches("## 設定").count(), 1, "{s}");
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

/// Charts: names from cells with their cached text, data labels,
/// trendlines, scatter charts, a chart sheet's absolute anchor and the
/// newer kinds.
#[test]
fn charts() {
    let _l = lock_lang("ja");
    let c = |xml: &str, ex: bool| chart::Chart { name: "グラフ 2".into(), range: "B2:H15".into(), sized: true, root: xml::parse(xml.as_bytes()).unwrap(), ex };
    let scatter = c(
        r#"<c:chartSpace xmlns:c="c" xmlns:a="a"><c:chart><c:autoTitleDeleted val="1"/><c:plotArea>
<c:scatterChart><c:scatterStyle val="lineMarker"/><c:ser><c:tx><c:strRef><c:f>Sheet1!$B$1</c:f><c:strCache><c:pt idx="0"><c:v>気温</c:v></c:pt></c:strCache></c:strRef></c:tx>
<c:dLbls><c:showVal val="1"/><c:showCatName val="0"/></c:dLbls><c:trendline><c:trendlineType val="poly"/></c:trendline>
<c:xVal><c:numRef><c:f>Sheet1!$A$2:$A$9</c:f></c:numRef></c:xVal><c:yVal><c:numLit><c:pt idx="0"><c:v>1</c:v></c:pt><c:pt idx="1"><c:v>2</c:v></c:pt></c:numLit></c:yVal></c:ser>
<c:axId val="1"/><c:axId val="2"/></c:scatterChart>
<c:valAx><c:axId val="1"/><c:scaling><c:orientation val="maxMin"/><c:max val="40"/></c:scaling><c:delete val="0"/><c:numFmt formatCode="0.0" sourceLinked="0"/><c:majorUnit val="5"/></c:valAx>
<c:valAx><c:axId val="2"/><c:scaling><c:logBase val="10"/></c:scaling><c:delete val="1"/></c:valAx><c:dTable/></c:plotArea></c:chart></c:chartSpace>"#,
        false,
    );
    assert_eq!(
        chart::describe(&scatter),
        [
            "種類: 散布図",
            "系列「気温」（`=Sheet1!$B$1`）: X `=Sheet1!$A$2:$A$9`、Y {1, 2}、データラベル（値）、近似曲線（多項式）",
            "横（値）軸: 最大値 40、軸を反転、目盛間隔 5、表示形式 `0.0`",
            "縦（値）軸: 非表示、対数目盛（底 10）",
            "凡例なし",
            "データテーブルあり",
        ]
    );
    let bars = c(
        r#"<c:chartSpace xmlns:c="c"><c:chart><c:title/><c:plotArea><c:bar3DChart><c:barDir val="bar"/><c:grouping val="percentStacked"/>
<c:ser><c:tx><c:v>計画</c:v></c:tx><c:val><c:numRef><c:f>S!$B$2:$B$4</c:f></c:numRef></c:val></c:ser><c:axId val="5"/><c:axId val="6"/><c:axId val="7"/></c:bar3DChart>
<c:catAx><c:axId val="5"/><c:title><c:tx><c:strRef><c:f>S!$A$1</c:f></c:strRef></c:tx></c:title></c:catAx><c:valAx><c:axId val="6"/></c:valAx><c:serAx><c:axId val="7"/><c:delete val="1"/></c:serAx>
</c:plotArea><c:legend><c:legendPos val="tr"/></c:legend></c:chart></c:chartSpace>"#,
        false,
    );
    assert_eq!(
        chart::describe(&bars),
        ["種類: 3-D 100% 積み上げ横棒", "タイトル（自動）", "系列「計画」: 値 `=S!$B$2:$B$4`", "縦（項目）軸: タイトル `=S!$A$1`", "奥行き軸: 非表示", "凡例: 右上"]
    );
    let ex = c(
        r#"<cx:chartSpace xmlns:cx="cx"><cx:chartData><cx:data id="0"><cx:strDim type="cat"><cx:f>S!$A$2:$A$6</cx:f></cx:strDim><cx:numDim type="val"><cx:f>S!$B$2:$B$6</cx:f></cx:numDim></cx:data></cx:chartData>
<cx:chart><cx:title><cx:tx><cx:txData><cx:v>収支</cx:v></cx:txData></cx:tx></cx:title><cx:plotArea><cx:plotAreaRegion><cx:series layoutId="waterfall"><cx:tx><cx:txData><cx:f>S!$B$1</cx:f><cx:v>金額</cx:v></cx:txData></cx:tx><cx:dataId val="0"/></cx:series></cx:plotAreaRegion></cx:plotArea></cx:chart></cx:chartSpace>"#,
        true,
    );
    assert_eq!(
        chart::describe(&ex),
        ["種類: ウォーターフォール", "タイトル「収支」", "系列「金額」（`=S!$B$1`）: 項目 `=S!$A$2:$A$6`、値 `=S!$B$2:$B$6`"]
    );
    let sh = Sheet { name: "グラフ".into(), charts: vec![chart::Chart { range: String::new(), ..ex }], ..Default::default() };
    let s = text("b.xlsx", &Workbook { sheets: vec![sh], ..Default::default() }, &Include::parse(&["charts"]).unwrap());
    assert!(s.contains("## グラフ\n\n### グラフ 2\n\n- 種類: ウォーターフォール\n"), "{s}");
    let s = text("b.xlsx", &Workbook::default(), &Include::parse(&["charts"]).unwrap());
    assert_eq!(s, "# b.xlsx のグラフ\n\nグラフはありません。\n\n");
}
