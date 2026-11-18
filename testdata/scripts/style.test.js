import { ok, equal, deepEqual, throws } from "./_assert.js";

// Tests run with the Japanese locale: new workbooks default to 游ゴシック.
const wb = excel.create();
deepEqual(wb.defaultFont, { name: "游ゴシック", size: 11 });
equal(excel.create({ font: "Meiryo UI" }).defaultFont.name, "Meiryo UI");

const ws = wb.addSheet("s");
ws.write([["a", 1], ["b", 2], ["c", 3]]);

// The flat form; styles merge. The nested form (font, fill, alignment, numberFormat) is accepted too.
ws.setStyle("A1", { bold: true, color: "#f00", background: "yellow", align: "center", format: "0.0" });
ws.cell("A1").setStyle({ italic: true, font: { size: 14 } });
const a1 = {
  fontName: "游ゴシック", fontSize: 14, bold: true, italic: true, color: "#FF0000",
  background: "#FFFF00", align: "center", format: "0.0",
};
deepEqual(ws.cell("A1").getStyle(), a1);
deepEqual(ws.getStyle("A1"), a1);
deepEqual(ws.range("A1:B2").getStyle(), a1, "a range: its top-left cell");
// getStyle() gives what setStyle() takes.
ws.cell("J1").setStyle(ws.cell("A1").getStyle());
deepEqual(ws.cell("J1").getStyle(), a1);
// replace: true discards the previous style; style() is the earlier name of setStyle().
ws.cell("A1").style({ underline: true }, { replace: true });
deepEqual(ws.cell("A1").getStyle(), { fontName: "游ゴシック", fontSize: 11, underline: "single" });
// Removing the background / format.
ws.setStyle("B1", { background: "#ABCDEF", format: "#,##0" });
ws.setStyle("B1", { background: null, format: "General" });
deepEqual(ws.cell("B1").getStyle(), { fontName: "游ゴシック", fontSize: 11 });

// Borders: outline + inside, and per side.
ws.range("A1:B3").setStyle({ border: { outline: "medium", inside: { style: "dashed", color: "#999" } } });
const b = (ref) => ws.cell(ref).getStyle().border;
deepEqual(b("A1"), {
  left: { style: "medium", color: "#000000" }, right: { style: "dashed", color: "#999999" },
  top: { style: "medium", color: "#000000" }, bottom: { style: "dashed", color: "#999999" },
});
equal(b("B3").right.style, "medium");
equal(b("B3").bottom.style, "medium");
equal(b("A2").left.style, "medium");
equal(b("A2").right.style, "dashed");
// A later border on a sub-range wins and the touching neighbor is updated.
ws.setStyle("A3:B3", { border: { top: "double" } });
equal(b("A3").top.style, "double");
equal(b("A2").bottom.style, "double");
ws.setStyle("A2", { border: "none" });
equal(ws.cell("A2").getStyle().border, undefined);
ws.setStyle("A2", { border: { style: "thick", color: "red" } });
equal(b("A2").top.color, "#FF0000");

// Alignment, rotation, wrap.
ws.setStyle("C1", { align: "right", valign: "middle", wrap: true, indent: 2, rotation: 45 });
const c1 = ws.cell("C1").getStyle();
deepEqual([c1.align, c1.valign, c1.wrap, c1.indent, c1.rotation], ["right", "center", true, 2, 45]);
// Shrink to fit.
ws.setStyle("C2", { shrink: true });
equal(ws.cell("C2").getStyle().shrink, true);

// Values keep their types after styling; dates keep a date format.
ws.cell("D1").value = new Date(2026, 0, 2);
ws.setStyle("D1", { bold: true });
ok(ws.cell("D1").value instanceof Date, "styled date stays a date");
equal(ws.cell("B2").value, 2);

// Merge, freeze, filter, sizes: each can be read back.
deepEqual(ws.merges, []);
ws.merge("E1:F2");
deepEqual(ws.merges, ["E1:F2"]);
equal(ws.frozen, null);
ws.freeze("A2");
deepEqual(ws.frozen, { rows: 1, columns: 0 });
ws.freeze({ rows: 1, columns: 1 });
deepEqual(ws.frozen, { rows: 1, columns: 1 });
ws.unfreeze();
equal(ws.frozen, null);
equal(ws.filter, null);
ws.autoFilter("A1:B3");
equal(ws.filter, "A1:B3");
ws.autoFilter(null);
equal(ws.filter, null);
ws.autoFilter("A1:B3");
equal(ws.getRowHeight(2), 18.75, "a row without a height of its own: the default for 游ゴシック 11");
ws.rows(1).height = 30;
equal(ws.getRowHeight(1), 30);
ws.columns("A:B").width = 12;
equal(ws.getColumnWidth("A"), 12);
equal(ws.lastColumn, 4, "the last column with a value (D)");
ws.cell("G1").value = "とても長い見出しのテキストです";
ws.autoFit("G");
ok(ws.getColumnWidth("G") > 25, "autoFit widens for wide text: " + ws.getColumnWidth("G"));

// Range values.
deepEqual(ws.range("A2:B3").values, [["b", 2], ["c", 3]]);
ws.range("H1").values = [[1, 2], [3, 4]];
equal(ws.cell("I2").value, 4);

// write() formatting options.
const t = wb.addSheet("table");
t.write([{ 名前: "a", 金額: 1200 }, { 名前: "b", 金額: 30 }], { table: true, columnStyles: { 金額: { format: "#,##0" } } });
equal(t.cell("A1").getStyle().bold, true);
equal(t.cell("A1").getStyle().background, "#DDEBF7");
equal(t.cell("B2").getStyle().format, "#,##0");
equal(t.cell("B3").getStyle().border.bottom.style, "thin");
equal(t.cell("B2").text, "1,200");

// Errors are reported in Japanese.
throws(() => ws.setStyle("A1", { color: "nope" }), /色の指定/);
throws(() => ws.setStyle("A1", { border: "wavy" }), /罫線の種類/);
throws(() => ws.setStyle("1A:B", { bold: true }), /セル範囲/);

// Hidden sheets: at least one stays shown.
equal(t.hidden, false);
t.hidden = true;
equal(t.hidden, true);
const only = excel.create().addSheet("x");
throws(() => { only.hidden = true; }, /非表示にできません/);

wb.save("styled.xlsx");
const back = excel.open("styled.xlsx");
equal(back.sheet("table").cell("A1").getStyle().background, "#DDEBF7");
equal(back.sheet("s").cell("C2").getStyle().shrink, true, "shrink to fit survives saving");
equal(back.sheet("table").hidden, true, "a hidden sheet stays hidden");
equal(back.sheet("s").filter, "A1:B3");
deepEqual(back.sheet("s").merges, ["E1:F2"]);
console.log("style ok");
