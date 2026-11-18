# リリース前の API の見直し

機能を足し続けるうちに、当初の「AI が迷わずに書ける API」からずれてきた箇所を洗い出し、リリース前に直す。
後方互換は考えない（直したら `types/xljs.d.ts`・`docs/site`・`docs/ai`・`examples`・テストをまとめて直す）。

この文書は作業用のメモ。項目ごとに方針を決め、直したら「状態」を更新する。

## 判断の基準

AI（と利用者）が迷うのは、主に次の 5 つ。直すときもこの基準で決める。

1. **1 つのことに書き方が 2 つ以上ある**: AI は両方を混ぜて書き、片方にしかない機能をもう片方で使おうとする。
   別名は、残すとしても d.ts とドキュメントには 1 つしか書かない（「別名の方針」を参照）。
2. **同じ名前のキーが場所によって別の意味を持つ**: `height` が px だったり行数だったりする。
3. **同じ種類のものが違う形をしている**: ボタンの書き方が 3 通り、フォームの処理の第 2 引数の型が 2 種類、キャンセルしたときの戻り値が `null` / `[]` / `false`。
4. **型で誤りを見つけられない**: どの type の項目にもすべてのキーが書けるので、tsc も開発モードの補完も誤りを指摘できない。
5. **暗黙の切り替え**: あるキーがあるかないかで動きが変わる（`onSubmit` の有無でフォームが閉じるかどうかが変わる）。

Web 標準に合わせたもの（File System Access API・`fetch`・`URL`・`TextDecoder`・`Notification` など）は、AI が知っている形なのでそのまま残す。

## 優先度

- **A**: AI が実際に誤りやすく、利用者の書くスクリプトの形が大きく変わるもの。リリース前に必ず直す。
- **B**: 不揃いだが、誤っても動くか、すぐに気づけるもの。リリース前に直したい。
- **C**: 小さな不揃い。余裕があれば直す。

## 一覧

| # | 優先 | 対象 | 要旨 | 状態 |
|---|---|---|---|---|
| A1 | A | `ui.form` | 送信ボタン（`submit`・`submitIcon`・`onSubmit`）を `buttons` に統一 | 完了 |
| A2 | A | フォームの第 2 引数 | `FormContext` / `FormChangeContext` を 1 つに。対になった重複メソッドの整理 | 完了 |
| A3 | A | `FormField` | type ごとの型に分け、意味の違う同名のキーをなくす | 完了 |
| A4 | A | 全体 | 別名の方針（d.ts・ドキュメントに出すのは 1 つ） | 完了 |
| B1 | B | グローバル | `workbooks` の名前、JSON は `fs`・CSV は `csv` の不揃い | 完了 |
| B2 | B | `fs` / ハンドル | `fs.list` と `dir.list` の戻り値の違い、`walk` / `glob` の重複 | 完了 |
| B3 | B | 全体 | キャンセルしたときの戻り値をそろえる | 完了 |
| B4 | B | 進捗 | `ui.progress` と `form.progress` の形の違い、`ui.cancelled` | 完了 |
| B5 | B | Excel の書式 | `CellStyle` の別名、`Range` と `sheet` のメソッドの重複 | 完了 |
| B6 | B | Excel の読み書き | `getValue` / `setValue`、`toObjects` / `rows` / `toArrays` / `Range.values` | 完了 |
| B7 | B | 印刷 | `preview` の渡し方と戻り値の違い | 完了 |
| B8 | B | ウィンドウ | ウィンドウのオプションの共通化 | 完了 |
| B9 | B | 全体 | 読み書きの対がそろっていないもの（書けるのに読めない・2 通りで書ける） | 完了 |
| C1 | C | `shell` | `open` / `openPath` / `openExternal` | 完了 |
| C2 | C | Excel のメモ | `cell.note`（文字列）と `cell.comment`（オブジェクト） | 変えない |
| C3 | C | `ChartSpec` | ガントチャートのキーが同じ型に混在 | 完了 |
| C4 | C | `app` | 画面とのやりとりの手段が多い | 完了 |
| C5 | C | `ui.table` | 表示だけの表と選ぶ表、`TableView` に `update` がない | 完了 |
| C6 | C | `shell.writeShortcutLink` | 位置で渡す `operation` | 完了 |
| C7 | C | メタデータ | `@size 900x600` と `width` / `height` | 変えない |
| D1 | B | Excel の行と列 | 行・列のメソッドを `sheet.rows(...)` / `sheet.columns(...)` にまとめる。表を読むのは `toRowObjects()` | 完了 |

