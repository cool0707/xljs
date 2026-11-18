---
title: Excel
api: Excel, excel, Workbook, Worksheet, Cell, Range, RowRange, ColumnRange, LastCellOptions, CellValue, RowsOptions, ColumnPick, WriteOptions, FindTarget, FindOptions, CellStyle, StyleOptions, BorderSpec, BorderSide, BorderStyleName, Color, CalcResult, CalcSkipped, VBAProject, VBAModule, CellComment, CommentReply, CommentEntry, CommentOptions, CellNote
---

```js
const book = excel.open(h);                  // パス・ハンドル・File・Blob
const sheet = book.sheets["売上"] ?? book.sheets[0]; // 名前は大文字小文字・前後の空白・全角半角の違いも許す
const rows = sheet.toRowObjects();           // [{ 日付: Date, 金額: 1200, ... }]
const grid = sheet.toArrays({ text: true }); // 表示形式を適用した文字列

sheet.cell("B2").value = 100;
sheet.cell(3, 2).value = new Date();         // (行, 列)
sheet.cell("C10").formula = "=SUM(C2:C9)";
sheet.append([{ 日付: new Date(), 金額: 500 }]);
book.save();                                 // 上書き（save(パスやハンドル) で別名保存）

const out = excel.create();
const total = out.addSheet("集計").write(summary);
total.columns("A:C").width = 16;
out.save("集計.xlsx");
```

- 新しいブック（`excel.create()`）の既定フォントは表示言語に合わせます（日本語では Excel と同じ游ゴシック）。`excel.create({ font: "Meiryo UI" })` で変更できます。
- 日付の書式が付いた数値セルは `Date` になります（Excel の日付はタイムゾーンを持たないので、ローカル時刻として扱います）。
- 数式は保存時に「開いたときに再計算」フラグを立てるので、Excel で開けば計算結果が表示されます。スクリプトの中で結果が必要なとき・計算済みの値をファイルに入れておきたいときは `book.calculate()`（下の「数式の計算」）。
- xlsx / xlsm に対応しています（旧形式の .xls は不可）。マクロ、ピボットテーブルなどは読み書きの過程で失われることがあります（グラフは開いて保存しても残ります）。
- シートの名前は `sheet.name = "新しい名前"` で変えられます。Excel で名前を変えたときと同じく、そのシートを参照する数式・名前の定義・入力規則・条件付き書式・グラフ・スパークライン・ピボットテーブルの元データ・フォームコントロールも新しい名前を指すようになります（ハイパーリンクの行き先と、`INDIRECT("Sheet1!A1")` のような文字列の中は変わりません）。
- シートの状態は、`sheet.hidden`（非表示。代入で切り替えます。表示しているシートがなくなる変更はエラー）、`sheet.lastRow()` / `sheet.lastColumn()`（最終行・最終列: 値か数式のある最後の行・列の番号）、`sheet.dimension`（値か数式のあるセルを囲む範囲）、`sheet.merges`（結合しているセル範囲の配列）、`sheet.frozen`（ウィンドウ枠の固定。`{ rows, columns }` か `null`）、`sheet.filter`（フィルターの範囲か `null`）で読めます（下の「シートの表示」）。行の高さ・列の幅・非表示は `sheet.rows(行)`・`sheet.columns(列)`（下の「行と列」）。セルの書式は `sheet.getStyle("A1")`（下の「セルの書式」）。
- シートのコピーは `book.copySheet(シート, 名前?)`（元のシートのすぐ後ろに置き、名前を省くと「売上 (2)」）。テーブルは「売上表_2」のような名前になり、コピーの数式もそれを使います。グラフ・スパークラインはコピーのセルを指し、数式は元と同じシートを参照します（Excel と同じ）。
- 行・列の挿入と削除は `sheet.rows(行).insert(数?)`・`sheet.rows("3:5").delete()`・`sheet.columns(列).insert(数?)`・`sheet.columns("B:C").delete()`（下の「行と列」）。

