# PDF 出力（book.toPdf / sheet.toPdf / xljs.htmlToPdf / printPreview）

Excel がなくても、ブックを「Excel で印刷したときのページ」で PDF にする機能の設計と実装のメモ。
シートのレイアウト（ページ分割まで）は Rust で HTML にし、WebView2 に描かせて PDF にする。

## 0. 方針

- **Excel の COM（`ExportAsFixedFormat`）は使わない**（Excel 未インストールでも動かす）。
- **ページ分割は自前**: Chromium の表の改ページに任せず、Excel と同じ考え方（印刷範囲・用紙から余白を引いた幅と高さ・拡大縮小・
  手動の改ページ・印刷タイトル）でページごとの行と列を決め、1 ページ = 1 つの `<section class="xljs-page">` にする。
  ページの大きさ・余白・ヘッダー/フッターは CSS の `@page`（シートごとの名前付きページ `page: sN`）で表す。
- **PDF にするのは Chromium**: WebView2 の DevTools プロトコル `Page.printToPDF`（`preferCSSPageSize: true`）。
  `ICoreWebView2_7::PrintToPdf` は用紙を 1 つしか指定できず、シートごとに向き・用紙が違うブックを 1 つの PDF にできないため使わない。
- **グラフは既存の仕組み**（`sheet.charts` → `ChartSpec` → `web/chart.js`）で描く。Rust はグラフの位置（アンカー）だけを HTML に書き、
  prelude が `ChartSpec` を作り、ページ `web/print.html` が AG Charts / Frappe Gantt で枠の中に描く。
- **見た目の再現は「だいたい同じ」まで**。自動の改ページ位置は列幅・行の高さから計算するので、フォントの計測が違う分だけずれることがある。

## 1. 使い方

```js
const book = workbooks.open("請求書.xlsx");
await book.toPdf("請求書.pdf");                                    // 非表示でないすべてのシートを 1 つの PDF に
await book.toPdf("一覧.pdf", { sheets: ["一覧"], fitToWidth: 1 });   // 横 1 ページに収める
await book.sheets["明細"].toPdf("明細.pdf", { paper: "A4", landscape: true, margin: 10 });
const html = book.toHtml();                                        // 同じページを HTML で（グラフは空の枠）
await xljs.htmlToPdf("<h1>報告書</h1>", "報告書.pdf", { paper: "A4" }); // 任意の HTML から
```

- `toPdf` は書き出したファイルの絶対パスで resolve する。WebView2 がない UI（コンソール）ではエラー。mock は書かずにログだけ出す
  （`pdf("…/a.pdf", 4 pages, 1/1 charts)`）。
- `options`（`PrintOptions`）: `sheets` `paper` `landscape` `scale` `zoom`（シートの倍率・「次のページ数に合わせて」の結果にさらに掛ける %）`fitToWidth` `fitToHeight` `fitToPage` `margin`（mm）`gridLines`
  `blackAndWhite` `centerHorizontally` `charts: false` `title` `skipBlankPages` `condense`。省略した項目はシートのページ設定に従う。
- `condense`（`true` は 4、0〜20）: セルと図形の文字の間隔を文字の大きさの n% ずつ詰める（`letter-spacing:-0.0nem`）。Excel の印刷・PDF は画面より文字の幅がやや広く、
  折り返しの行が増えたり右端が切れたりするので、画面の見た目に近づけるためのもの。`font-feature-settings: "palt"` は使わない
  （游ゴシックなどのかなだけが大きく詰まり、Excel の画面とも違う形になるため）。em は要素ごとの文字の大きさなので、規則はセルの `div` と図形の段落の中のすべての要素に付ける
  （`condense_css`）。ワードアートの変形した文字（`.xljs-warp`）は外す。ランの文字間隔（`spc`。インラインの `letter-spacing`）があればそちらが勝つ。
  列幅・行の高さ・ページの分かれ方は変わらない。「縮小して全体を表示」の倍率と数値の `###` は `print-page.js` が詰めたあとの幅を測るので合う（`toHtml` の縮小の倍率は詰めない幅で見積もる）。
- `skipBlankPages`（native には 1 / 2 で渡す）: ページを分けたあとで、本体（印刷タイトルを除く行・列）に何もないページを外す（`blank_page`）。
  `true` は値・塗り・罫線（斜線・テーブルのスタイルも）・画像・図形・グラフのどれもないページ、`"values"` は値と画像・図形・グラフのないページ。
  結合セルは切られたページそれぞれに値があるとみなす。空白だけの文字列は値なし。枠線（`gridLines`）は数えない。
  ページ番号（`counter(page)` / `counter(pages)`）は残ったページで数え、`printPages` の `pages`（範囲ごとの `areas[].pages` も）は残ったページ数（`across × down` より少なくなる）。
- `printPages(options)`（ブック: 印刷するシートの配列、シート: 1 つか null）は同じレイアウトの計算で、HTML を作らずにページの分かれ方
  （`pages` `across` `down` `scale`（%）`areas: [{ range, across, down, pages }]`）だけを返す（`PrintOptions::layout_only`）。
- `htmlToPdf` は `options` の用紙・向き・余白を `@page` にして文書の `<head>` の先頭に入れる（文書の `@page` があればそちらが勝つ）。
  文書のスクリプトは動かない（`DOMParser` で取り込むため）。

## 2. シートのレイアウト（`src/api/xlsx_print.rs`、native は `xlsxPrintHtml`）

### ページ設定の読み方

| 項目 | 読み方 |
|---|---|
| 用紙・向き・拡大縮小 | umya の `PageSetup`（`paperSize` のコード → mm。不明なら A4） |
| 次のページ数に合わせて印刷 | `fitToPage`（umya）＋ `fitToWidth` / `fitToHeight`（**シートの XML から直接**。umya は属性がないこと＝1 と 0＝自動を区別できない） |
| 余白・ヘッダー/フッターの位置 | `PageMargins`（ないときは Excel の「標準」0.7/0.75/0.3 インチ） |
| 印刷範囲・印刷タイトル | 定義された名前 `_xlnm.Print_Area` / `_xlnm.Print_Titles`（シートの名前、ブックの `localSheetId` 付きの名前）。複数の範囲は範囲ごとにページを分ける。列全体・行全体（`$A:$Q`）は使用範囲の端まで。数式の印刷範囲（`OFFSET($A$1,0,0,MAX(IFNA(MATCH(10^10,$B:$B),0)+5,43),7)` のような可変の範囲）は `xlsx_calc::eval_area` で計算する（`OFFSET` と単純な参照だけ。計算できなければ使用範囲） |
| 白黒印刷 | `pageSetup blackAndWhite`（シートの XML から直接）。塗りを消し、文字・罫線を黒にする。`options.blackAndWhite` が優先（`false` で確認用にカラーで出せる） |
| テーブル（リスト）の書式 | `src/api/xlsx_table.rs`。テーブルの範囲とスタイル名・縞模様などの表示は umya の `Table`、ブック独自のテーブルスタイルは styles.xml の `tableStyles` と `dxfs`（開いたときに `Book::table_styles`）。組み込みの `TableStyleLight1`〜`Dark11` は系統ごとの近似。要素（全体・縞・先頭/最後の列・見出し行・集計行・角のセル）を優先順に重ね、セル自身の塗り・罫線・文字色（既定の黒以外）が優先する |
| 改ページ | `rowBreaks` / `colBreaks`（`id` は 0 始まりで、その次の行・列から新しいページ） |
| ページの順序・枠線の印刷 | `pageOrder="overThenDown"`、`printOptions gridLines`（シートの XML から直接） |
| 非表示のシート | `workbook.xml` の `state`（umya は読まない）。ブックを開いたときに `RawPrint`（`Book::print`）に保持する。シート名の変更に追従する |
| ヘッダー/フッター | `oddHeader` / `oddFooter` の `&L` `&C` `&R`。`&P` `&N` → `counter(page)` / `counter(pages)`、`&D` `&T` `&A` `&F` `&Z`、`&B` `&I` `&U` `&S`、`&"フォント,太字"`、`&nn`（サイズ）、`&K`（色）。先頭ページ・偶数ページ用と画像（`&G`）は使わない |