---

## A1. フォームのボタンを `buttons` に統一する

**現状**

- 送信ボタンは `submit`（文字・`false`）・`submitIcon`・`onSubmit` の 3 つのキーに分かれている。ほかのボタンは `buttons: [{ label, icon, onClick, ... }]`。
- 送信ボタンにだけ `confirm`・`tooltip`・`variant`・`menu`・`validate: false` がない。
- 閉じるかどうか: 送信ボタンは「`onSubmit` がなければ閉じる」、ほかのボタンは「`close: true` なら閉じる」（`prelude.js` の `case "submit"` の中）。
- アプリでは `submit: false` が要る（`docs/ai` に「操作が複数あるなら `submit: false`」と書いている）。
- `ui.forms` の説明にも「送信ボタン（onSubmit なし）は閉じる」がある。

**案**

- `submit` / `submitIcon` / `onSubmit` をなくし、ボタンは `buttons` だけにする。
- `buttons` を省くと「OK」ボタン 1 つ（押すと検証して閉じ、値を返す）。いまの「onSubmit も buttons もない」フォームと同じ。
- `onClick` のないボタンは、検証して閉じ、値を返す。`onClick` に `close: true` を付けた形は残す。
- Enter で押されるのは `variant: "primary"` のボタン（1 つまで。2 つ以上はエラー）。primary がなければ Enter では押さない（`submit: false` は不要になる）。
- 戻り値は、最後に検証を通って押されたボタンのときの値（いまと同じ）。

```js
// 今
ui.form({ fields, submit: "集計", submitIcon: "play", onSubmit: async (v, form) => { ... } })
// 案
ui.form({ fields, buttons: [{ label: "集計", icon: "play", variant: "primary", onClick: async (v, form) => { ... } }] })
```

**決定**

- ボタンが 1 つで primary がなくても、それを primary とみなさない（暗黙の規則を増やさない）。
- ツールバーのボタン（`FormToolbarButton`）は A2 で第 2 引数の型をそろえる。

**影響**: `prelude.js`（ウィンドウ・コンソール・mock の 3 経路）、`web/form.js`（Enter）、`ui.forms`、記録（`recordForm` の `$button`）と `case.json`、`docs/ai` の各指示・スキル、`examples` の 5 本、`testdata/scripts` のフォームのテスト。

## A2. フォームの処理の第 2 引数を 1 つの型にする

引数の名前は、ドキュメント・サンプル・`docs/ai` の例で `ctx` から `form` に変えた（`form.log(...)`・`form.setValue(...)` と読める。済み）。

**現状**

- `onSubmit`・ボタンの `onClick` は `FormContext`（進捗・ログ・`table`・`chart`・`setValue` など）。
- 項目の `onChange`・ツールバーのボタンの `onClick` は `FormChangeContext`（`setValue`・`setDisabled` など・`table`。`chart` がない）。
- 同じ「ボタンの onClick」でも、置き場所（`buttons` かツールバーか）で第 2 引数の型が違う。
- 対になった重複: `setDisabled(name, bool)` と `setEnabled(name, bool)`、`setHidden(name, bool)` と `setVisible(name, bool)`。どちらも引数で逆にもできるので、4 つのメソッドで 2 つのことをしている。
- 項目を変える手段がばらばら: 値は `setValue`、表の行は `form.table(name, rows, columns)`、グラフは `form.chart(name, spec)`。表の `setValue` は「選択する行」で、行の入れ替えは `table`。

**案**

- 第 2 引数は `FormContext` 1 つにする。どこから呼ばれても同じメソッドを持つ。
  進捗・ログが意味を持たない場面（`onChange`・ツールバー）では、`progress` は何もしない、`log` はフォームの下に出す、など動きを決める（エラーにはしない）。
