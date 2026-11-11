---
title: アプリ（HTML の画面）
api: App, app, AppWindow, AppWindowOptions, AppElements, AppDomEvent, WireValue
---

`@mode app`（または `app.open`）のウィンドウでは、画面（HTML）とスクリプトが関数呼び出し・イベント・共有の値でやり取りできます。画面の `xljs` オブジェクトは自動で使えるようになります（`<script>` の読み込みは不要）。

```js
// ==xljs==
// @mode   app
// @html   ui/index.html
// @assets ui/
// ==/xljs==
const win = app.main;

// export した関数は画面から呼べる（this は呼び出し元のウィンドウ）
export async function aggregate({ month }) {
  win.state.progress = 0;                 // 共有の値: 画面の data-bind に反映される
  // …
  await win.invoke("render", summary);    // 画面の関数を呼ぶ（xljs.handle）
  win.emit("done", { count: 10 });        // 画面へイベント（xljs.on）
  return summary.length;                  // 戻り値は画面の Promise に返る
}
win.on("hello", (name) => console.log(name)); // 画面からのイベント（xljs.emit）
win.$("#status").text("準備完了").addClass("ok"); // 簡易 DOM 操作（送りっぱなし）
const title = await win.$("h1").text();          // 読み取りは await
const size = await win.evaluate(() => [innerWidth, innerHeight]); // 関数を画面で実行
app.handle("dynamic", (x) => x * 2);       // export 以外の名前で登録
```

```html
<!-- ui/index.html -->
<progress data-bind="progress" max="100"></progress>   <!-- win.state.progress を表示 -->
<span data-bind="summary.total"></span>                 <!-- ネストした値も表示できる -->
<div data-show="done">完了しました</div>                  <!-- 値が真のときだけ表示（!done で逆） -->
<form data-action="aggregate" data-result="count">       <!-- 送信でフォームの値を渡して呼ぶ -->
  <input name="month" type="month" required>
  <button>集計</button>
</form>
<input type="checkbox" data-model="tax">                 <!-- 入力と win.state.tax を双方向に同期 -->
<button data-action="cancel">中止</button>               <!-- フォーム外: 引数なし（data-args='[1,"a"]' で指定） -->
<script>
  xljs.handle("render", (rows) => { /* 表を描く */ return rows.length; });
  xljs.on("done", (r) => console.log(r.count));
  document.querySelector("#drop").addEventListener("drop", (e) => {
    e.preventDefault();
    xljs.invoke("load", e.dataTransfer.files);  // ファイルはスクリプトで実パス付きのハンドルになる
  });
</script>
```

- 画面→スクリプトの呼び出しは常に非同期（`await xljs.invoke(...)`）。スクリプト→画面は、読み取り（`invoke` `evaluate` `$().text()` など）は `await`、書き込み（`state` `emit` `$().text(v)` など）は送りっぱなしで、送った順に反映されます。ページの読み込み前に送ったものは読み込み後に届きます。
- 受け渡せる値は JSON の値と `Date`、ファイル/フォルダのハンドル、`ArrayBuffer`、`Error` です。画面で選んだ・ドロップしたファイル（`File`）はスクリプトで `FileSystemFileHandle`、ドロップしたフォルダは `FileSystemDirectoryHandle` になります（WebView2 113 以降）。
- `win.state` / `xljs.state` は最上位のキーだけ同期します（`win.state.summary = {...}` のように代入し直す）。相手側で値が変わると `on("state", (key, value) => ...)` が呼ばれます。
- `data-action` の関数がエラーになると、エラーのダイアログを表示します（スクリプトは続行）。`xljs.invoke` のエラーは Promise の reject で、`catch` しなければ同じくダイアログになります。スクリプトの `win.on` / `$().on` のハンドラのエラーも同様です。
- `win.$(sel).on(type, (e) => ...)` の `e` は DOM イベントではなく値（`type` `id` `name` `value` `checked` `dataset` `key`、submit ならフォームの値 `values`）。`{ preventDefault: true }` で既定の動作を止めます。
- `data-action` の実行中は要素が無効になり、`xljs-busy` クラスが付きます。
- `titleBar: "custom"` のウィンドウでは、タイトルバーが `<body>` の先頭に差し込まれます（高さは CSS 変数 `--xljs-titlebar-height`。`height: 100vh` のレイアウトは `calc(100vh - var(--xljs-titlebar-height))` に）。色は `--xljs-titlebar-bg` `--xljs-titlebar-fg` `--xljs-titlebar-accent` で変えられます。ページに `data-xljs-titlebar` 属性の要素があれば差し込まずにそれをタイトルバーとして使い（ドラッグで移動）、`data-xljs-window="minimize"` / `"maximize"` / `"close"` を付けたボタンでウィンドウを操作できます。

```html
<header data-xljs-titlebar class="my-bar">
  <span>売上集計</span>
  <button data-xljs-window="minimize">_</button>
  <button data-xljs-window="close">×</button>
</header>
```

- アイコンには同梱の [Font Awesome Free](https://fontawesome.com/icons)（7.3.1）が使えます。`<link rel="stylesheet" href="/_xljs/fontawesome/css/all.min.css">` を書き、`<i class="fa-solid fa-file-excel"></i>` のように使います（インターネット接続は不要）。

モック UI では応答ファイルの `"app"` にウィンドウごとの操作の列を書くと、画面の代わりに実行してログに出します（サンプル: `testdata/scripts/bridge.test.answers.json`）。

```json
{ "app": [[
  { "invoke": "aggregate", "args": [{ "month": "2026-04" }], "expect": 3 },
  { "emit": "hello", "args": ["画面"] },
  { "state": { "tax": true } },
  { "event": "click", "selector": "#run" },
  { "handle": "render", "result": 3 },
  { "close": true }
]] }
```
