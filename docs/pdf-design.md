# PDF 出力（book.toPdf / sheet.toPdf / xljs.htmlToPdf）

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
  `blackAndWhite` `centerHorizontally` `charts: false` `title` `skipBlankPages`。省略した項目はシートのページ設定に従う。
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

印刷範囲がなければ使用範囲（値・塗り・罫線のあるセル、値のある結合セル、テーブル、画像・グラフ）。先頭の空の行・列は印刷しない。

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
- 書式はセルの CSS をまとめてクラス（`.k0` …）にする。フォント（游ゴシック → `Yu Gothic` などの別名と代わりのフォント）、太字・斜体・下線・取り消し線、
  文字の色（テーマの色は umya の `argb_with_theme`）、表示形式の色（`[Red]` `[赤]` など）、塗り（単色）、罫線（細線 = 0.75pt）、斜線（グラデーション）、
  横・縦の配置、折り返し、インデント、回転（縦書きは `writing-mode`）、リッチテキスト（ランごとの `<span>`）。
- リッチテキスト（1 つのセルで一部だけフォント・色・大きさが違う）: ランは 1 つの `<span>` で包む（セルの `<div>` は縦方向の flex なので、
  包まないとランごとに行が分かれて最後のランしか見えない）。`rPr` のないランはセルのフォントのまま、`rPr` のあるランはそれがフォントのすべて
  （Excel はすべての属性を書く）なので、書いていない太字・斜体・下線は「なし」としてセルの書式を打ち消す。色がなければセルの色、自動は黒。
  インライン文字列（`t="inlineStr"`。openpyxl などが書く）のリッチテキストは umya が最後のランの文字しか残さないので、開くときに
  シートの XML から読み直してセルに入れる（`xlsx.rs` の `inline_rich_text`）。
- 縮小して全体を表示（`shrinkToFit`）: umya はこの属性を読まない（保存でも落ちる）ので、開くときに styles.xml の `cellXfs` と
  シートの XML のセルの `s` から該当するセルを集めておく（`RawPrint::shrink`）。セルの文字を `<span class="xljs-shrink" style="zoom:…">` で包み、
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
| 図形（`sp`）の形 | SVG の path。アンカーの実寸（pt）を viewBox にする。よく使う既定の形だけを手で書いている: 四角・角丸四角・楕円・三角・直角三角・ひし形・平行四辺形・台形・五角形（ホームベース）・山形・六角形・八角形・十字・星 5・上下左右の矢印・左右/上下両方向矢印・吹き出し（四角・角丸・楕円）・かっこ・中かっこ・額縁・円柱・フローチャートの主な記号。調整値（`avLst` の `adj`）も使う。知らない形は四角 |
| フリーフォーム（`custGeom`） | `pathLst` の `moveTo` `lnTo` `cubicBezTo` `quadBezTo` `close`（`arcTo` は使わない） |
| 線・コネクタ（`cxnSp`） | 直線・カギ線（`bentConnector2`〜`5`、中間の調整値）・曲線。矢じり（`headEnd` / `tailEnd` の triangle・stealth・arrow・oval・diamond、大きさ sm/med/lg）は SVG の marker。破線（`prstDash`） |
| 塗り・線 | `spPr` の `solidFill`（グラデーションは最初の色）・`noFill`・`alpha`、`ln` の幅・色。指定がなければ図形のスタイル（`fillRef` / `lnRef` のテーマの色。線の幅は idx から 0.5/1/1.5pt） |
| 文字（`txBody`） | 段落ごとの揃え、ランの大きさ・太字・斜体・下線・取り消し線・色・フォント（`ea` / `latin`。`+mn-ea` などはブックの既定のフォント）、改行、箇条書きの文字。`bodyPr` の内側の余白・上下の位置・折り返しなし・縦書き。色の既定は `fontRef` |
| 回転・反転 | 回転は外側の `div` の `transform`（文字も一緒に回る）、反転は SVG の形だけ |
| グループ（`grpSp`） | 子の位置を子の座標（`chOff` / `chExt`）からグループの枠の割合にして置く（入れ子も） |
| 画像（`pic`） | `blip` の画像を data URL で（PNG・JPEG・GIF・BMP・SVG。EMF/WMF は描けないので出さない）。SVG の画像は `blip` が画面の大きさの PNG の代替画像なので、拡張（`extLst` の `asvg:svgBlip`）の SVG を使う（PDF でも線のまま）。Excel と同じく枠いっぱいに伸ばす（`preserveAspectRatio` がなければ `none`、`viewBox` がなければ `width` / `height` から） |
| 印刷しないもの | 非表示（`cNvPr hidden`）と「オブジェクトを印刷する」をオフにしたもの（`clientData fPrintsWithSheet="0"`） |
- 画面で開いたとき（`@media screen`）はページを灰色の背景の上に用紙の大きさで並べる（`toHtml` の確認用）。