- `setEnabled` / `setVisible` をなくし、`setDisabled(name, bool)` / `setHidden(name, bool)` だけにする（項目のキー `disabled` / `hidden` と同じ言葉）。
- 表とグラフの更新の名前をそろえる。候補:
  - そのまま `form.table(name, rows)` / `form.chart(name, spec)` を残す（いまの名前は短く、AI も書けている）
  - `form.update(name, { rows, columns })` / `form.update(name, { chart })` のように項目のキーで書く（項目の定義と同じ言葉になる）

**決定**: 表・グラフの更新は `form.table` / `form.chart` のまま残す。

## A3. `FormField` を type ごとの型に分ける

**現状**

`FormField` は 1 つの平たい型で、どの type の項目にもすべてのキーが書ける。同じ名前で意味が違うキーがある。

| キー | 意味 |
|---|---|
| `height` | table は行数、chart は 1 行 25px として数えた行数、image は px |
| `width` | image は px（ほかは使わない） |
| `rows` | textarea / files は行数、table は表示する行のデータ |
| `variant` | note の種類（info / warning など）。ボタンの `variant`（primary など）とは別物 |
| `default` | 型により、値・選択する行（table）・本文（note）・画像（image） |
| `multiple` | segmented だけ（file / files は type で分けている） |

**案**

- d.ts を type ごとの型の共用体にする（`FormTextField | FormNumberField | FormTableField | ...`。共通のキーは基底の型）。
  type に合わないキーは tsc と開発モードの補完でエラーになり、AI が書いたものの誤りを見つけられる。
  実行時も、type に合わないキーは警告かエラーにする（いまは無視される）。
- 意味の違う同名のキーを分ける:
  - table のデータは `rows` のまま（`ui.table(rows)` と同じ）、textarea / files の行数は `lines` にする
  - table・chart の高さ: どちらも「行数」か「px」のどちらかにそろえる（chart の「1 行 25px」は分かりにくい）
  - note の `variant` → `kind`（ボタンの `variant` と分ける）か、ボタンを `style` にする
  - note・image の `default` → note は `text`、image は `src`（値ではないので `default` と呼ばない）
- 値の型の説明（いまは `FormField` のコメントに長く書いてある）は、type ごとの型のコメントに移す。

**決定**: 上の名前（`lines` / `kind` / `text` / `src`）にする。`height` はどの項目でも px にする（AI は `height: 300` を px と読む。表の既定は今と同じ 8 行分）。

**影響**: d.ts が大きく変わる。`docs/site/api/form.md`。`tools/docs/build.mjs` のコード例の型検査で、既存の例の誤りが見つかるはず。

## A4. 別名の方針

**現状**

AI が書きそうな名前を受けるための別名と、機能の重複が混ざっている。d.ts とドキュメントに両方が載っているため、AI はどちらが正か分からず、混ぜて書く。

| 別名・重複 | 備考 |
|---|---|
| `sheet.rows()` = `sheet.toObjects()` | `prelude.js` で `rows(opts) { return this.toObjects(opts) }` |
| `fs.move` = `fs.rename` | 同じ native |
| `shell.showInFolder` = `shell.showItemInFolder` | |
| `exit()` = `xljs.exit()` | |
| `ui.notify` と `Notification` | `Notification` は Web 標準の形 |
| `ui.error` と `alert` | 見た目（アイコン）が違うだけ |
| `CellStyle` の `fill` = `background`、`numberFormat` = `format`、`font.bold` = `bold` など | B5 |
| `WalkOptions` の `glob` = `include` | |
| `PdfTextPosition` の英語・日本語（`"bottom-center"` = `"下中央"` = `"bottom"`） | |
| `setDisabled` / `setEnabled`、`setHidden` / `setVisible` | A2 |

**案**

- **AI が推測で書きがちな別名は、実行時には受け付けるが、d.ts とドキュメントには載せない**（AI には 1 つの書き方だけを教え、推測で書かれたものは動くようにする）。
  - 例: `sheet.rows()`、`fs.move`、`shell.showInFolder`、`include`、`PdfTextPosition` の日本語と短縮形。
  - d.ts から消すと、tsc で検査する利用者（`examples` の CI など）にはエラーになる。それでよい（正の書き方に寄せられる）。
