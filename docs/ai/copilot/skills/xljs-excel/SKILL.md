---
name: xljs-excel
description: xljs の Excel（excel.open・excel.create。ブック・シート・セルの読み書き、表の読み書き、セルの検索、結合セル、数式、書式）のリファレンス。xljs のスクリプトで Excel のブックを読む・書くときに使う。
---

# xljs API リファレンス: Excel

xljs の Excel の読み書きです。基本の API（ファイル・CSV・JSON・フォーム・表・日付など）とレシピはスキル `xljs-reference`、PDF への書き出しと印刷は `xljs-pdf` にあります。ここに書いていない関数・オプションは存在しないものとして扱ってください。

## Excel

対応形式は `.xlsx` / `.xlsm`。`.xls` は不可。マクロ・ピボットテーブルは保存で失われることがある。

### ブック

| API | 説明 |
|---|---|
| `excel.open(p)` | 開く |
| `excel.create({ font? })` | 新しいブック。最初の `addSheet` が既定のシートを置き換える |
| `book.sheets[0]` / `book.sheets["名前"]` / `book.sheet(名前か番号)` | シート（無ければ `undefined` / `null`）。大文字小文字・空白・全角半角の違いは許す。`book.sheet(/^\d+月$/)` は名前が一致する最初のシート |
| `book.sheetNames` / `book.addSheet(name)` / `book.removeSheet(name)` / `book.copySheet(sheet, name?)` | シート名の配列 / 追加・削除・コピー（元の後ろに） |
| `book.save()` / `book.save(p)` / `book.close()` | 上書き / 別名で保存 / 閉じる（メモリ解放） |
| `await book.toPdf(p, options?)` / `await sheet.toPdf(p, options?)` | PDF に（「PDF 出力」） |
| `excel.readVBA(p)` | マクロブックの VBA コード `{ name, modules: [{ name, type, code, sheet }], text }`（`text` は Markdown）か `null` |
| `excel.columnName(3)` → `"C"`、`excel.cellName(2, 3)` → `"C2"` | 番地の変換 |

### シートの読み取り

| API | 説明 |
|---|---|
| `sheet.toRowObjects({ header, columns, text, skipEmpty })` | 行をオブジェクトに。`header` は見出しの行（既定 1）。`header: 0, columns: [...]` で見出しなし |
| `sheet.toRowObjects({ start, untilBlank, pick })` | 途中にある表。`start` は見出しの左端（`"B4"` か `find` の結果）、`untilBlank: true` で空行まで、`pick` で列を見出しから選んでキーを付け替える |
| `sheet.find(x)` / `sheet.findAll(x)` | 値が一致するセル（`Cell` か `null` / 配列）。`sheet.columns("A").find(x)`・`sheet.range("A1:F20").find(x)` で範囲を絞る |
| `cell.offset(行, 列)` | 下・右に進んだセル（VBA の `Offset`）。`cell.row` / `cell.column` / `cell.address` |
| `sheet.toArrays({ text })` | 2 次元配列（0 始まり）。`text: true` で表示の文字列 |
| `sheet.cell("B2")` / `sheet.cell(行, 列)` | セル（1 から） |
| `cell.value` / `cell.text` / `cell.formula` | 値（number / string / boolean / Date / null）/ 表示どおりの文字列 / `"=SUM(A1:A3)"` か `null` |
| `cell.phonetic` / `cell.note` / `cell.comment` | ふりがな（値の後に代入）/ メモ / コメント（`addComment`） |
| `sheet.lastRow()` / `sheet.lastColumn()` / `sheet.columns("A").lastRow()` | 最終行 / 最終列 / A 列の最終行（`{ skipBlank: true }` で `""` を返す数式を除く） |
| `sheet.dimension` / `sheet.merges` / `sheet.frozen` / `sheet.filter` | 値のある範囲 / 結合範囲の配列 / 固定 `{ rows, columns }` か `null` / フィルターの範囲か `null` |
| `sheet.range("A1:C3").values` / `sheet.getStyle("A1")` | 範囲の 2 次元配列（代入で書く）/ 書式（`setStyle` と同じ形） |
| `sheet.name` / `sheet.hidden` / `sheet.rows(3).hidden` | シート名 / 非表示（行・列も）。代入で変更 |

