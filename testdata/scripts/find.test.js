import { ok, equal, deepEqual, throws } from "./_assert.js";

// sheet.find() / findAll(), range.find(), book.sheet(RegExp).
const wb = excel.create();
const ws = wb.addSheet("帳票");
ws.write([
  ["請求書", null, null, null],
  ["氏　名", null, "山田 太郎", null],
  ["氏名カナ", null, "ﾔﾏﾀﾞ ﾀﾛｳ", null],
  ["コード", "品名", "数量", "金額"],
  ["A-001", "りんご", 3, 1200],
  ["Ａ－００２", "みかん", 10, 800],
  ["a-003", "ぶどう", 1, 1500.5],
  ["合計", null, 14, 3500.5],
]);
ws.merge("A2:B2");
ws.merge("A3:B3");
ws.cell("D5").setStyle({ format: "#,##0" });
ws.cell("E5").value = new Date(2026, 3, 1);

const addr = (cells) => cells.map((c) => c.address);

// A string: the whole cell, ignoring full/half width, case and spaces.
equal(ws.find("氏名").address, "A2");
equal(ws.find("氏名").value, "氏　名");
equal(ws.find("a-002").address, "A6");
equal(ws.find("A-003").address, "A7");
equal(ws.find("ヤマダ タロウ").address, "C3");
equal(ws.find("ない"), null);
deepEqual(ws.findAll("ない"), []);
// strict: as written.
equal(ws.find("氏名", { strict: true }), null);
equal(ws.find("氏　名", { strict: true }).address, "A2");
equal(ws.find("a-003", { strict: true }).address, "A7");
equal(ws.find("A-003", { strict: true }), null);
// partial.
deepEqual(addr(ws.findAll("氏名", { match: "partial" })), ["A2", "A3"]);
deepEqual(addr(ws.findAll("a-00", { match: "partial" })), ["A5", "A6", "A7"]);
deepEqual(addr(ws.findAll("A-00", { match: "partial", strict: true })), ["A5"]);

// A merged area is found once, as its top-left cell.
deepEqual(addr(ws.findAll(/^氏/)), ["A2", "A3"]);

// RegExp: against the text (NFKC unless strict); the flags g and y do not matter.
const re = /^a-\d+$/gi;
deepEqual(addr(ws.findAll(re)), ["A5", "A6", "A7"]);
deepEqual(addr(ws.findAll(re)), ["A5", "A6", "A7"]);
deepEqual(addr(ws.findAll(/^a-\d+$/i, { strict: true })), ["A5", "A7"]);
equal(ws.find(/^合/).offset(0, 3).value, 3500.5);

// Numbers, dates, booleans: by value; a string or RegExp sees them as displayed (a number as it is too).
equal(ws.find(1200).address, "D5");
equal(ws.find(1500.5).address, "D7");
equal(ws.find("1,200").address, "D5");
equal(ws.find("1200").address, "D5");
equal(ws.find(10).address, "C6");
equal(ws.find("10").address, "C6");
equal(ws.find(new Date(2026, 3, 1)).address, "E5");
equal(ws.find(dayjs("2026-04-01")).address, "E5");
equal(ws.find("2026/4/1").address, "E5");
equal(ws.find(/^2026\//).address, "E5");

// A function: called with each cell that has a value.
deepEqual(addr(ws.findAll((c) => typeof c.value === "number" && c.value >= 1000)), ["D5", "D7", "D8"]);
equal(ws.find((c) => c.row > 4 && c.column === 2).address, "B5");

// Ranges, rows and columns.
equal(ws.range("A5:D8").find("金額"), null);
equal(ws.range("B5:D8").find(/^\d+$/).address, "C5");
equal(ws.columns("A").find("a-002").row, 6);
equal(ws.columns("C:D").find(14).address, "C8");
equal(ws.rows(4).find("数量").column, 3);
equal(ws.rows("5:7").find("合計"), null);
deepEqual(addr(ws.columns("C").findAll((c) => typeof c.value === "number")), ["C5", "C6", "C7", "C8"]);
// The cell where a row heading and a column heading cross.
equal(ws.cell(ws.find("みかん").row, ws.find("金額").column).value, 800);

// order: by rows (default) or by columns.
equal(ws.find(/^\d/).address, "C5");
ws.cell("B8").value = 99;
equal(ws.find((c) => typeof c.value === "number" && c.row === 8 || c.address === "C5").address, "C5");
equal(ws.find((c) => typeof c.value === "number", { order: "columns" }).address, "B8");
deepEqual(addr(ws.range("B5:C6").findAll(() => true, { order: "columns" })), ["B5", "B6", "C5", "C6"]);
ws.cell("B8").value = null;

// visibleOnly: not in hidden rows and columns.
ws.rows(6).hidden = true;
ws.columns("D").hidden = true;
equal(ws.find("みかん").address, "B6");
equal(ws.find("みかん", { visibleOnly: true }), null);
equal(ws.find(1200, { visibleOnly: true }), null);
deepEqual(addr(ws.findAll(/^a-/i, { visibleOnly: true })), ["A5", "A7"]);
ws.rows(6).hidden = false;
ws.columns("D").hidden = false;

// Formulas are found by their saved result.
ws.cell("F1").formula = "=1+2";
equal(ws.find(3, { order: "columns" }).address, "C5");
wb.calculate();
equal(ws.columns("F").find(3).address, "F1");

// Bad arguments.
throws(() => ws.find({}), /find\(\): 探すものは/);
throws(() => ws.findAll(null), /findAll\(\): 探すものは/);
throws(() => ws.find("x", { match: "start" }), /match は/);
throws(() => ws.find("x", { order: "down" }), /order は/);

// An empty sheet.
const empty = wb.addSheet("空");
equal(empty.find("x"), null);
deepEqual(empty.findAll(() => true), []);
equal(empty.columns("A").find("x"), null);

// book.sheet(RegExp): the first sheet whose name matches.
wb.addSheet("2026年4月");
wb.addSheet("２０２６年５月");
equal(wb.sheet(/^\d+年\d+月$/).name, "2026年4月");
equal(wb.sheet(/5月$/).name, "２０２６年５月");
equal(wb.sheet(/^帳/g).name, "帳票");
equal(wb.sheet(/ない/), null);
equal(wb.sheet("帳票").name, "帳票");
equal(wb.sheet(0).name, "帳票");
console.log("find ok");