- **Web 標準の形とそろえるための重複**（`Notification`、`exit` と `xljs.exit`）は残す。ドキュメントではどちらを使うか決めて片方だけを書く。
- **機能の重複**（`setEnabled` など）はなくす。

**決定**: `sheet.rows()` を正にする（`ui.table(rows)`・`db.insert(table, rows)` と並ぶ）。`toObjects()` は受け付けるだけにする。
→ D1 で `sheet.toRowObjects()` を正に変えた（`rows` は行全体を指す `sheet.rows(5)` に使う）。引数なしの `rows()` と `toObjects()` は受け付けるだけ。

---

## B1. グローバルの名前

**現状**

- Excel は `workbooks.open` / `workbooks.create`（VBA の `Workbooks.Open` に合わせた）。ほかは `pdf.open`・`sqlite.open`・`csv.read` と、扱うものの名前。
  AI は `excel.open`・`xlsx.open` と書きがち（リファレンスサイトのページ名も `excel.md`）。
- JSON は `fs.readJSON` / `fs.writeJSON` / `fs.readJSONL`、CSV は `csv.read` / `csv.write`、文字列からの変換は `csv.parse` / `JSON.parse`。

**案**

- `workbooks` → `excel` にする（`excel.open`・`excel.create`・`excel.readVBA`・`excel.columnName`）。`workbooks` は A4 の方針で受け付けるだけにする。
- JSON は `fs` のまま（`JSON` は組み込みのグローバルと衝突するので `json.read` は作らない）。ドキュメントで「CSV は `csv`、JSON は `fs.readJSON`」と並べて示す。

**決定**: `excel` にする。

## B2. `fs` とハンドル

**現状**

- `fs.list(path)` はファイル名の配列（`string[]`）、`dir.list()` はハンドルの配列。
- 探す手段が 3 つ: `fs.walk(path, options)`（`WalkResult`）、`dir.walk(options)`（`WalkResult`）、`fs.glob(pattern, { cwd })`（配列）。
- `fs.writeBytes` は文字列も受けるが、`handle.writeBytes` は受けない。`fs.readJSON` は `Blob` も受けるが、`fs.readText` は受けない。

**案**

- `fs.list` もハンドルの配列を返す（名前だけなら `.map((h) => h.name)`）。
- `fs.glob` をなくし、`fs.walk(cwd, pattern)` にまとめる（`walk` は文字列・配列のパターンも受ける）。
- 受け付ける型の差は、どちらかにそろえる（小さいので一緒に直す）。

## B3. キャンセルしたときの戻り値

**現状**

| API | キャンセル |
|---|---|
| `prompt`・`showSaveFilePicker`・`showDirectoryPicker`・`ui.form`・`printPreview`・`toPdf({ preview })` | `null` |
| `showOpenFilePicker`・`ui.table`（選ぶとき） | `[]` |
| `print({ preview })` | `false` |
| `ui.withProgress` | 既定でスクリプトを終了（`onCancel: "break"` で反復を終える） |

**案**

- 「1 つを返すものは `null`、配列を返すものは `[]`」という規則で説明できるので、`showOpenFilePicker` と `ui.table` はそのままにする。
- `print` は `Promise<boolean>`（印刷したか）のままでよいが、`toPdf` と並べて「キャンセルすると `null` / `false`」とドキュメントの表にまとめる。
- 規則を `docs/ai/reference.md` の冒頭に書く。

この項目は API を変えず、ドキュメントで規則を示すだけにする見込み。

## B4. 進捗

**現状**

- `ui.progress()` はオブジェクトを返し、`update(done, message)`・`increment(message)`・`total` の代入。
- フォームの `form.progress(done, total?, message?)` は関数で、引数の並びが違う（`form.progress(done, message)` も可）。
- `ui.cancelled` は「直近の進捗表示がキャンセルされたか」というグローバルな状態。`form.cancelled` はフォームの実行ごと。

**案**

