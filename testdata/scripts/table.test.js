import { ok, equal, deepEqual, throws } from "./_assert.js";

// Answers come from table.test.answers.json.
const rows = [
  { 顧客: "A", 件数: 3, 日付: new Date(2026, 3, 1) },
  { 顧客: "B", 件数: 5 },
  { 顧客: "C", 備考: "新規" },
];

// Showing only: returns at once with something that has `closed`.
const w = ui.table(rows, { title: "集計結果" });
ok(w.closed instanceof Promise);
await w.closed;

deepEqual(await ui.table(rows, { title: "選択", selectable: "multiple" }), [rows[0], rows[2]]);
deepEqual(await ui.table(rows, { selectable: "single" }), [], "cancel");
deepEqual(await ui.table(new Set(["x", "y"]), { selectable: true }), ["y"]);

ui.table([[1, 2], [3]], "配列");
ui.table(["a", 1, null], { columns: ["値"] });
ui.table([]);
throws(() => ui.table(rows, { selectable: "x" }), /selectable/);
console.log("table ok");