印刷範囲がなければ使用範囲（値・塗り・罫線（斜線も）のあるセル、値のある結合セル、テーブル、画像・グラフ）。先頭の空の行・列は印刷しない。

### 大きさ

- 列幅（文字数）→ ピクセル: Excel と同じ `trunc(((256·w + trunc(128/mdw)) / 256)·mdw)`。最大の数字の幅 `mdw` は既定のフォントから
  （日本語のフォント（名前が日本語、Yu・MS・Meiryo・BIZ で始まる、JP・CJK を含む）は 11pt で 8px、それ以外は 7px）。既定の列幅は `defaultColWidth`、なければ `baseColWidth·mdw + 5` を 8 の倍数に切り上げ。
- 行の高さ: 行の `ht`、なければ既定のフォントから（游ゴシック 11pt → 18.75pt、ＭＳ 系 → 13.5pt、Calibri → 15pt）。`customHeight` のときと
  知らないフォントのときはシートの `defaultRowHeight`。非表示の行・列は 0。
- 「次のページ数に合わせて印刷」: 倍率 = min(1, 幅×横のページ数÷全体の幅, 高さ×縦のページ数÷全体の高さ)（1% 単位に切り捨て、10% 以上）。
  このとき手動の改ページは使わない（Excel と同じ）。

### ページの分け方

列と行を別々に、先頭から足していき「余白を引いた幅（高さ）÷倍率」を超える手前で切る（手動の改ページでも切る）。
印刷タイトルの行・列は、それより後ろから始まるページの先頭に入れ、その分だけ使える高さ（幅）を減らす。
ページの順序は既定で「左から右へ」ではなく「上から下へ」（列のまとまりごとに行のまとまりを並べる）。

### HTML

```html
<section class="xljs-page" style="page:s0;--w:…;--h:…;--pw:…;--ph:…;--m:…">   <!-- 1 ページ -->
  <div class="xljs-sheet" style="zoom:0.85">                                 <!-- 拡大縮小 -->
    <table>…</table>                                                          <!-- 印刷タイトル＋このページの行・列 -->
    <div class="xljs-objs" style="left;top;width;height">                    <!-- 画像・グラフ（本体の範囲で切る） -->
      <img src="data:image/png;base64,…" style="left;top;width;height">
      <div class="xljs-print-chart" data-chart="0" style="…"></div>
    </div>
  </div>
</section>
```

- セルは `<td>` の中の絶対配置の `<div>`（大きさは pt で指定）に値を入れる。行の高さは内容で伸びない。
  文字のはみ出しは Excel と同じく隣の空のセルへ（左揃え・標準は右へ、右揃えは左へ、中央は両側へ）。
  印刷範囲がないときの使われている範囲（`Sheet::used`）も、Excel と同じくはみ出した文字が届く列まで広げる（`Sheet::overflow`。幅は `text_width` の見積もり。
  多めに見積もると 1 列多く入って余計なページが出るので、余裕は足さない。見積もりはプロポーショナルの和文では広め）。
- 絵文字は文字の色で描くため、`body` に `font-variant-emoji:text`（カラー絵文字のフォントだとセルの文字の色が付かない）。
- 書式はセルの CSS をまとめてクラス（`.k0` …）にする。フォント（游ゴシック → `Yu Gothic` などの別名と代わりのフォント）、太字・斜体・下線・取り消し線、
  文字の色（テーマの色は umya の `argb_with_theme`）、表示形式の色（`[Red]` `[赤]` など）、塗り（単色）、罫線（後述）、斜線（セル全体に重ねた SVG の線。座標はセルに対する %、線の太さ・破線・二重線は罫線の種類から。CSS のグラデーションだと PDF で粗い画像になる）、
  横・縦の配置、折り返し、インデント、回転（縦書きは `writing-mode`）、リッチテキスト（ランごとの `<span>`）。
- 罫線はセルの CSS の border ではなく、ページの表に重ねた 1 つの SVG（`svg.xljs-lines`。`viewBox` の 1 単位 = 1pt）の線（`page_lines`）。
  Chromium は CSS の罫線を CSS の 1px（0.75pt）単位に丸めた塗りの四角で PDF に描くため、ヘアラインが細線と同じ太さになり、
  縮小（`zoom`）すると中線が細線になり（1.5pt × 0.96 = 1.92px → 1px）、PDF ビューアーが倍率によって 1 画素に満たない塗りを落として罫線が消えた。
  SVG の線は PDF でも指定どおりの太さの線（stroke）になり、ビューアーは線を 1 画素より細くは描かない（Excel の PDF と同じ）。
  - 太さ: ヘアライン 0.25pt、細線・点線・破線 0.75pt、中線 1.5pt、太線 2.25pt、二重線は 0.75pt の線 2 本（中心から ±0.75pt）。グリッド線（`gridLines`）は 0.5pt の #c8c8c8。
  - セルの境目ごとに、両側のセルの罫線（条件付き書式 → セル → テーブルのスタイルの順）の重いほう（`line_rank`、同じなら上・左のセル）を引く。
    結合セルの中の境目には引かない。結合セルの周りはセルごとの罫線（Excel と同じく、端のセルに罫線がなければその部分は出ない）。
  - 行・列に沿って同じ線が続くところは 1 本にする。実線は端を `square` にして角をふさぐ。
    破線・点線は `stroke-dasharray` を使わず短い線に分けて書く（Chromium は破線の stroke を PDF で塗りの図形に変えてしまうため）。
  - 線は表の外側に太さの半分はみ出すので、`.xljs-page` は `overflow:clip` ＋ `overflow-clip-margin:3pt`。
  - 同じ太さの線がビューアーで 1 画素と 2 画素に見え分かれるのは、線の位置が画面の画素の境目とずれるため（Excel の PDF も同じ）で、PDF の側では直せない。
- リッチテキスト（1 つのセルで一部だけフォント・色・大きさが違う）: ランは 1 つの `<span>` で包む（セルの `<div>` は縦方向の flex なので、
  包まないとランごとに行が分かれて最後のランしか見えない）。`rPr` のないランはセルのフォントのまま、`rPr` のあるランはそれがフォントのすべて
  （Excel はすべての属性を書く）なので、書いていない太字・斜体・下線は「なし」としてセルの書式を打ち消す。色がなければセルの色、自動は黒。
  インライン文字列（`t="inlineStr"`。openpyxl などが書く）のリッチテキストも、共有文字列と同じくランごとに読む
  （本家の umya は最後のランの文字しか残さない。フォークで修正。7 章）。