- `form.progress` の引数を `ui.progress` の `update` に合わせるか、両方を `{ done, total, message }` のオブジェクトで受ける形にそろえる。
- `ui.cancelled` は、`withProgress` の `onCancel: "break"` のためにあるので、反復のオブジェクト側（`for (const x of it)` の `it.cancelled`）に持たせられないか検討する。

**決定**: `ui.progress()` の `update` を `form.progress` と同じ `(done, total?, message?)`（`(done, message)` も可）にそろえる。`ui.cancelled` は今のまま。

## B5. Excel の書式

**現状**

- `CellStyle` に同じ意味のキーが 2 つずつある: `fill` / `background`、`numberFormat` / `format`、`font: { bold }` / `bold`、`alignment: { horizontal }` / `align`。
- 書式を付ける入口が 3 つ: `sheet.style(address, style)`、`sheet.range(address).style(style)`、`cell.style(style)`。
  さらに `Range` には `font()`・`fill()`・`border()`・`align()`・`numberFormat()` の個別のメソッドがある。
- 結合も `sheet.merge(address)` と `range.merge()` の 2 つ。
- 読むときは `cell.getStyle()`（書くのは `cell.style()`）。

**案**

- `CellStyle` は平たい形（`bold`・`color`・`background`・`format`・`align`・`border`）を正にする。`TableFormat`（表の条件付き書式）とも同じ言葉になる。
  入れ子の形（`font`・`fill`・`alignment`・`numberFormat`）は A4 の方針で受け付けるだけにする。平たい形にない設定（`vertAlign`・`indent`・`rotation`・`shrink` など）は平たい形に足す。
- 読み書きのメソッドの名前は `setStyle(style, { replace })` / `getStyle()` の対にする。`setStyle` は既存の書式に重ね（`replace: true` で置き換え）、`this` を返す。`getStyle()` は平たい形で返し、そのまま `setStyle` に渡せる。
  - 引数のない `style()` で読む形（jQuery の `css()` と同じ）は、引数の有無で読み書きが変わる暗黙の切り替え（基準 5）になる。AI は `style()` を「書式を消す」「既定に戻す」とも読みかねない。
  - `style(s)` が設定後の書式を返す形は、設定のメソッドとしては珍しく（AI は `this` か何も返さないと考える）、`sheet.style("A1:D1", s)` のように範囲に付けたときは 1 つの書式に決まらない。
  - プロパティ（`cell.style = {...}`）は、代入なのに重ねる動きになる。`cell.style.bold = true` と中身を書き換えると何も起きない（`win.state` と同じ落とし穴）。
  - 値（`cell.value`）は丸ごとの置き換えなのでプロパティ、書式は部分的に重ねるのでメソッド、と分ける。
- 入口は `sheet.setStyle(address, style)` と `cell.setStyle(style)` を正にする。`Range` の個別のメソッドはなくす。いまの `style()` は A4 の方針で受け付けるだけにする。

**決定**: `Range` は残し、`values`・`setStyle`・`getStyle`・`merge`・`unmerge` だけにする（`font()` などの個別のメソッドはなくす）。

## B6. シートの読み書き

**現状**

- セルの値: `sheet.cell("A1").value`、`sheet.getValue("A1")` / `sheet.setValue("A1", v)`。
- 表として読む: `sheet.toObjects()`・`sheet.rows()`（A4）・`sheet.toArrays()`・`sheet.range("A1:D10").values`。
- 書く: `sheet.write(rows)`・`sheet.append(rows)`・`range.values = [[...]]`。
- 行数は `sheet.rowCount`、範囲は `sheet.dimension`（列数はない）。→ D1 で `sheet.lastRow` / `sheet.lastColumn` にした。

**案**

- `getValue` / `setValue` をなくし、`sheet.cell(address).value` だけにする（`cell` は数式・書式・メモも持つので、1 つの入口に寄せたほうが AI が迷わない）。
- 表として読むのは `rows()`（または `toObjects()`。A4 で決める）と `toArrays()`。
- `range.values` は範囲を読み書きする手段として残すか、`toArrays({ range })` / `write(rows, { start })` に寄せるかを B5 と一緒に決める。