- `find` に渡すもの: 文字列（セル全体の一致。全角半角・大文字小文字・空白の違いは区別しない。`{ match: "partial" }` で一部、`{ strict: true }` で区別）、正規表現、数値・`Date`、関数 `(cell) => 真偽`。`{ visibleOnly: true }` で非表示の行・列を除く。
- `pick` の条件: 見出しの文字列（全角半角・空白の違いは区別しない）、正規表現、位置（0 から）、`{ header, nth, offset, optional }`。見つからない見出しはエラー（`optional: true` で `null`）。
- 結合セルは、中のどのセルを指定しても左上のセルとして読み書きする（`A1:C1` が結合なら `cell("B1").value` は A1 の値）。`toRowObjects`・`toArrays` は結合のどの行・列にも同じ値を返す（縦に結合した「部署」は各行に入る）。結合セルからの `offset` は範囲の端から数える。
- 数式セルの `value` は保存された計算結果。書いたら `book.calculate()` で入る（`skipped` は計算できないもの）。`cell.calc()` は数式 1 つの結果。`=FILTER(...)` はスピルする。

```js
const name = sheet.find("氏名")?.offset(0, 1).value;         // 見出しの右のセル（帳票から値を拾う）
const row = sheet.columns("A").find("A-1024")?.row;           // A 列でコードのある行
const rows = sheet.toRowObjects({ start: sheet.find("コード") ?? "B4", untilBlank: true,
  pick: { code: "コード", name: "品名", amount: /金額|売上/ } });   // [{ code, name, amount }]
```

### シートへの書き込み

| API | 説明 |
|---|---|
| `sheet.write(rows, options)` | オブジェクトの配列（見出し＋データ）か 2 次元配列を書く |
| `sheet.append(rows, options)` | 最終行の下に追記（見出しは書かない） |
| `cell.value = v` / `cell.formula = "=..."` | 1 セル |
| `sheet.rows(5).insert(n?)` / `sheet.rows("2:4").delete()` / `sheet.columns("C").insert(n?)` | 行・列の挿入・削除（数式の参照も動く） |

`write` のオプション:

| オプション | 説明 |
|---|---|
| `start` | 書き始めのセル（既定 `"A1"`） |
| `matchHeader` | いまある見出しに合わせて書く（キーと同じ見出しの列にだけ。見出しは書かず、ほかの列はそのまま）。`true`（`append` は 1 行目、`write` は `start`）・見出しの行番号・左端のセル `"B3"`。例 `sheet.append(rows, { matchHeader: true })` |
| `header` | 見出し行を書くか（既定 `true`） |
| `columns` | 書く列と順番（`["日付", "金額"]`） |
| `table` | 表の体裁（見出しの太字・塗り・罫線、データの罫線、列幅自動、先頭行固定、フィルター） |
| `headerStyle` / `style` / `columnStyles` | 見出し / 全体 / 列ごとの書式（`{ 金額: { format: "#,##0" } }`） |
| `autoFit` / `freeze` / `filter` | 列幅自動 / 見出しの固定 / フィルター（`table` では既定で有効） |

### 書式

```js
sheet.setStyle("A1:D1", { bold: true, color: "#FFFFFF", background: "#305496", align: "center" });
sheet.cell("D10").setStyle({ fontSize: 14, bold: true, format: "¥#,##0", border: { top: "double" } });
sheet.range("A3:D10").setStyle({ border: { outline: "medium", inside: { style: "thin", color: "#BFBFBF" } } });
sheet.merge("A1:D1");            // セル結合（unmerge で解除）
sheet.freeze("A2");              // 1 行目を固定（"B2" で 1 行目と A 列）
sheet.autoFilter("A3:D10");      // フィルター（null で外す）
sheet.autoFit();                 // 列幅を内容に合わせる
sheet.columns("A:C").width = 16; // 列幅（文字数）
sheet.rows(1).height = 30;       // 行の高さ（ポイント）
```

| 項目 | 指定 |
|---|---|
| 文字 | `bold` `italic` `underline` `strike` `color` `fontSize` `fontName` |
| 塗り | `background: "#DDEBF7"`（`null` で塗りなし） |
| 配置 | `align`（`left` `center` `right`）、`valign`（`top` `center` `bottom`）、`wrap: true`（折り返し） |
| 表示形式 | `format`: `"#,##0"` `"#,##0.00"` `"0.0%"` `"¥#,##0"` `"yyyy/m/d"` `"yyyy年m月d日"` `"h:mm"` `"@"`（文字列） |
| 罫線 | `border: "thin"`（全セルの上下左右）、または `{ outline, inside, horizontal, vertical, top, bottom, left, right }`。線種 `"thin"` `"medium"` `"thick"` `"double"` `"dashed"` `"dotted"`、色付きは `{ style: "thin", color: "#999999" }` |

色は `"#RRGGBB"` か `red` などの色名。書式は既存の書式に重なる（`{ replace: true }` で置き換え）。入れ子のキー（`font` `fill` など）はない。

