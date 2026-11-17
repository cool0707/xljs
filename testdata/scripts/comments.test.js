import { ok, equal, deepEqual, throws } from "./_assert.js";

// Threaded comments and notes of a workbook Excel saved (testdata/xlsx/make_comments.py).
const src = path.join(xljs.scriptDir, "..", "xlsx", "comments.xlsx");
const ws = excel.open(src).sheets[0];

const h5 = ws.cell("H5").comment;
equal(h5.text, "👤😀絵文字");
equal(h5.author, "山田 太郎");
equal(h5.date.toISOString(), "2026-11-15T23:10:33.810Z", "dT is in UTC");
equal(h5.resolved, false);
equal(h5.replies.length, 0);
equal(h5.address, "H5");
equal(h5.cell.value, "a");

const i5 = ws.cell("I5").comment;
deepEqual(i5.replies.map((r) => [r.author, r.text]), [["山田 太郎", "返信１"], ["山田 太郎", "返信２"]]);
const j5 = ws.cell("J5").comment;
equal(j5.resolved, true);
equal(j5.replies[0].text, "@山田 太郎 返信ですね");

// The notes Excel writes in place of the threads are not notes.
for (const a of ["H5", "I5", "J5"]) equal(ws.cell(a).note, null, a);
equal(ws.cell("K5").note, "佐藤 花子:\n古いメモ");
equal(ws.cell("K5").comment, null);
deepEqual(ws.notes, [{ address: "K5", text: "佐藤 花子:\n古いメモ", author: "佐藤 花子" }]);
deepEqual(ws.comments.map((c) => c.address), ["H5", "I5", "J5"]);

// Kept when saved: authors, replies, resolved, mentions, the notes in their place.
const bytes = ws.book.toArrayBuffer();
const back = excel.open(bytes).sheets[0];
deepEqual(back.comments.map((c) => [c.address, c.author, c.text, c.resolved, c.replies.map((r) => r.text)]), [
  ["H5", "山田 太郎", "👤😀絵文字", false, []],
  ["I5", "山田 太郎", "かきくけこ", false, ["返信１", "返信２"]],
  ["J5", "山田 太郎", "あいうえお", true, ["@山田 太郎 返信ですね"]],
]);
equal(back.cell("K5").note, "佐藤 花子:\n古いメモ");

// Writing: a thread, replies, resolving, editing, deleting.
const wb = excel.create();
const s = wb.addSheet("確認");
s.write([["項目", "金額"], ["交通費", 1200]]);
const c = s.cell("B2").addComment("金額を確認してください", { author: "鈴木 一郎" });
equal(c.author, "鈴木 一郎");
ok(c.date instanceof Date && Math.abs(c.date - Date.now()) < 60000, "written now");
const r = c.reply("領収書で確認しました", { author: "高橋 次郎" });
c.reply("ありがとうございます", { author: "鈴木 一郎" });
equal(c.replies.length, 2);
equal(r.author, "高橋 次郎");
c.resolved = true;
r.text = "領収書で確認済み";
equal(s.cell("B2").comment.replies[0].text, "領収書で確認済み");
throws(() => s.cell("B2").addComment("もう一つ"), /すでにコメントがあります/);
throws(() => { s.cell("B2").note = "メモ"; }, /コメントがあります/);
throws(() => { c.text = ""; }, /空です/);
equal(s.cell("B2").note, null);
equal(s.cell("A2").addComment("既定の作成者").author, xljs.userName);

// Notes.
s.cell("A1").note = "見出し";
equal(s.cell("A1").note, "見出し");
s.cell("A1").note = "見出しの行";
throws(() => s.cell("A1").addComment("x"), /メモがあります/);

const again = excel.open(wb.toArrayBuffer()).sheet("確認");
const t = again.cell("B2").comment;
deepEqual([t.author, t.text, t.resolved], ["鈴木 一郎", "金額を確認してください", true]);
deepEqual(t.replies.map((x) => [x.author, x.text]), [["高橋 次郎", "領収書で確認済み"], ["鈴木 一郎", "ありがとうございます"]]);
equal(again.cell("A1").note, "見出しの行");
deepEqual(again.notes.map((n) => n.address), ["A1"], "the notes in place of the threads are not listed");

// Deleting: a reply, then the thread with the rest; then the cell takes a note.
t.replies[0].delete();
deepEqual(t.replies.map((x) => x.text), ["ありがとうございます"]);
t.delete();
equal(again.cell("B2").comment, null);
throws(() => t.text, /削除されています/);
again.cell("B2").note = "メモに";
equal(again.cell("B2").note, "メモに");
again.cell("A1").note = null;
equal(again.cell("A1").note, null);
const last = excel.open(again.book.toArrayBuffer()).sheet("確認");
deepEqual(last.comments.map((x) => x.address), ["A2"]);
deepEqual(last.notes.map((n) => [n.address, n.text]), [["B2", "メモに"]]);
console.log("comments ok");
