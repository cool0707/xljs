---
name: xljs-app-design
description: xljs の画面のあるスクリプト（フォームアプリ＝ui.form、カスタムアプリ＝@mode app と HTML）の設計の方針・型・アンチパターンと、アプリモードのリファレンス。xljs で画面のあるスクリプトを新しく作る・画面を直すときは、コードを書く前に必ず読む。利用者の氏名・所属などを使う、一覧を出して選ばせる・直させる、結果を表で見せる、複数の人が同じデータを使う、という依頼のときも読む。
---

# xljs の画面の設計とアプリモード

xljs の画面のあるスクリプトをどう組み立てるかの方針と、アプリモード（HTML の画面）の API です。フォーム・表の全オプションはスキル `xljs-reference`、Active Directory は `xljs-network`、SQLite は `xljs-sqlite` にあります。ここに書いていない関数・オプションは存在しないものとして扱ってください。

コードを書く前に、次を決めて回答の最初に 1〜2 行ずつ書いてください。

1. 形（フォームアプリ / カスタムアプリ / ステップごとのスクリプト）と、その理由
2. 画面の構成（1 つのウィンドウに何を置くか: 入力の項目、表、ボタン）
3. 利用者に入力させるものと、プログラムが調べるもの（利用者の情報・日付・前回値・データの場所）

## 画面の設計（フォームアプリ・カスタムアプリ）

画面のあるスクリプトは、書く前にここで形を決める。細かい API は「入力フォーム（ui.form）」「表の表示（ui.table）」「アプリモード」。

### 形を選ぶ

| 形 | 使うとき |
|---|---|
| フォームアプリ（`ui.form` を 1 つ） | 既定。「入力 → 実行 → 結果」「一覧を見て、直して、登録」「条件を変えて一覧を見る」は、どれも 1 つのフォームで作れる |
| フォームのまとまり（`ui.forms`） | 登録・設定・照会など、切り替えて使う画面がいくつかあるとき。1 つのウィンドウのサイドメニューでフォームを切り替える |
| カスタムアプリ（`@mode app` と HTML） | 利用者が頼んだとき。または `ui.form` / `ui.forms` にないものが要るとき: 行を追加・削除しながら入力する明細、一覧と詳細の切り替え、開いたままにして使う業務画面、入力に応じて計算結果がその場で変わる表 |
| ステップごとのスクリプト | 利用者が頼んだとき（バッチ・使い捨て）。画面は標準のダイアログだけ |

迷ったらフォームアプリ。フォームアプリで無理に作って使いにくくなるなら、理由を書いてカスタムアプリを提案する。

### 原則

1. **ウィンドウは 1 つ。** 起動から終了まで同じ画面で作業が終わるようにする。フォームを順に出さない（設定のフォーム → 本体のフォーム、など）。表のウィンドウ（`ui.table`）を続けて出さない。
2. **入力させるのは、プログラムが調べられないことだけ。** 利用者の氏名・所属・メール・役職は Active Directory から取る（下）。今日の日付・今月は初期値にする。ファイルを見れば分かること（シート名・対象の件数）は読んで決める。前回と同じになりやすい値は `remember: true`。
3. **入力は入力欄で受ける。** 値に合った `type` を選ぶ: 決まった選択肢は `select` / `radio` / `segmented`、多い候補から選ぶなら `combobox`、日付は `date` / `month` / `dateRange`、はい・いいえは `checkbox` / `switch`。選べるものを文字で打たせない。
4. **表は「見る・選ぶ・すでにある行を直す」ためのもの。** 新しいデータの入力欄の代わりにしない（フォームの表は行の追加・削除ができない）。1 件を登録するなら入力欄を並べ、何件も続けて入れるならカスタムアプリにする。
5. **結果は同じウィンドウに出す。** 経過と件数は `ctx.log`、一覧は同じフォームの `table` 項目に `ctx.table(name, rows)`、できたファイルは `return`（「フォルダを開く」ボタンが出る）。`ui.table` の別ウィンドウは、利用者が頼んだときと、1 行をダブルクリックして開く明細のような補助の表示だけ。
6. **処理のあとも画面を最新にする。** 登録・更新のあとは `ctx.table` で一覧を読み直す。「閉じてもう一度実行してください」と案内しない（フォームは処理のあとも入力に戻り、続けて実行できる）。
7. **`onSubmit` / `onClick` の中で `alert` を出さない。** 知らせることは `ctx.log`、失敗は `throw new Error("…")`（フォームに赤字で出る）、取り消せない操作の確認はボタンの `confirm`。
8. **機能が複数あるなら** 1 つのフォームの `tabs` と `buttons`（数が多ければ `menu`）にまとめる。めったに使わない管理用の機能は、条件付きで足す（`buttons: [...(admin ? [管理用のボタン] : [])]`）か、別のスクリプトにする。ボタンから別のフォームを開かない。