### セルに配置した画像（`src/api/xlsx_cellimage.rs`）

Excel の「セルに配置」の画像はリッチ値で、umya-spreadsheet は読まない（セルはエラー `#VALUE!` になる）。ブックを開いたときに読んでおく（`Book::cell_images`）。

- セル: `<c r="B2" t="e" vm="1"><v>#VALUE!</v></c>`。`vm`（1 から）→ `xl/metadata.xml` の `valueMetadata` の `bk` → `rc t`（`XLRICHVALUE` の `metadataType`）`v`
  → `futureMetadata name="XLRICHVALUE"` の `bk` → `xlrd:rvb i` → `xl/richData/rdrichvalue.xml` の `rv`。`rv` の値はその構造（`rdrichvaluestructure.xml` の `s`）のキーの順で、
  `_rvRel:LocalImageIdentifier` が `richValueRel.xml` の `rel` の位置（→ そのリレーションの `xl/media/*`）、`Text` が代替テキスト。
  部品はブックのリレーションの種類（`sheetMetadata` `rdRichValue` `rdRichValueStructure` `richValueRel`）から探す（名前の大文字・小文字は問わない）。
- `IMAGE("https://…")`（構造 `_webImage`）は画像がファイルにないので空欄にする（`#VALUE!` は出さない）。EMF/WMF も空欄。
- 画像として扱うのは、開いたときに `vm` があり、いまの値がエラーのままのセル（スクリプトが値を書いたら画像ではない）。位置で持っているので、行・列の挿入・削除はずれる（`shrink` と同じ）。
- **参照するセル**: Excel は `=B2` のセルにも `vm` を付けて保存するので、それはそのまま画像（保存された結果を使う。ほかの数式と同じ）。
  `vm` のない数式（スクリプトで書いた数式、`vm` を書かないツールが保存した数式）は `xlsx_calc` で評価し、結果が画像なら画像（`xlsx_calc::cell_image`）。
  計算の値 `V::Image` は参照・`IF`・`IFERROR`・`INDEX`・`VLOOKUP` / `HLOOKUP`・`XLOOKUP`（完全一致）をそのまま通り、数値・文字列として使うと `#VALUE!`（`=B2&""` など。Excel と同じ）。
- 保存（`book.save`）では umya がリッチ値の部品を書かないので画像は消える（セルは `#VALUE!`）。

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

## 4. できないこと・違い

- 条件付き書式、SmartArt、ワードアートの効果、図形の影・3-D・グラデーション（最初の色で塗る）・パターン、よく使うもの以外の既定の図形の形（四角になる）、
  画像のトリミング、EMF/WMF の画像、Web の画像（`IMAGE` 関数）、コメント、フォームのコントロール、ヘッダー/フッターの画像、先頭ページ・偶数ページ用のヘッダー。
- 塗りのパターン（単色以外は前景色で塗る）、グラデーションの塗り。
- 先頭ページ番号（`firstPageNumber`）、行列番号の印刷、簡易印刷、印刷の品質。白黒印刷は塗りと色を消すだけ（画像・図形はそのまま）。
- 印刷ダイアログでだけ選んだ拡大縮小（「シートを 1 ページに印刷」など。保存されないもの）は分からないので、Excel の PDF とページ数が違うことがある。
- Excel 2016 以降のグラフ（chartEx）やレーダーなど `sheet.charts` で描けないもの（枠の中に理由を書く）。
- 行の高さの自動調整（折り返しのある行で `ht` がないとき、Excel が開いたときに広げる分）は再現しない。

