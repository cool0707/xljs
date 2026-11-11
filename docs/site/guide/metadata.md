---
title: メタデータ
---

```js
// ==xljs==
// @mode    app            // console（既定）| app
// @html    ui/index.html  // 最初に開く HTML（スクリプトからの相対パス）
// @assets  ui/            // exe に同梱するフォルダ・ファイル（複数行可）
// @title   売上集計ツール  // ダイアログ・ウィンドウのタイトル（既定はスクリプト名）
// @size    960x640        // ウィンドウの大きさ
// @icon    ui/app.ico     // ウィンドウのアイコン（.ico か .png。exe にも同梱）
// @titlebar custom        // メインウィンドウを自前のタイトルバーにする（既定は system）
// @lang    ja             // メッセージの言語（--lang / XLJS_LANG が優先）
// ==/xljs==
```

- エントリのスクリプトの先頭（最初のコード行より前）の `//` コメントだけを読みます。知らないキーは警告を出して無視します。
- `@mode app` にすると、スクリプトの実行前に `@html` のページを表示するウィンドウ（`app.main`）が開き、ウィンドウを閉じるまでプロセスは終了しません。`app.open({ html, title, width, height })` で別のウィンドウも開けます（`await win.closed` で閉じるのを待てます）。画面とのやり取りは「[アプリ（HTML の画面）](#アプリhtml-の画面)」を参照。サンプル: `examples/app-hello/`、`examples/app-aggregate/`
- ウィンドウは Microsoft Edge WebView2 で表示します（Windows 10/11 には通常インストール済み）。ランタイムがない場合はダウンロードページを案内して終了します。ページは `https://app.xljs/` から配信され、見えるのは `@html` と `@assets` に書いたファイルだけです（pack した exe と同じ）。ほかの URL へのリンクは既定のブラウザで開きます。
- `@icon` を書くと、メインウィンドウ・`app.open` のウィンドウ・`ui.form` / `ui.table` のウィンドウのタイトルバーとタスクバーのアイコンがそのファイルになります（.ico は複数サイズ入りを推奨）。exe ファイル自体のアイコン（エクスプローラーでの表示）は変わりません。
- `titleBar: "custom"`（`app.open` / `ui.form` / `ui.table` のオプション。メインウィンドウはメタデータの `@titlebar custom`）にすると、PWA のように OS のタイトルバーを使わず、ページの上端に自前のタイトルバー（アイコン・タイトル・最小化／最大化／閉じるボタン）を表示します。ドラッグで移動、ダブルクリックで最大化でき、ライト・ダークの配色に従います。`icon` に Font Awesome のアイコン（`"table"` や `"fa-regular fa-calendar"`）を指定するとタイトルの前に表示します（`titleBar` が `"system"` のときは `icon` は無視）。`width` / `height` はタイトルバー（32px）を除いた大きさです。コンソール・モックでは無視されます。