- 縮小して全体を表示（`shrinkToFit`）: セルのスタイルの `Alignment::shrink_to_fit`（フォークで追加。7 章）。セルの文字を `<span class="xljs-shrink" style="zoom:…">` で包み、
  倍率はまず文字から大まかに見積もる（全角 1em、数字 0.5em、区切り記号 0.28em、ほか 0.55em。リッチテキストはランの大きさで）。
  PDF にするときは `print-page.js` の `shrinkToFit` が実際の幅を測って倍率を直す。折り返し・回転のセルでは縮小しない（Excel と同じ）。
  縮小するセルは隣へはみ出さず、数値も `###` にしない。
- 数式で値の保存がないセル（スクリプトで書いた数式）は `xlsx_calc` で計算する。
- セルに配置した画像（後述）は `<div>` の中の `<img class="xljs-cell-image">`。セルの大きさに縦横比を保って収め（`object-fit:contain`）、
  位置はセルの配置（横は標準・中央 → 中央、左・右はそのまま。縦は下が既定、上・中央）で `object-position`。代替テキストは `alt`。
- 結合セルはページの中で `colspan` / `rowspan`。ページの境目をまたぐ結合はページごとに切る。
- 画像・図形・グラフはアンカー（two-cell / one-cell / absolute）からシート上の位置を求め、重なるすべてのページに置いて本体の範囲で切る（ページをまたぐ図は分かれて出る）。
  重なり順は描画（drawing）の順。

### 図形・テキストボックス・画像（`src/api/xlsx_shape.rs`）

ブックを開いたときに描画の部品（`xl/drawings/*`、`xl/media/*`、テーマ。`ChartParts`）を保持しておき、描画の XML を順に読む。
umya-spreadsheet の図形のモデルは一部しか持たないので使わない（新しいブックは描画の部品がないので、umya の画像だけを置く）。

| 要素 | 描き方 |
|---|---|
| 図形（`sp`）の形 | SVG の path。アンカーの実寸（pt）を viewBox にする。既定の形（`prstGeom`）は ECMA-376 の定義（Part 1 の付録 "Preset Shape Definitions"、187 種）をそのまま評価する（`src/api/xlsx_geom.rs`）: 調整値（`avLst`）→ ガイドの式（`gdLst`。`*/` `+-` `?:` `pin` `at2` `sin` など。EMU で計算）→ パス（`moveTo` `lnTo` `arcTo` `quadBezTo` `cubicBezTo` `close`。パスごとの `w`/`h` の座標系）。パスごとの塗りの有無・濃淡（`darken` `lighten` など。立方体の側面や額縁の内側）・線の有無も定義のとおり。文字は定義の `rect`（楕円なら内接する四角）の中に置く（反転すると左右・上下も入れ替える）。定義は `src/api/preset_shapes.xml`（`tools/preset-shapes/make.py` で ECMA の XML から名前空間・空白・接続点・ハンドルを除いたもの）。定義にない名前は四角 |
| フリーフォーム（`custGeom`） | 既定の形と同じ評価（`avLst` `gdLst` `rect` `pathLst`。`arcTo` も）。`rect` が空（幅 0）なら文字は枠全体 |
| 線・コネクタ（`cxnSp`） | 既定の形の直線・カギ線（`bentConnector2`〜`5`）・曲線（`curvedConnector2`〜`5`）。矢じりは閉じていないパスに付ける。矢じり（`headEnd` / `tailEnd` の triangle・stealth・arrow・oval・diamond、大きさ sm/med/lg）は SVG の marker。破線（`prstDash`） |
| 塗り・線 | `spPr` の `solidFill`（グラデーションは最初の色）・`noFill`・`alpha`、`ln` の幅・色。指定がなければ図形のスタイル（`fillRef` / `lnRef` のテーマの色。線の幅は idx から 0.5/1/1.5pt） |
| 文字（`txBody`） | 段落ごとの揃え、ランの大きさ・太字・斜体・下線・取り消し線・色・フォント（`ea` / `latin`。`+mn-ea` などはブックの既定のフォント）、改行、箇条書きの文字。`bodyPr` の内側の余白・上下の位置・折り返しなし・縦書き。色の既定は `fontRef` |
| ワードアート（文字の効果） | ランの `rPr` の塗り（`solidFill`・`alpha`・`gradFill` は CSS のグラデーションを `background-clip:text` で文字に・`noFill` は輪郭だけ）、輪郭（`ln` → `-webkit-text-stroke`）、`effectLst` の光彩（`glow` → 2 重の `drop-shadow`）と影（`outerShdw` → `filter:drop-shadow`。`rotWithShape="0"` なら図形の回転を戻す）、反射（`reflection` → 文字を包む `div` の `-webkit-box-reflect`。`stA`/`endA`/`endPos` の濃さで消える）、文字間隔（`spc`）、大文字（`cap`）。図形の `spPr` の `effectLst`（影・光彩・反射）は図形の `div` 全体に（塗りのない図形なら文字に付く。Excel と同じ） |
| 文字の変形（`bodyPr` の `prstTxWarp`） | 定義は ECMA-376 の "Preset Text Warp Definitions"（`src/api/preset_text_warps.xml`、`tools/preset-shapes/make.py` で作る。40 種）。Rust が文字の枠（内側の余白を除く）でパスを評価して点列にし（`xlsx_geom::text_warp`）、`<div class="xljs-warp" data-warp="{w,h,paths}">` に段落と一緒に出す。印刷ページ（`print-page.js` の `warpText`）が 1 文字ずつ span にして測り、アフィン変換（`matrix`）で置く: パスが偶数なら 2 本ずつが帯の上下で、帯の行を帯いっぱいに引き伸ばす（文字は約 5pt 幅の縦の短冊に分け（`clip-path`）、短冊ごとに上下のパスの間に置くので曲がる）。奇数（アーチ・円・ボタン）なら 1 本ずつに行が乗り、パスの長さいっぱいに並べてパスに垂直に立てる（枠の中心の側に）。行は帯・パスに順に割り振る。グラデーションは行の幅で 1 つ |
| 回転・反転 | 回転は外側の `div` の `transform`（文字も一緒に回る）、反転は SVG の形だけ |
| グループ（`grpSp`） | 子の位置を子の座標（`chOff` / `chExt`）からグループの枠の割合にして置く（入れ子も） |
| 画像（`pic`） | `blip` の画像を data URL で（PNG・JPEG・GIF・BMP・SVG。EMF/WMF は描けないので出さない）。SVG の画像は `blip` が画面の大きさの PNG の代替画像なので、拡張（`extLst` の `asvg:svgBlip`）の SVG を使う（PDF でも線のまま）。Excel と同じく枠いっぱいに伸ばす（`preserveAspectRatio` がなければ `none`、`viewBox` がなければ `width` / `height` から） |
| 印刷しないもの | 非表示（`cNvPr hidden`）と「オブジェクトを印刷する」をオフにしたもの（`clientData fPrintsWithSheet="0"`） |
- 画面で開いたとき（`@media screen`）はページを灰色の背景の上に用紙の大きさで並べる（`toHtml` の確認用）。

