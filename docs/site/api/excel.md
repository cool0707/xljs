---
title: Excel
api: Workbooks, workbooks, Workbook, Worksheet, Cell, Range, CellValue, ToObjectsOptions, WriteOptions, CalcResult, CalcSkipped, VBAProject, VBAModule, CellComment, CommentReply, CommentEntry, CommentOptions, CellNote
---

```js
const book = workbooks.open(h);              // パス・ハンドル・File・Blob
const sheet = book.sheets["売上"] ?? book.sheets[0]; // 名前は大文字小文字・前後の空白・全角半角の違いも許す
const rows = sheet.toObjects();              // [{ 日付: Date, 金額: 1200, ... }]
const grid = sheet.toArrays({ text: true }); // 表示形式を適用した文字列

sheet.cell("B2").value = 100;
sheet.cell(3, 2).value = new Date();         // (行, 列)
sheet.cell("C10").formula = "=SUM(C2:C9)";
sheet.append([{ 日付: new Date(), 金額: 500 }]);
book.save();                                 // 上書き（save(パスやハンドル) で別名保存）

const out = workbooks.create();
out.addSheet("集計").write(summary).setColumnWidth("A:C", 16);
out.save("集計.xlsx");
```

- 新しいブック（`workbooks.create()`）の既定フォントは表示言語に合わせます（日本語では Excel と同じ游ゴシック）。`workbooks.create({ font: "Meiryo UI" })` で変更できます。
- 日付の書式が付いた数値セルは `Date` になります（Excel の日付はタイムゾーンを持たないので、ローカル時刻として扱います）。
- 数式は保存時に「開いたときに再計算」フラグを立てるので、Excel で開けば計算結果が表示されます。スクリプトの中で結果が必要なとき・計算済みの値をファイルに入れておきたいときは `book.calculate()`（下の「数式の計算」）。
- xlsx / xlsm に対応しています（旧形式の .xls は不可）。マクロ、ピボットテーブルなどは読み書きの過程で失われることがあります（グラフは開いて保存しても残ります）。
- `workbooks.readVBA(src)` はマクロブック（.xlsm / .xlsb / .xlam など）の VBA コードを、ブックを開かずに読みます。`{ name, codePage, modules: [{ name, type, code, sheet }], text }` を返し、マクロがなければ `null` です。`type` は `"standard"`（標準モジュール）・`"class"`・`"document"`（ThisWorkbook とシート）・`"form"`、`sheet` はシートのモジュールのシート名（.xlsm などで分かる場合）、`text` は `xljs vba` と同じ Markdown です。

## コメントとメモ

いまの Excel には、返信を付けられる**コメント**（スレッド形式のコメント）と、以前の Excel のコメントにあたる**メモ**があります。1 つのセルにはどちらか一方を付けられます。

```js
const book = workbooks.open("申請一覧.xlsx");
const sheet = book.sheet("申請");

// 読む
for (const c of sheet.comments) {
  console.log(c.address, c.author, c.text, c.resolved ? "（解決済み）" : "");
  for (const r of c.replies) console.log("  返信:", r.author, r.text);
}
console.log(sheet.cell("A1").note);              // メモ（なければ null）

// 書く
const c = sheet.cell("D5").addComment("金額を確認してください");
c.reply("領収書と一致しました", { author: "経理 担当" });
c.resolved = true;                               // スレッドを解決する
sheet.cell("A1").note = "4 月分から単価を変更";  // null で消す
book.save();
```

- `author` を省くと作成者は `xljs.userName`（Windows のユーザー名）です。日時は書いたときの時刻になります。
- 開いて保存しても、コメントの作成者・返信・解決済み・メンション（`@名前`）は残ります。
- Excel はコメントの代わりのメモ（「[スレッド化されたコメント]…」）を一緒に保存し、コメントのない Excel（2019 以前）ではそれが表示されます。xljs もコメントを書き換えるたびにこのメモを作り直します。このメモは `cell.note`・`sheet.notes` には出ません。
- Excel が付けたメモの文字列は、先頭に「作成者:」の行があります（`cell.note` にもそのまま入ります）。

## 数式の計算

Excel がなくても、ブックの数式を xljs が計算できます。

```js
const book = workbooks.open("見積書.xlsx");
const sheet = book.sheets["見積"] ?? book.sheets[0];
sheet.cell("C5").value = 30;             // 数式が参照しているセルを書き換える
sheet.cell("E20").formula = "=SUM(E5:E19)";

const r = book.calculate();              // すべての数式を計算し直して、結果をセルに入れる
console.log(sheet.cell("E20").value);    // 計算された値
if (r.skipped.length) {
  console.log("計算できなかった数式:\n" + r.skipped.map((s) => `${s.sheet}!${s.address} ${s.message}`).join("\n"));
}
book.save("見積書_記入済み.xlsx");        // 計算済みの値がファイルに入る
```