### 利用者の情報は Active Directory から

「記入者」「担当者」「申請者」「所属」「メール」のように実行している本人の情報は、入力欄にせず `ad.me()` で取る（ドメインに参加していないパソコンでも動くように備える）。ほかの人を選ばせるときは `ad.search` の結果を `combobox` の選択肢にする。

```js
const me = ad.available() ? ad.me() : null;
const user = {
  name: me?.displayName ?? xljs.userName,   // 表示名（AD がなければログオン名）
  mail: me?.mail ?? "",
  department: me?.department ?? "",
  title: me?.title ?? "",
};
// フォームでは確認のために見せるだけ（入力させない）:
// { name: "user", type: "note", label: "記入者", default: `${user.department} ${user.name}` }
// 同じ部署の人から選ばせる:
// { name: "to", label: "承認者", type: "combobox", options: ad.search({ filter: { department: user.department } }).map((u) => u.displayName) }
```

利用者ごとに出すものを変えるとき（自分の担当分だけ、管理者だけの機能）も、名前や役割を入力・選択させず、この `user` と手元のデータ（社員名簿など）を突き合わせて決める。見つからなければ、何と突き合わせて見つからなかったかをエラーで知らせる。

### フォームアプリの型

**入力 → 実行 → 結果**: ファイル・条件・保存先を 1 画面で受け、`onSubmit` で処理する。結果の一覧も見せるなら、空の `table` 項目を置いて `ctx.table` で入れる。

**一覧を見て、直して、登録する**: 起動時に一覧を読んでフォームの表に出し、登録のあと同じ表を読み直す。

```js
// 自分が担当する未出荷の受注を出し、出荷したものにチェックを付けて登録する
const me = ad.available() ? ad.me() : null;
const name = me?.displayName ?? xljs.userName;
const db = sqlite.open(path.join(xljs.scriptDir, "受注.db")); // みんなで使うデータの場所は毎回選ばせない
const load = () => db.all("SELECT 受注番号, 顧客名, 納期 FROM 受注 WHERE 担当者 = ? AND 出荷日 IS NULL ORDER BY 納期", name)
  .map((r) => ({ 出荷: false, ...r, 配送: "" }));

await ui.form({
  title: "出荷の登録",
  description: `${name} さんが担当する未出荷の受注です。出荷したものにチェックを付けて「登録」を押してください。`,
  width: 900,
  fields: [
    { name: "date", label: "出荷日", type: "date", default: new Date(), required: true }, // 今日が初期値。行ごとの列にしない
    {
      name: "items", label: "未出荷", type: "table", rows: load(), editable: true,
      columns: [
        { key: "出荷", type: "checkbox" },
        { key: "受注番号", editable: false },
        { key: "顧客名", editable: false },
        { key: "納期", editable: false },
        { key: "配送", type: "select", options: ["", "宅配便", "自社便"] },
      ],
      validateRow: (r) => (r.出荷 && !r.配送 ? { 配送: "配送の方法を選んでください" } : ""),
    },
  ],
  validate: (v) => (v.items.some((r) => r.出荷) ? null : { items: "出荷したものにチェックを付けてください" }),
  submit: "登録",
  onSubmit(v, ctx) {
    const done = v.items.filter((r) => r.出荷);
    db.transaction(() => {
      for (const r of done) db.run("UPDATE 受注 SET 出荷日 = ?, 配送 = ? WHERE 受注番号 = ?", v.date, r.配送, r.受注番号);
    });
    ctx.log(`${done.length} 件を登録しました`);
    ctx.table("items", load()); // 同じ画面のまま、残りの一覧に入れ替える
  },
});
db.close();
```