```js
const sheet = book.sheets["明細"];
sheet.rows(2).insert(3);         // 2〜4 行目に空の行（=SUM(B2:B9) は =SUM(B5:B12) に）
sheet.rows(10).delete();         // 10 行目を削除して詰める
sheet.columns("C").insert();     // C 列の前に 1 列
const may = book.copySheet("4月", "5月");
```

- `excel.readVBA(src)` はマクロブック（.xlsm / .xlsb / .xlam など）の VBA コードを、ブックを開かずに読みます。`{ name, codePage, modules: [{ name, type, code, sheet }], text }` を返し、マクロがなければ `null` です。`type` は `"standard"`（標準モジュール）・`"class"`・`"document"`（ThisWorkbook とシート）・`"form"`、`sheet` はシートのモジュールのシート名（.xlsm などで分かる場合）、`text` は `xljs vba` と同じ Markdown です。

## 表の読み書き

`sheet.toRowObjects()` は 1 行目を見出しにして A 列から読みます。帳票の途中にある表・見出しの文字がそろっていない表は、表の左上（`start`）と読む列（`pick`）を指定します。

```js
const sheet = book.sheets["受注"];
const rows = sheet.toRowObjects({
  start: sheet.find("コード") ?? "B4",   // 表の左上（見出しの左端）。セルか "B4"
  untilBlank: true,                       // 空の行で表の終わり（下の合計・注記を読まない）
  pick: {                                 // 読む列を見出しで選び、キーを付け替える
    code: "コード",
    name: "品名",                         // 「品　名」・改行入りの見出しにも一致
    amount: /金額|売上/,                  // 正規表現
    tax: { header: "金額", nth: 2 },      // 同じ見出しの 2 番目
    memo: { header: "備考", optional: true }, // なくてもよい列（なければ null）
  },
});
// [{ code: "A-1", name: "りんご", amount: 100, tax: 10, memo: null }, ...]
```

- `start` を指定すると、そこから右へ見出しが続くところ（最初の空のセルの手前）までが表の列になります。指定しないときは `header`（見出しの行番号。既定 1）の行を A 列から読みます。
- `pick` の条件は、文字列（その見出しの列。全角半角・大文字小文字・空白と改行の違いは区別しません）、正規表現、数値（表の左端からの位置。0 から）、`{ header, nth, offset, optional }`（同じ見出しの `nth` 番目、見出しから `offset` 列右）です。見つからない見出しがあるとエラーになり、メッセージに見つかった見出しが出ます。
- 結合セルは、どの行・列でも左上のセルの値になります（縦に結合した「部署」は各行に入り、2 列にまたがる見出し「金額」は `金額`・`金額_2` になります）。

いまある表に書き足すときは `matchHeader` を付けます。行のキーと同じ見出しの列にだけ値を入れ、見出しは書かず、ほかの列（数式の列など）には触りません。

```js
const ledger = book.sheets["台帳"];
// 1 行目の見出しに合わせて、最終行の下に追記する（キーの順は見出しの順と違ってよい）
ledger.append([{ 金額: 500, 日付: new Date(), 品名: "りんご" }], { matchHeader: true });
// 見出しが 3 行目の B 列から始まる表
ledger.append(rows, { matchHeader: "B3" });
// 見出しのすぐ下から書く（ひな形への流し込み）
ledger.write(rows, { matchHeader: 3 });
// 別のシートの表を、見出しの同じ列へ写す
ledger.append(book.sheets["受注"].toRowObjects({ start: "B4", untilBlank: true }), { matchHeader: true });
```

- `matchHeader` は、`true`（見出しは `start` のセルから右。`append` では 1 行目）、行番号、見出しの左端のセル（`"B3"` か `find()` の結果）で指定します。
- キーと見出しは `toRowObjects()` が付ける名前で比べ、なければ全角半角・大文字小文字・空白の違いを区別せずに比べます。見出しにないキーは書きません。一致する見出しが 1 つもなければエラーです。
- `append` は、その表の列の最終行の下に書きます（表の右に別の表があっても、その行数には影響されません）。

