---
title: 書式
api: CellStyle, StyleOptions, BorderSpec, BorderSide, BorderStyleName, Color
---

```js
// 表の体裁をまとめて付ける（見出しの太字・塗り・罫線、列幅自動、先頭行固定、フィルター）
sheet.write(rows, { table: true, columnStyles: { 金額: { format: "#,##0" } } });

// 範囲・セル単位（指定した項目だけが既存の書式に上書きされる）
sheet.setStyle("A1:D1", { bold: true, color: "#FFFFFF", background: "#305496", align: "center" });
sheet.cell("D10").setStyle({ fontSize: 14, bold: true, format: "¥#,##0", border: { top: "double" } });
sheet.range("A3:D10").setStyle({ border: { outline: "medium", inside: { style: "thin", color: "#BFBFBF" } } });

sheet.merge("A1:D1");          // セル結合（sheet.merges で結合範囲の一覧）
sheet.freeze("A2");            // 1 行目を固定（freeze({ rows: 1, columns: 1 }) も可。sheet.frozen で読める）
sheet.autoFilter("A3:D10");    // フィルター（null で外す。sheet.filter で範囲を読める）
sheet.autoFit();               // 列幅を内容に合わせる（全角は 2 文字分）
sheet.setRowHeight(1, 30);     // getRowHeight(1) で読める
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
- 既定では既存の書式にマージします。`{ replace: true }` を渡すと書式を置き換えます。
- 範囲の端に引いた罫線は、隣の（書式が設定済みの）セルの接する辺にも反映するので、線が食い違いません。
- サンプル: `examples/xlsx-style-report.xljs`