**条件を変えて一覧を見る**: 条件の項目の `onChange` で `ctx.table` を呼ぶ（実行ボタンを押させない）。保存などの操作は `buttons`、送信ボタンが要らなければ `submit: false`。

```js
const all = workbooks.open(path.join(xljs.scriptDir, "商品台帳.xlsx")).sheets[0].toObjects();
const pick = (v) => all.filter((r) => (v.kind === "すべて" || r.区分 === v.kind) && (!v.word || String(r.品名 ?? "").includes(v.word)));
await ui.form({
  title: "商品の検索",
  fields: [
    { name: "kind", label: "区分", type: "segmented", options: ["すべて", "製品", "部品", "消耗品"], default: "すべて",
      onChange(_, v, ctx) { ctx.table("list", pick(v)); } },
    { name: "word", label: "品名に含む文字",
      onChange(_, v, ctx) { ctx.table("list", pick(v)); } },
    { name: "list", label: "一覧", type: "table", rows: all, height: 15 },
  ],
  submit: false,
  buttons: [{
    label: "Excel に保存", icon: "file-excel", variant: "primary",
    async onClick(v, ctx) {
      const dest = await showSaveFilePicker({ suggestedName: "検索結果.xlsx" });
      if (!dest) return;
      const out = workbooks.create();
      out.addSheet("検索結果").write(pick(v), { table: true });
      out.save(dest);
      return dest;
    },
  }],
});
```

### 続けて使うデータ・設定

- 毎回同じになるものを毎回選ばせない。入力の前回値は `remember: true`。みんなで使うデータ（SQLite・マスターのブック）は、スクリプトと同じフォルダ（`xljs.scriptDir`）に置く決まりにするか、初回だけ選ばせて `fs.writeJSON(path.join(xljs.scriptDir, "設定.json"), …)` に覚える。データベースを選ぶだけのフォームを毎回最初に出さない。
- 複数人が書き込むデータは Excel のブックではなく SQLite（共有フォルダでは自動で共有モード）にする。Excel は取り込み元と出力先に使う。
- テーブルは起動時に `CREATE TABLE IF NOT EXISTS` で完成形を 1 回だけ書く。作っている途中のやり直しで付けた `ALTER TABLE` などの移行のコードは、本番のデータがまだないなら残さない。
- 画面を開く前に必要なもの（データベース・設定・利用者）がそろわないなら、そこでエラーにして何が足りないかを伝える（`ui.error` か `throw`）。足りないまま空の画面を出さない。

### アンチパターン

| してはいけない | 代わりに |
|---|---|
| 氏名・所属・役職・メールの入力欄 | `ad.me()`（上）。見せるだけなら `note` |
| フォームを閉じてから次のフォームや表を出す、の繰り返し | 1 つのフォーム（`tabs`・グループ・`hidden` / `disabled` で出し分け） |
| 結果を `ui.table` で次々に出す（件数の表 → 明細の表 → `alert`） | 件数は `ctx.log`、明細は同じフォームの `table` 項目に `ctx.table` |
| 新しいデータを入力させるために編集できる表を出す | 入力欄を並べる。何行も入力するならカスタムアプリ |
| 選択肢が決まっているのに `text` | `select` / `radio` / `segmented` / `combobox` |
| 全行に同じ値（今日の日付など）を入れる列を表に作る | 表の外に 1 つの項目（`default: new Date()`）を置き、保存のときに全行へ使う |
| 表のセルを変えたら同じ行のほかのセルを書き換えようとする（その仕組みはない） | 保存のときに計算する。その場で見せる必要があるならカスタムアプリ |
| 「閉じて再実行してください」という案内 | `ctx.table` で読み直す |
| 処理の中の `alert`（特にループの中） | `ctx.log` / `ctx.error`、最後に件数をまとめて 1 行 |
| 起動のたびに同じファイル・データベースを選ばせる | `remember: true`、`xljs.scriptDir`、設定ファイル |
| 説明のためだけの `note` を並べる | `description` に 1〜2 行、項目ごとの補足は `help` |
| 進捗の出ない長い処理 | `ctx.withProgress(list)`（中止もできる） |