### 条件付き書式（`src/api/xlsx_cond.rs`）

規則は umya-spreadsheet のもの（`Worksheet::conditional_formatting_collection`）を印刷のときに読む（`xlsx_cond::sheet_rules`）。
Excel 2010 の拡張（`extLst` の `x14:conditionalFormattings`。負の値と軸のあるデータバー、`3Stars` など）は `conditional_formatting_2010_collection` で、
両方に書かれたデータバーは `ext_id`（`<x14:id>`）と拡張側の `id` で結ぶ。アイコンセットの種類・データバーの設定・拡張・差分の書式の表示形式はフォークで読み書きできるようにした（7 章）。

- 規則は優先順位（`priority`）の順に当てる。書式の項目（塗り・文字の色・太字・斜体・取り消し線・下線・表示形式・罫線）は先に当たった規則が勝ち、
  「条件を満たす場合は停止」（`stopIfTrue`）で後ろの規則を止める。条件付き書式はセルの書式・テーブルスタイルより優先。白黒印刷では塗りと文字の色を出さない。
- 規則の種類: `cellIs`（比較・範囲内外。値は数式で、`xlsx_calc::eval_at` が相対参照を範囲の先頭のセルからのずれだけ動かして評価）、`expression`、
  `top10`（件数・%、上位・下位）、`aboveAverage`（以上・以下、標準偏差）、`duplicateValues` / `uniqueValues`、`containsText` などの文字列（`text` 属性。大文字・小文字は区別しない）、
  空白・エラー、`timePeriod`（今日・昨日・明日・過去 7 日・今週/先週/来週（日曜始まり）・今月/先月/来月）。
- しきい値（`cfvo`）: `min` `max` `num` `percent` `percentile`（PERCENTILE.INC）`formula` `autoMin` `autoMax`。範囲のうち数値のセルで求める。
- カラースケール: 2 色・3 色を RGB で線形に補間してセルの塗りにする。
- データバー: `<span class="xljs-bar">` をセルの中（上下左右 1.5pt 内側）に絶対配置する。長さは `minLength`〜`maxLength`（2007 形式は 10〜90%）。
  拡張があれば単色/グラデーション（グラデーションは色から白に近い色へ）、枠線、負の値の色、軸（自動＝最小と最大の比、中央、なし。破線）、右から左。値を表示しない設定も。
- アイコンセット: すべての組み込みのセット（3〜5 個の矢印・旗・信号・記号・評価・四分円・星・三角・ボックス）を SVG で描き、セルの左に置く（大きさは既定のフォントの 12/11 倍。
  縦はセルの縦の配置）。値の文字はアイコンの分だけ左を空ける。逆順（`reverse`）、個別に選んだアイコン（`cfIcon`、「セルのアイコンなし」）、アイコンのみ表示（`showValue="0"`）。
- umya のモデルを読むので、スクリプトで行・列を挿入・削除すれば範囲も動き、保存したブックにも残る（条件付き書式を書く xljs の API はない）。

### セルに配置した画像（`src/api/xlsx_cellimage.rs`）

Excel の「セルに配置」の画像はリッチ値で、セルの値としてはエラー `#VALUE!` になる。umya-spreadsheet（フォーク）はセルの `vm` を持ち、
リッチ値の部品を中身を見ずに読んだまま書き戻す（7 章）。`vm` がどの画像を指すかは、ブックを開いたときにここで読んでおく（`Book::cell_images`）。

- セル: `<c r="B2" t="e" vm="1"><v>#VALUE!</v></c>`。`vm`（1 から）→ `xl/metadata.xml` の `valueMetadata` の `bk` → `rc t`（`XLRICHVALUE` の `metadataType`）`v`
  → `futureMetadata name="XLRICHVALUE"` の `bk` → `xlrd:rvb i` → `xl/richData/rdrichvalue.xml` の `rv`。`rv` の値はその構造（`rdrichvaluestructure.xml` の `s`）のキーの順で、
  `_rvRel:LocalImageIdentifier` が `richValueRel.xml` の `rel` の位置（→ そのリレーションの `xl/media/*`）、`Text` が代替テキスト。
  部品はブックのリレーションの種類（`sheetMetadata` `rdRichValue` `rdRichValueStructure` `richValueRel`）から探す（名前の大文字・小文字は問わない）。
- `IMAGE("https://…")`（構造 `_webImage`）は画像がファイルにないので空欄にする（`#VALUE!` は出さない）。EMF/WMF も空欄。
- 画像として扱うのは、`vm`（`Cell::value_meta_index`）が画像を指し、値がエラーのままのセル（`CellImages::of`）。スクリプトが値や数式を書くと
  `vm` を外す（`Book::cell_mut`。エラーを返す数式を書いても画像ではない）。`vm` はセルが持つので、行・列の挿入・削除では画像もセルと一緒に動く。
- **参照するセル**: Excel は `=B2` のセルにも `vm` を付けて保存するので、それはそのまま画像（保存された結果を使う。ほかの数式と同じ）。
  計算し直して結果が変わったら、その数式の `vm` も外す（`xlsx_calc` の `store`）。
  `vm` のない数式（スクリプトで書いた数式、`vm` を書かないツールが保存した数式）は `xlsx_calc` で評価し、結果が画像なら画像（`xlsx_calc::cell_image`）。
  計算の値 `V::Image` は参照・`IF`・`IFERROR`・`INDEX`・`VLOOKUP` / `HLOOKUP`・`XLOOKUP`（完全一致）をそのまま通り、数値・文字列として使うと `#VALUE!`（`=B2&""` など。Excel と同じ）。
- 保存（`book.save`）: 画像はセルに残る。umya が `vm` と `xl/metadata.xml` の値のメタデータ、リッチ値の部品（`xl/richData/*` とその画像）を書く。
  `vm` を持つセルがなくなったブックには書かない。スクリプトで書いた `=B2` のような数式は `vm` なしで保存され、Excel が開いたときの再計算で画像になる
  （数式を書いたブックには `fullCalcOnLoad` が付く）。画像をセルに置く API はない。

## 3. PDF にする（`Ui::print_pdf`、`src/ui/native/webview.rs`、`src/ui/web/print.html` / `print-page.js`）

1. prelude の `printToPdf`: `xlsxPrintHtml` でレイアウト、`sheet.charts[i]` から `ChartSpec`（描けないグラフは `{ error }`）を作り、
   `native.pdfPrint(html, charts, texts, path)`（Promise。成功なら ""、失敗ならエラーの文）。
2. `Ui::print_pdf(job, done)`: ネイティブ UI は**見えないウィンドウ**（`WindowOptions::hidden`: `WS_EX_LAYERED` の不透明度 1/255、
   `WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT`、`SW_SHOWNOACTIVATE`）に `print.html` を開く。
   完全に隠したウィンドウでは `requestAnimationFrame` が止まり AG Charts が描けないおそれがあるため、表示はしたまま見えなくしている。
   ウィンドウの一覧（`WINDOWS`）には入れず、通知も出さない。