## 5. テスト

- `src/api/xlsx_shape.rs` の単体テスト: 既定の形の path。
- `src/api/xlsx_table.rs` の単体テスト: テーブルスタイルの読み方、要素の罫線の当て方。
- `src/api/xlsx_print.rs` の単体テスト: 範囲の読み方、ページの分け方（印刷タイトル・改ページ・非表示）、ヘッダー/フッター、表示形式の色、ページ設定の XML。
- `testdata/scripts/pdf.test.js`（mock）: 新しいブックの HTML（値・結合・罫線・塗り・負数の赤）、オプション、検査、複数シート、
  `charts.xlsx` のグラフの枠、`testdata/xlsx/print.xlsx`（`make_print.py` で openpyxl から生成。横向き・横 1 ページ・印刷タイトル・
  ヘッダー/フッター・改ページ・画像・グラフ・非表示の列とシート・印刷範囲・B5）、`testdata/xlsx/tables.xlsx`（`make_tables.py`。
  独自のテーブルスタイル（styles.xml に足す）と TableStyleMedium2・白黒印刷・OFFSET の印刷範囲・列全体の印刷範囲）、`testdata/xlsx/shapes.xlsx`（`make_shapes.py`。openpyxl の
  出力に描画の XML を足す: テキストボックス・既定のスタイルの図形・吹き出し・矢印・星（回転）・矢印付きの線・破線のカギ線コネクタ（反転）・グループ・
  フリーフォーム・縦書き・非表示・印刷しない図形）、`testdata/xlsx/cellimages.xlsx`（`make_cellimages.py`。openpyxl の出力にリッチ値の部品を足す:
  セルに配置した PNG・JPEG・代替テキスト付き、同じシートと別のシートから参照するセル（`vm` あり・なし）。スクリプトで書いた `=`・`IF`・`XLOOKUP`・`VLOOKUP`・`INDEX` の参照、
  `&` で `#VALUE!`、画像に値を上書きしたとき）、mock のログ。デバッガ経由でも同じ（`scripts_in_debugger`）。
- `src/ui/native/tests.rs`（Windows CI）: `print_page_draws_charts_hidden`（見えないウィンドウで縦棒とガントが描けること）、
  `print_pdf_writes_a_file`（`print.xlsx` を PDF にしてファイルができること）。
- 見た目は Playwright の Chromium で `print.html` に偽の `chrome.webview` を付けて `init` を送り、`page.pdf({ preferCSSPageSize: true })` →
  `pdftoppm` で画像にして確認した（`Page.printToPDF` と同じ経路）。

## 6. 残務

umya-spreadsheet（3.1）の制約で残っているもの。いずれライブラリをフォークするなどして対応する。

- **縮小して全体を表示（`shrinkToFit`）の保持と設定**: umya の `Alignment` にこの属性がなく、読み込みでも保存でも落ちる。
  - 印刷は開くときに styles.xml とシートの XML から集めた `RawPrint::shrink`（セルの位置）で対応しているが、位置で持っているので
    スクリプトで行・列の挿入や削除をすると印刷の縮小がずれる。
  - xljs で開いて保存したブックからは縮小の指定が消える。
  - `types/xljs.d.ts` の `AlignmentSpec.shrink` は型にあるだけで、`style({ alignment: { shrink: true } })` は効かない
    （`xlsx_style.rs` の `AlignSpec` に項目がない）。`getStyle` も返さない。
  - フォーク後は `Alignment` に `shrink_to_fit` を足して読み書きし、`RawPrint::shrink` をやめてセルのスタイルから読む。
- **インライン文字列のリッチテキスト**: umya は `t="inlineStr"` の `<r>` の文字を最後のランしか残さない。開くときに
  `xlsx.rs` の `inline_rich_text` でシートの XML から読み直して補っている。フォーク後は umya 側で直してこの処理を消す。
