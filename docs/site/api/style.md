---
title: 書式
api: CellStyle, StyleOptions, FontSpec, BorderSpec, BorderSide, BorderStyleName, AlignmentSpec, Color
---

```js
// 表の体裁をまとめて付ける（見出しの太字・塗り・罫線、列幅自動、先頭行固定、フィルター）
sheet.write(rows, { table: true, columnStyles: { 金額: { format: "#,##0" } } });

// 範囲・セル単位（指定した項目だけが既存の書式に上書きされる）
sheet.style("A1:D1", { bold: true, color: "#FFFFFF", background: "#305496", align: "center" });
sheet.cell("D10").style({ font: { size: 14, bold: true }, format: "¥#,##0", border: { top: "double" } });
sheet.range("A3:D10").border({ outline: "medium", inside: { style: "thin", color: "#BFBFBF" } });

sheet.merge("A1:D1");          // セル結合
sheet.freeze("A2");            // 1 行目を固定（freeze({ rows: 1, columns: 1 }) も可）
sheet.autoFilter("A3:D10");    // フィルター
sheet.autoFit();               // 列幅を内容に合わせる（全角は 2 文字分）
sheet.setRowHeight(1, 30);
sheet.cell("A1").getStyle();   // 現在の書式を取得
```

| 項目 | 指定方法 |
|---|---|
| `font` | `{ name, size, bold, italic, underline, strike, color, vertAlign }` |
| `fill` | `"#RRGGBB"`（`null` で塗りなし） |
| `border` | `"thin"` など（全セルの上下左右）、または `{ all, outline, inside, horizontal, vertical, top, bottom, left, right, diagonalUp, diagonalDown }`。各辺は `"thin"` `"medium"` `"thick"` `"double"` `"dashed"` `"dotted"` `"hair"` など、または `{ style, color }` |
| `alignment` | `{ horizontal, vertical, wrap, shrink, indent, rotation }` |
| `numberFormat` | `"#,##0"` `"yyyy/m/d"` `"0.0%"` `"¥#,##0"` など（`"General"` で標準） |
| 省略形 | `bold` `italic` `underline` `strike` `color` `fontSize` `fontName` `background` `align` `valign` `wrap` `format` |

- 色は `"#RRGGBB"`・`"#RGB"`・`red` などの色名で指定します。
- 既定では既存の書式にマージします。`{ replace: true }` を渡すと書式を置き換えます。
- 範囲の端に引いた罫線は、隣の（書式が設定済みの）セルの接する辺にも反映するので、線が食い違いません。
- サンプル: `examples/xlsx-style-report.js`
