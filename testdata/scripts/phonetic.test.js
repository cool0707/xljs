import { ok, equal, throws } from "./_assert.js";

// Furigana: set on the text of a cell, kept when saved, read back, and what PHONETIC gives.
const wb = workbooks.create();
const ws = wb.addSheet("名簿");
ws.write([["氏名", "よみ"], ["山田太郎"], ["佐藤花子"], ["鈴木一郎"]]);
equal(ws.cell("A2").phonetic, null, "none yet");
ws.cell("A2").phonetic = "ヤマダタロウ";
ws.cell("A3").phonetic = "さとうはなこ";
ws.cell("A3").showPhonetic = true;
equal(ws.cell("A2").phonetic, "ヤマダタロウ");
equal(ws.cell("A3").showPhonetic, true);
equal(ws.cell("A2").showPhonetic, false);

// PHONETIC: the furigana, or the text itself when it has none.
for (let r = 2; r <= 4; r++) ws.cell(`B${r}`).formula = `=PHONETIC(A${r})`;
wb.calculate();
equal(ws.cell("B2").value, "ヤマダタロウ");
equal(ws.cell("B3").value, "さとうはなこ");
equal(ws.cell("B4").value, "鈴木一郎");

// Without furigana, PHONETIC gives the text as full-width katakana do, as Excel does
// (kana converted, letters and kanji as they are); a block one cell after the other.
const raw = wb.addSheet("読み");
raw.write([["さとう"], ["ｽｽﾞｷ"], ["abcＡＢＣ"], ["東京"], [123]]);
for (let r = 1; r <= 5; r++) raw.cell(`B${r}`).formula = `=PHONETIC(A${r})`;
raw.cell("C1").formula = "=PHONETIC(A1:A3)";
wb.calculate();
equal(raw.cell("B1").value, "サトウ");
equal(raw.cell("B2").value, "スズキ");
equal(raw.cell("B3").value, "abcＡＢＣ");
equal(raw.cell("B4").value, "東京");
equal(raw.cell("B5").text, "", "a number has none");
equal(raw.cell("C1").value, "サトウスズキabcＡＢＣ");

// Kept when saved (as Excel writes them: hiragana are marked as such).
const back = workbooks.open(wb.toArrayBuffer()).sheet("名簿");
equal(back.cell("A2").phonetic, "ヤマダタロウ");
equal(back.cell("A3").phonetic, "さとうはなこ");
equal(back.cell("A3").showPhonetic, true);
equal(back.cell("B2").value, "ヤマダタロウ", "the result saved");

// Another text: the furigana were of the old one.
ws.cell("A2").value = "山田次郎";
equal(ws.cell("A2").phonetic, null);
// The same text again keeps them.
ws.cell("A3").value = "佐藤花子";
equal(ws.cell("A3").phonetic, "さとうはなこ");
// null removes them.
ws.cell("A3").phonetic = null;
equal(ws.cell("A3").phonetic, null);

// Only a text takes furigana.
ws.cell("C2").value = 5;
throws(() => { ws.cell("C2").phonetic = "ゴ"; }, /ふりがなは文字列に付けます/);
ws.cell("C2").phonetic = null;
console.log("phonetic ok");
