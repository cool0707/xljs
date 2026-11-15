import { ok, equal, deepEqual, throws } from "./_assert.js";

// A workbook made by the script: formulas have no result until they are calculated.
const wb = workbooks.create();
const ws = wb.addSheet("見積");
ws.write([
  ["品名", "単価", "数量", "金額"],
  ["りんご", 120, 3],
  ["みかん", 80, 10],
  ["ぶどう", 500, 2],
]);
for (let r = 2; r <= 4; r++) ws.cell(`D${r}`).formula = `=B${r}*C${r}`;
ws.cell("C6").value = "小計";
ws.cell("D6").formula = "=SUM(D2:D4)";
ws.cell("C7").value = "消費税";
ws.cell("D7").formula = "=ROUNDDOWN(D6*0.1,0)";
ws.cell("C8").value = "合計";
ws.cell("D8").formula = "=D6+D7";
equal(ws.cell("D8").value, null, "not calculated yet");

// cell.calc() works a formula out from the cells as they are, and stores nothing.
equal(ws.cell("D8").calc(), "2376");
equal(ws.cell("B2").calc(), "120", "a value as text");
equal(ws.cell("Z9").calc(), "");
equal(ws.cell("D8").value, null);

// book.calculate() calculates every formula and stores the results.
let r = wb.calculate();
deepEqual(r, { formulas: 6, changed: 6, skipped: [] });
equal(ws.cell("D6").value, 2160);
equal(ws.cell("D7").value, 216);
equal(ws.cell("D8").value, 2376);
deepEqual(ws.toArrays()[1], ["りんご", 120, 3, 360]);
equal(ws.cell("D6").formula, "=SUM(D2:D4)", "the formulas stay");
deepEqual(wb.calculate(), { formulas: 6, changed: 0, skipped: [] }, "nothing changed");

// After a value changes, the stored results are the old ones until the next calculate().
ws.cell("C2").value = 30;
equal(ws.cell("D8").value, 2376);
equal(ws.cell("D8").calc(), "5940", "calc() follows the change");
r = wb.calculate();
equal(r.changed, 4, "D2, D6, D7, D8");
equal(ws.cell("D8").value, 5940);

// Functions, text and dates.
const fx = wb.addSheet("関数");
fx.cell("A1").value = new Date(2026, 3, 1);
fx.cell("A2").value = "東京都ﾁﾖﾀﾞ区";
const cases = [
  ['=XLOOKUP("みかん",見積!A2:A4,見積!D2:D4)', 800],
  ['=VLOOKUP("ぶどう",見積!A2:D4,4,FALSE)', 1000],
  ['=IFERROR(VLOOKUP("なし",見積!A2:D4,4,FALSE),"該当なし")', "該当なし"],
  ['=SUMIFS(見積!D2:D4,見積!B2:B4,">=100")', 4600],
  ['=COUNTIF(見積!A2:A4,"*ん*")', 2],
  ['=TEXT(見積!D8,"#,##0")&"円"', "5,940円"],
  ['=TEXT(A1,"yyyy/m/d(aaa)")', "2026/4/1(水)"],
  ['=TEXT(A1,"ggge年m月d日")', "令和8年4月1日"],
  ["=EOMONTH(A1,0)-A1", 29],
  ['=DATEDIF(A1,DATE(2027,6,15),"Y")&"年"&DATEDIF(A1,DATE(2027,6,15),"YM")&"か月"', "1年2か月"],
  ["=LEN(A2)&\"/\"&LENB(A2)", "8/12"],
  ["=JIS(A2)", "東京都チヨダ区"],
  ['=IFS(見積!D8>=5000,"大",見積!D8>=1000,"中",TRUE,"小")', "大"],
  ['=TEXTJOIN("・",TRUE,見積!A2:A4)', "りんご・みかん・ぶどう"],
  ["=SUM(FILTER(見積!D2:D4,見積!C2:C4>=10))", 4400],
  ["=LET(税,0.1,ROUND(見積!D6*(1+税),0))", 5940],
  ["=1/0", "#DIV/0!"],
];
cases.forEach(([f], i) => { fx.cell(i + 1, 3).formula = f; });
r = wb.calculate();
equal(r.skipped.length, 0);
cases.forEach(([f, want], i) => equal(fx.cell(i + 1, 3).value, want, f));
// The formulas read as they were written; the file holds the prefixes Excel needs.
equal(fx.cell("C1").formula, '=XLOOKUP("みかん",見積!A2:A4,見積!D2:D4)');
equal(fx.cell("C16").formula, "=LET(税,0.1,ROUND(見積!D6*(1+税),0))");

