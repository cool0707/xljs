import { ok, equal, deepEqual, throws } from "./_assert.js";

// Merged cells: any cell of a merged area stands for its top-left cell.
const wb = excel.create();
const ws = wb.addSheet("s");
ws.write([
  ["部署", "氏名", "金額"],
  ["営業", "山田", 100],
  [null, "佐藤", 200],
  [null, "鈴木", 300],
  ["総務", "田中", 400],
]);
ws.merge("A2:A4");

// Reading a covered cell reads the top-left one.
equal(ws.cell("A3").value, "営業");
equal(ws.cell("A4").text, "営業");
equal(ws.cell(4, 1).value, "営業");
// The cell keeps the address it was asked by.
equal(ws.cell("A3").address, "A3");
equal(ws.cell("A3").row, 3);

// Tables: the merged value is in every row.
deepEqual(ws.toRowObjects().map((r) => r.部署), ["営業", "営業", "営業", "総務"]);
deepEqual(ws.toArrays()[3], ["営業", "鈴木", 300]);
deepEqual(ws.range("A2:B4").values, [["営業", "山田"], ["営業", "佐藤"], ["営業", "鈴木"]]);
deepEqual(ws.columns("A").values.map((r) => r[0]), ["部署", "営業", "営業", "営業", "総務"]);

// Writing a covered cell writes the top-left one.
ws.cell("A4").value = "営業部";
equal(ws.cell("A2").value, "営業部");
equal(ws.cell("A3").value, "営業部");
ws.cell("A3").formula = "=B2&\"課\"";
equal(ws.cell("A2").formula, '=B2&"課"');
equal(ws.cell("A4").formula, '=B2&"課"');
wb.calculate();
equal(ws.cell("A4").value, "山田課");
equal(ws.cell("A3").calc(), "山田課");
ws.cell("A3").value = "営業";
equal(ws.cell("A2").formula, null);

// The formula engine sees the cells as Excel does: a covered cell is empty.
ws.cell("E1").formula = '=A3&"|"&A2';
wb.calculate();
equal(ws.cell("E1").value, "|営業");
ws.cell("E1").value = null;

// Rows written over a merged area: what the area covers is not written.
ws.range("A2:A4").values = [["A"], ["B"], ["C"]];
equal(ws.cell("A2").value, "A");
equal(ws.cell("A4").value, "A");
ws.write([["営業", "山田", 100], ["x", "佐藤", 200]], { start: "A2" });
equal(ws.cell("A3").value, "営業");
deepEqual(ws.toArrays()[2], ["営業", "佐藤", 200]);

// Notes and comments are those of the top-left cell.
ws.cell("A3").note = "3 人";
equal(ws.cell("A2").note, "3 人");
equal(ws.cell("A4").note, "3 人");
deepEqual(ws.notes.map((n) => n.address), ["A2"]);
ws.cell("A4").note = null;
equal(ws.cell("A2").note, null);
const c = ws.cell("A4").addComment("確認");
equal(c.address, "A2");
equal(ws.cell("A3").comment.text, "確認");
c.delete();

// Formats: a cell of a merged area is formatted as the area, and read from its top-left cell.
ws.cell("A3").setStyle({ border: "thin", background: "#DDEBF7" });
for (const a of ["A2", "A3", "A4"]) {
  const st = ws.range(a).getStyle();
  equal(st.background, "#DDEBF7", a);
}
// The borders are those of the area: its last cell has the bottom line.
ws.merge("G1:H2");
ws.cell("G1").setStyle({ border: { outline: "medium" } });
const wide = excel.open(wb.toArrayBuffer()).sheet("s");
equal(wide.cell("H2").getStyle().border.left.style, "medium", "getStyle() of a covered cell is the top-left one's");
ws.unmerge("G1:H2");
equal(ws.cell("H2").getStyle().border.bottom.style, "medium");
equal(ws.cell("H2").getStyle().border.right.style, "medium");
equal(ws.cell("H2").getStyle().border.left, undefined);
equal(ws.cell("G1").getStyle().border.top.style, "medium");
equal(ws.cell("G1").getStyle().border.bottom, undefined);
// A range that is not within one merged area is formatted as it is.
ws.setStyle("A4:B4", { italic: true });
equal(ws.cell("B4").getStyle().italic, true);
equal(ws.cell("A2").getStyle().italic, undefined);

// Merging keeps one value: the first there is, in the top-left cell (Excel's "セルを結合").
ws.write([["x", "y"], ["z", null]], { start: "J1" });
ws.merge("J1:K2");
equal(ws.cell("J1").value, "x");
ws.unmerge("J1:K2");
deepEqual(ws.range("J1:K2").values, [["x", null], [null, null]]);
ws.cell("J1").value = null;
ws.cell("N2").value = 5;
ws.merge("M1:N2");
equal(ws.cell("M1").value, 5, "the only value moves to the top-left cell");
ws.unmerge("M1:N2");
equal(ws.cell("N2").value, null);
ws.cell("M1").value = null;

// The last row and column reach to the end of a merged area.
equal(ws.lastRow(), 5);
ws.cell("A7").value = "備考";
ws.merge("A7:C9");
equal(ws.lastRow(), 9);
equal(ws.columns("B").lastRow(), 9);
equal(ws.columns("D").lastRow(), 0);
equal(ws.rows(8).lastColumn(), 3);
equal(ws.dimension, "A1:C9");
equal(ws.toArrays().length, 9);
deepEqual(ws.toArrays()[8], ["備考", "備考", "備考"]);
// append() writes below it.
ws.append([["次", 1]]);
equal(ws.cell("A10").value, "次");
// Whole rows merged do not make every column a used one.
ws.cell("A12").value = "注";
ws.rows(12).merge();
equal(ws.lastColumn(), 3);
equal(ws.rows(12).lastColumn(), 1);
equal(ws.toArrays()[11].length, 3);
equal(ws.cell("XFD12").value, "注");

// A file whose covered cells hold values (other tools write them): they are not read.
const raw = excel.create();
const rs = raw.addSheet("s");
rs.write([["a", "b"], ["c", "d"]]);
rs.merge("A1:B2");
const again = excel.open(raw.toArrayBuffer()).sheet("s");
deepEqual(again.toArrays(), [["a", "a"], ["a", "a"]]);
deepEqual(again.merges, ["A1:B2"]);

// offset(): from a merged area, the count starts at its edge.
const os = wb.addSheet("o");
os.write([["氏名", null, "山田"], [null, null, null], ["住所", null, "東京"]]);
os.merge("A1:B2");
equal(os.cell("A1").offset(0, 1).address, "C1");
equal(os.cell("B2").offset(0, 1).address, "C1");
equal(os.cell("A1").offset(1, 0).address, "A3");
equal(os.cell("C1").offset(0, -1).address, "B1");
equal(os.cell("C1").offset(0, -1).value, "氏名");
throws(() => os.cell("C1").offset(0, -1).offset(0, -1), /シートの外/);
equal(os.cell("C3").offset(-1, 0).address, "C2");
equal(os.cell("C3").offset(0, 0).address, "C3");
equal(os.cell("A3").offset(-1, 1).address, "B2");
equal(os.cell("A3").offset(-1, 1).value, "氏名");
throws(() => os.cell("A1").offset(-1, 0), /シートの外/);
throws(() => os.cell("A1").offset(1.5, 0), /整数/);

// The merged cells survive saving.
const back = excel.open(wb.toArrayBuffer()).sheet("s");
equal(back.cell("A4").value, "営業");
ok(back.merges.includes("A2:A4"));
console.log("merge ok");
