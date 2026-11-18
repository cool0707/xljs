import { ok, equal, deepEqual, throws } from "./_assert.js";

// toRowObjects({ start, untilBlank, pick }) and write() / append() with matchHeader.
const wb = excel.create();
const ws = wb.addSheet("売上");
ws.write([
  ["売上一覧", null, null, null, null, null, null],
  [null, null, null, null, null, null, null],
  [null, "コード", "品　名", "金額", null, "備考", null, "担当"],
  [null, "A-1", "りんご", 100, 10, "x", null, "山田"],
  [null, "A-2", "みかん", 200, 20, null, null, "佐藤"],
  [null, null, null, null, null, null, null, "鈴木"],
  [null, "合計", null, 300, 30, null, null, null],
], { start: "A1" });
ws.merge("D3:E3"); // 金額 over two columns (税抜・税)

// start: the table from its top-left header; the columns are the headers up to the first empty one.
deepEqual(ws.toRowObjects({ start: "B3" }), [
  { コード: "A-1", 品名: undefined, "品　名": "りんご", 金額: 100, 金額_2: 10, 備考: "x" },
  { コード: "A-2", "品　名": "みかん", 金額: 200, 金額_2: 20, 備考: null },
  { コード: "合計", "品　名": null, 金額: 300, 金額_2: 30, 備考: null },
].map((r) => { delete r.品名; return r; }));
// A Cell works as start (the result of find()).
equal(ws.toRowObjects({ start: ws.find("コード") }).length, 3);
// The row with only 担当 (outside the table's columns) is an empty row of the table.
deepEqual(ws.toRowObjects({ start: "B3", untilBlank: true }).map((r) => r.コード), ["A-1", "A-2"]);
deepEqual(ws.toRowObjects({ start: "B3", skipEmpty: false }).map((r) => r.コード), ["A-1", "A-2", null, "合計"]);
// Without a header: start is the first row, columns name them.
deepEqual(ws.toRowObjects({ start: "B4", header: 0, columns: ["code", "name"], untilBlank: true }), [
  { code: "A-1", name: "りんご" },
  { code: "A-2", name: "みかん" },
]);
deepEqual(Object.keys(ws.toRowObjects({ start: "B4", header: 0 })[0]), ["列1", "列2", "列3", "列4", "列5", "列6", "列7"]);
// header (a row number) still reads from column A.
deepEqual(Object.keys(ws.toRowObjects({ header: 3 })[0]), ["列1", "コード", "品　名", "金額", "金額_2", "備考", "列7", "担当"]);
equal(ws.toRowObjects({ header: 3 }).length, 4);