## セルを探す

`sheet.find(探すもの)` は一致する最初のセル（なければ `null`）、`sheet.findAll(探すもの)` はすべてのセルを返します。範囲・行・列の中だけを探すときは `sheet.range("A1:F20").find(...)`・`sheet.columns("A").find(...)`・`sheet.rows(4).find(...)` です。

```js
const sheet = book.sheets["申請書"];
const name = sheet.find("氏名")?.offset(0, 1).value;      // 見出し「氏名」の右のセルの値
const row = sheet.columns("A").find("A-1024")?.row;        // A 列でコードのある行（なければ undefined）
const total = sheet.find(/^合計/);                         // 「合計」で始まるセル
const big = sheet.findAll((c) => typeof c.value === "number" && c.value >= 10000); // 条件を関数で
const subtotals = sheet.findAll("小計", { match: "partial" });                      // 「小計」を含むセル

// 行の見出しと列の見出しが交わるセル
const r = sheet.find("4月"), c = sheet.find("売上");
if (r && c) console.log(sheet.cell(r.row, c.column).value);
```

| 探すもの | 一致するセル |
|---|---|
| 文字列 | セルの文字がそれと同じもの（セル全体。`{ match: "partial" }` で一部）。全角半角・大文字小文字・空白と改行の違いは区別しません（`"氏名"` は「氏　名」に、`"a-1"` は「Ａ－１」に一致） |
| 正規表現 | セルの文字に当てます（全角は半角にしてから。`/^\d+月$/` は「４月」にも一致） |
| 数値・`Date`・真偽値 | セルの値がそれと同じもの |
| 関数 | 値のあるセルごとに呼ばれ、`true` を返したセル |

- 文字列と正規表現は、数値・日付のセルを表示されている文字（`"1,200"`・`"2026/4/1"`）で比べます（数値は `"1200"` でも一致します）。数式のセルは保存されている結果で比べるので、数式を書いたあとは先に `book.calculate()` を呼びます。
- `{ strict: true }` で全角半角・大文字小文字・空白を区別します。`{ visibleOnly: true }` で非表示の行・列（フィルターで隠れた行も）を探しません。`{ order: "columns" }` で列ごとに上から探します（既定は行ごとに左から）。
- `cell.offset(行, 列)` は、そのセルから下・右（負の数で上・左）に進んだセルです（VBA の `Offset`）。

## 結合セル

結合セルは、中のどのセルを指定しても左上のセルとして扱います。VBA のように、結合セルの途中を指定してエラーになったり、見えないセルに値が入ったりしません。

```js
sheet.merge("A1:C1");
sheet.cell("B1").value = "売上一覧";     // A1 に入る
sheet.cell("C1").value;                  // "売上一覧"（A1 の値）
sheet.cell("B1").setStyle({ border: "thin", align: "center" }); // 結合セル全体に付く（外枠の罫線は結合セルの外枠）
sheet.cell("A1").offset(0, 1).address;   // "D1"（結合セルからは、その範囲の端から数える）
```

- 値・数式・表示文字・書式・メモ・コメント・ふりがなの読み書きが左上のセルに向きます（`cell.address`・`row`・`column` は指定したまま）。
- `toRowObjects()`・`toArrays()`・`range.values` は、結合セルのどの位置にも左上のセルの値を返します。`write()`・`append()`・`range.values = ...` は、結合セルに掛かる位置では左上の位置の値だけを書きます。
- `find()`・`findAll()` は、結合セルを左上のセルとして 1 回だけ返します。
- `lastRow()`・`lastColumn()`・`dimension` は、値のある結合セルをその範囲の最後の行・列まで数えます（`append()` は結合セルの下に書きます）。
- `sheet.merge()` は、Excel の「セルを結合」と同じく、値を 1 つだけ（行ごとに左から見て最初の値）左上のセルに残して、ほかを消します。
- 数式の中の参照は Excel と同じです（`A1:C1` が結合のとき `=B1` は空）。

