import { ok, equal, deepEqual, throws } from "./_assert.js";

// Whole rows and columns: sheet.rows(5) / rows("2:4"), sheet.columns("C") / columns("A:C").
const wb = excel.create();
const s = wb.addSheet("売上");
s.write([
  ["品名", "数量", "単価"],
  ["りんご", 3, 100],
  ["みかん", 5, 80],
  ["ぶどう", 2, 300],
]);
s.cell("E2").value = "メモ";
s.cell("A7").value = "合計";

// The last row and column with a value: of the sheet, of columns, of rows.
equal(s.lastRow(), 7);
equal(s.lastColumn(), 5);
equal(s.columns("B").lastRow(), 4, "the last row of one column (End(xlUp))");
equal(s.columns("A").lastRow(), 7);
equal(s.columns("B:C").lastRow(), 4);
equal(s.columns(4).lastRow(), 0, "an empty column");
equal(s.rows(1).lastColumn(), 3, "the last column of one row (End(xlToLeft))");
equal(s.rows(2).lastColumn(), 5);
equal(s.rows("3:4").lastColumn(), 3);
equal(s.rows(6).lastColumn(), 0, "an empty row");
// The earlier names (not documented).
equal(s.rowCount, 7);
equal(s.columnCount, 5);

// What counts: a cell with a value or a formula. The options count the cells with only a format
// too, and leave out the cells whose value is empty or only spaces (a formula that gives "").
const u = wb.addSheet("最終行");
u.write([["品名", "数量", "金額"], ["りんご", 3], ["みかん", 5]]);
for (let r = 2; r <= 6; r++) u.cell(`C${r}`).formula = `=IF(B${r}="","",B${r}*100)`;
equal(u.columns("C").lastRow(), 6, "a formula counts, calculated or not");
equal(u.lastRow(), 6);
equal(u.dimension, "A1:C6");
wb.calculate();
equal(u.cell("C3").value, 500);
equal(u.cell("C5").value, null, 'a formula that gives ""');
equal(u.columns("C").lastRow(), 6, "and counts whatever it gives");
equal(u.columns("C").lastRow({ skipBlank: true }), 3, 'skipBlank leaves out the formulas that give ""');
equal(u.lastRow({ skipBlank: true }), 3);
equal(u.rows(5).lastColumn(), 3);
equal(u.rows(5).lastColumn({ skipBlank: true }), 0);
u.cell("A8").value = " ";
u.cell("A9").value = "\u3000";
equal(u.columns("A").lastRow(), 9, "spaces are a value");
equal(u.columns("A").lastRow({ skipBlank: true }), 3, "skipBlank leaves out the cells with only spaces");
u.cell("A9").value = null;
u.cell("A8").value = null;
equal(u.columns("A").lastRow(), 3);
u.setStyle("A12:E12", { border: { top: "thin" } });
equal(u.lastRow(), 6, "a cell with only a format does not count");
equal(u.lastRow({ formats: true }), 12, "formats counts the cells with only a format");
equal(u.lastColumn(), 3);
equal(u.lastColumn({ formats: true }), 5);
equal(u.columns("E").lastRow(), 0);
equal(u.columns("E").lastRow({ formats: true }), 12);
equal(u.rows(12).lastColumn({ formats: true }), 5);
equal(u.lastRow({ formats: true, skipBlank: true }), 12);
equal(u.columns("C").lastRow({ formats: true, skipBlank: true }), 12);
// The rows are written below the formulas, not over them.
u.append([["ぶどう", 2]]);
equal(u.cell("A7").value, "ぶどう");
equal(u.cell("C6").formula, '=IF(B6="","",B6*100)');

// The addresses.
equal(s.rows(5).address, "5:5");
equal(s.rows("4:2").address, "2:4");
equal(s.rows("$2:$4").address, "2:4");
equal(s.columns("c").address, "C:C");
equal(s.columns(3).address, "C:C");
equal(s.columns("C:A").address, "A:C");
ok(s.rows(1).sheet === s);
equal(xljs.inspect(s.rows("2:4")), "RowRange 売上!2:4");
equal(xljs.inspect(s.columns("A:C")), "ColumnRange 売上!A:C");
throws(() => s.rows(0), /rows/);
throws(() => s.rows("A"), /rows/);
throws(() => s.rows("1:2:3"), /rows/);
throws(() => s.rows(1048577), /rows/);
throws(() => s.columns("1A"), /columns/);
throws(() => s.columns(16385), /columns/);

