---
title: PDF 出力
api: PrintOptions, PrintPages, PrintPreviewSetting, PrintPreviewOptions, HtmlPdfOptions
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
- 数式は印刷の前に計算する: スクリプトが書き換えたセルに依存する数式、結果が保存されていない数式、`TODAY()`・`NOW()` を使う数式（[数式の計算](excel.md)。結果はセルにも入る）。それ以外は Excel が保存した値のまま。
- `toHtml(options?)` は同じページを HTML の文字列で返す（グラフは空の枠）。

## 印刷プレビュー

Excel の印刷プレビューのように、ページを画面に出して設定を変えてもらってから PDF にできる。

```js
const book = workbooks.open("請求書.xlsx");
// プレビューを出してから PDF に（キャンセルなら null）
const path = await book.toPdf("請求書.pdf", { preview: true });
if (!path) exit();

// 出す設定を選ぶ（並べた順に出る）
await book.toPdf("一覧.pdf", { fitToWidth: 1, preview: { settings: ["scaling", "blackAndWhite"] } });

// 設定だけ選んでもらい、あとで使う
const options = await book.printPreview({ paper: "A4" }, { settings: true, ok: "決定" });
if (options) {
  await book.toPdf("控え.pdf", options);
  await book.toPdf("配布用.pdf", { ...options, blackAndWhite: true });
}
```

- `toPdf(dest, { preview })`: `preview` が `true` かオブジェクト（`PrintPreviewOptions`）なら、書き出す前にプレビューを出す。キャンセルされたら PDF を作らずに `null` を返す。
- `book.printPreview(options?, preview?)` / `sheet.printPreview(options?, preview?)`: プレビューを出し、確定したら `options` に利用者が変えた設定を重ねたものを返す（キャンセルは `null`）。`toPdf`・`toHtml`・`printPages` にそのまま渡せる。
- 出す設定は `preview.settings` で選ぶ: `"sheets"`（印刷するシート。ブックで印刷できるシートが 2 つ以上のとき）`"paper"` `"landscape"`（向き）`"scaling"`（拡大縮小なし・シートを 1 ページに印刷・すべての列 / 行を 1 ページに印刷・倍率・横 n × 縦 n ページ）`"margin"`（標準・広い・狭い・ユーザー設定）`"blackAndWhite"` `"gridLines"` `"centerHorizontally"` `"charts"` `"skipBlankPages"` `"condense"`。既定は `["sheets", "paper", "landscape", "scaling", "margin", "blackAndWhite"]`、`true` ですべて、`false` で設定なし（ページの確認だけ）。
- 設定の初めの値は、`options` を当てたシートのページ設定。シートによって違う設定（用紙・向き・拡大縮小・余白）は「シートごとの設定」になる。利用者が変えなかった設定は `options` に入れない（シートごとの設定のまま印刷される）。
- そのほか: `title`（ウィンドウのタイトル）`ok`（確定のボタンの文字）`width` `height`。
- ページを表示するのは Windows のネイティブ UI（WebView2）だけ。コンソール・mock ではページを出さずに、同じ設定をフォームで尋ねる（mock の応答は `ui.form` と同じ `"form"` の列。項目の名前は `sheets` `paper` `landscape`（`"portrait"` / `"landscape"`）`scaling`（`"none"` `"page"` `"columns"` `"rows"` `"scale"` `"fit"`）`scale` `fitWidth` `fitHeight` `margin`（`"normal"` `"wide"` `"narrow"` `"custom"`）`marginTop` `marginRight` `marginBottom` `marginLeft` `blackAndWhite` `gridLines` `centerHorizontally` `charts` `skipBlankPages`（`"none"` `"all"` `"values"`）`condense`）。