## セルの書式

文字・塗りつぶし・罫線・配置・表示形式（Excel の「セルの書式設定」）を付けます。

```js
// 表の体裁をまとめて付ける（見出しの太字・塗り・罫線、列幅自動、先頭行固定、フィルター）
sheet.write(rows, { table: true, columnStyles: { 金額: { format: "#,##0" } } });

// 範囲・セル単位（指定した項目だけが既存の書式に重なる）
sheet.setStyle("A1:D1", { bold: true, color: "#FFFFFF", background: "#305496", align: "center" });
sheet.cell("D10").setStyle({ fontSize: 14, bold: true, format: "¥#,##0", border: { top: "double" } });
sheet.range("A3:D10").setStyle({ border: { outline: "medium", inside: { style: "thin", color: "#BFBFBF" } } });
sheet.cell("A1").getStyle();   // 現在の書式（setStyle に渡せる形。sheet.getStyle("A1") も同じ）
```

| 項目 | 指定方法 |
|---|---|
| 文字 | `bold` `italic` `strike` `underline`（`true` / `"double"` など）`color` `fontSize` `fontName` `vertAlign`（`"superscript"` `"subscript"`） |
| 塗りつぶし | `background: "#RRGGBB"`（`null` で塗りなし） |
| 罫線 | `border: "thin"` など（全セルの上下左右）、または `{ all, outline, inside, horizontal, vertical, top, bottom, left, right, diagonalUp, diagonalDown }`。各辺は `"thin"` `"medium"` `"thick"` `"double"` `"dashed"` `"dotted"` `"hair"` など、または `{ style, color }` |
| 配置 | `align`（`"left"` `"center"` `"right"` など）`valign`（`"top"` `"center"` `"bottom"` など）`wrap` `shrink` `indent` `rotation` |
| 表示形式 | `format: "#,##0"`・`"yyyy/m/d"`・`"0.0%"`・`"¥#,##0"` など（`"General"` で標準）。漢数字の `"[DBNum1]G/標準"`（一万二千三百四十五）・`"[DBNum2]G/標準"`（壱萬弐阡参百四拾伍）・`"[DBNum3]0"`（全角数字）も `cell.text` と PDF に出ます |
| 保護 | `locked`（シートの保護でセルをロックするか） |

- 色は `"#RRGGBB"`・`"#RGB"`・`red` などの色名で指定します。
- 既定では既存の書式に重ねます。`{ replace: true }` を渡すと書式を置き換えます。
- 範囲の端に引いた罫線は、隣の（書式が設定済みの）セルの接する辺にも反映するので、線が食い違いません。
- サンプル: `examples/xlsx-style-report.xljs`

## シートの表示

セルの結合、ウィンドウ枠の固定、フィルター、列幅の自動調整です。それぞれ今の状態を読めます。

```js
sheet.merge("A1:D1");          // セル結合（unmerge で解除。sheet.merges で結合範囲の一覧。上の「結合セル」）
sheet.freeze("A2");            // 1 行目を固定（freeze({ rows: 1, columns: 1 }) も可。unfreeze で解除。sheet.frozen で読める）
sheet.autoFilter("A3:D10");    // フィルター（null で外す。sheet.filter で範囲を読める）
sheet.autoFit();               // すべての列の幅を内容に合わせる（全角は 2 文字分）
```

## 行と列

`sheet.rows(5)`・`sheet.rows("2:4")` は行全体、`sheet.columns("C")`・`sheet.columns(3)`・`sheet.columns("A:C")` は列全体です（VBA の `Rows("2:4")`・`Columns("A:C")`）。高さ・幅・非表示・書式・挿入・削除と、その行・列の値を扱います。