### カスタムアプリの方針

- 画面（HTML）は見た目と操作、スクリプト（`.js`）はファイル・Excel・データベースの処理、と分ける。画面の JavaScript から xljs の API（`workbooks` など）は呼べないので、スクリプトの `export` した関数を `data-action` か `xljs.invoke` で呼ぶ。
- 1 つのウィンドウの中で画面を切り替える（`data-show` で出し分け）。`app.open` で別のウィンドウを増やすのは、同時に見比べる必要があるときだけ。
- 起動時に見せるデータ（利用者の名前、一覧）はスクリプトの先頭で `win.state` に入れる。利用者にボタンを押させてから読むのではなく、開いたときに出ているようにする。
- ファイルの選択は `<input type="file">` ではなく、スクリプトの関数で `showOpenFilePicker` などを呼ぶ（パスが取れる）。ドロップは `e.dataTransfer.files` をそのまま `xljs.invoke` に渡す。
- 入力の検査は HTML の `required` `min` `max` `pattern`（`data-action` は送信の前に検査する）と、スクリプトの関数の中の両方で行う。エラーは `throw new Error("…")`。
- 長い処理は進捗を `win.state` に入れ、ループの中で `await sleep(0)` して中止のボタンを受け取れるようにする。処理中はボタンを `data-show` / `disabled` で押せなくする（`data-action` のボタンは処理が終わるまで自動で無効になる）。
- 見た目は Windows の業務アプリとして落ち着いたものにする: フォントは `"Yu Gothic UI", "Meiryo UI", sans-serif`、文字は 14px 前後、余白と枠線で区切り、色はアクセント 1 色。`color-scheme: light dark` を付け、色は CSS 変数にして `@media (prefers-color-scheme: dark)` で切り替える。外部の CSS・フォント・CDN は使わない（社内のパソコンで読めない）。アイコンは同梱の Font Awesome。
- 表は `<table>` を自分で描く（何百行もあるなら絞り込みか、明細は `ui.table` に任せる）。グラフは `xljs.chart`。
- `alert` / `confirm` / `ui.form` / `ui.table` / ピッカーはスクリプトの関数から呼べる。確認や簡単な入力はこれで足りる。
- ファイルは `アプリ名.js` と `ui/index.html`（必要なら `ui/style.css`）。回答ではすべてのファイルを省略せずに出す。

## アプリモード（HTML の画面）

自由なレイアウトの画面が必要なときに使う（使いどころと作り方の方針は「画面の設計」）。スクリプト（`.js`）と HTML ファイルを別に用意する。画面は WebView2（Edge と同じ）で表示される。

```js
// ==xljs==
// @mode   app
// @html   ui/index.html
// @assets ui/
// @title  月次集計
// @size   900x600
// ==/xljs==
const win = app.main;                     // メインウィンドウ（スクリプトの実行前に開いている）
win.state = { status: "月を選んでください", busy: false, rows: 0 }; // 画面と共有する値（起動時の表示）

export async function run({ month }) {    // export した関数は画面から呼べる
  win.state.busy = true;
  try {
    const rows = aggregate(month);
    await win.invoke("render", rows);     // 画面の関数（xljs.handle）を呼ぶ
    win.state = { status: `${month} を集計しました`, rows: rows.length };
    return rows.length;                   // 呼び出し元（data-result・xljs.invoke の戻り値）へ
  } finally {
    win.state.busy = false;
  }
}

function aggregate(month) {
  return [{ 顧客名: "A 社", 合計: 1200 }]; // ここでファイルやデータベースを読む
}
```

```html
<!-- ui/index.html -->
<!doctype html>
<html lang="ja">
<head>
  <meta charset="utf-8">
  <link rel="stylesheet" href="/_xljs/fontawesome/css/all.min.css">
  <link rel="stylesheet" href="style.css">
</head>
<body>
  <form data-action="run" data-result="count">
    <input name="month" type="month" required>
    <button><i class="fa-solid fa-play"></i> 実行</button>
  </form>
  <p data-bind="status"></p>
  <p data-show="rows">件数: <span data-bind="rows"></span></p>
  <table id="result"><tbody></tbody></table>
  <script>
    xljs.handle("render", (rows) => {
      document.querySelector("#result tbody").replaceChildren(...rows.map((r) => {
        const tr = document.createElement("tr");
        for (const v of [r.顧客名, r.合計.toLocaleString()]) tr.append(Object.assign(document.createElement("td"), { textContent: v }));
        return tr;
      }));
    });
  </script>
</body>
</html>
```

