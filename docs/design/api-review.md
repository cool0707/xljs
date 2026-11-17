# リリース前の API の見直し

機能を足し続けるうちに、当初の「AI が迷わずに書ける API」からずれてきた箇所を洗い出し、リリース前に直す。
後方互換は考えない（直したら `types/xljs.d.ts`・`docs/site`・`docs/ai`・`examples`・テストをまとめて直す）。

この文書は作業用のメモ。項目ごとに方針を決め、直したら「状態」を更新する。

## 判断の基準

AI（と利用者）が迷うのは、主に次の 5 つ。直すときもこの基準で決める。

1. **1 つのことに書き方が 2 つ以上ある**: AI は両方を混ぜて書き、片方にしかない機能をもう片方で使おうとする。
   別名は、残すとしても d.ts とドキュメントには 1 つしか書かない（「別名の方針」を参照）。
2. **同じ名前のキーが場所によって別の意味を持つ**: `height` が px だったり行数だったりする。
3. **同じ種類のものが違う形をしている**: ボタンの書き方が 3 通り、ctx が 2 種類、キャンセルしたときの戻り値が `null` / `[]` / `false`。
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
| A1 | A | `ui.form` | 送信ボタン（`submit`・`submitIcon`・`onSubmit`）を `buttons` に統一 | 未着手 |
| A2 | A | フォームの ctx | `FormContext` / `FormChangeContext` を 1 つに。対になった重複メソッドの整理 | 未着手 |
| A3 | A | `FormField` | type ごとの型に分け、意味の違う同名のキーをなくす | 未着手 |
| A4 | A | 全体 | 別名の方針（d.ts・ドキュメントに出すのは 1 つ） | 未着手 |
| B1 | B | グローバル | `workbooks` の名前、JSON は `fs`・CSV は `csv` の不揃い | 未着手 |
| B2 | B | `fs` / ハンドル | `fs.list` と `dir.list` の戻り値の違い、`walk` / `glob` の重複 | 未着手 |
| B3 | B | 全体 | キャンセルしたときの戻り値をそろえる | 未着手 |
| B4 | B | 進捗 | `ui.progress` と `ctx.progress` の形の違い、`ui.cancelled` | 未着手 |
| B5 | B | Excel の書式 | `CellStyle` の別名、`Range` と `sheet` のメソッドの重複 | 未着手 |
| B6 | B | Excel の読み書き | `getValue` / `setValue`、`toObjects` / `rows` / `toArrays` / `Range.values` | 未着手 |
| B7 | B | 印刷 | `preview` の渡し方と戻り値の違い | 未着手 |
| B8 | B | ウィンドウ | ウィンドウのオプションの共通化 | 未着手 |
| C1 | C | `shell` | `open` / `openPath` / `openExternal` | 未着手 |
| C2 | C | Excel のメモ | `cell.note`（文字列）と `cell.comment`（オブジェクト） | 未着手 |
| C3 | C | `ChartSpec` | ガントチャートのキーが同じ型に混在 | 未着手 |
| C4 | C | `app` | 画面とのやりとりの手段が多い | 未着手 |
| C5 | C | `ui.table` | 表示だけの表と選ぶ表、`TableView` に `update` がない | 未着手 |
| C6 | C | `shell.writeShortcutLink` | 位置で渡す `operation` | 未着手 |
| C7 | C | メタデータ | `@size 900x600` と `width` / `height` | 未着手 |

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
ui.form({ fields, submit: "集計", submitIcon: "play", onSubmit: async (v, ctx) => { ... } })
// 案
ui.form({ fields, buttons: [{ label: "集計", icon: "play", variant: "primary", onClick: async (v, ctx) => { ... } }] })
```

**未決**

- ボタンが 1 つで primary がないとき、それを primary とみなすか（ツールの例が 1 キー短くなるが、暗黙の規則が増える。いまの考えは「みなさない」）。
- ツールバーのボタン（`FormToolbarButton`）との関係は A2 で決める。

**影響**: `prelude.js`（ウィンドウ・コンソール・mock の 3 経路）、`web/form.js`（Enter）、`ui.forms`、記録（`recordForm` の `$button`）と `case.json`、`docs/ai` の各指示・スキル、`examples` の 5 本、`testdata/scripts` のフォームのテスト。

## A2. フォームの ctx を 1 つにする

**現状**

- `onSubmit`・ボタンの `onClick` は `FormContext`（進捗・ログ・`table`・`chart`・`setValue` など）。
- 項目の `onChange`・ツールバーのボタンの `onClick` は `FormChangeContext`（`setValue`・`setDisabled` など・`table`。`chart` がない）。
- 同じ「ボタンの onClick」でも、置き場所（`buttons` かツールバーか）で ctx の型が違う。
- 対になった重複: `setDisabled(name, bool)` と `setEnabled(name, bool)`、`setHidden(name, bool)` と `setVisible(name, bool)`。どちらも引数で逆にもできるので、4 つのメソッドで 2 つのことをしている。
- 項目を変える手段がばらばら: 値は `setValue`、表の行は `ctx.table(name, rows, columns)`、グラフは `ctx.chart(name, spec)`。表の `setValue` は「選択する行」で、行の入れ替えは `table`。

**案**

- ctx は `FormContext` 1 つにする。どこから呼ばれても同じメソッドを持つ。
  進捗・ログが意味を持たない場面（`onChange`・ツールバー）では、`progress` は何もしない、`log` はフォームの下に出す、など動きを決める（エラーにはしない）。
- `setEnabled` / `setVisible` をなくし、`setDisabled(name, bool)` / `setHidden(name, bool)` だけにする（項目のキー `disabled` / `hidden` と同じ言葉）。
- 表とグラフの更新の名前をそろえる。候補:
  - そのまま `ctx.table(name, rows)` / `ctx.chart(name, spec)` を残す（いまの名前は短く、AI も書けている）
  - `ctx.update(name, { rows, columns })` / `ctx.update(name, { chart })` のように項目のキーで書く（項目の定義と同じ言葉になる）

**未決**: 表・グラフの更新の名前（いまの考えは「`table` / `chart` を残す」。変える効果が小さい）。

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

**未決**: 名前の付け方（`lines` / `kind` / `text` / `src`）、高さの単位。

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

**未決**: `sheet.rows()` と `toObjects()` のどちらを正にするか（`ui.table(rows)`・`ChartSpec.data`・`db.insert(table, rows)` との並びでは `rows()` が自然。いまのドキュメントは `toObjects()`）。

---

## B1. グローバルの名前

**現状**

- Excel は `workbooks.open` / `workbooks.create`（VBA の `Workbooks.Open` に合わせた）。ほかは `pdf.open`・`sqlite.open`・`csv.read` と、扱うものの名前。
  AI は `excel.open`・`xlsx.open` と書きがち（リファレンスサイトのページ名も `excel.md`）。
- JSON は `fs.readJSON` / `fs.writeJSON` / `fs.readJSONL`、CSV は `csv.read` / `csv.write`、文字列からの変換は `csv.parse` / `JSON.parse`。

**案**

- `workbooks` → `excel` にする（`excel.open`・`excel.create`・`excel.readVBA`・`excel.columnName`）。`workbooks` は A4 の方針で受け付けるだけにする。
- JSON は `fs` のまま（`JSON` は組み込みのグローバルと衝突するので `json.read` は作らない）。ドキュメントで「CSV は `csv`、JSON は `fs.readJSON`」と並べて示す。

**未決**: `excel` にするか（ほかの候補 `xlsx`）。

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
- フォームの `ctx.progress(done, total?, message?)` は関数で、引数の並びが違う（`ctx.progress(done, message)` も可）。
- `ui.cancelled` は「直近の進捗表示がキャンセルされたか」というグローバルな状態。`ctx.cancelled` はフォームの実行ごと。

**案**

- `ctx.progress` の引数を `ui.progress` の `update` に合わせるか、両方を `{ done, total, message }` のオブジェクトで受ける形にそろえる。
- `ui.cancelled` は、`withProgress` の `onCancel: "break"` のためにあるので、反復のオブジェクト側（`for (const x of it)` の `it.cancelled`）に持たせられないか検討する。

**未決**: そろえる形。

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
- `getStyle()` が返す形も平たい形にする。
- 入口は `sheet.style(address, style)` と `cell.style(style)` を正にする。`Range` の個別のメソッドはなくす。

**未決**: `Range` を残すか（`range.values` の読み書きにも使っている。B6）。

## B6. シートの読み書き

**現状**

- セルの値: `sheet.cell("A1").value`、`sheet.getValue("A1")` / `sheet.setValue("A1", v)`。
- 表として読む: `sheet.toObjects()`・`sheet.rows()`（A4）・`sheet.toArrays()`・`sheet.range("A1:D10").values`。
- 書く: `sheet.write(rows)`・`sheet.append(rows)`・`range.values = [[...]]`。
- 行数は `sheet.rowCount`、範囲は `sheet.dimension`（列数はない）。

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

---

## 進め方

1. この文書で各項目の方針（特に「未決」）を決める。
2. A から順に直す。1 項目ごとに、d.ts → `prelude.js` / Rust → テスト → `docs/site` → `docs/ai` → `examples` まで直してコミットする
   （`cd tools/docs && npm ci && node build.mjs` と `cargo test --lib`、`examples` の tsc の検査を通す）。
3. 直したら一覧の「状態」を「完了」にする。リリース後はこの文書を消すか、決めた方針だけを「API の流儀」として CLAUDE.md の規約に移す。
