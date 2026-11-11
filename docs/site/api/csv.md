---
title: CSV
api: Csv, csv, CsvParseOptions, CsvReadOptions, CsvStringifyOptions, CsvWriteOptions, Row
---

```js
const rows = csv.read(h);                         // 見出し行をキーにしたオブジェクトの配列。数値・日付・真偽値は変換、空欄は null
const grid = csv.read("a.tsv", { header: false });// 2 次元配列（.tsv はタブ区切り）
const strs = csv.read(h, { cast: false });        // 変換せず、すべて文字列のまま

csv.write("out.csv", rows);                       // 既定は BOM 付き UTF-8・CRLF
csv.write("out.csv", rows, { encoding: "cp932", columns: ["日付", "金額"] });
const parsed = csv.parse(text);                   // 文字列から（read と同じオプション）
const out = csv.stringify(rows);                  // 文字列に（write と同じオプション）
```

- 値の変換（既定）: `1234`・`-2.5`・`"1,234"` は数値、`2026/4/1`・`2026-04-01 09:30` は `Date`、`TRUE`/`FALSE` は真偽値、空欄は `null`。Excel で開いたときに近い結果ですが、`00123` のように先頭が 0 のコード、16 桁以上の数字、`123-4567` などは文字列のままです。
- 見出しの前後の空白は取り除きます（`"金額 "` も `r.金額` で読めます）。
