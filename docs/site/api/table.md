---
title: 表表示（ui.table）
api: TableOptions, TableSelectOptions, TableColumn, TableView, TableDoubleClickEvent
---

PowerShell の `Out-GridView` のように、配列を表で表示します。列見出しのクリックで並べ替え、上の欄で絞り込み（空白区切りで AND）、列の境界のドラッグで幅を変更、Ctrl+C でコピー（タブ区切り）、CSV / Excel に保存できます。数十万行でも表示できます。

```js
ui.table(rows, { title: "集計結果", columns: ["顧客名", "件数", "合計"] });   // 表示のみ。すぐに戻る
await ui.table(rows).closed;                                                   // 閉じるまで待つ
const picked = await ui.table(files, { title: "処理するファイル", selectable: "multiple" });
// 選んだ行（元のオブジェクト）の配列。キャンセルは []。1 行だけなら selectable: "single"
```

`columns` は省略するとオブジェクトのキー（配列の行は A, B, C…）。`{ key, label, width }` でも指定できます。

行やセルのダブルクリックには `onDoubleClick` で処理を登録できます（ネイティブ UI のみ）。

```js
ui.table(files, {
  title: "ファイル一覧",
  onDoubleClick(e) {            // e: { row, index, column, key, value, close() }
    if (e.column === "更新日時") alert(`${e.row.名前} の更新日時: ${e.value}`);
    else shell.openPath(e.row.handle);
  },
});
```サンプル: `examples/table-file-list.js`

フォームと表はネイティブ UI では WebView2 のウィンドウ、コンソール（`--ui=console`）では対話入力と罫線の表で表示します。モック UI（`--ui=mock`）では応答ファイルの `"form": [{ 項目名: 値 }]`（既定値との差分。表の項目は選ぶ行の番号の配列。`null` でキャンセル）と `"table": [[0, 2]]`（選ぶ行の番号）を使います。