// pick: columns by header, with keys of the script's own.
const pick = { code: "コード", name: "品名", amount: "金額", tax: { header: "金額", nth: 2 }, note: /備考|メモ/ };
deepEqual(ws.toRowObjects({ start: "B3", untilBlank: true, pick }), [
  { code: "A-1", name: "りんご", amount: 100, tax: 10, note: "x" },
  { code: "A-2", name: "みかん", amount: 200, tax: 20, note: null },
]);
// offset from a header, a position, an optional header; a header beyond the first empty one is not the table's.
deepEqual(ws.toRowObjects({ start: "B3", untilBlank: true, pick: { tax: { header: "金額", offset: 1 }, first: 0, who: { header: "担当", optional: true }, far: { header: "備考", offset: 2 } } }), [
  { tax: 10, first: "A-1", who: null, far: "山田" },
  { tax: 20, first: "A-2", who: null, far: "佐藤" },
]);
// Without start, every header of the row counts.
deepEqual(ws.toRowObjects({ header: 3, pick: { who: "担当", code: /^コ/ } }).map((r) => r.who), ["山田", "佐藤", "鈴木", null]);
// text: the displayed strings.
ws.columns("D").setStyle({ format: "#,##0円" });
equal(ws.toRowObjects({ start: "B3", pick: { amount: "金額" }, text: true })[0].amount, "100円");
// A header that is not there.
const e = throws(() => ws.toRowObjects({ start: "B3", pick: { qty: "数量" } }), /pick\.qty: 見出し "数量" が見つかりません/);
ok(e.message.includes("コード, 品　名, 金額, 金額, 備考"), e.message);
throws(() => ws.toRowObjects({ start: "B3", pick: { tax: { header: "金額", nth: 3 } } }), /見出し "金額" \(3\)/);
throws(() => ws.toRowObjects({ start: "B3", pick: { x: /数量/ } }), /見出し \/数量\//);
throws(() => ws.toRowObjects({ pick: { x: null } }), /pick\.x: 見出し（文字列か RegExp）/);
throws(() => ws.toRowObjects({ pick: { x: -1 } }), /pick\.x:/);
throws(() => ws.toRowObjects({ pick: { x: { header: "金額", nth: 0 } } }), /pick\.x:/);

// matchHeader: write under the headers there are, by key.
const dst = wb.addSheet("台帳");
dst.write([["台帳"], [], ["No", "品 名", "金額", "税込", "コード"]]);
dst.cell("D4").formula = "=C4*1.1";
dst.write([{ コード: "A-1", 品名: "りんご", 金額: 100, 余分: "x" }, { コード: "A-2", 品名: "みかん", 金額: 200 }], { matchHeader: 3 });
deepEqual(dst.range("A4:E5").values, [[null, "りんご", 100, null, "A-1"], [null, "みかん", 200, null, "A-2"]]);
equal(dst.cell("D4").formula, "=C4*1.1", "the other columns are left as they are");
equal(dst.cell("A3").value, "No", "no header is written");
// append: below the last row of the table's columns.
dst.cell("G9").value = "メモ";
dst.append([{ 金額: 300, "品 名": "ぶどう" }], { matchHeader: "A3" });
deepEqual(dst.range("A6:E6").values, [[null, "ぶどう", 300, null, null]]);
dst.append([{ ｺｰﾄﾞ: "A-4" }], { matchHeader: dst.cell("A3"), style: { bold: true } });
deepEqual(dst.range("A7:E7").values, [[null, null, null, null, "A-4"]]);
equal(dst.cell("E7").getStyle().bold, true);
// columns limits the keys written.
dst.append([{ コード: "A-5", 金額: 500 }], { matchHeader: 3, columns: ["金額"] });
deepEqual(dst.range("A8:E8").values, [[null, null, 500, null, null]]);
// A header in the middle of the sheet; start gives it with matchHeader: true.
dst.write([["名前", "値"]], { start: "I3" });
dst.write([{ 値: 1, 名前: "a" }, { 値: 2, 名前: "b" }], { start: "I3", matchHeader: true });
deepEqual(dst.range("I3:J5").values, [["名前", "値"], ["a", 1], ["b", 2]]);
dst.append([{ 名前: "c", 値: 3 }], { matchHeader: "I3" });
deepEqual(dst.range("I6:J6").values, [["c", 3]]);
// append with matchHeader: true: the header is row 1.
const log = wb.addSheet("ログ");
log.write([["日付", "内容"]]);
log.append([{ 内容: "開始", 日付: new Date(2026, 3, 1) }], { matchHeader: true });
log.append([{ 内容: "終了", 日付: dayjs("2026-04-02") }], { matchHeader: true });
deepEqual(log.toRowObjects().map((r) => [r.日付.getDate(), r.内容]), [[1, "開始"], [2, "終了"]]);
// Copying a table by its headers.
const copy = wb.addSheet("写し");
copy.write([["金額", "コード"]]);
copy.append(ws.toRowObjects({ start: "B3", untilBlank: true }), { matchHeader: true });
deepEqual(copy.toArrays(), [["金額", "コード"], [100, "A-1"], [200, "A-2"]]);
// Duplicate headers are named as toRowObjects() names them.
const dup = wb.addSheet("重複");
dup.write([["月", "月", "計"]]);
dup.append([{ 月: 1, 月_2: 2, 計: 3 }], { matchHeader: true });
deepEqual(dup.toArrays()[1], [1, 2, 3]);

// Errors.
throws(() => dst.write([["a", 1]], { matchHeader: 3 }), /matchHeader を使うときは/);
throws(() => dst.write([{ ない: 1 }], { matchHeader: 3 }), /行のキー（ない）に 台帳!A3 の見出しと一致するものがありません。見出し: No, 品 名, 金額, 税込, コード/);
throws(() => dst.append([{ 金額: 1 }], { matchHeader: 20 }), /台帳!A20 に見出しがありません/);
dst.append([], { matchHeader: 3 });
console.log("rowpick ok");