メタデータ（スクリプトの先頭のコメント）: `@mode app`、`@html`（最初に開く HTML）、`@assets`（画面が使うフォルダ。複数行可）、`@title`、`@size 幅x高さ`、`@icon app.ico`、`@titlebar custom`（OS のタイトルバーの代わりに自前のタイトルバー）。

画面側（HTML。`xljs` は自動で使える。`<script src>` で読み込まない）:

| 書き方 | 働き |
|---|---|
| `data-bind="key"` | `win.state.key` を表示する（`<progress>` は値、入力欄は値、ほかは文字。`"a.b"` で中の値、`"!key"` で否定） |
| `data-model="key"` | 入力欄と `state.key` を双方向につなぐ（入力するとスクリプトの `win.state.key` も変わる） |
| `data-show="key"` | 値が真のときだけ表示する（`"!busy"` で逆） |
| `data-action="関数名"` | クリック（`<form>` に付ければ送信）でスクリプトの `export` した関数を呼ぶ。フォームの中なら引数は入力値のオブジェクト（`name` 属性がキー）で、呼ぶ前に `required` などを検査する。`data-args='[1, "a"]'` で引数を指定、`data-result="key"` で戻り値を `state.key` に入れる。処理中は押せなくなる |
| `await xljs.invoke("関数名", ...引数)` | スクリプトの関数を呼んで戻り値を受け取る（エラーは reject） |
| `xljs.handle("名前", fn)` | スクリプトの `win.invoke("名前", ...)` から呼ばれる関数を登録する |
| `xljs.on("名前", fn)` / `xljs.emit("名前", ...)` | スクリプトの `win.emit` を受け取る / スクリプトの `win.on` へ送る |
| `xljs.state` | `win.state` と同じ値（画面の JavaScript から読み書き）。`await xljs.ready` で最初の値が届くのを待てる |
| `await xljs.chart("#el", spec)` | `ui.chart` と同じ定義のグラフを要素に描く |

スクリプト側:

| API | 説明 |
|---|---|
| `export function 名前(...)` | 画面から呼べる関数（エントリのファイルの export だけ）。`app.handle(name, fn)` でも登録できる |
| `win.state.key = 値` / `win.state = { ... }` | 画面へ送る（代入したキーだけが変わる）。同期するのは最上位のキーだけなので、中身を変えたら `win.state.list = [...list]` のように代入し直す |
| `await win.invoke(name, ...args)` / `win.emit(name, ...args)` / `win.on(name, fn)` | 画面の関数を呼ぶ / イベントを送る / 画面の `xljs.emit` を受け取る |
| `win.$(selector).text(v)` `.html(v)` `.val(v)` `.attr(n, v)` `.addClass(c)` `.show()` `.hide()` `.on("click", fn)` | 画面の要素を直接変える・イベントを受ける簡易 DOM（読み取りは `await win.$(sel).val()`） |
| `app.open({ html, title, width, height })` | 別のウィンドウを開く（`await w.closed` で閉じるまで待つ） |
| `await win.closed` / `win.close()` | メインウィンドウが閉じるとスクリプトも終わる |

- 受け渡せる値: JSON の値・`Date`・ファイル / フォルダのハンドル・`ArrayBuffer`。画面でドロップ・選択した `File` はスクリプト側でハンドル（パス付き）になる。
- スクリプトの関数で起きたエラーは、画面で `catch` しなければ xljs のエラーダイアログに出る。
- 画面から呼べるのは同梱のファイルだけ（`@assets` のフォルダ）。外部の URL・CDN は読み込まない。アイコンは同梱の Font Awesome Free 7（上の `<link>`。`fa-solid` `fa-regular` `fa-brands`）。