// What cannot be calculated is told, and keeps the value it has.
fx.cell("E1").formula = "=MYMACRO(A1)";
fx.cell("E2").formula = "='[台帳.xlsx]Sheet1'!A1";
fx.cell("E3").formula = "=存在しない名前+1";
fx.cell("E4").formula = "=E1&\"!\"";
r = wb.calculate();
deepEqual(r.skipped.map((s) => [s.sheet, s.address, s.reason, s.name]), [
  ["関数", "E1", "function", "MYMACRO"],
  ["関数", "E2", "external", undefined],
  ["関数", "E3", "name", "存在しない名前"],
]);
equal(r.skipped[0].formula, "=MYMACRO(A1)");
ok(/MYMACRO/.test(r.skipped[0].message) && /計算できません/.test(r.skipped[0].message), r.skipped[0].message);
equal(fx.cell("E1").value, null);
equal(fx.cell("E1").calc(), "#NAME?");
equal(fx.cell("E4").value, "!", "calculated from the value E1 has");
throws(() => { fx.cell("E5").formula = "=SUM(1,"; fx.cell("E5").calc(); }, /formula|数式/);
fx.cell("E5").formula = null;
equal(fx.cell("E5").formula, null);

// The stored results are in the file: they are there without calculating again.
wb.save("calc.xlsx");
const again = workbooks.open("calc.xlsx");
equal(again.sheet("見積").cell("D8").value, 5940);
equal(again.sheet("関数").cell("C1").value, 800);
equal(again.sheet("関数").cell("C1").formula, '=XLOOKUP("みかん",見積!A2:A4,見積!D2:D4)');
equal(again.sheet("関数").cell("C7").text, "2026/4/1(水)");
// Sheets added, removed or renamed: everything is calculated again from the cells.
again.sheet("見積").cell("B4").value = 1000;
again.addSheet("メモ").cell("A1").formula = "=見積!D8*2";
equal(again.sheet("メモ").cell("A1").calc(), "14080");
again.removeSheet("関数");
r = again.calculate();
equal(r.formulas, 7);
equal(again.sheet("メモ").cell("A1").value, 14080);

// A workbook Excel saved (testdata/xlsx/make_calc.py): results are kept until
// something they depend on changes; printing calculates what changed.
const saved = workbooks.open(path.join(xljs.scriptDir, "..", "xlsx", "calc.xlsx"));
const keep = saved.sheet("保存値");
equal(keep.cell("B1").value, 123, "a function of a macro: the result Excel saved");
keep.cell("A1").value = 7;
equal(keep.cell("D1").value, 999);
ok(keep.toHtml().includes(">70<"), "the pages show the result of =A1*10");
equal(keep.cell("D1").value, 70, "and it is stored");
equal(keep.cell("C1").value, 130, "=B1+A1, from the saved 123");
r = saved.calculate();
equal(r.skipped.filter((s) => s.sheet === "保存値").length, 4);
equal(keep.cell("B1").value, 123);
const arr = saved.sheet("配列");
deepEqual(arr.toArrays().map((row) => row[4] ?? null), [3, 4, 5, null, null], "FILTER fills three cells");
equal(saved.sheet("表").cell("F1").value, 5500, "the table's column by name");

