import { ok, equal, deepEqual, throws } from "./_assert.js";

// Inserting and deleting rows and columns: formulas, names and data
// validations follow the cells, as in Excel.
const wb = excel.create();
const data = wb.addSheet("データ");
data.write([
  ["品名", "数量", "単価"],
  ["りんご", 3, 100],
  ["みかん", 5, 80],
  ["ぶどう", 2, 300],
]);
data.cell("D1").value = "金額";
for (let r = 2; r <= 4; r++) data.cell(`D${r}`).formula = `=B${r}*C${r}`;
data.cell("D5").formula = "=SUM(D2:D4)";
data.setStyle("A1:D1", { bold: true, background: "#DDEBF7" });
data.setStyle("A4:D4", { border: "thin" });
data.setRowHeight(4, 30);
data.merge("A6:B6");
const sum = wb.addSheet("集計");
sum.cell("A1").formula = "=SUM(データ!$D$2:$D$4)";
sum.cell("A2").formula = "=データ!B3";
sum.cell("A3").formula = "=COUNTA(データ!A:A)";

data.insertRows(3, 2);
equal(data.cell("A5").value, "みかん", "the cells move down");
equal(data.cell("A3").value, null);
equal(data.cell("D5").formula, "=B5*C5", "formulas in the cells moved follow them");
equal(data.cell("D7").formula, "=SUM(D2:D6)", "a range the rows go into widens");
equal(sum.cell("A1").formula, "=SUM(データ!$D$2:$D$6)", "absolute references move too");
equal(sum.cell("A2").formula, "=データ!B5");
equal(sum.cell("A3").formula, "=COUNTA(データ!A:A)", "whole columns stay");
// The rows inserted take the format of the row above.
equal(data.cell("A3").getStyle().bold ?? false, false);
const above = data.cell("A6").getStyle(); // the old row 4 (ぶどう) with borders
ok(above.border, "the row moved keeps its borders");

data.insertRows(7, 1); // below ぶどう: takes its borders
ok(data.cell("A7").getStyle().border, "an inserted row takes the format of the row above");

data.deleteRows(3, 2);
equal(data.cell("A3").value, "みかん");
equal(data.cell("D3").formula, "=B3*C3");
equal(sum.cell("A1").formula, "=SUM(データ!$D$2:$D$4)");
equal(sum.cell("A2").formula, "=データ!B3");
data.deleteRows(5); // the empty row inserted below ぶどう
equal(sum.cell("A1").formula, "=SUM(データ!$D$2:$D$4)");
equal(data.cell("D5").formula, "=SUM(D2:D4)");

// References to deleted cells become #REF!.
data.deleteRows(3);
equal(sum.cell("A2").formula, "=#REF!");
equal(data.cell("D4").formula, "=SUM(D2:D3)");

// Columns, by name or number.
data.insertColumns("B");
equal(data.cell("C1").value, "数量");
equal(data.cell("E2").formula, "=C2*D2");
data.deleteColumns(2, 1);
equal(data.cell("D2").formula, "=B2*C2");
data.deleteColumns("C");
equal(data.cell("C2").formula, "=B2*#REF!");

throws(() => data.insertRows(0), /insertRows/);
throws(() => data.deleteColumns("1A"), /deleteColumns/);
throws(() => data.insertRows(2, -1), /insertRows/);
data.insertRows(2, 0); // nothing

// Copying a sheet: right after it, "名前 (2)", with its formulas.
const copy = wb.copySheet("データ");
equal(copy.name, "データ (2)");
deepEqual(wb.sheetNames, ["データ", "データ (2)", "集計"]);
equal(copy.cell("D3").formula, data.cell("D3").formula);
equal(copy.cell("A2").value, "りんご");
copy.cell("A2").value = "なし";
equal(data.cell("A2").value, "りんご", "the copy is a sheet of its own");
const copy2 = wb.copySheet(data, "コピー");
equal(copy2.name, "コピー");
deepEqual(wb.sheetNames, ["データ", "コピー", "データ (2)", "集計"]);
throws(() => wb.copySheet(data, "集計"), /集計/);
throws(() => wb.copySheet("ない"), /ない/);

// Tables of a copy get names of their own; formulas using them follow.
const tb = excel.open(path.join(xljs.scriptDir, "..", "xlsx", "tables.xlsx"));
const t = tb.sheet("テーブル");
t.cell("E1").formula = "=COUNTA(参加報告[研修名])";
const tc = tb.copySheet(t);
equal(tc.name, "テーブル (2)");
equal(tc.cell("E1").formula, "=COUNTA(参加報告_2[研修名])");
equal(t.cell("E1").formula, "=COUNTA(参加報告[研修名])");
// Rows inserted inside a table widen it; its formulas follow.
t.insertRows(3);
equal(t.cell("E1").formula, "=COUNTA(参加報告[研修名])");
throws(() => t.deleteRows(1), /参加報告/);
t.insertColumns("B");
equal(t.cell("B1").value, "列1", "a column inserted into a table gets a name");
tb.save("tables-edited.xlsx");
const tb2 = excel.open("tables-edited.xlsx");
deepEqual(tb2.sheetNames.slice(0, 2), ["テーブル", "テーブル (2)"]);
equal(tb2.sheet("テーブル").cell("B1").value, "列1");
equal(tb2.sheet("テーブル (2)").cell("E1").formula, "=COUNTA(参加報告_2[研修名])");
tb2.close();
tb.close();

// The workbook saved and opened again has it all.
wb.save("sheetedit.xlsx");
const again = excel.open("sheetedit.xlsx");
deepEqual(again.sheetNames, ["データ", "コピー", "データ (2)", "集計"]);
equal(again.sheet("集計").cell("A1").formula, "=SUM(データ!$C$2:$C$3)");
again.close();
console.log("sheetedit ok");
