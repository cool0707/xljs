---
title: PDF の編集（pdf）
api: Pdf, pdf, PdfDocument, PdfSource, PdfPages, PdfPageInfo, PdfInsertOptions, PdfBlankPageOptions, PdfTextOptions, PdfTextPosition, PdfTextPage
---

```js
const doc = pdf.open("報告書.pdf");                   // パス・ハンドル・File・Blob・ArrayBuffer（パスワード付きは { password }）
doc.append("別紙.pdf", { pages: "1-3" });              // 結合（ページを選んで）
doc.insert(2, "差し込み.pdf");                         // 2 ページ目の前に挿入
doc.insertBlankPage(1, { paper: "A4" });               // 白紙のページ
doc.removePages("5-6,-1");                             // 削除（負の番号は後ろから）
doc.rotatePages(3, 90);                                // 時計回りに回転
doc.addText("all", (p) => `${p.number} / ${p.count}`, { position: "bottom-center", size: 9 }); // ページ番号
doc.addText("all", "資料番号 A-001", { position: "top-right", size: 9 });                   // 資料番号
doc.addText(1, "社外秘", { position: "center", size: 48, color: "red", opacity: 0.3, rotate: 30 });
doc.save("報告書_提出用.pdf");
pdf.merge(["a.pdf", "b.pdf"], "まとめ.pdf");           // まとめて結合して保存
```

- Excel・Acrobat・WebView2 がなくても動きます（コンソール・モックでも可）。ページ番号は 1 始まり、位置は mm（表示したときのページの左上から。回転したページでも見た目の向き）、文字の大きさは pt です。
- `addText` の置き場所は `position` で指定します: `"top-left"` `"top-center"` `"top-right"` `"center"` `"bottom-left"` `"bottom-center"` `"bottom-right"`（`"右上"` `"下中央"` `"右下"` なども可）。ページの端から `margin`（mm、既定 10。`{ x: 15, y: 8 }` で横・縦を別々に）だけ内側に置きます。細かく決めるときは `x` `y` `right` `bottom`（mm）で指定します。
- `addText` の文字は、フォントのうち使った字だけを PDF に埋め込みます（検索・コピーできます）。フォントは `font` に名前（`"游ゴシック"` `"メイリオ"` `"ＭＳ 明朝"` など）かファイルのパスで指定し、既定は表示言語の Excel のフォント（日本語は游ゴシック）です。配列にすると文字ごとに前から順に探します。
- ページを取り込むとき、しおり（アウトライン）・フォームの定義・ページラベルは引き継ぎません。保存した PDF は暗号化されず、電子署名は無効になります。
- サンプル: `examples/pdf-merge-stamp.js`。仕組みは `docs/design/pdf-edit.md`
