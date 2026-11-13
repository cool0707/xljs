import { ok, equal, deepEqual, throws } from "./_assert.js";

// Answers come from table.test.answers.json (indices of the chosen rows).
const rows = [
  { 顧客名: "A 商事", 件数: 3, 合計: 1200, 日付: new Date(2026, 3, 1) },
  { 顧客名: "B 物産", 件数: 1, 合計: 300, 備考: "新規" },
  { 顧客名: "C 工業", 件数: 7, 合計: 9800 },
];

// Display only: returns at once with a closed promise.
const view = ui.table(rows, { title: "集計結果" });
ok(view.closed instanceof Promise);
await view.closed;
view.close();

// Columns: names, or { key, label }.
ui.table(rows, { title: "列指定", columns: ["顧客名", { key: "合計", label: "金額" }] });
ui.table([[1, "x"], [2, "y"]], "配列");
ui.table(["a", "b"], "値");
ui.table([], "空");

// Selection returns the original objects; cancel returns [].
const picked = await ui.table(rows, { selectable: "multiple", title: "選択" });
deepEqual(picked, [rows[0], rows[2]]);
const one = await ui.table(rows, { selectable: "single" });
equal(one.length, 1);
equal(one[0], rows[1]);
deepEqual(await ui.table(rows, { selectable: true }), [], "cancel");
throws(() => ui.table(rows, { selectable: "many" }), /single/);
throws(() => ui.table(rows, { onDoubleClick: "open" }), /関数/);
ui.table(rows, { title: "ダブルクリック", onDoubleClick() {} }); // console/mock: never called

// Conditional formats: checked here (shown only in windows).
ui.table(rows, { title: "書式", formats: [
  { column: "合計", gt: 1000, color: "red", bold: true },
  { column: ["件数", "合計"], dataBar: true },
  { column: "日付", between: [new Date(2026, 0, 1), new Date(2026, 11, 31)], background: "#ffc7ce", row: true },
  { when: (r) => r.備考 === "新規", italic: true },
  { column: "件数", colorScale: ["red", "yellow", "green"] },
] });
throws(() => ui.table(rows, { formats: [{ column: "合計", greater: 1 }] }), /greater/);
throws(() => ui.table(rows, { formats: [{ column: "無い列", bold: true }] }), /無い列/);
throws(() => ui.table(rows, { formats: [{ column: "合計", between: [1] }] }), /between/);
throws(() => ui.table(rows, { formats: [{ colorScale: ["red"] }] }), /colorScale/);
throws(() => ui.table(rows, { formats: [{ when: "x" }] }), /when/);
throws(() => ui.table(rows, { formats: { bold: true } }), /formats/);
console.log("table ok");