- `cell.value`・`toObjects()` などで読めるのは、セルに保存されている計算結果です。数式を書いたあと・数式が参照しているセルを書き換えたあとは、`book.calculate()` を呼ぶまで前の結果のまま（スクリプトが書いたばかりの数式は `null`）です。
- `book.calculate()` はブックのすべての数式を計算し、`{ formulas, changed, skipped }` を返します。`formulas` は計算した数式の数、`changed` は値が変わったセルの数です。
- `cell.calc()` は、そのセルの数式をいまのセルの値で計算した結果を文字列で返します（セルには保存しません）。1 つ 2 つの結果を見るだけならこちらで足ります。
- `book.toPdf()`・`sheet.toPdf()`・`toHtml()` は、スクリプトが書き換えたセルに依存する数式・結果が保存されていない数式・`TODAY()` や `NOW()` を使う数式を自動で計算してから印刷します（計算結果はセルにも入ります）。
- 関数は Excel のものをほぼ使えます（約 500 個。`XLOOKUP`・`FILTER`・`LET` などの新しい関数、テーブルの列名を使う参照、名前の定義、配列数式も）。`XLOOKUP`・`IFS`・`TEXTJOIN` などは Excel に表示されるとおりに書けば、ファイルに必要な接頭辞（`_xlfn.`）は自動で付きます。
- 日本語の Excel に合わせて、`NOW()`・`TODAY()` はパソコンの時刻、`LENB`・`LEFTB` などは全角を 2 バイトと数え、`TEXT` は `aaa`（曜日）・`ggge`（和暦）・`[DBNum1]`（漢数字）の表示形式、`ASC`・`JIS`・`YEN`・`DATESTRING`・`NUMBERSTRING` も使えます。
- ブックで「反復計算を行う」（Excel のオプション → 数式）が有効なら、循環参照を設定の回数まで繰り返し計算します（設定は保存しても残ります）。
- 計算できない数式は、保存されていた値をそのまま残し、`skipped` に `{ sheet, address, formula, reason, name, message }` を入れて知らせます。`reason` は次のとおりです。

| `reason` | 内容 |
|---|---|
| `"function"` | 計算できない関数（マクロ・アドインの関数、`PHONETIC`（ふりがな）、`CUBEVALUE` などのキューブ関数、`WEBSERVICE` など）。`name` が関数名 |
| `"name"` | ブックに定義されていない名前。`name` がその名前 |
| `"external"` | ほかのブックを参照している |
| `"circular"` | 循環参照（数式が自分自身の結果を参照している。その結果を参照する数式も）。「反復計算を行う」が有効なブックでは計算します |
| `"dataTable"` | データテーブル（What-If 分析） |
| `"large"` | 範囲が大きすぎる配列数式 |
| `"syntax"` | 数式として読み取れない |


### スピルする数式

`cell.formula` は、いまの Excel のセルに入力するのと同じ数式として読み書きします。`FILTER`・`UNIQUE`・`SORT` や `=B2:B100*1.1` のように複数の値を返す数式は**スピル**します（結果が下・右のセルに広がります）。

```js
const book = workbooks.open("売上.xlsx");
const sheet = book.sheet("明細");
sheet.cell("H1").formula = "=SORT(UNIQUE(B2:B100))";   // 担当者の一覧が H1 から下に広がる
book.calculate();
const names = sheet.range("H1:H50").values.flat().filter((v) => v !== null);
book.save("売上_集計.xlsx");                            // Excel で開いてもスピルする
```

- 広がった値は、`book.calculate()` のあと、それぞれのセルの `value` で読めます（広がり先のセルの `formula` は `null`）。計算せずに保存したファイルは、Excel が開いたときに計算して広げます。
- Excel で入力して保存したスピルの数式・配列数式（Ctrl+Shift+Enter）も、そのまま計算し、保存しても数式のまま残ります。
- 広がる先のセルに値があると、数式の結果は `#SPILL!` です（`cell.calc()` は `"#SPILL!"`、`cell.value` は `"#VALUE!"`）。広がった値の上に値や数式を書くと、Excel と同じく、その数式が広げていた値は消えます。
- 範囲のうち数式と同じ行（列）の 1 つの値だけを使うときは、`=@B2:B100` のように `@` を付けます。スピルのない Excel（2019 以前）で作った数式は、Excel が表示するのと同じく `@` が付いて読めることがあります（読んだ数式をほかのセルに書けば、同じ数式になります）。
- スピルのない Excel で開くと、スピルする数式は配列数式（`{=...}`）として表示されます。

- 大きいブックでは、最初の計算のときにブック全体を計算用に読み込むので、時間とメモリを使います（数式 10 万個で 1〜2 秒ほど）。そのあとの `cell.calc()`・`calculate()` は、書き換えたセルの分だけ取り込みます。
