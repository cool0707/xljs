---
title: グラフ（ui.chart）
api: ChartSpec, ChartType, ChartSeries, ChartAxis, ChartWindowOptions, ChartView, ExcelChart
---

```js
const rows = sheet.toObjects();                              // [{ 月: "4月", 売上: 1200, 原価: 800 }, ...]
ui.chart({ type: "column", data: rows, x: "月", y: ["売上", "原価"], title: "月別売上" }); // 表示するだけ（すぐ次へ進む）
const view = ui.chart({
  type: "column", data: rows, x: "月",
  y: ["売上", { key: "利益率", type: "line", axis: "y2", format: "0%" }], // 組み合わせ・第 2 軸
  axes: { y: { title: "金額", format: "#,##0\"円\"" } },
  onClick(row, info) { alert(`${row.月} ${info.series}`); },
});
view.update({ data: rows.slice(0, 3) });                     // 指定したキーだけ変える
await view.closed;                                           // 閉じるまで待つ

// Excel のブックにあるグラフをそのまま表示する（値はセルから読む）
for (const c of workbooks.open("売上.xlsx").sheets[0].charts) {
  if (c.supported) ui.chart(c);                              // c.toSpec() で定義（ChartSpec）になる
  else console.log(c.name, c.warnings[0]);                   // レーダーなど表示できないもの
}

// ガントチャート（使い方は同じ。終了日はその日を含む）
ui.chart({
  type: "gantt", data: tasks, name: "作業", start: "開始", end: "終了", progress: "進捗", dependencies: "前提",
  view: "week", editable: true, holidays: [new Date(2026, 4, 3), { date: "2026-05-04", name: "みどりの日" }],
  onChange(row, changes) { /* row には書き込み済み。changes: { start, end } か { progress } */ },
});

// フォームの中に置く（ctx.chart で変える。editable なガントは値が全行）
await ui.form({
  fields: [{ name: "g", type: "chart", height: 12, chart: { type: "line", data: [], x: "月", y: "合計" } }],
  async onSubmit(v, ctx) { ctx.chart("g", { data: aggregate() }); },
});
```

- 種類（`type`）: `column`（縦棒、既定）`bar`（横棒）`line` `area` `scatter` `bubble`（系列に `size`）`histogram`（`bins`）`pie` `donut` `gantt`。`stacked: true` / `"percent"` で積み上げ。
- `data` は行オブジェクトの配列（`ui.table` と同じ）。`x` を省くと先頭の列、`y` を省くと数値の列すべて。日付の列は時間の軸になる（折れ線・面）。
- 書式は Excel の書式文字列（`format`、`axes.x/y/y2.format`、系列の `format`）。色は `colors`（既定は Excel と同じ Office の色）。
- ガントには進捗線（イナズマ線、`progressLine: true`）と出来高累計曲線（S カーブ、`sCurve: { weight: "金額", actual: [{ date, value }] }`）を重ねられる。基準日は `baseDate`（既定は今日）。
- ガントの列は省くと `名前`/`作業`・`開始`/`開始日`・`終了`/`終了日`・`進捗`・`前提` の列を探す。進捗は 0〜100（すべて 0〜1 なら割合）。前提は名前（`id` 列があればその値）を「,」区切りで。土日は既定で色が付く（`weekends: false` で消す）。
- 関数は `onClick` / `onChange` だけ使える（ほかのオプションは JSON で表せる値のみ）。ライブラリ固有の指定は `options`（AG Charts / Frappe Gantt のオプションに重ねる）。
- `sheet.charts` で読めないもの: レーダー・株価・等高線、Excel 2016 以降のウォーターフォール・ツリーマップ・サンバースト・ヒストグラム・箱ひげ図・じょうご・マップ（`supported: false`）。3-D は平面に、近似曲線・誤差範囲・データテーブル・補助円は省く（`warnings` に出る）。開始日の系列を塗りなしにした積み上げ横棒はガントチャートにする（`toSpec({ gantt: false })` で横棒のまま）。
- 画面（アプリモード）では `await xljs.chart("#el", spec)` で同じ定義を描ける（ハンドラーは画面側で動く）。
