---
title: PDF 出力
api: PrintOptions, PrintPages, HtmlPdfOptions
---

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