3. ページが `ready` → `Emulation.setEmulatedMedia`（`prefers-color-scheme: light`。ダークモードでもグラフを明るい色で）→ `init` を送る。
4. ページは文書を `DOMParser` で取り込み（`<style>` と `<link rel=stylesheet>` は head へ、スクリプトは捨てる）、フォントと画像を待ち、
   グラフを描く（アニメーションなし、`overrideDevicePixelRatio: 3` で印刷に耐える解像度、ガントは表示の切り替えなし）。
   描けたグラフ（キャンバスに色のある画素がある、または SVG がある）が揃うまで最大 5 秒待って `rendered`（`charts` と `drawn` の数を添える）。
5. `Page.printToPDF`（`printBackground`、`preferCSSPageSize`、余白 0 = `@page` の余白を使う）→ base64 を復号してファイルに書き、`done`、ウィンドウを閉じる。
   ページが `error` を送ったときと、途中でウィンドウが閉じたときは失敗。
6. スクリプト側は `poster.add_pending()` で待っているので、終わるまでプロセスは終了しない。スレッドの転送（`Threaded`）は待たずに UI スレッドへ投げる。

## 3.0 左右の余白の入れ替え（`mirrorMargins`）

両面印刷用に、偶数ページの左右の余白を入れ替える。ページの幅は変わらないのでページの分け方はそのまま。
`page_rule` がシートの名前付きページに `@page sN:left{margin-left:R;margin-right:L}` を足す（文書は右ページから始まるので、偶数ページが `:left`。
Chromium の `printToPDF` は名前付きページと `:left` の組み合わせを守る。Playwright の `page.pdf` で確かめた）。数え方は文書全体の通し番号。
画面（`@media screen`。プレビューと `toHtml`）とプリンター向けの配置では、ページの `style` の `--mm`（入れ替えた余白）を偶数ページ（`mark_pages` が付ける
属性 `data-even`）が使う。表・裏は元のページ番号ではなく出力する並びで数える（ページを選んだら選んだページの順。`"2,4,6"` を両面に印刷すると
2・6 が紙の表、4 が裏なので、元の番号で決めると綴じしろがそろわない）。プリンター（`printer_layout`）では用紙の続く範囲が別の印刷ジョブ（表から始まる）なので、
用紙が変わるところで数え直す（prelude の `printBook` のグループと同じ、`papers` の幅・高さが変わるところ）。PDF は 1 つの文書なのでシートが変わっても続ける。
`mirrorMargins` を省くと、`duplex`（prelude の `printOptions` が `print` の `duplex` を渡す）で紙がページの左右の辺で裏返るとき
（縦向きの `"long"`、横向きの `"short"`。シートごとの向きで決める）に入れ替える。プレビューで余白と両面印刷を選ぶだけで綴じしろがそろう。

## 3.0.1 印刷するページ（`pages`）

`"1,3,5-7"` `"8-"` `"-3"`（全角の数字・区切り、`〜` も可）を `xlsx_print::page_selection` が全体のページ数に当てる（最後より後ろは無視、1 ページも残らなければエラー）。
ページを選ぶとプリンター向けの配置（`PrintOptions::printer`）にして、`mark_pages` が選ばなかったページ（`<section class="xljs-page">`）を HTML から外し、
残したページに `counter-set:xljs-page <元の番号>` を付ける（&P は元の番号、&N は全体のページ数、表・裏の `data-even` は残したページの順。3.0）。
`papers[].pages` と `pages` は残したページの数（`print` の用紙ごとのジョブの範囲はこれで数える）、`total` は全体の数（プレビューの「n ページ（全 N ページ）」）。
PDF の `@page` の余白とページの余白ボックス（`counter(page)`）では印刷したページで数え直してしまうので、プリンター向けの配置を使う。

## 3.1 印刷プレビュー（`printPreview`・`toPdf` の `preview`、`web/print-preview.html` / `print-preview.js`）

Excel の印刷プレビューのように、ページを出して設定を変えてもらう。確定したら変えた設定を重ねた `PrintOptions` を返す（キャンセルは `null`）。
`toPdf(dest, { preview })` はそれを通してから PDF にする（キャンセルなら PDF を作らずに `null`）。

- **出す設定**（`preview.settings`。prelude の `PREVIEW_SETTINGS`）: `sheets` `pages`（文字列の入力）`paper` `landscape` `scaling` `margin` `blackAndWhite` `gridLines`
  `centerHorizontally` `charts` `skipBlankPages` `condense`。既定は `PREVIEW_DEFAULT`（シート・ページ・用紙・向き・拡大縮小・余白・白黒）。`toPdf` の `preview`（`"pdf"` のモード）は余白の後に `mirrorMargins` を足す（PDF には `duplex` がなく入れ替えが自動で掛からないため。説明文も PDF 用の `preview.mirrorMarginsPdfHelp`）。`sheets` はブックで印刷できるシートが 2 つ以上のときだけ。
- **初めの値**: native の `xlsxPrintSetup(book, options)`（`xlsx_print::print_setup`）が、印刷するシートごとに `options` を当てたページ設定
  （用紙の名前（`paper_name`。名前のない用紙は null）・向き・倍率か「次のページ数」・余白 mm・枠線・白黒・左右中央）を返す。
  シートによって違う設定（用紙・向き・拡大縮小・余白）は「シートごとの設定」（値 `""`）から始める（向きはそのときだけセグメントではなく選択）。
- **値**（wire）: 項目の名前と値は `PREVIEW_KEYS`（`scaling` は `"none"` `"page"` `"columns"` `"rows"` `"scale"` `"fit"`＋`scale` `fitWidth` `fitHeight`、
  `margin` は `"normal"` `"wide"` `"narrow"` `"custom"`＋`marginTop` など。Excel の標準・広い・狭いの mm は `PREVIEW_MARGINS`）。
  `previewOptions` は初めの値から変わった設定だけを `options` に重ねる（変えなかった設定はシートごとの設定のまま印刷される）。
- **ネイティブ UI**: 組み込みページ `print-preview.html`（左に設定、右にページ）。設定が変わるたび（数値は入力が止まって 400ms 後）に
  `{ type: "options", seq, values }` → prelude が `xlsxPrintHtml` とグラフ（`printCharts`）で `{ type: "layout", seq, html, charts, texts, pages }`、
  誤り（`pdf.empty`・不正な数値・シートなし）は `layoutError`。ページは `print.html?frame` の iframe（`bridge.js` の枠のモードで
  `postMessage` の `init` / `rendered`）に描かせ、描き終わってから前の iframe と入れ替える（古い `seq` の結果は捨てる）。
  iframe は文書の大きさ（いちばん広いページ）にして、親が `transform: scale()` で縮める（中のグラフは用紙の大きさのまま描かれる）。
  表示倍率は「ページ全体」（既定）・「ページ幅」・%。ページの移動は前後のボタン・PageUp/PageDown・←→。Esc でキャンセル。
- **コンソール・mock**: ページは出さず、同じ項目を `ui.form`（`showForm`）で尋ねる（`showIf` を `hidden` に）。mock の応答は `"form"` の列。
  テストの記録（`native.recording`）では、ネイティブの画面でも変えた値だけを `form` の応答として記録する（再生はフォームの経路）。