// A formula that gives several values spills, as when it is typed into Excel.
const sb = workbooks.create();
const sp = sb.addSheet("スピル");
sp.write([
  ["品名", "数量"],
  ["りんご", 3],
  ["みかん", 0],
  ["ぶどう", 2],
]);
sp.cell("D1").formula = "=FILTER(A2:B4,B2:B4>0)";
sp.cell("G1").formula = "=B2:B4*10";
sp.cell("I1").formula = "=SUM(B2:B4*10)";
sp.cell("I2").formula = "=@B2:B4";
equal(sp.cell("D1").calc(), "りんご", "the first of its values");
equal(sp.cell("D2").value, null, "not laid out yet");
r = sb.calculate();
equal(r.formulas, 4);
deepEqual(sp.range("D1:E3").values, [["りんご", 3], ["ぶどう", 2], [null, null]]);
deepEqual(sp.range("G1:G4").values.flat(), [30, 0, 20, null]);
equal(sp.cell("I1").value, 50, "every value of the range, not the one of the row");
equal(sp.cell("I2").value, 3, "@: the value of the formula's row");
equal(sp.cell("I2").formula, "=@B2:B4");
equal(sp.cell("D2").formula, null, "a spilled value is a value");
// The result follows the data: it grows and shrinks.
sp.cell("B3").value = 5;
sb.calculate();
deepEqual(sp.range("D1:E4").values, [["りんご", 3], ["みかん", 5], ["ぶどう", 2], [null, null]]);
sp.cell("B2").value = 0;
sp.cell("B3").value = 0;
sb.calculate();
deepEqual(sp.range("D1:E3").values, [["ぶどう", 2], [null, null], [null, null]]);
// A value in the way: #SPILL! (cell.value has the #VALUE! files hold for it).
sp.cell("B2").value = 3;
sp.cell("D2").value = "x";
equal(sp.cell("D1").calc(), "#SPILL!");
sb.calculate();
deepEqual(sp.range("D1:D3").values.flat(), ["#VALUE!", "x", null]);
sp.cell("D2").value = null;
sb.calculate();
deepEqual(sp.range("D1:E3").values, [["りんご", 3], ["ぶどう", 2], [null, null]]);
// The file has the formulas as Excel saves them: opened again they spill the same.
sb.save("spill.xlsx");
const sb2 = workbooks.open("spill.xlsx");
const sp2 = sb2.sheet("スピル");
equal(sp2.cell("D1").formula, "=FILTER(A2:B4,B2:B4>0)");
equal(sp2.cell("I2").formula, "=@B2:B4");
deepEqual(sp2.range("D1:E3").values, [["りんご", 3], ["ぶどう", 2], [null, null]]);
equal(sb2.calculate().changed, 0, "the same results");
sp2.cell("B3").value = 1;
sb2.calculate();
deepEqual(sp2.range("D1:E3").values, [["りんご", 3], ["みかん", 1], ["ぶどう", 2]]);
// Writing over the formula takes the values it laid out with it.
sp2.cell("D1").value = "なし";
deepEqual(sp2.range("D1:E3").values, [["なし", null], [null, null], [null, null]]);
// The spills of a workbook Excel saved are still spills after a save here.
saved.save("calc-saved.xlsx");
const resaved = workbooks.open("calc-saved.xlsx");
const arr2 = resaved.sheet("配列");
equal(arr2.cell("E1").formula, "=FILTER(A1:A5,B1:B5>20)");
equal(arr2.cell("I3").formula, "=@A1:A5", "saved without the metadata: one value, as Excel shows it");
arr2.cell("B1").value = 21;
resaved.calculate();
deepEqual(arr2.range("E1:E5").values.flat(), [1, 3, 4, 5, null]);

// Numbers in kanji: NUMBERSTRING and the [DBNum] number formats.
const kj = sb.addSheet("漢数字");
kj.cell("A1").value = 12345;
kj.cell("A2").value = new Date(2026, 9, 7);
[
  ["=NUMBERSTRING(A1,1)", "一万二千三百四十五"],
  ["=NUMBERSTRING(A1,2)", "壱萬弐阡参百四拾伍"],
  ["=NUMBERSTRING(A1,3)", "一二三四五"],
  ['=TEXT(A1,"[DBNum1]G/標準")', "一万二千三百四十五"],
  ['=TEXT(A1,"[DBNum3]#,##0")&"円"', "１２,３４５円"],
  ['=TEXT(A2,"[DBNum1]ggge年m月d日")', "令和八年十月七日"],
].forEach(([f, want], i) => {
  kj.cell(i + 1, 3).formula = f;
  equal(kj.cell(i + 1, 3).calc(), want, f);
});
equal(sb.calculate().skipped.length, 0, "NUMBERSTRING is calculated");
kj.cell("A1").style({ numberFormat: "[DBNum2][$-411]G/標準" });
equal(kj.cell("A1").text, "壱萬弐阡参百四拾伍");
equal(kj.cell("A1").getStyle().numberFormat, "[DBNum2][$-411]General", "as the file holds it");
kj.cell("A2").style({ numberFormat: '[DBNum1]yyyy"年"m"月"d"日"' });
equal(kj.cell("A2").text, "二〇二六年十月七日");

console.log("calc ok");