// The values: as far as the rows (columns) have values.
deepEqual(s.rows(1).values, [["品名", "数量", "単価"]]);
deepEqual(s.rows("3:4").values, [["みかん", 5, 80], ["ぶどう", 2, 300]]);
deepEqual(s.rows(6).values, [[]]);
deepEqual(s.columns("B").values, [["数量"], [3], [5], [2]]);
deepEqual(s.columns("B:C").values, [["数量", "単価"], [3, 100], [5, 80], [2, 300]]);
deepEqual(s.columns("D").values, []);
s.rows(9).values = [["a", "b"]];
equal(s.cell("B9").value, "b");
s.columns("G").values = [[1], [2]];
equal(s.cell("G2").value, 2);
s.rows(9).delete();
s.columns("G").delete();

// Without a row, rows() is the earlier name of toRowObjects() (not documented).
deepEqual(s.toRowObjects()[0], { 品名: "りんご", 数量: 3, 単価: 100 });
deepEqual(s.rows(), s.toRowObjects());
deepEqual(s.rows({ header: 0, columns: ["a", "b"] })[0], { a: "品名", b: "数量" });
deepEqual(s.toObjects(), s.toRowObjects());

// Height and width.
const defaultHeight = s.rows(2).height;
s.rows(2).height = 30;
equal(s.rows(2).height, 30);
equal(s.getRowHeight(2), 30);
s.rows("3:4").height = 24;
equal(s.rows(3).height, 24);
equal(s.rows(4).height, 24);
equal(s.rows("3:4").height, 24, "the height of a range is that of its first row");
equal(s.rows(5).height, defaultHeight);
equal(s.rows(3).autoFit().height, defaultHeight, "autoFit gives the height back to Excel");
equal(s.rows(4).height, 24);
throws(() => { s.rows(2).height = 500; }, /409/);

const defaultWidth = s.columns("D").width; // a column without a width of its own
s.columns("A").width = 20;
equal(s.columns("A").width, 20);
equal(s.getColumnWidth("A"), 20);
s.columns("B:C").width = 12;
equal(s.columns(2).width, 12);
equal(s.columns("C").width, 12);
equal(s.columns("D").width, defaultWidth);
s.columns("A:C").autoFit({ min: 8 });
ok(s.columns("A").width >= 8 && s.columns("A").width < 20, "autoFit sets the width from the contents");
equal(s.columns("D").width, defaultWidth);

// Hiding.
equal(s.rows(3).hidden, false);
s.rows("3:4").hidden = true;
equal(s.rows(3).hidden, true);
equal(s.rows(4).hidden, true);
equal(s.rows(2).hidden, false);
s.rows(4).hidden = false;
equal(s.rows(4).hidden, false);
const widthOfC = s.columns("C").width;
s.columns("C").hidden = true;
equal(s.columns("C").hidden, true);
equal(s.columns("B").hidden, false);
equal(s.columns("C").width, widthOfC, "a hidden column keeps its width");
s.columns("H").hidden = true;
equal(s.columns("H").width, defaultWidth, "and so does a column without a width of its own");
s.columns("H").hidden = false;
// SUBTOTAL(109, ...) leaves the hidden rows out.
s.cell("B6").formula = "=SUBTOTAL(109,B2:B4)";
wb.calculate();
equal(s.cell("B6").value, 5, "みかん (row 3) is hidden");
s.rows(3).hidden = false;
wb.calculate();
equal(s.cell("B6").value, 10);
s.cell("B6").formula = null;