## B7. 印刷

**現状**

- `toPdf(dest, { preview })`・`print({ preview })` はオプションの中で、`printPreview(options, preview)` は第 2 引数。
- キャンセルの戻り値は `toPdf` が `null`、`print` が `false`、`printPreview` が `null`（B3）。
- 拡大縮小のキーが多い: `scale`・`zoom`・`fitToWidth`・`fitToHeight`・`fitToPage`（Excel のページ設定に合わせたもの）。

**案**

- `printPreview(options)` も `options.preview` で受ける形にそろえる（`printPreview({ ...options, preview: { settings } })`）。
- 拡大縮小のキーは Excel のページ設定の言葉なので残す。`zoom` だけは「さらに掛ける倍率」と分かる名前にできないか検討する。

## B8. ウィンドウのオプションの共通化

**現状**

`ui.form`・`ui.forms`・`ui.table`・`ui.chart`・`app.open`・印刷プレビューがそれぞれウィンドウのオプションを持ち、持っているものが少しずつ違う。

| | title | width / height | resizable | titleBar / icon | accent / background / radius |
|---|---|---|---|---|---|
| `ui.form` | ○ | ○ | ○ | ○ | ○ |
| `ui.forms` | ○ | ○ | ○ | ○ | ○ |
| `ui.table` | ○ | ○ | ○ | ○ | × |
| `ui.chart` | ○ | ○ | ○ | ○ | × |
| `app.open` | ○ | ○ | ○ | ○ | ×（CSS で） |
| 印刷プレビュー | ○ | ○ | × | × | × |

**案**

- d.ts に `WindowOptions`（`title`・`width`・`height`・`resizable`・`titleBar`・`icon`）を作り、各オプションはそれを継承する。
- 組み込みのページ（form・table・chart・印刷プレビュー）は見た目のオプション（`accent`・`background`・`radius`）も共通にする。

## B9. 読み書きの対

書ける（設定できる）のに読めないもの、読めるのに書けないもの、同じ値を 2 通りで書けるものを洗い出した。
Web 標準の型（`URLSearchParams`・`Headers` など）と Day.js は対象外。

**書けるのに読めない**

| 書く | 読む | 案 |
|---|---|---|
| `sheet.setRowHeight(row, h)` | なし（列は `getColumnWidth` がある） | `getRowHeight(row)` を足す |
| `range.style()`・`sheet.style(address, s)` | なし（セルは `cell.getStyle()`） | B5 の `setStyle` / `getStyle` を `Range` と `sheet` にもそろえる（範囲の `getStyle` は左上のセルの書式） |
| `sheet.freeze(cell)` / `unfreeze()` | なし | 読み取り専用の `sheet.frozen`（`{ rows, columns }`、なければ null）を足す |
| `sheet.merge(address)` / `unmerge(address)` | なし（どこが結合されているか分からない） | 読み取り専用の `sheet.merges`（`["A1:C1", ...]`）を足す |
| `sheet.autoFilter(address)` | なし。外す手段もない | 読み取り専用の `sheet.filter`（範囲か null）と、`autoFilter(null)` で外すのを足す |
| `form.setValue(name, v)` | 第 1 引数の `values` は呼ばれた時点の値のまま（`setValue` のあとで `values.n` を読むと古い値） | `setValue` で `values` も書き換える（メソッドは足さない） |
| `ui.table(...)` の行・`form.table(name, rows)` | ― | 読む場面がないので足さない |

**読めるのに書けない**

| 読む | 書く | 案 |
|---|---|---|
| 印刷のページ設定（`printPreview` の初めの値・`printPages`。用紙・向き・余白・拡大縮小） | 印刷のときの `options` だけ。シートに保存できない | 今回は対象外（機能の追加になる）。要望が出たら `sheet.pageSetup` で読み書きする |
| 非表示のシート（`toPdf` は非表示でないシートだけを出す） | なし。読むこともできない | `sheet.hidden`（読み書き）を足す |
| `sheet.rowCount` | ― | 列の数がない。`columnCount` を足す |

**2 通りで書ける**

