---
title: JSON
api: JsonReadOptions, JsonWriteOptions
---

```js
const data = fs.readJSON(h);                          // パス・ハンドル・File・Blob。BOM・CP932 も自動判定
const withDates = fs.readJSON(h, { dates: true });    // ISO 形式の日時文字列を Date に戻す
fs.writeJSON("out.json", rows);                       // 2 スペースでインデント、UTF-8
fs.writeJSON("out.json", rows, { space: 0, encoding: "cp932", dates: "iso" });
fs.readJSONL("log.jsonl");                            // JSON Lines（1 行に 1 つの値）
fs.writeJSONL("log.jsonl", rows, { append: true });
import master from "./master.json";                   // import でも読める（exe に埋め込まれる）
```

- `Date` は既定でローカル時刻＋オフセット（`2026-04-01T09:00:00+09:00`）で書き出します。`dates: "iso"` で `JSON.stringify` と同じ UTC 表記になります。
- 解析エラーはファイル名（JSON Lines は行番号も）付きで表示します。
