# API のページの画面の写真に使うメッセージ

`screenshots.mjs` が組み込みのページ（`form.html`）に渡す、スクリプトの `ui.form` の最初のメッセージ。開発モードのフォームのプレビューと同じ仕組みで、サンプルを実際に実行して取ったもの。フォームの項目・文言・サンプルを変えたら取り直してから `node screenshots.mjs` で撮り直す。

| ファイル | サンプル | 写真 |
|---|---|---|
| `form-aggregate.json` | `examples/form-aggregate.xljs` | `form-tool.png`（実行のあとの進捗とログは `screenshots.mjs` が送る） |
| `form-tabs-table.json` | `examples/form-tabs-table.xljs` | `form-tabs.png` |
| `chart-gantt.json` | `examples/chart-gantt.xljs` | `chart-gantt.png`（今日は `screenshots.mjs` の `TODAY`） |

取り直し（`examples` をコピーしたフォルダで。ガントのサンプルは最初にブックを選ぶので、モックの応答で `data/工程表.xlsx` を渡す。日付がずれないように時間帯は UTC で）:

```
cd <examples のコピー>
echo '{"showOpenFilePicker":[["data/工程表.xlsx"]]}' > a.json
D=<リポジトリ>/tools/docs/shots
TZ=UTC XLJS_DEV_PREVIEW=$D/form-aggregate.json xljs --ui=mock --lang=ja form-aggregate.xljs
TZ=UTC XLJS_DEV_PREVIEW=$D/form-tabs-table.json xljs --ui=mock --lang=ja form-tabs-table.xljs
TZ=UTC XLJS_DEV_PREVIEW=$D/chart-gantt.json xljs --ui=mock --ui-answers=a.json --lang=ja chart-gantt.xljs
```

テーブル（`table-formats.png`・`table-filter.png`）は、`ui.table` が `table.html` に送るメッセージ（`prelude.js` の `tableWindow`）を `screenshots.mjs` の中で組み立てている。グラフのウィンドウ（`home-chart.png`）と印刷プレビュー（`home-print.png`）はトップページと同じ写真。
