---
title: 入力フォーム（ui.form）
api: FormOptions, FormField, FormFieldType, FormGroup, FormValues, FormContext, FormChangeContext, FormButton, FormTableColumn
---

HTML を書かずに、項目の定義だけで入力フォームを出せます。キャンセルは `null`。

```js
const v = await ui.form({
  title: "売上集計",
  description: "フォルダ内の Excel を集計します",
  fields: [
    { name: "src",  label: "集計フォルダ", type: "folder", required: true },
    { name: "out",  label: "出力先", type: "saveFile", accept: [".xlsx"], default: "集計.xlsx" },
    { name: "mode", label: "集計単位", type: "select", options: ["顧客別", "商品別"] },
    { name: "tax",  label: "税込で集計", type: "checkbox" },
  ],
  submit: "実行",
  remember: true,            // 前回の値を %APPDATA%\xljs\<スクリプト名>.json に保存して次回の初期値にする
});
if (!v) exit();              // v.src は FileSystemDirectoryHandle、v.out は FileSystemFileHandle
```

- 項目の型: `text` `password` `number`（`min` `max` `step`）`date`（`Date`）`month` `time` `select` `radio`（`options`）`checkbox` `textarea` `file` `files` `folder` `saveFile` `table`。ファイル系は「参照…」ボタンとドラッグ＆ドロップで入力でき、値はハンドルになります（`folder` にファイルを落とすとそのフォルダ、`saveFile` にフォルダを落とすと入力済みのファイル名のままそのフォルダへの保存先になります）。
- `type: "table"` で表を表示します（`rows` `columns` は `ui.table` と同じ。`height` は表示する行数）。`selectable: true`（複数）/ `"single"` を付けると行をクリックして選べ、値は選んだ行（元のオブジェクト）になります。`selectable` なしは表示だけです。列見出しのクリックで並べ替えできます。`onSubmit` の中で `ctx.table(name, rows)` を呼ぶと行を入れ替えられます（結果の表示など）。
- `editable: true`（または列ごとに `columns: [{ key, editable: true }]`）でセルをその場で編集できます。列に `type`（`text` `number` `date` `month` `time` `select` `checkbox`。既定は値から判断）、`required` `min` `max` `pattern` `options`、`validate(value, row, index)` を書くと入力のたびに検証し、エラーのセルを赤く表示して表の下に一覧を出します（クリックでそのセルへ）。行全体の検証は項目の `validateRow(row, index)`。エラーが残っていると送信できません。検証を通った値は元の行オブジェクトに直接書き込まれます。Enter で下のセルへ移ります。数値は全角数字や「1,000」も受け付けます。コンソールでは表のあとに編集する行の番号を尋ねます。
- `fields`（タブの `fields` でも可）に `{ group: "見出し", icon, help, fields: [...] }` を項目と並べて書くと、見出しの下に項目をまとめて表示します。値はグループに関係なく 1 つのオブジェクトにまとまります（グループの中にグループは入れられません）。コンソールでは `-- 見出し --` を出してから順に尋ねます。
- `tabs: [{ label, fields }]` を指定するとタブで項目を切り替えて表示します（`fields` はタブの上に常に表示）。値はタブに関係なく 1 つのオブジェクトにまとまります。エラーのある項目のタブには印が付き、そのタブに切り替わります。Ctrl+Tab でも切り替えられます。サンプル: `examples/form-tabs-table.js`
- ウィンドウのアイコンはメタデータの `@icon` で変えられます（`@mode app` でなくても有効。表も同じ）。
- 同梱の [Font Awesome Free](https://fontawesome.com/icons) のアイコンを、項目のラベル（`icon`）、タブ（`tabs: [{ label, icon, fields }]`）、ボタン（`buttons: [{ label, icon, ... }]`、送信ボタンは `submitIcon`）に付けられます。`"folder-open"` のように名前だけなら solid、`"fa-regular fa-calendar"` のようにクラスでも書けます。フォームの `icon` は `titleBar: "custom"` のときに自前のタイトルバーでタイトルの前に表示します（`"system"` では無視）。コンソール・モックでは無視されます。
- `accent` でフォームのアクセントカラー（送信ボタン・タブ・フォーカス枠・チェックボックス・表の選択行・タイトルバーのアイコン）を変えられます。`accent: "#c42b1c"` のような CSS の色はライト・ダークの両方に、`accent: { light: "#0f7b0f", dark: "#6ccb5f" }` なら配色ごとに使います。ボタンの文字色（黒か白）やホバー色は指定した色から決めます。コンソール・モックでは無視されます。
- `background` でフォームの背景色（ページとタイトルバーの地の色）を変えられます。`background: "#fff8e1"` のような CSS の色はライト・ダークの両方に、`background: { light: "#eef6ff", dark: "#1a2533" }` なら配色ごとに使います。暗い背景色を指定すると文字・入力欄・枠線もダークの配色になります（ダークの配色で明るい背景色なら逆）。コンソール・モックでは無視されます。
- 検証: `required`、`pattern`（＋`patternMessage`）、`min` / `max`、ファイル・フォルダの存在、項目ごとの `validate(value, values)`（メッセージを返す）。これらは入力欄を離れたときに検証して項目の下に表示し、エラーの表示中は入力のたびに検証し直します（直すとすぐ消えます）。フォームの `validate(values)`（`{ 項目名: "メッセージ" }` か全体へのメッセージを返す）は送信時だけ呼ばれるので、項目をまたぐ検証に使います。
- `onSubmit(values, ctx)` を書くと、実行後もウィンドウが開いたままになり、進捗（`ctx.progress(done, total, message)` / `ctx.withProgress(iterable)`。中止ボタンで `ctx.withProgress` のループから onSubmit が終わる）、ログ（`ctx.log` / `ctx.error`）、中止ボタン（`ctx.cancelled`）を表示します。出力ファイルを返すと完了後に「フォルダを開く」ボタンが出ます。サンプル: `examples/form-aggregate.js`
- `buttons: [{ label, onClick(values, ctx) }]` で送信ボタンの左にボタンを追加できます。押すと `onClick` が `onSubmit` と同じように実行されます（進捗・ログ・中止。ウィンドウは開いたまま）。`validate: false` を付けると検証せずに入力のままの値で実行し（読み込み・クリアなど）、`close: true` を付けると正常に終わったらフォームを閉じます。`submit: false` で送信ボタンを出さないこともできます。`ctx.setValue(name, value)` で項目の値を変えられます。コンソールでは項目のあとにどのボタンを押すか尋ねます。
- `remember` ではパスワードは保存しません。保存したくない項目には `remember: false` を付けます。保存先のフォルダは環境変数 `XLJS_CONFIG_DIR` で変えられます。