// Formats: the rows (columns) themselves, the cells there are, the cells written later.
s.rows(1).setStyle({ bold: true, background: "#DDEBF7" });
equal(s.cell("A1").getStyle().bold, true);
equal(s.rows(1).getStyle().background, "#DDEBF7");
equal(s.cell("Z1").getStyle().bold, true, "an empty cell shows the format of its row");
equal(s.cell("A2").getStyle().bold ?? false, false);
s.columns("C").setStyle({ format: "#,##0" });
equal(s.cell("C3").getStyle().format, "#,##0");
equal(s.cell("C100").getStyle().format, "#,##0", "an empty cell shows the format of its column");
equal(s.columns("C").getStyle().bold, true, "where the row and the column cross, the cell has both");
equal(s.cell("C1").getStyle().format, "#,##0");
equal(s.cell("B3").getStyle().format ?? null, null);
// A row formatted after a column: the crossing cell gets a format of its own.
s.rows(8).setStyle({ italic: true });
deepEqual([s.cell("C8").getStyle().italic, s.cell("C8").getStyle().format], [true, "#,##0"]);
equal(s.cell("B8").getStyle().italic, true);
// A cell written later keeps the format of its row.
s.cell("D8").value = 1;
equal(s.cell("D8").getStyle().italic, true);
// Borders: the outline of whole rows is the line above and below.
s.rows(7).setStyle({ border: { top: "thin" } });
equal(s.cell("A7").getStyle().border.top.style, "thin");
equal(s.cell("A6").getStyle().border ?? null, null, "a cell without a format next to the row stays as it is");
s.cell("B6").setStyle({ bold: true });
s.rows(7).setStyle({ border: { top: "medium" } });
equal(s.cell("B6").getStyle().border.bottom.style, "medium", "a formatted cell above takes the line it touches");
s.cell("B6").setStyle({}, { replace: true });
s.rows(8).setStyle({ italic: false }, { replace: true });
equal(s.cell("B8").getStyle().italic ?? false, false);

// Inserting and deleting: formulas follow, as with insertRows.
s.cell("D5").formula = "=SUM(B2:B4)";
s.rows(3).insert();
equal(s.cell("A4").value, "みかん");
equal(s.cell("D6").formula, "=SUM(B2:B5)");
s.rows("3:4").insert();
equal(s.cell("A6").value, "みかん", "a range inserts as many rows as it has");
s.rows(3).insert(0);
equal(s.cell("A6").value, "みかん");
s.rows("3:5").delete();
equal(s.cell("A3").value, "みかん");
equal(s.cell("D5").formula, "=SUM(B2:B4)");
const third = s.rows(3);
ok(third.insert(2) === third, "insert returns the range");
s.rows("3:4").delete();
s.columns("B").insert(2);
equal(s.cell("D1").value, "数量");
equal(s.cell("F5").formula, "=SUM(D2:D4)");
s.columns("B:C").delete();
equal(s.cell("B1").value, "数量");
equal(s.cell("D5").formula, "=SUM(B2:B4)");
// The earlier forms (not documented).
s.insertRows(2);
s.deleteRows(2);
s.insertColumns("B");
s.deleteColumns(2);
equal(s.cell("B1").value, "数量");

// Merging whole rows.
s.rows(10).merge();
deepEqual(s.merges, ["A10:XFD10"]);
s.rows(10).unmerge();
deepEqual(s.merges, []);

// The used range starts at the first cell with a value.
const t = wb.addSheet("表");
equal(t.dimension, "");
equal(t.lastRow(), 0);
equal(t.lastColumn(), 0);
t.cell("C5").value = 1;
equal(t.dimension, "C5:C5");
t.cell("E9").value = 2;
t.cell("B7").value = 3;
equal(t.dimension, "B5:E9");
equal(t.lastRow(), 9);
equal(t.lastColumn(), 5);

// Saved and opened again.
s.rows(2).height = 30;
s.rows(4).hidden = true;
s.columns("C").hidden = true;
s.columns("A").width = 18;
wb.save("lines.xlsx");
const back = excel.open("lines.xlsx").sheet("売上");
equal(back.rows(2).height, 30);
equal(back.rows(4).hidden, true);
equal(back.rows(3).hidden, false);
equal(back.columns("C").hidden, true);
equal(back.columns("A").width, 18);
equal(back.cell("Z1").getStyle().bold, true, "the format of the row is saved");
equal(back.cell("C100").getStyle().format, "#,##0", "the format of the column is saved");
equal(back.columns("B").lastRow(), 4);
console.log("lines ok");
