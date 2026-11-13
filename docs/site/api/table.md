---
title: 表表示（ui.table）
api: TableOptions, TableSelectOptions, TableColumn, TableFormat, TableView, TableDoubleClickEvent
---

PowerShell の `Out-GridView` のように、配列を表で表示します。列見出しのクリックで並べ替え、上の欄で絞り込み（空白区切りで AND）、列の境界のドラッグで幅を変更、Ctrl+C でコピー（タブ区切り）、CSV / Excel に保存できます。数十万行でも表示できます。

```js
ui.table(rows, { title: "集計結果", columns: ["顧客名", "件数", "合計"] });   // 表示のみ。すぐに戻る
await ui.table(rows).closed;                                                   // 閉じるまで待つ
const picked = await ui.table(files, { title: "処理するファイル", selectable: "multiple" });
// 選んだ行（元のオブジェクト）の配列。キャンセルは []。1 行だけなら selectable: "single"
```

`columns` は省略するとオブジェクトのキー（配列の行は A, B, C…）。`{ key, label, width }` でも指定できます。

## 列の操作

列見出しにマウスを乗せると、フィルタ（≡）と列メニュー（⋮）のボタンが出ます。

- **列ごとのフィルタ**: 文字の列は「含む・含まない・等しい・で始まる・で終わる・空白」、数値の列は「等しい・未満・以下・より大きい・以上・範囲・空白」、日付の列は日付で比べます。条件は 2 つまで（AND / OR）。フィルタ中の列はボタンが青く残り、上の「列フィルタを解除」でまとめて外せます。上の絞り込み欄と一緒に使えます。
- **列の固定**: 列メニューの「左端に固定」「右端に固定」でその列を端に寄せ、横にスクロールしても見えたままにします。「この列まで固定」は Excel のウィンドウ枠の固定のように、左からその列までを固定します（並び順は変わりません）。
- **表示する列**: 「この列を非表示」と、列の一覧のチェックで列を隠します。「すべて表示」「初期状態に戻す」（列の表示・固定・幅と並べ替え・列フィルタを最初の状態に）もあります。
- 列の境界のドラッグで幅を変え、ダブルクリックで内容に合わせます。

コピーと CSV / Excel への保存は、表示している列を表示している順に、表示している行（並べ替え・絞り込みのあと）を書き出します。

## 条件付き書式

`formats` に規則を並べると、条件に合うセル（`row: true` なら行全体）に色・太字などを付けます。数値の列には Excel と同じようなデータバーとカラースケールも付けられます。複数の規則が当てはまるときは先の規則が優先です。

```js
ui.table(rows, {
  title: "売上一覧",
  formats: [
    { column: "金額", gte: 100000, color: "#9c0006", background: "#ffc7ce" },  // 10 万以上を赤く
    { column: "状態", equals: "未完了", row: true, bold: true },               // 行全体を太字
    { column: "金額", dataBar: true },                                         // データバー
    { column: "達成率", colorScale: ["#f8696b", "#ffeb84", "#63be7b"] },       // 赤 → 黄 → 緑
    { when: (row) => row.期限 < new Date(), color: "red", row: true },         // 関数で判定（行ごと）
  ],
});
```

条件は `equals` `notEquals` `gt` `gte` `lt` `lte` `between: [下限, 上限]` `contains` `notContains` `startsWith` `endsWith` `empty`（すべてを満たすとき）、または `when(value, row, index)` 関数です。書式は `color` `background` `bold` `italic` `underline` `strike` `dataBar`（`true` か色）`colorScale`（2 色か 3 色）。フォームの `table` 項目にも同じ `formats` が使えます。

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