- テスト: `testdata/scripts/printpreview.test.js`（`print.xlsx`。変えた設定だけが入ること、キャンセル、シートの選択、余白・倍率・空のページなど、`toPdf` の `preview`）。
  画面は Playwright で `print-preview.html` に偽の `chrome.webview` を付けて `init` / `layout` を送って確かめた。

## 3.2 印刷（`book.print` / `sheet.print`、`xljs.printers`）

ダイアログを出さずにプリンターで印刷する。PDF と同じ見えないウィンドウ（`print.html`）でページを描き、`rendered` のあと
`ICoreWebView2_16::Print`（WebView2 112 以降。`ICoreWebView2PrintSettings2` でプリンター名・両面・部数・部単位・カラー・ページの範囲）を呼ぶ
（`webview::print_groups`）。`PdfJob::printer`（`PrinterJob`）があれば印刷、なければ PDF。

- **プリンター向けのレイアウト**（`PrintOptions::printer_layout`。prelude が付ける。スクリプトからは指定できない）: WebView2 の印刷は
  設定の余白（既定 1cm）で CSS の `@page` の余白を上書きするおそれがあり、そのときは余白の箱（`@top-left` など）のヘッダー/フッターも消える。
  そこで `@page sN{size:…;margin:0}` にし、各ページ（`section`）を用紙いっぱい（`--pw` × `--ph` − 1pt）にして余白を `padding`（偶数ページは `--mm`）に、
  ヘッダー/フッターを `section` の中の `.xljs-hf` の箱（`::before` の `content`。位置は `--hl` `--hr` `--ht` `--hb`、左右の入れ替えは `--hl2` `--hr2`）にする（`printer_rule`・`PRINTER_CSS`）。
  `&P` は `section` を数えるカウンター `xljs-page`、`&N` はページ数の文字列に置き換える。印刷の設定は余白 0・倍率 1・ヘッダー/フッターなし・背景あり。
  Chromium の PDF で、本体・ヘッダー・フッターの位置が PDF 出力と同じになることを確かめた（`printer_layout` の単体テストは構造のみ）。
- **用紙**: `xlsxPrintHtml` が `papers`（印刷するシートごとの用紙 mm と ページ数）を返し、prelude が同じ用紙の続くページを 1 つのグループにする。
  グループごとに `MediaSize` = custom、`PageWidth` / `PageHeight`（縦向きの寸法、インチ）、`Orientation` で 1 回ずつ `Print` する
  （用紙の違うシートは別の印刷ジョブ。両面・部単位はジョブごと）。
  ページの範囲は `PageRanges` ではなく、ページにグループのページだけを出させて（`{ type: "pages", group, from, to }` → `print-page.js` の `showPages` が
  ほかの `section` を `display:none` にし、`&P` のカウンターを `from - 1` から始め、出したグラフが描かれるのを待って `shown`）全部を印刷する。
  2 回目以降の `Print` で `PageRanges` の始まりが 4 ページ目以降だと、ジョブが 0 バイトのままスプールされず完了も返らないため
  （Microsoft Print to PDF・CubePDF とも。2026-11 に実機で確認）。偶数ページの余白は `data-even`（用紙のグループごとに数えたもの）で決まるので、ほかのページを隠しても変わらない。
- **プリンターの一覧**（`Ui::printers`、`src/ui/native/printers.rs`）: `EnumPrintersW`（ローカルと接続、レベル 4）と `GetDefaultPrinterW`、
  両面・カラーは `DeviceCapabilitiesW`（`DC_DUPLEX`・`DC_COLORDEVICE`）。mock とテストの再生は架空の 2 台（`mock::mock_printers`）。
- **プレビュー**: `print` の `preview` は `printPreview` の `"print"` のモード（既定の設定にプリンター・両面・部数を足す、ボタンは「印刷」）。
  両面・カラーの項目は `onlyFor: { printer: [対応するプリンター] }` で、対応しないプリンターを選ぶと無効になり、結果からも外す（`dropOff`）。
  `toPdf` の `preview`（`"pdf"` のモード）ではプリンターの設定を出さない。
- mock は `print("プリンター", n pages, …, duplex=…, copies=…, collate=…, color=…, [1-3 297x210, …])` をログに出す。テストの再生は `print` のイベント。
  テストは `testdata/scripts/print.test.js`。
- **実機で確かめたこと**（Microsoft Print to PDF・CubePDF）: `MediaSize` custom は縦向きの寸法＋向きで合う（A4 横・B5 縦）、用紙の違うグループが別のジョブで出る、
  本体・ヘッダー・フッターの位置とページ番号が PDF 出力と同じ、部数・白黒（グレースケール）が届く。
  **確かめていないこと**: 両面（両面に対応した仮想プリンターがない）、実物のプリンターの印刷できない領域と余白 0 の兼ね合い。
- 時間切れ（`PRINT_TIMEOUT`）のときは、プリンターのキューにジョブが残ることがある（`print.timeout` がキューを見るよう案内する）。

## 4. できないこと・違い

- SmartArt、図形と文字の 3-D（面取り・奥行き・`scene3d`）・内側の影・ぼかし・テーマの効果（`effectRef`）・グラデーション（図形は最初の色で塗る）・パターン、
  文字の変形は文字の形そのものは曲げず短冊ごとのアフィン変換（大きい字の曲がりは近似）、変形した文字は PDF の中で短冊の数だけ重なる（文字の検索・コピーで重複する）、
  画像のトリミング、EMF/WMF の画像、Web の画像（`IMAGE` 関数）、コメント、フォームのコントロール、ヘッダー/フッターの画像、先頭ページ・偶数ページ用のヘッダー。
- 塗りのパターン（単色以外は前景色で塗る）、グラデーションの塗り。
- 先頭ページ番号（`firstPageNumber`）、行列番号の印刷、簡易印刷、印刷の品質。白黒印刷は塗りと色を消すだけ（画像・図形はそのまま）。
- 印刷ダイアログでだけ選んだ拡大縮小（「シートを 1 ページに印刷」など。保存されないもの）は分からないので、Excel の PDF とページ数が違うことがある。
- Excel 2016 以降のグラフ（chartEx）やレーダーなど `sheet.charts` で描けないもの（枠の中に理由を書く）。
- 行の高さの自動調整（折り返しのある行で `ht` がないとき、Excel が開いたときに広げる分）は再現しない。

## 5. テスト

