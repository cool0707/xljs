---
title: フォーム（ui.form）
api: FormOptions, FormField, FormFieldType, FormGroup, FormRow, FormToolbarButton, FormsOptions, FormsItem, FormValues, FormContext, FormChangeContext, FormButton, FormTableColumn
---

HTML を書かずに、項目の定義だけで画面を作れます。フォームは入力のダイアログではなく、入力欄・表・グラフ・ボタン・タブを置ける画面で、処理のあとも開いたまま続けて使えます（Excel VBA のユーザーフォームや C# のフォームアプリに当たります）。同じ `ui.form` で、次のどちらも作れます。

| 作るもの | 画面 | 書き方 |
|---|---|---|
| **ツール**（入力して実行する） | 入力欄と実行ボタン。実行すると進捗・ログ・結果が同じ画面に出る | 処理を `onSubmit` に書く |
| **アプリ**（開いたまま使う） | 開くと一覧が出ている。検索・追加・更新・削除・出力ができ、操作のたびに一覧が最新になる | 一覧は `table` 項目、操作は `buttons`、連動は `onChange`。機能ごとに画面を分けるなら [`ui.forms`](#複数のフォームをまとめるuiforms) |

フォームにない見た目・操作（表のセルに打ち込みながら行を増やす明細、独自のレイアウト）が要るときは [カスタムアプリ](app.md)（HTML の画面）にします。

## 入力して実行する（ツール）

`onSubmit(values, ctx)` を書くと、送信ボタンを押したあともウィンドウは開いたままで、進捗・ログ・中止ボタンを表示します。処理が終わると入力できる状態に戻り、値を変えて続けて実行できます。サンプル: `examples/form-aggregate.xljs`

```js
await ui.form({
  title: "売上集計",
  description: "フォルダ内の Excel を集計します",
  fields: [
    { name: "src",  label: "集計フォルダ", type: "folder", required: true },
    { name: "out",  label: "出力先", type: "saveFile", accept: [".xlsx"], default: "集計.xlsx", required: true },
    { name: "mode", label: "集計単位", type: "select", options: ["顧客別", "商品別"] },
    { name: "tax",  label: "税込で集計", type: "checkbox" },
  ],
  submit: "実行",
  remember: true,            // 前回の値を %APPDATA%\xljs\<スクリプト名>.json に保存して次回の初期値にする
  async onSubmit(v, ctx) {   // v.src は FileSystemDirectoryHandle、v.out は FileSystemFileHandle
    const files = v.src.walk("*.xlsx").toArray();
    for (const h of ctx.withProgress(files)) {  // 進捗をフォームに表示。中止ボタンでここから onSubmit が終わる
      ctx.log(h.relativePath);                  // ログ欄に 1 行（ctx.error で赤字）
    }
    return v.out;            // 出力ファイルを返すと「フォルダを開く」ボタンが出る
  },
});
```

## 開いたまま使う（アプリ）

一覧を出しておき、行の選択やボタンで処理して、同じ画面の一覧を読み直します。データを次回に持ち越すなら [SQLite](sqlite.md) に置きます。

```js
// 商品マスター: 一覧から選ぶと下の入力欄に出る。保存・削除のたびに一覧を読み直す
const db = sqlite.open(path.join(xljs.scriptDir, "商品.db"));
db.exec("CREATE TABLE IF NOT EXISTS 商品 (コード TEXT PRIMARY KEY, 品名 TEXT NOT NULL, 区分 TEXT, 単価 INTEGER)");
const load = (word) => db.all("SELECT * FROM 商品 WHERE 品名 LIKE ? ORDER BY コード", `%${word ?? ""}%`);
const show = (ctx, r) => {   // 入力欄に 1 件を出す（r なしは空にする）
  ctx.setValue("code", r?.コード ?? "");
  ctx.setValue("name", r?.品名 ?? "");
  ctx.setValue("kind", r?.区分 ?? "製品");
  ctx.setValue("price", r?.単価 ?? null);
};

await ui.form({
  title: "商品マスター",
  width: 760,
  fields: [
    { name: "word", label: "検索", placeholder: "品名の一部", onChange(word, v, ctx) { ctx.table("list", load(word)); } },
    { name: "list", label: "商品", type: "table", rows: load(), columns: ["コード", "品名", "区分", "単価"], selectable: "single", height: 10,
      onChange(row, v, ctx) { show(ctx, row); } },   // 行を選ぶと入力欄に出す
    { group: "選んだ商品（新しい商品は「新規」を押して入力）", fields: [
      { row: "", fields: [
        { name: "code", label: "コード", required: true, pattern: "[A-Z]\\d{3}", patternMessage: "英大文字 1 字と数字 3 桁で入力してください" },
        { name: "kind", label: "区分", type: "select", options: ["製品", "部品", "消耗品"], default: "製品" },
        { name: "price", label: "単価", type: "number", min: 0, suffix: "円", required: true },
      ] },
      { name: "name", label: "品名", required: true },
    ] },
  ],
  submit: false,             // 操作はボタンに分ける（Enter で送信されない）
  buttons: [
    { label: "新規", icon: "plus", validate: false, onClick(v, ctx) { show(ctx); } },
    { label: "削除", icon: "trash", variant: "destructive", validate: false, confirm: "選んだ商品を削除しますか？",
      onClick(v, ctx) {
        if (!v.list) throw new Error("削除する商品を一覧から選んでください");
        db.run("DELETE FROM 商品 WHERE コード = ?", v.list.コード);
        ctx.log(`${v.list.コード} ${v.list.品名} を削除しました`);
        ctx.table("list", load(v.word));
        show(ctx);
      } },
    { label: "保存", icon: "floppy-disk", variant: "primary",
      onClick(v, ctx) {      // 同じコードがあれば更新、なければ追加
        db.run("INSERT OR REPLACE INTO 商品 VALUES (?, ?, ?, ?)", v.code, v.name, v.kind, v.price);
        ctx.log(`${v.code} ${v.name} を保存しました`);
        ctx.table("list", load(v.word));
      } },
  ],
});
db.close();
```

アプリを組み立てる部品:

- **一覧**: `type: "table"` の項目。開いたときに出しておく行は `rows`、あとから入れ替えるのは `ctx.table(name, rows)`（`onSubmit`・ボタンの `onClick`・項目の `onChange` のどれからでも）。1 件もないときにも見出しが出るように、列は `columns` に書いておきます。`selectable: "single"` なら行を選ぶたびに `onChange(row, values, ctx)` が呼ばれます。`editable: true` で表の上でそのまま直せます。
- **操作**: `buttons` に動作ごとのボタンを並べます（`variant` で見た目、`confirm` で実行前の確認、`validate: false` で検証なし、数が多ければ `menu`）。送信ボタンが要らなければ `submit: false`。右上には `toolbar`。
- **連動**: 項目の `onChange` で `ctx.setValue`（ほかの項目を埋める）、`ctx.table`（絞り込み）、`ctx.setDisabled` / `ctx.setHidden`（出し分け）。
- **結果**: 経過と件数は `ctx.log`、グラフは `chart` 項目と `ctx.chart`、できたファイルは `return`。
- **画面の分け方**: 項目が多ければ `tabs` とグループ、機能ごとに画面を分けるなら [`ui.forms`](#複数のフォームをまとめるuiforms)（サイドメニュー）。

サンプル: `examples/form-master.xljs`（一覧から選んで追加・更新・削除）、`examples/form-tabs-table.xljs`（タブと表）、`examples/chart-gantt.xljs`（フォームの中のガントチャート）

## 尋ねて閉じる

`onSubmit` も `buttons` もないフォームは、送信すると閉じて値を返します（キャンセルは `null`）。処理の途中で少し尋ねるだけのときに使います。

```js
const v = await ui.form({
  title: "出力の設定",
  fields: [
    { name: "out",  label: "出力先", type: "folder", required: true },
    { name: "tax",  label: "税込で出力", type: "checkbox" },
  ],
  submit: "決定",
});
if (!v) exit();              // v.out は FileSystemDirectoryHandle
```

## 項目とオプション

- 項目の型: `text` `password` `number`（`min` `max` `step`）`date`（`Date`）`month` `time` `select` `radio`（`options`）`checkbox` `textarea` `file` `files` `folder` `saveFile` `table`。ファイル系は「参照…」ボタンとドラッグ＆ドロップで入力でき、値はハンドルになります（`folder` にファイルを落とすとそのフォルダ、`saveFile` にフォルダを落とすと入力済みのファイル名のままそのフォルダへの保存先になります）。
- `type: "table"` で表を表示します（`rows` `columns` は `ui.table` と同じ。`height` は表示する行数）。`selectable: true`（複数）/ `"single"` を付けると行をクリックして選べ、値は選んだ行（元のオブジェクト）になります。`selectable` なしは表示だけです。列見出しのクリックで並べ替えできます（列幅は最初に表示したときのまま変わりません）。列見出しのボタンから列ごとのフィルタ、列の固定（左端・右端・この列まで）、列の表示・非表示、境界のドラッグで幅の変更ができ、`formats` で条件付き書式を付けられます（いずれも [表表示](table.md) と同じ）。`onSubmit` の中で `ctx.table(name, rows)` を呼ぶと行を入れ替えられます（結果の表示など）。
- `editable: true`（または列ごとに `columns: [{ key, editable: true }]`）でセルをその場で編集できます。列に `type`（`text` `number` `date` `month` `time` `select` `combobox` `checkbox`。既定は値から判断）、`required` `min` `max` `pattern` `options`（`select` と `combobox` のセルは入力した文字を含む選択肢に絞り込んで一覧に出します。選択肢に空欄は書かず（`""`）、入力を消すとセルが空になります。選択肢にない入力は `combobox` の `allowCustom: true` のときだけ受け付けます）、`validate(value, row, index)` を書くと入力のたびに検証し、エラーのセルを赤く表示して表の下に一覧を出します（クリックでそのセルへ）。行全体の検証は項目の `validateRow(row, index)`。エラーが残っていると送信できません。検証を通った値は元の行オブジェクトに直接書き込まれます。Enter で下のセルへ移ります。数値は全角数字や「1,000」も受け付けます。コンソールでは表のあとに編集する行の番号を尋ねます。
- `fields`（タブの `fields` でも可）に `{ group: "見出し", icon, help, fields: [...] }` を項目と並べて書くと、見出しの下に項目をまとめて表示します。値はグループに関係なく 1 つのオブジェクトにまとまります（グループの中にグループは入れられません）。コンソールでは `-- 見出し --` を出してから順に尋ねます。
- `fields`（タブ・グループの `fields` でも可）に `{ row: "氏名", fields: [...] }` を書くと、`row` を左のラベルにして項目を横に並べます（姓・名、郵便番号と都道府県など）。各項目の `label` は入力欄の前に小さく出ます（`""` で出しません）。幅は等分で、`maxLength` や数値の項目はその幅になります。値はほかの項目と同じく 1 つのオブジェクトにまとまります。

```js
const v = await ui.form({
  title: "顧客登録",
  fields: [
    { row: "氏名", fields: [{ name: "last", label: "姓", required: true }, { name: "first", label: "名", required: true }] },
    { row: "フリガナ", fields: [{ name: "lastKana", label: "", placeholder: "セイ" }, { name: "firstKana", label: "", placeholder: "メイ" }] },
  ],
});
```

- `type: "image"` で画像を表示します。`default` に画像ファイル（PNG・JPEG・GIF・BMP・WebP・ICO・SVG のパス・ハンドル）、バイト列（`Uint8Array`）、`"https://..."` の URL を指定します。`width` / `height`（px。片方だけなら縦横比を保つ）、`align`（`"left"` `"center"` `"right"`）、`alt` を指定できます。`label` を付けなければフォームの幅いっぱいに置きます。値はありません（`values` に入りません）。`ctx.setValue(name, 画像)` で差し替えられます。コンソール・モックでは表示しません。

```js
await ui.form({
  title: "商品の確認",
  fields: [
    { name: "logo", type: "image", default: "logo.png", height: 48, align: "center" },
    { name: "photo", label: "写真", type: "image", default: "images/item01.jpg", width: 240 },
    { name: "code", label: "商品コード" },
  ],
});
```

- `toolbar` に項目（`name` のあるもの。`switch` `checkbox` `select` など）とボタンを書くと、フォームの右上に並べます（説明があればその右）。項目の値はほかの項目と同じく `values` に入ります。ボタンは `{ icon: "gear", tooltip: "設定", onClick(values, ctx) }` のように `label` なしならアイコンだけのボタンになり、押すと検証せずに入力のままの値で `onClick` を呼びます（進捗やログの表示はしません。`ctx.setValue` などで項目を変えられ、別の `ui.form` を開くこともできます）。`menu: [{ label, onClick }]` でメニューを開くボタンにできます。コンソール・モックではボタンは `label`（なければ `tooltip`）でほかのボタンのあとに並びます。

```js
const v = await ui.form({
  title: "集計",
  toolbar: [
    { name: "detail", type: "switch", label: "詳細表示" },
    { icon: "gear", tooltip: "設定", async onClick(values, ctx) {
      const s = await ui.form({ title: "設定", fields: [{ name: "dir", label: "既定の出力先", type: "folder" }] });
      if (s && s.dir) ctx.setValue("out", s.dir);
    } },
  ],
  fields: [{ name: "out", label: "出力先", type: "folder", required: true }],
});
```

- `tabs: [{ label, fields }]` を指定するとタブで項目を切り替えて表示します（`fields` はタブの上に常に表示）。値はタブに関係なく 1 つのオブジェクトにまとまります。エラーのある項目のタブには印が付き、そのタブに切り替わります。Ctrl+Tab でも切り替えられます。サンプル: `examples/form-tabs-table.xljs`
- ウィンドウのアイコンはメタデータの `@icon` で変えられます（`@mode app` でなくても有効。表も同じ）。
- 同梱の [Font Awesome Free](https://fontawesome.com/icons) のアイコンを、項目のラベル（`icon`）、タブ（`tabs: [{ label, icon, fields }]`）、ボタン（`buttons: [{ label, icon, ... }]`、送信ボタンは `submitIcon`）に付けられます。`"folder-open"` のように名前だけなら solid、`"fa-regular fa-calendar"` のようにクラスでも書けます。フォームの `icon` は `titleBar: "custom"` のときに自前のタイトルバーでタイトルの前に表示します（`"system"` では無視）。コンソール・モックでは無視されます。
- `accent` でフォームのアクセントカラー（送信ボタン・タブ・フォーカス枠・チェックボックス・表の選択行・タイトルバーのアイコン）を変えられます。`accent: "#c42b1c"` のような CSS の色はライト・ダークの両方に、`accent: { light: "#0f7b0f", dark: "#6ccb5f" }` なら配色ごとに使います。ボタンの文字色（黒か白）やホバー色は指定した色から決めます。コンソール・モックでは無視されます。
- `background` でフォームの背景色（ページとタイトルバーの地の色）を変えられます。`background: "#fff8e1"` のような CSS の色はライト・ダークの両方に、`background: { light: "#eef6ff", dark: "#1a2533" }` なら配色ごとに使います。暗い背景色を指定すると文字・入力欄・枠線もダークの配色になります（ダークの配色で明るい背景色なら逆）。コンソール・モックでは無視されます。
- 検証: `required`、`pattern`（＋`patternMessage`）、`min` / `max`、ファイル・フォルダの存在、項目ごとの `validate(value, values)`（メッセージを返す）。これらは入力欄を離れたときに検証して項目の下に表示し、エラーの表示中は入力のたびに検証し直します（直すとすぐ消えます）。フォームの `validate(values)`（`{ 項目名: "メッセージ" }` か全体へのメッセージを返す）は送信時だけ呼ばれるので、項目をまたぐ検証に使います。
- `onSubmit(values, ctx)` を書くと、実行後もウィンドウが開いたままになり、進捗（`ctx.progress(done, total, message)` / `ctx.withProgress(iterable)`。中止ボタンで `ctx.withProgress` のループから onSubmit が終わる）、ログ（`ctx.log` / `ctx.error`）、中止ボタン（`ctx.cancelled`）を表示します。出力ファイルを返すと完了後に「フォルダを開く」ボタンが出ます。ウィンドウが閉じられると、`ui.form` は最後に送信された（検証を通った）値を返します。
- `buttons: [{ label, onClick(values, ctx) }]` で送信ボタンの左にボタンを追加できます。押すと `onClick` が `onSubmit` と同じように実行されます（進捗・ログ・中止。ウィンドウは開いたまま）。`validate: false` を付けると検証せずに入力のままの値で実行し（読み込み・クリアなど）、`close: true` を付けると正常に終わったらフォームを閉じます。`submit: false` で送信ボタンを出さないこともできます。`ctx.setValue(name, value)` で項目の値を変えられます。コンソールでは項目のあとにどのボタンを押すか尋ねます。
- `remember` ではパスワードは保存しません。保存したくない項目には `remember: false` を付けます。保存先のフォルダは環境変数 `XLJS_CONFIG_DIR` で変えられます。

## 複数のフォームをまとめる（ui.forms）

`ui.forms({ forms: [...] })` で複数のフォームを 1 つのウィンドウにまとめ、サイドメニューで切り替えて表示します。各フォームは `ui.form` と同じオプション（`fields` `onSubmit` `buttons` など）に、メニューに出す `label` と `icon` を加えたものです。メニューの上のハンバーガーボタンでメニューをアイコンだけの幅に畳めます。

```js
const [order, setting] = await ui.forms({
  title: "受注管理",
  side: "left",            // "right" で右側
  size: "fixed",           // "each" ならフォームを切り替えるたびにそのフォームの大きさにする
  width: 860, height: 560,
  forms: [
    { label: "受注登録", icon: "cart-plus", fields: [{ name: "customer", label: "顧客", required: true }],
      onSubmit(v, ctx) { ctx.log(`${v.customer} を登録しました`); } },
    { label: "設定", icon: "gear", fields: [{ name: "dir", label: "出力先", type: "folder" }], remember: true },
  ],
});
```

- `size: "fixed"`（既定）はウィンドウの大きさを `width` / `height`（省略時はいちばん大きいフォームに合わせる）で決め、各フォームの `width` / `height` は無視します。`size: "each"` はフォームを切り替えるたびに、そのフォームの大きさ（`width` / `height`、なければ項目数から）にウィンドウを合わせます。
- `collapsed: true` でメニューを畳んだ状態で開きます。`start` で最初に表示するフォーム（番号・`label`・`id`）を選べます。`accent` `background` `radius` はメニューと、指定のないフォームに使います。
- 値はフォームごとに別々で、ウィンドウを閉じると各フォームの最後に送信された値（送信されていなければ `null`）の配列を返します。`onSubmit` のない送信ボタン・キャンセル・Esc はウィンドウを閉じます。
- コンソール・モックではフォームを順に 1 つずつ尋ねます。
