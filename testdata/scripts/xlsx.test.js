import { ok, equal, deepEqual, throws } from "./_assert.js";

const wb = excel.create();
equal(wb.path, null);
deepEqual(wb.sheetNames, ["Sheet1"]);
const ws = wb.addSheet("売上"); // replaces the untouched default sheet
deepEqual(wb.sheetNames, ["売上"]);
ws.write([
  { 日付: new Date(2026, 3, 1), 顧客: "A社", 金額: 12000, 確定: true },
  { 日付: new Date(2026, 3, 2, 13, 30), 顧客: "B社", 金額: 800.5, 確定: false },
  { 日付: null, 顧客: "C社", 金額: -5 },
]);
equal(ws.dimension, "A1:D4");
equal(ws.lastRow, 4);

// Cell access by address and (row, column).
equal(ws.cell("B2").value, "A社");
equal(ws.cell(3, 3).value, 800.5);
equal(ws.cell("D2").value, true);
equal(ws.cell(4, 1).value, null);
const d = ws.cell("A3").value;
ok(d instanceof Date, "date cells come back as Date");
equal(d.getHours(), 13);
equal(d.getMinutes(), 30);
equal(ws.cell("A2").text, "2026/4/1");

// Formulas: stored with recalculation on load; calc() evaluates.
ws.cell("E1").value = "合計";
ws.cell("E2").formula = "=SUM(C2:C4)";
equal(ws.cell("E2").formula, "=SUM(C2:C4)");
equal(ws.cell("E2").calc(), "12795.5");
equal(ws.cell("A1").formula, null);

// Strings starting with "=" are text, not formulas.
ws.cell("F1").value = "=not formula";
equal(ws.cell("F1").formula, null);
equal(ws.cell("F1").value, "=not formula");
ws.cell("F1").value = null;
equal(ws.cell("F1").value, null);

// More sheets, append, arrays.
const s2 = wb.addSheet("集計");
s2.write([["a", "b"], [1, 2]], { start: "B2" });
deepEqual(s2.toArrays(), [[], [null, "a", "b"], [null, 1, 2]]);
deepEqual(s2.toArrays({ skipEmpty: true }), [[null, "a", "b"], [null, 1, 2]]);
s2.append([[3, 4]]);
equal(s2.cell("A4").value, 3);
const s3 = wb.addSheet("tmp");
s3.append([{ x: 1 }]);
s3.append([{ x: 2 }]);
deepEqual(s3.toRowObjects(), [{ x: 1 }, { x: 2 }]);
// Renaming a sheet renames the references to it, as in Excel.
ws.cell("G1").formula = "=SUM(tmp!A2:A3)+LEN(\"tmp!A1\")";
s3.name = "tmp 2";
equal(ws.cell("G1").formula, "=SUM('tmp 2'!A2:A3)+LEN(\"tmp!A1\")");
equal(ws.cell("G1").calc(), "9");
s3.name = "renamed";
equal(ws.cell("G1").formula, "=SUM(renamed!A2:A3)+LEN(\"tmp!A1\")");
ws.cell("G1").value = null;
deepEqual(wb.sheetNames, ["売上", "集計", "renamed"]);
wb.removeSheet("renamed");
throws(() => wb.addSheet("集計"), /既に存在/);
s2.columns("A:C").width = 20;

// sheets array with name lookup.
equal(wb.sheets.length, 2);
equal(wb.sheets["集計"].name, "集計");
equal(wb.sheets[0].name, "売上");
equal(wb.sheets["なし"], undefined);
equal(wb.sheet("なし"), null);
equal(wb.sheet(1).name, "集計");

wb.save("out.xlsx");
ok(wb.path.endsWith("out.xlsx"), "save() records the path");
wb.close();
throws(() => wb.sheetNames, /閉じ/);

// Re-open from a handle and read objects with typed values.
const b = await excel.open(fs.file("out.xlsx"));
const rows = b.sheets["売上"].toRowObjects();
equal(rows.length, 3);
deepEqual(rows[0], { 日付: new Date(2026, 3, 1), 顧客: "A社", 金額: 12000, 確定: true, 合計: null });
equal(rows[1].金額, 800.5);
equal(rows[2].日付, null);
equal(rows[2].金額, -5);
deepEqual(b.sheet(0).toRowObjects({ text: true })[0].日付, "2026/4/1");
deepEqual(b.sheet(0).toRowObjects({ header: 0, columns: ["d", "c"] })[1], { d: new Date(2026, 3, 1), c: "A社" });

// toBlob / open from Blob and File.
const blob = b.toBlob();
ok(blob.size > 0 && blob.type.includes("spreadsheetml"));
const b2 = excel.open(blob);
deepEqual(b2.sheetNames, ["売上", "集計"]);
const file = await fs.file("out.xlsx").getFile();
equal(excel.open(file).sheet(0).cell("B2").value, "A社");

// Save through a save-picker style handle.
const dest = fs.file("copy.xlsx");
b2.sheet(0).cell("B2").value = "Z社";
b2.save(dest);
equal(excel.open("copy.xlsx").sheet(0).cell("B2").value, "Z社");
throws(() => excel.create().save(), /保存先/);

equal(excel.columnName(28), "AB");
equal(excel.cellName(3, 2), "B3");

// Sheet names match ignoring case, surrounding spaces and full/half width
// when only one sheet fits; exact names win.
const names = excel.create();
names.addSheet("明細 ");
names.addSheet("Data");
names.addSheet("DATA2");
equal(names.sheets["明細"].name, "明細 ");
equal(names.sheets["data"].name, "Data");
equal(names.sheets["ＤＡＴＡ２"].name, "DATA2");
equal(names.sheet("data2").name, "DATA2");
equal(names.sheets["none"], undefined);
equal(names.sheet("none"), null);
equal(names.sheets.length, 3);
ok(Array.isArray(names.sheets));
deepEqual(names.sheets.map((s) => s.name), ["明細 ", "Data", "DATA2"]);
const twins = excel.create();
twins.addSheet("a");
twins.addSheet("ａ");
equal(twins.sheets["ａ"].name, "ａ");
equal(twins.sheet(" A "), null, "ambiguous");

// The earlier names still work (not documented): workbooks, toObjects(), rows(), style().
equal(workbooks, excel);
const alias = excel.create().addSheet("a");
alias.write([{ x: 1 }]);
deepEqual(alias.toObjects(), alias.toRowObjects());
deepEqual(alias.rows(), alias.toRowObjects());
alias.style("A1", { bold: true });
equal(alias.getStyle("A1").bold, true);
console.log("xlsx ok");