- `src/api/xlsx_geom.rs` の単体テスト: すべての既定の形が評価できること、形の path（`arcTo` の楕円・調整値）、文字の枠、塗りの濃淡、フリーフォームのガイド、すべての文字の変形が評価できること（平ら・アーチ・調整値）。
- `src/api/xlsx_table.rs` の単体テスト: テーブルスタイルの読み方、要素の罫線の当て方。
- `src/api/xlsx_cond/tests.rs`: `testdata/xlsx/conditional.xlsx`（`make_conditional.py`。openpyxl の出力に Excel 2010 の拡張のデータバー（負の値・軸）と `3Stars` を足す）で、強調表示（停止・相対参照の数式・重複・文字列）、カラースケール、データバー、アイコンセット、期間、HTML。
- `src/api/xlsx_print.rs` の単体テスト: 範囲の読み方、ページの分け方（印刷タイトル・改ページ・非表示）、ヘッダー/フッター、表示形式の色、ページ設定の XML。
- `src/api/xlsx_print.rs` の単体テスト `border_lines`: 罫線の重さの比べ方、線・二重線・破線の書き方。
- `testdata/scripts/pdf.test.js`（mock）: 新しいブックの HTML（値・結合・罫線（SVG の線）・塗り・負数の赤）、オプション、検査、複数シート、
  `charts.xlsx` のグラフの枠、`testdata/xlsx/print.xlsx`（`make_print.py` で openpyxl から生成。横向き・横 1 ページ・印刷タイトル・
  ヘッダー/フッター・改ページ・画像・グラフ・非表示の列とシート・印刷範囲・B5）、`testdata/xlsx/tables.xlsx`（`make_tables.py`。
  独自のテーブルスタイル（styles.xml に足す）と TableStyleMedium2・白黒印刷・OFFSET の印刷範囲・列全体の印刷範囲）、`testdata/xlsx/shapes.xlsx`（`make_shapes.py`。openpyxl の
  出力に描画の XML を足す: テキストボックス・既定のスタイルの図形・吹き出し・矢印・星（回転）・矢印付きの線・破線のカギ線コネクタ（反転）・グループ・
  フリーフォーム・縦書き・非表示・印刷しない図形・雲形吹き出し・スマイル・ハート、ワードアート（影・グラデーション＋輪郭＋光彩のアーチ・輪郭だけの 2 行の波・反射の膨らみ・円・大文字と文字間隔に図形の影））、`testdata/xlsx/cellimages.xlsx`（`make_cellimages.py`。openpyxl の出力にリッチ値の部品を足す:
  セルに配置した PNG・JPEG・代替テキスト付き、同じシートと別のシートから参照するセル（`vm` あり・なし）。スクリプトで書いた `=`・`IF`・`XLOOKUP`・`VLOOKUP`・`INDEX` の参照、
  `&` で `#VALUE!`、画像に値を上書きしたとき、保存して開き直しても画像が残ること）、mock のログ。デバッガ経由でも同じ（`scripts_in_debugger`）。
- `src/api/xlsx_cellimage.rs` の単体テスト: セルの `vm` から画像を引くこと（行の挿入で一緒に動く、値を書くと外れる）、保存したファイルにリッチ値の部品が元のまま入ること、画像のセルがなくなれば書かないこと。
- `src/ui/native/tests.rs`（Windows CI）: `print_page_draws_charts_hidden`（見えないウィンドウで縦棒とガントが描けること）、
  `print_pdf_writes_a_file`（`print.xlsx` を PDF にしてファイルができること）。
- 見た目は Playwright の Chromium で `print.html` に偽の `chrome.webview` を付けて `init` を送り、`page.pdf({ preferCSSPageSize: true })` →
  `pdftoppm` で画像にして確認した（`Page.printToPDF` と同じ経路）。

## 6. 残務

なし（umya-spreadsheet の制約で残っていたものはフォークで直した。7 章）。

## 7. umya-spreadsheet のフォーク

`Cargo.toml` の `[patch.crates-io]` で、本家（MathNya/umya-spreadsheet）の代わりにフォーク
`cool0707/umya-spreadsheet` の `excel-compat` ブランチを使う。ブランチは本家の 3.1.1 のタグから切ってある（フォークの master は本家の
4.0 系）。

フォークを直すときも `Cargo.toml` は書き換えない。フォークに xljs と同じ名前のブランチを push すれば、CI の `tools/umya-branch.sh` が
`.cargo/config.toml` に `[patch.crates-io]` を足してそのブランチで組み立てる（設定ファイルの `[patch]` は `Cargo.toml` のものより優先される）。
同じ名前のブランチがなければ `Cargo.lock` が固定したコミットのままで、それが `excel-compat` の先頭より古ければ警告を出す。
フォークのブランチを `excel-compat` に取り込んだら、xljs 側で `cargo update -p umya-spreadsheet` して `Cargo.lock` を進める。

直したもの:

- **縮小して全体を表示（`shrinkToFit`）**: 本家の `Alignment` にこの属性がなく、読み込みでも保存でも落ちていた。
  `shrink_to_fit` / `set_shrink_to_fit` を足して styles.xml と読み書きする。xljs では `style({ alignment: { shrink: true } })` で設定し、
  `getStyle().alignment.shrink` で読める。開いて保存したブックにも残る。
- **インライン文字列のリッチテキスト**: 本家は `t="inlineStr"` の `<is>` の中の `<t>` を読むたびに値を上書きするので、最後のラン
  （`<rPh>` があればふりがな）しか残らなかった。`<is>` を共有文字列の `<si>` と同じに読む。

- **条件付き書式**: 本家はアイコンセットの種類（`iconSet` 属性）を持たず `<dataBar>` として書き、データバーの `minLength` などの属性、
  `cfvo` の `gte`、規則の `text`・2 つ目の数式（範囲内）、差分の書式の表示形式（`numFmt`）も落としていた。Excel 2010 の拡張（`x14:conditionalFormattings`）も読まなかった。
  これらを読み書きし（拡張は `Worksheet::conditional_formatting_2010_collection`。規則・データバー・アイコンセット・`cfvo` の構造体は 2007 形式と共用で、
  要素名は接頭辞を除いて読む）、空の `<dxf/>` で後ろの `dxfId` がずれるのも直した。

- **配列数式とスピルする数式**: 本家は配列数式（`t="array"`）を読むが普通の数式として書き、セルのメタデータ（`cm` と `xl/metadata.xml`）は読んだ番号を持つだけで
  書かなかった。`CellFormula` が `t="array"` と範囲（`ref`）を書き、スピルする数式かどうか（`dynamic_array`。`cm` が指す動的配列のプロパティ `XLDAPR` から読む）を持つ。
  保存では、スピルする数式のセルに `cm` を付け、その分だけの `xl/metadata.xml`（リレーションと Content_Types も）を書く。行・列の挿入と削除で `ref` も動く。
  `docs/design/calc-design.md` 4 章。
- **セルに置いた画像などのリッチ値**: 本家はセルの値のメタデータ（`vm`）を読まず、`xl/richData` の部品も書かないので、セルに配置した画像は保存すると
  `#VALUE!` だけになった。セルが `vm` を読んだ番号のまま持ち（`Cell::value_meta_index` / `set_value_meta_index`）、値がエラーで、その番号がリッチ値を指す間は書く。
  `xl/metadata.xml` の `valueMetadata` は読んだ並びのまま（番号が変わらない）動的配列のプロパティと一緒に書く。リッチ値の部品（ブックのリレーションの種類が
  `rd…`・`richValue…`・`richStyles` のもの）と、そこから参照される部品（`xl/media` の画像）は中身を見ずに持っておき、`vm` を書いたセルがあるとき（と、
  読んだままの形で書くシートがあるとき）にリレーション・Content_Types と一緒に書き戻す。シートの画像と名前が重なった画像は別の名前で書く。
  リッチ値の中身は見ないので、連結データ型（株価・地理）や `IMAGE` の結果も同じ仕組みで残る。`tests/cell_images.rs`。