| 値 | 書き方 | 案 |
|---|---|---|
| 進捗の総数・メッセージ | `p.total = n`・`p.message = s` と `p.update(done, message)` | B4 で `update(done, total?, message?)` にそろえたら、`total`・`message` は読み取り専用にする |
| ファイルへの追記 | `fs.appendText` / `h.appendText`（メソッド）と `fs.writeJSONL(..., { append: true })`・`csv.write(..., { append: true })`（オプション） | `writeText(path, text, { append: true })` にそろえ、`appendText` は A4 の方針で受け付けるだけにする（`writeText` の第 3 引数は文字コードの文字列か、`{ encoding, append }`） |
| セルの値 | `cell.value` と `sheet.setValue` / `getValue` | B6 のとおり `getValue` / `setValue` をなくす |

**そろえないもの**

- `cell.note`（プロパティで読み書き）と `cell.comment`（読み取り専用。付けるのは `addComment`、消すのは `comment.delete()`）: C2 のとおり、メモは 1 つの文字列、コメントはスレッドという Excel の違いに合わせる。
- カスタムアプリの `win.$(selector)` の `text()` / `text(v)` のように、引数の有無で読み書きを切り替える形: jQuery と同じ形で AI が知っているので残す（セルの書式は B5 のとおり `setStyle` / `getStyle`）。
- `form.setDisabled` / `setHidden` の読み取り: 状態はスクリプトが決めたもの（項目の `disabled` / `hidden` の関数）なので足さない。

---

## C1. `shell.open` / `openPath` / `openExternal`

`open` は「URL なら `openExternal`、それ以外は `openPath`」。3 つある必要は薄い。`shell.open` を正にし、ほかの 2 つは Electron の名前として受け付けるだけにする（A4）。

## C2. メモとコメント

`cell.note` は文字列（代入で付ける）、`cell.comment` はオブジェクト（付けるのは `cell.addComment()`）。`sheet.notes` は `{ address, text, author }` の配列。
Excel の機能の違い（メモは 1 つの文字列、コメントはスレッド）に合わせた形なので、そのままにする見込み。ドキュメントで違いを先に示す。

## C3. `ChartSpec` のガントチャートのキー

`ChartSpec` にガントチャート専用のキー（`name`・`start`・`end`・`progress`・`dependencies`・`id`・`view`・`holidays`・`progressLine`・`sCurve` など）と、ほかのグラフのキーが同じ型に並んでいる。
A3 と同じく、`type: "gantt"` とそれ以外で型を分ける。`name`（作業名の列）は `title` と紛らわしいので `task` などにするか検討する。

## C4. `app` の画面とのやりとり

画面を変える手段が `win.state`（data-bind）、`win.invoke`（画面の関数）、`win.emit`、`win.$(selector)`（簡易 DOM）、`win.evaluate` と 5 つある。
API は残し、ドキュメント（`docs/ai/reference.md` の「カスタムアプリの方針」）で「表示は `state`、まとまった描画は `invoke`」を基本と示し、`$`・`evaluate` は補助として後ろに置く。

## C5. `ui.table`

- `selectable` の有無で戻り値が変わる（`Promise<T[]>` と `TableView`）。オーバーロードで型は分かれているので、そのままでよい。
- `ChartView` には `update` があるが `TableView` にはない。表示だけの表の行を差し替えたい場面（監視・定期更新）があれば `TableView.update(rows)` を足す。

## C6. `shell.writeShortcutLink` の `operation`

`writeShortcutLink(path, "update", options)` のように 2 番目の引数で操作を渡すオーバーロードがある。`options.mode: "create" | "update" | "replace"` にして、引数の形を 1 つにする。

## C7. メタデータの大きさ

`@size 900x600` と、各 API の `width` / `height`。メタデータは 1 行で書く形なのでそのままにする。`@titlebar custom` と `titleBar: "custom"` も同じ。

## D1. 行と列

B の見直しのあとで出てきたもの。

**現状（直す前）**