```js
sheet.rows(1).height = 30;                 // 行の高さ（ポイント）。読むこともできる
sheet.columns("A:C").width = 16;           // 列の幅（半角の文字数）
sheet.columns("B").autoFit();              // 内容に合わせる（行は rows(1).autoFit() で高さの指定を外す）
sheet.rows("3:5").hidden = true;           // 非表示（false で再表示）
sheet.columns("E").hidden = true;

sheet.rows(1).setStyle({ bold: true, background: "#DDEBF7" }); // 行全体の書式
sheet.columns("C").setStyle({ format: "#,##0" });              // 列全体の書式

sheet.rows(2).insert(3);                   // 2〜4 行目に空の 3 行
sheet.rows("8:9").delete();                // 8〜9 行目を削除して詰める
sheet.columns("C").insert();               // C 列の前に 1 列

const last = sheet.columns("A").lastRow(); // A 列の最終行（値か数式のある最後の行。なければ 0）
const right = sheet.rows(1).lastColumn();  // 1 行目の最終列
const header = sheet.rows(1).values[0];    // 1 行目の値（["品名", "数量", "単価"]）
const names = sheet.columns("A").values.map((r) => r[0]); // A 列の値（最終行まで）
```

- 表のデータを読むのは `sheet.toRowObjects()`・`sheet.toArrays()` です。`sheet.rows(5)` は 5 行目そのもの（高さ・書式など）を指します。
- 最終行・最終列は、シート全体なら `sheet.lastRow()`・`sheet.lastColumn()`、列ごと・行ごとなら `sheet.columns("A").lastRow()`・`sheet.rows(1).lastColumn()`（VBA の `Cells(Rows.Count, "A").End(xlUp).Row`）。値か数式のあるセルを数えます。VBA と違い、行・列が非表示でも、フィルターで隠れていても結果は変わりません。
- 数えるセルはオプションで変えられます。`{ skipBlank: true }` は、値が空か空白だけのセルを数えません（下まで入れた `=IF(A2="","",...)` の結果が `""` の行、半角・全角の空白だけのセル）。数式の結果は保存されている値で見るので、数式を書いたあとは先に `book.calculate()` を呼びます。`{ formats: true }` は、罫線・塗りだけで値のないセルも数えます（Excel の `UsedRange` に近い数え方）。

```js
sheet.columns("C").lastRow({ skipBlank: true });   // 結果が "" の数式の行を数えない
sheet.lastRow({ formats: true });                   // 罫線・塗りだけの行も数える
```
- 複数の行・列（`"2:4"`・`"A:C"`）では、`height`・`width`・`hidden` を読むと先頭の行・列の値、代入するとすべてに設定します。`lastRow()`・`lastColumn()` はその中でいちばん大きい番号です。
- `setStyle` は行・列そのものの書式になります。いまあるセルに付き、あとから値を書いたセルにも付きます（Excel で行・列を選んで書式を付けたときと同じ）。
- 挿入と削除では、Excel と同じく、ずれたセルを参照する数式（ほかのシートの数式・絶対参照も）・名前の定義・印刷範囲・入力規則・条件付き書式・テーブル・グラフ・スパークライン・ピボットテーブルの元データが付いてきます。範囲の途中に挿入すると範囲が広がり、削除したセルだけを参照していた数式は `#REF!` になります。挿入した行（列）は上の行（左の列）の書式を引き継ぎます。`insert()` の数を省くと、その範囲の行数・列数だけ挿入します（`rows("2:4").insert()` は 3 行）。
- 非表示の行は `SUBTOTAL(109, ...)` などの計算から外れます。

## コメントとメモ

いまの Excel には、返信を付けられる**コメント**（スレッド形式のコメント）と、以前の Excel のコメントにあたる**メモ**があります。1 つのセルにはどちらか一方を付けられます。

```js
const book = excel.open("申請一覧.xlsx");
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
const book = excel.open("見積書.xlsx");
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

- `cell.value`・`sheet.toRowObjects()` などで読めるのは、セルに保存されている計算結果です。数式を書いたあと・数式が参照しているセルを書き換えたあとは、`book.calculate()` を呼ぶまで前の結果のまま（スクリプトが書いたばかりの数式は `null`）です。
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
const book = excel.open("売上.xlsx");
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
