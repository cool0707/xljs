# 開発モードの体験版（サイトのトップページ）

`build.mjs` の `buildDemo` が `_site/demo/web/` に組み立てる。

| ファイル | 中身 |
|---|---|
| `dev-demo.js` | `dev.html` の前に入れる。`chrome.webview` を差し替えて Rust 側（`src/dev/mod.rs`）の代わりをする: メモリ上のプロジェクト（`form-master.xljs` 1 つ）、置き換え・履歴、ブラウザの枠の位置に `chat.html`、［実行］でページの中の窓にフォーム（`form.html?preview`）を開く。体験版にない操作は「体験版では使えません」 |
| `chat.html` | 架空の AI チャット（実在のサービスに似せない）。決まった答えを返し、［ダウンロード］で `examples/form-master.xljs` を渡す |
| `form-master.json` | `examples/form-master.xljs` の `ui.form` が `form.html` に送る最初のメッセージ。フォームの項目や文言を変えたら取り直す（下） |

`build.mjs` が作る `demo-data.js` には、開発モードの文言（ロケールの `devPage.*`・`window.*`）、`examples/form-master.xljs`（体験の前の版は一覧の `formats:` の行を消したもの）、`form-master.json` が入る。

`form-master.json` の取り直し（開発モードのプレビューと同じ仕組み。`examples` をコピーしたフォルダで実行する。一覧に単価のない行が出るように、`P007` を単価なしで足したデータで撮っている）:

```
cd <examples のコピー>
XLJS_DEV_PREVIEW=../tools/docs/demo/form-master.json xljs --lang=ja form-master.xljs
```