- 行・列のメソッドがシートに 9 つ並ぶ: `insertRows` / `deleteRows` / `insertColumns` / `deleteColumns` / `setColumnWidth` / `getColumnWidth` / `setRowHeight` / `getRowHeight` / `autoFit(columns)`。
  引数も不揃い（`setColumnWidth` は `"A:C"` だけ、`insertColumns` は `"C"` か `3`、`setRowHeight` は 1 行だけ）。
- 行・列の非表示、行全体・列全体の書式、列ごとの最終行（VBA の `Cells(Rows.Count, "A").End(xlUp).Row`）がない。今の形で足すと、機能ごとに行用・列用の get / set がシートに増える。
- 最終行は `sheet.rowCount`。名前が「数」なので最終行だと気づけない。
- `sheet.dimension` は必ず `A1` から（表が C5 から始まっていても `"A1:..."`）。

**決定**

- `sheet.rows(5)` / `sheet.rows("2:4")` が `RowRange`、`sheet.columns("C")` / `sheet.columns(3)` / `sheet.columns("A:C")` が `ColumnRange`（VBA の `Rows`・`Columns`）。
  どちらも `Range` を継承する（`address`・`setStyle`・`getStyle`・`merge`・`values`）。
  - `RowRange`: `height`・`hidden`・`insert(count?)`・`delete()`・`autoFit()`・`lastColumn`
  - `ColumnRange`: `width`・`hidden`・`insert(count?)`・`delete()`・`autoFit(options?)`・`lastRow`
  - 幅・高さ・非表示は丸ごと置き換える値なのでプロパティ（B5 の `cell.value` と同じ分け方）。複数の行・列では、読むと先頭、代入するとすべて。
- 表をオブジェクトの配列で読むのは `sheet.toRowObjects()`（`toArrays()` と対）。名詞のメソッド（`cell`・`range`・`rows`・`columns`）はブックにつながったものを返し、`to...` は値の写しを返す。
  - `sheet.rows` を両方に使う案（引数なしでデータ、引数ありで行）は、引数の有無で意味が変わる（基準 2・5）のでやめた。
    単数形の `sheet.row(5)` も、`sheet.rows()` と 1 文字違いで別物になるのでやめた。
  - 引数なし・オプションだけの `sheet.rows()` は、実行時には今までどおりデータを返す（A4 の扱い。d.ts では引数が必須なので tsc はエラーにする）。
- 型の名前: 行のデータは `Row`（`[column: string]: any`。CSV・SQLite の挿入と共通）のまま。行全体は `RowRange`。
  `Row` を行全体の型にすると、ExcelJS を知っている AI が `toRowObjects()` の要素にも `height` があると考える。`SheetRow` は `SqlRow` と並んで「シートから読んだ行データ」に見える。
- 最終行・最終列は `sheet.lastRow` / `sheet.lastColumn`（列ごと・行ごとは `sheet.columns("A").lastRow` / `sheet.rows(1).lastColumn`）。
- `sheet.dimension` は値のあるセルを囲む範囲（左上も実際のセル）。
- 前の名前（`insertRows` など 8 つ、`autoFit("A:C")`、`rowCount` / `columnCount`）は受け付けるだけ。

**残り**

- 行・列のグループ化（アウトライン）は入れていない（保存で残るかの確認が先。`save-fidelity.md`）。
- `RowRange.autoFit()` は高さの指定を外すだけ（Excel が開いたときに合わせる）。xljs の PDF 出力は、高さの指定がない行を既定の高さで描く（折り返した文字に合わせない）。

---

## 進め方

1. この文書で各項目の方針を決める（済み。`setStyle` / `getStyle` 以外は書いてあるとおり、「決定」のとおりに進める）。
2. A から順に直す。1 項目ごとに、d.ts → `prelude.js` / Rust → テスト → `docs/site` → `docs/ai` → `examples` まで直してコミットする
   （`cd tools/docs && npm ci && node build.mjs` と `cargo test --lib`、`examples` の tsc の検査を通す）。
3. 直したら一覧の「状態」を「完了」にする（C2・C7 は「変えない」、C3 の `name` の改名と B9 のページ設定の読み書きは見送り）。リリース後はこの文書を消すか、決めた方針だけを「API の流儀」として CLAUDE.md の規約に移す。