- **グラフ**: 本家はグラフを自分のモデル（`ChartSpace`）に読んで書き直すので、`oneCellAnchor` に置いたグラフ（openpyxl などが作る）と
  新しい種類のグラフ（chartEx。`mc:AlternateContent` の中）は保存で消え、モデルにあるグラフも、モデルにないもの（グラフのスタイル `style1.xml`・
  色 `colors1.xml`、拡張など）が落ちていた。描画の中でグラフを持つアンカー（`twoCellAnchor`・`oneCellAnchor`・`absoluteAnchor`、それを囲む
  `mc:AlternateContent`）は、読んだ XML のまま（描画のルートで宣言された名前空間を自分で宣言し直して）持ち（`KeptAnchor`）、グラフの部品と
  そこから参照される部品（スタイル・色・データの埋め込みブック・グラフの上の図形の描画とその画像）は中身を見ずに持っておき（`kept_parts.rs`）、
  保存でリレーション・Content_Types と一緒に書き戻す。名前が使われていれば次の空いた番号にする。`twoCellAnchor` のグラフは今までどおりモデルにも読み、
  モデルが読んだときのままなら読んだものを、モデルで変えたらモデルから書く（`Chart::kept_unchanged`。`Debug` の表現のハッシュで比べる）。
  行・列の挿入と削除では、アンカーの位置と系列の参照（`c:f`・`cx:f`。絶対参照も動かす）を動かす。`tests/chart_fidelity.rs`。
- **入力規則**: 2 つの数式を持つ規則（`between`・`notBetween`）は `formula1`・`formula2` が両方落ちて条件のない規則になっていた（片方だけのときしか
  中身を書かなかった）。`errorStyle`（停止・注意・情報）、`imeMode`、`showDropDown` を読み書きする。
- **計算の設定（`calcPr`）**: 本家の 4.0 系のコミットを取り込んだ（`Workbook::calculation_properties`。反復計算・手動計算・`fullCalcOnLoad` が保存で残る）。
  データテーブルの数式の `ref` を書く修正も同じく取り込んだ。
- **Excel で開けなくなるもの（`docs/design/excel-verify.md`）**: Excel 2021 で開いて見つけた。`tests/excel_compat.rs`。
  - 文字列の制御文字（U+0001 など）をそのまま書き、ブック全体が Excel で開けなかった。Excel と同じく `_xHHHH_` で書き、`_xHHHH_` の形の文字列は
    `_x005F_` で書く。読むときは戻す。ファイルの中の生の CR LF は（XML の決まりどおり）LF として読む。
  - 空の `<a:ln w="0"/>` を開始タグとして読み、次の図形の `</a:ln>` まで読み進めていた（後ろの図形が消え、`a:ln` に塗りが 2 つ入って Excel が開けない）。
  - 空の `<a:prstGeom prst="rect"/>`（openpyxl などが書く）を読まず、画像に `prst=""` を書いて Excel が開けなかった。
  - ワードアートの文字の変形（`a:prstTxWarp`）・自動調整（`a:noAutofit`・`a:normAutofit`）・縦書き（`bodyPr` の `vert` ほか読まない属性）、
    テキストボックスの印（`cNvSpPr` の `txBox`・`a:spLocks`）、フリーフォームの形（`a:custGeom`。読んだ XML のまま）が保存で落ちていた。
  - 定義のない組み込みの表示形式（ID 5〜8・23〜26・41〜43 など。日本語版 Excel が書く）を表示形式なしとして読み、保存でそれらのスタイルが 1 つにまとめられていた。
    ID だけの組み込みの表示形式として持つ。
- **セル以外の機能（`docs/design/save-fidelity.md`）**: Excel 2021 に機能を入れたブックを作らせて見つけた。`tests/excel_features.rs`。
  - 縦横比を固定しない画像を `<a:picLocks noChangeAspect="2"/>`（真偽値でない値）で書き、Excel で開けなかった（本家から）。
  - 外部リンク・スライサーとそのキャッシュ・タイムラインとそのキャッシュ・ピボットテーブルとキャッシュ・フォームコントロールのプロパティ・カスタム XML の部品が
    落ちていた（外部リンクとタイムラインは Excel で開けなくなった）。読んだまま持って書き戻す（`KeptRelationships`。7 章の上のグラフと同じ仕組み）。
    ピボットはモデルに読まない（xljs は使わない）。ピボットキャッシュはブックとピボットテーブルの両方から参照されるので 1 度だけ書く。
  - シートの読まない `ext`（スパークライン・スライサーとタイムラインの一覧）と `controls`、`styles.xml` の `tableStyles`（独自のテーブルスタイル）と `extLst`
    （スライサー・タイムラインのスタイル）を読んだまま書き戻す。描画のスライサー・タイムラインの枠もグラフと同じく `KeptAnchor` に持つ。
  - 行・列のグループ（`outlineLevel`・`collapsed`）、ふりがな（共有文字列の `rPh`・`phoneticPr`、セルの `ph`）、`pageSetup` のモデルにない属性
    （`blackAndWhite`・`firstPageNumber` など）、差分書式の `<b val="0"/>`・`<i val="0"/>` が落ちていた。
- **シートの名前の変更**: 本家の `Workbook::set_sheet_name` は名前を変えるだけで、そのシートを参照する数式などは古い名前のままだった
  （シートの名前の定義だけは、どのシートを指していてもそのシートに書き換えていた）。Excel と同じく、セルの数式・名前の定義・入力規則（2010 の拡張も）・
  条件付き書式（`cfvo` も）・テーブルの計算列・ピボットテーブルの元データと、読んだまま持っている部品（グラフの `c:f`・`cx:f`、スパークラインなど
  シートの `ext` の `xm:f`、フォームコントロールの `fmlaLink`・`fmlaRange`、ピボットキャッシュの `worksheetSource` の `sheet`）の参照を書き換える
  （`helper/sheet_rename.rs`。名前は大文字小文字を区別せずに比べ、必要なら引用符で囲む。外部のブックへの参照・文字列の中・ハイパーリンクは変えない）。
  2010 の入力規則の数式（`xm:f`）は読んだ文字のまま持つ（アドレスとして読み直すと `OFFSET(...)` などが崩れた）。`tests/rename_sheet.rs`。
- **スレッド形式のコメント**: 作成者（`xl/persons/person.xml` を読まず、空の一覧と `personId` がすべて 0 の ID を書いていた）・返信のつながり（`parentId`）・
  解決済み（`done`）・メンション（`mentions`）と、代わりのメモの `xr:uid` が落ちていた。新しいブックに書いたときは Content_Types にコメントと作成者の部品がなかった。
  行・列の挿入と削除でコメントの位置が動かなかった。`Workbook::persons`、`ThreadedComment` の `person_id`・`parent_id`・`done`・`mentions`、`Comment::uid`。
  `tests/threaded_comments.rs`。スクリプトの `cell.comment` などは `src/api/xlsx_comment.rs`（代わりのメモを書き換えのたびに作り直す）。

本家の版を上げるときは、`excel-compat` ブランチを新しいタグへ載せ替える（本家に取り込まれた修正は落とす）。どちらも取り込まれたら
`[patch.crates-io]` を消して本家に戻す。
