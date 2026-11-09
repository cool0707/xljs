---
name: xljs-pdf
description: xljs の PDF 出力（book.toPdf・sheet.toPdf・xljs.htmlToPdf・printPages）と PDF の編集（pdf。結合・ページの削除・挿入・回転・文字の追加）のリファレンス。xljs のスクリプトで PDF を作る・編集するときに使う。
---

# xljs API リファレンス: PDF

xljs の PDF 出力と PDF の編集です。基本の API（ファイル・Excel・CSV・JSON・フォーム・表・日付など）はスキル `xljs-reference` にあります。ここに書いていない関数・オプションは存在しないものとして扱ってください。

## PDF 出力

Excel がなくても、印刷したときと同じページで PDF にできる（Windows の WebView2 を使う。コンソール・mock では使えない）。

```js
const book = workbooks.open("請求書.xlsx");
await book.toPdf("請求書.pdf");                                  // 非表示でないすべてのシート
await book.toPdf("一覧.pdf", { sheets: ["一覧"], fitToWidth: 1 }); // 横 1 ページに収める
await book.sheets["明細"].toPdf("明細.pdf", { paper: "A4", landscape: true, margin: 10 });
await xljs.htmlToPdf("<h1>報告書</h1><p>本文</p>", "報告書.pdf", { paper: "A4" }); // HTML から
```

- ブックのページ設定に従う: 用紙・向き・余白・拡大縮小と「次のページ数に合わせて印刷」・印刷範囲・印刷タイトル・改ページ・ページの順序・ヘッダー/フッター（`&P` `&N` `&D` `&A` `&F` など）・左右中央。
- `options` で上書きできる: `paper`（`"A3"` `"A4"` `"B5"` `"Letter"` など）`landscape` `scale`（%）`zoom`（%。シートの倍率や「次のページ数に合わせて」にさらに掛ける。はみ出す分だけ縮めたいとき）`fitToWidth` `fitToHeight` `fitToPage` `margin`（mm）`gridLines` `blackAndWhite`（`false` でシートの白黒印刷を無視） `centerHorizontally` `charts: false` `sheets` `skipBlankPages`（`true` で何も印刷されないページ、`"values"` で値のないページ（罫線・塗りだけのページも）を飛ばす） `condense`（`true` か 0〜20 の %。セルと図形の文字の間隔を詰める。Excel の印刷は画面より文字が広く、折り返しが増えたり右端が切れたりするのを画面の見た目に近づける）。
- `book.printPages(options)` / `sheet.printPages(options)`: PDF を作らずに、ページの分かれ方を返す（シートごとに `{ sheet, pages, across, down, scale, areas: [{ range, across, down, pages }] }`。`across` が横、`down` が縦のページ数、`scale` は %）。はみ出しの確認と `zoom` の調整に使う（例: `if (sheet.printPages().across > 1) await sheet.toPdf(p, { zoom: 90 })`）。
- 値・表示形式・フォント・塗り・罫線・結合・折り返し・回転・画像・図形とテキストボックス（Excel の既定の形すべてとフリーフォーム。影・3-D は描かない）・線と矢印・グラフ（`sheet.charts` で描けるもの）を描く。条件付き書式・SmartArt・Excel 2016 以降のグラフは描かない。自動の改ページの位置は Excel と少し違うことがある。
- `toHtml(options?)` は同じページを HTML の文字列で返す（グラフは空の枠）。

## PDF の編集（pdf）

PDF の結合・ページの削除・挿入・回転・文字の追加ができる（Excel・Acrobat・WebView2 は不要。mock・コンソールでも動く）。ページ番号は 1 始まり、位置は mm、文字の大きさは pt。

```js
const doc = pdf.open("報告書.pdf");                    // パスワード付きは pdf.open(p, { password: "..." })
doc.append("別紙.pdf");                                 // 最後に結合（{ pages: "1-3" } で一部だけ）
doc.insert(2, "差し込み.pdf", { pages: [1] });          // 2 ページ目の前に挿入
doc.insertBlankPage(1, { paper: "A4" });                // 白紙のページ（表紙）
doc.removePages("5-6,-1");                              // 削除（-1 は最後のページ）
doc.rotatePages(3, 90);                                 // 時計回りに回転
doc.addText("all", (p) => `${p.number} / ${p.count}`, { position: "bottom-center", size: 9 }); // ページ番号
doc.addText("all", "資料番号 A-001", { position: "top-right", size: 9 }); // 右上に資料番号
doc.addText(1, "提出用", { x: 20, y: 15, size: 14, color: "red" });     // 位置を mm で
doc.save("報告書_提出用.pdf");                          // 省略で上書き
doc.close();
const p = pdf.merge(["a.pdf", "b.pdf", "c.pdf"], "まとめ.pdf"); // 結合して保存（保存先を省くと文書を返す）
```

- ページの指定: 番号・番号の配列・`"1-3,5"`・`"3-"`（3 ページ目から最後まで）・`"all"`。負の番号は後ろから。
- `doc.pageCount`、`doc.pages`（`[{ number, width, height, rotation }]`。表示したときの大きさ（mm）と回転）。
- `insert` / `append` の取り込み元は PDF ファイル（パス・ハンドル・`File`・`Blob`・`ArrayBuffer`）か別の `PdfDocument`。自分自身を渡すとページの複製になる。しおり（アウトライン）とフォームの定義は取り込まない。
- `addText` の置き場所は `position` が簡単: `"top-left"` `"top-center"` `"top-right"` `"center"` `"bottom-left"` `"bottom-center"` `"bottom-right"`（`"右下"` `"下中央"` なども可）。ページの端から `margin`（mm、既定 10。`{ x, y }` で別々に）だけ内側に置き、複数行は寄せた側にそろう。
- ほかのオプション: `x` `y`（左上からの位置。`y` は文字の上端）、`right` `bottom`（右端・下端からの位置。指定した向きは `position` より優先）、`size`（既定 11）、`color`（`"#rrggbb"` か色名）、`opacity`（0〜1）、`align`（行の揃え）、`lineHeight`（既定 1.3）、`rotate`（度、反時計回り）、`font`（`"游ゴシック"` `"メイリオ"` `"ＭＳ 明朝"` などの名前かフォントファイルのパス。配列なら文字ごとに前から探す）。`"\n"` で改行。回転したページでも見た目の向きで置く。
- 文字はフォントの使った字だけを埋め込むので、検索・コピーできる。既定のフォントは游ゴシック（Windows）。
- 保存した PDF は暗号化しない（パスワードは外れる）。電子署名は無効になる。

