# 引き継ぎ: HTML UI・簡易フォーム・表表示・デバッガ

チャットで検討・合意した設計と実装計画。ここから Claude Code で実装を進める。
既存コードの構成と規約は `/CLAUDE.md` を参照。

## 0. 現状（2026-10-22 時点、main = 09598f4）

実装済み: スクリプト実行（esbuild→goja、TLA、ソースマップ付きエラー）、ファイル/フォルダ API と walk、
CSV/JSON/JSONL、xlsx 読み書き＋書式設定、ネイティブダイアログ（MessageBox、IFileDialog、入力、
IProgressDialog、コピー可能なエラーダイアログ）、エクスプローラー連携、exe 埋め込み（pack）、
多言語化（locales/*.json）、アイコン、CI。

今回の対象（未着手）:

1. **アプリモード** — HTML で画面を作る。スクリプトと画面が相互に関数呼び出し・イベント・状態共有
2. **簡易フォーム `ui.form`** — HTML を書かずに入力フォーム（ファイル/フォルダ/設定＋実行ボタン）
3. **表表示 `ui.table`** — PowerShell の Out-GridView 相当（並べ替え・絞り込み・コピー・選択して返す）
4. **デバッガ** — PowerShell ISE のようなエディタ一体型。サイズが大きくなりすぎるなら見送り可

## 0.1 進捗

- **フェーズ 1（基盤）: 実装済み**（ブランチ `claude/xljs-handoff-ui-phase1-rd6ht0`）。デバッガ（フェーズ 4）は見送り。
  - スレッド分離: `ui.Run`（`internal/ui/thread.go`）。スクリプトは専用 goroutine、UI 呼び出しは main スレッドへ転送して同期で待つ。
    Native は message-only ウィンドウ（`thread_windows.go`）経由で実行するのでモーダルダイアログ中も処理される。
    console/mock は Go のキュー（`chanThread`）で同じ構造。`Progress.Update` と `Window.Post` は待たない（順序は保証）。
    Native では `Progress.Cancelled` を 50ms キャッシュ（往復コスト削減）。ダイアログの親は最後に開いた HTML ウィンドウ。
  - WebView2: `internal/ui/webview_windows.go`。`UI.OpenWindow(WindowOptions) (Window, error)` を追加（Console はエラー、Mock は
    ログを出して即クローズ）。ランタイム有無は `WebView2Version()`、無ければ `ui.webview2Missing`（ダウンロード URL 付き）。
    アセットは `https://app.xljs/*` の WebResourceRequested で `fs.FS` から配信（pack 時は zip、非 pack 時は `meta.AssetFS`）。
  - メタデータ: `internal/meta`。`@mode app` でスクリプト実行前に `app.main` を開く。JS API は最小限の `app.open()` / `win.closed` / `win.close()`。
  - pack v2: `internal/pack`（zip＋`XLJSPAK2`、v1 も読める）。`@lang` は `--lang` / `XLJS_LANG` が無いときだけ適用。`--debug`（`XLJS_DEBUG`）で開発者ツール。
  - テスト: `internal/ui/thread_test.go`（転送先スレッド）、`engine_test.go` の `TestAppMode` / `TestAppWindowKeepsRunning`、
    `testdata/scripts/app.test.js`、Windows CI では `TestWebViewWindow`（ランタイムがあれば実ウィンドウを開いて閉じる）。

### 1.1 の調査結果（go-webview2）

- `jchv/go-webview2` は WebResourceRequested はあるが、追加オブジェクト（ファイル実パス）などが無い。
  **`wailsapp/go-webview2` v1.0.23 の `pkg/edge` を採用**: WebMessageReceived、WebResourceRequested、
  `GetAdditionalObjects`（ランタイム 113.0.1774.30 以上。`ICoreWebView2File.GetPath` で実パス）、純 Go のローダー（DLL 埋め込み不要）。exe 増分は約 0.5MB。
- このラッパーの癖（回避済み・今後も注意）:
  - ページからのメッセージを**そのままページへ送り返す**（`MessageReceived` 内の `PostWebMessageAsString`）。ホスト→画面の
    メッセージには先頭に `\x02` を付けている（`hostPrefix`）ので、画面側ブリッジは `\x02` で始まらないメッセージを無視すること。
  - `ICoreWebView2Environment.CreateWebResourceResponse(content, ...)` は変数のシャドーイングで本文が空になる。
    本文は `nil` で作ってから `PutByteContent` で入れる。
  - エラー時はコールバックを呼んだあと `os.Exit(1)`。環境作成の失敗時は nil 参照で panic するので `recover` している。
  - 標準 `log` に出力するので `log.SetOutput(io.Discard)` している（xljs は標準 log を使わない）。
  - NavigationStarting / NewWindowRequested のハンドラが無い。外部 URL を既定ブラウザで開く処理（2.6）は
    `pkg/edge` を拡張するか自前で COM ハンドラを書く必要がある（フェーズ 3）。

## 1. 共通基盤

### 1.1 WebView2

- ライブラリ: `github.com/jchv/go-webview2`（純 Go・cgo 不要、WebView2Loader.dll を埋め込み可能）。
  WebResourceRequested など足りない機能があれば `pkg/edge` を直接使うか、Wails のフォーク
  （`github.com/wailsapp/go-webview2`）を検討。**最初にこの点を調査してから設計を確定する。**
- WebView2 ランタイムは同梱しない。未インストール時は案内（i18n メッセージ＋ダウンロード URL）を出して終了。
- exe の増分見込み: WebView2 組み込み <1MB、フォーム/表の HTML/CSS/JS ≈0.2MB、エディタ/デバッガ ≈1MB。
  現状 exe ≈24MB。

### 1.2 スレッド構成（要リファクタリング）

現状は main スレッド（`runtime.LockOSThread`）で goja もダイアログも動いている。重い処理中に画面が
固まるため次の形に変える。

- **UI スレッド** = main スレッド: Win32 メッセージループ、WebView2、ネイティブダイアログ、IProgressDialog。
- **スクリプトスレッド** = 専用 goroutine（`LockOSThread`）: goja とイベントループ（`engine/loop.go`）。
- スクリプトからの UI 呼び出し（`ui.UI` の各メソッド）は UI スレッドへ依頼して同期で待つ
  （ダイアログの親はアプリのウィンドウ）。UI → スクリプトは `loop.post`（既存）で渡す。
- コンソール/mock では従来どおりでもよいが、構造は共通化しておく。
- 注意: COM（STA）の初期化スレッド、`testAutoClose` を使う既存テスト、Ctrl+C 処理が壊れないこと。

### 1.3 先頭コメントのメタデータ

```js
// ==xljs==
// @mode    app            // console（既定）| app
// @html    ui/index.html  // 最初に開く HTML（スクリプトからの相対）
// @assets  ui/            // exe に同梱するフォルダ（複数行可）
// @title   売上集計ツール
// @size    960x640
// @lang    ja             // --lang と同じ
// ==/xljs==
```

- エントリファイルの先頭コメントだけを読む（バンドル前に自前でパース）。未知のキーは警告。
- **`@ui` ではなく `@mode`**（CLI の `--ui=native|console|mock` と衝突するため）。
- `ui.form` / `ui.table` はどのモードでも使える関数なのでモードには含めない。

### 1.4 pack 形式 v2

- 現行: 末尾に「名前＋バンドル済みコード」。v2 では zip（`manifest.json`、`main.js`、`assets/...`）を
  載せる。マジックを `XLJSPAK2` にして v1 も読めるようにする。
- 実行時、アセットはメモリから `https://app.xljs/` として配信（WebResourceRequested）。
  非 pack 時はディスクのフォルダを同じ URL で配信（SetVirtualHostNameToFolderMapping でも可）。

## 2. アプリモード

### 2.1 方針（合意済み）

- 画面とスクリプトのやり取りはすべてメッセージ。**画面→スクリプトは常に非同期**。
- **スクリプト→画面: 読み取りは `await`、書き込みは送りっぱなしで順序保証**（await 不要）。
- 実装順: ① 相互呼び出し → ④ データバインド → ③ 簡易 DOM → ② evaluate。
- **完全な DOM 代理（`win.document...`）はやらない**（往復コスト、参照リーク、同期/非同期の混在）。

### 2.2 命名（合意済み・両側でそろえる）

| 用途 | スクリプト側 | 画面側 |
|---|---|---|
| 相手の関数を呼ぶ | `await win.invoke(name, ...args)` | `await xljs.invoke(name, ...args)` |
| 呼ばれる関数を公開 | エントリの `export function`（基本）/ `app.handle(name, fn)`（動的） | `xljs.handle(name, fn)` |
| イベント | `win.emit` / `win.on` / `win.off` | `xljs.emit` / `xljs.on` / `xljs.off` |
| 状態共有 | `win.state` | `xljs.state`、`data-bind` / `data-model` |
| 画面で関数を実行 | `await win.evaluate(fn, ...args)` | — |
| 簡易 DOM | `win.$(sel).text(...)`、`win.$(sel).on("click", fn)` | — |
| ボタン→関数 | — | `data-action="aggregate"` |

- **公開は ESM の `export`**（CommonJS の `exports` は採用しない）。`export * from "./handlers.js"` 可。
  画面から呼べるのはエントリの export と `app.handle` 登録分のみ。同名が両方にあれば起動時エラー。
- `win.invoke` は `xljs.handle` 登録を優先し、無ければ画面の `window[name]`（関数）を呼ぶ。
- DOM イベントは `win.$(sel).on(...)`。`win.on(event, fn)` は独自イベント専用（引数の形を分ける）。
- `app.run()` は廃止。`@mode app` ならスクリプト実行前にメインウィンドウを自動で開き、
  ウィンドウが開いている間はプロセスを終了しない（イベントループの pending として数える）。

```js
// 最小のアプリ
// ==xljs==
// @mode app
// @html ui/index.html
// ==/xljs==
export async function aggregate(month) { /* … */ return summary; }
export function pickFolder() { return showDirectoryPicker(); }

// 追加のウィンドウ
const win = await app.open({ html: "ui/sub.html", title: "詳細", width: 600, height: 400 });
await win.closed;      // Promise
app.main;              // メインウィンドウ
```

```html
<!-- 画面側: xljs オブジェクトは自動注入 -->
<progress data-bind="progress" max="100"></progress>
<span data-bind="message"></span>
<form>
  <input name="month" type="month" data-model="month">
  <button data-action="aggregate">実行</button>  <!-- form 内なら引数はフォーム値のオブジェクト -->
</form>
<script>
  xljs.handle("render", (rows) => { /* … */ return rows.length; });
  xljs.on("progress", (p) => { /* … */ });
</script>
```

### 2.3 簡易 DOM（`win.$`）

- 対象は `text` `html` `val` `attr` `css` `addClass` `removeClass` `toggleClass` `show` `hide` `on` `off`。
- セレクタ単位で 1 往復。複数要素は一括適用。読み取り系は先頭要素、`await` で値。
- イベントは要素ではなく値に変換して渡す（`type` `value` `checked` `dataset` `key` `id` `name`、
  submit ならフォーム値オブジェクト）。`preventDefault` は登録時オプション（`{ preventDefault: true }`）。

### 2.4 evaluate

- 関数を `toString` して画面で実行、引数は構造化コピー（下記コーデック）。クロージャ不可。
- スクリプト→画面の一方向のみ（画面からスクリプトを任意実行する経路は作らない）。

### 2.5 メッセージとコーデック

- WebView2 の `postMessage` / `WebMessageReceived` 上に JSON-RPC 風の形式:
  `{ id, type: "invoke"|"result"|"error"|"event"|"state", name, args, value }`。
- 値: JSON ＋ `Date`（`{"$date": iso}`）、`ArrayBuffer`（base64）、ファイル/フォルダハンドル
  （`{"$file": path}` / `{"$dir": path}` → スクリプト側でハンドルに復元）、Error（name/message/stack を再構成）。
- 画面上で選択・ドロップしたファイルの**実パス**: WebView2 の
  `chrome.webview.postMessageWithAdditionalObjects` で File を渡すとホスト側でパスが取れる
  （必要な WebView2 バージョンは実装時に確認）。画面側に `xljs.files(fileList)` のような補助を用意し、
  スクリプトでは `FileSystemFileHandle` として受け取れるようにする。

### 2.6 安全・その他

- 表示は同梱ページ（`https://app.xljs/`）のみ。外部 URL への遷移は既定ブラウザで開く。
- 開発者ツール（F12）とコンテキストメニューは `--debug` 時のみ。
- スクリプト側エラー（invoke 先で throw）は画面側の Promise を reject。未処理なら通常のエラー表示。
- ウィンドウのアイコンは exe のアイコン、タイトルは `@title` → スクリプト名。

## 3. 簡易フォーム `ui.form`

決まった CSS で、項目定義だけからフォームを作る（内部はアプリモードの仕組み＋同梱テンプレート）。

```js
const v = await ui.form({
  title: "売上集計",
  description: "フォルダ内の Excel を集計します",
  fields: [
    { name: "src",  label: "集計フォルダ", type: "folder", required: true },
    { name: "out",  label: "出力先", type: "saveFile", accept: [".xlsx"], default: "集計.xlsx" },
    { name: "mode", label: "集計単位", type: "select", options: ["顧客別", "商品別"] },
    { name: "tax",  label: "税込で集計", type: "checkbox" },
  ],
  submit: "実行",
  remember: true,            // 前回値を %APPDATA%\xljs\<スクリプト名>.json に保存・復元
});
if (!v) exit();              // キャンセルは null。folder/file 系の値はハンドル
```

- 型: `text` `password` `number`（min/max/step）`date` `month` `time` `select` `radio` `checkbox`
  `textarea` `file` `files` `folder` `saveFile`。ファイル系はドラッグ＆ドロップ可、ピッカーボタン付き。
- 検証: `required`、`pattern`、`validate(values) => { field: "メッセージ" }`（スクリプト側で実行）。
- 実行後も開いたままにする形: `onSubmit: async (values, ctx) => { ctx.progress(i, n); ctx.log("…"); }`。
  ウィンドウ内に進捗・ログ・完了後の「フォルダを開く」ボタンを表示。キャンセルで `ctx.cancelled`。
- コンソール UI: 対話入力に置き換え。mock: 応答 JSON の `"form": [{...values}]`（null でキャンセル）。

## 4. 表表示 `ui.table`

```js
ui.table(rows, { title: "集計結果", columns: ["顧客名", "件数", "合計"] });           // 表示のみ
const picked = await ui.table(rows, { selectable: "multiple", title: "処理するファイル" }); // 選択を返す
```

- 列クリックで並べ替え、絞り込み欄、列幅変更、コピー（TSV）、CSV/Excel に保存。
- 大量行は仮想スクロール（表示行だけ描画）。数十万行を目標。
- `selectable` 指定時は OK/キャンセル付き。選択行（元のオブジェクト）を返す。キャンセルは `[]`。
- 表示のみの場合はウィンドウを開いて即座に戻る（プロセスは閉じるまで残る）。`await ... .closed` 可。
- コンソール UI: `console.table` 相当を出力（選択モードは番号入力）。mock: ログ＋応答 `"table": [[0, 2]]`（選択インデックス）。
- WebView2 で実装（フォーム・アプリと見た目をそろえる）。Win32 ListView は採用しない。

## 5. デバッガ（最後・見送り可）

- 公式 goja にはデバッガ API がない。フォークの先行例 `mostafa/goja`（debugger ブランチ）と CLI
  `goja_debugger` はブレークポイント・step・式評価に対応していたが 2023-01 で開発終了。
  → goja をフォークして命令実行ごとのフック（行判定・停止・スコープ取得）を自前で入れる必要がある。
  フォーク維持の負担を見積もってから着手判断。
- ブレークポイントは esbuild のソースマップで元ファイルの行に対応付け。
- UI: `xljs edit script.js` で WebView2 のエディタ一体型画面。エディタは CodeMirror 6（≈0.5〜1MB）。
  画面: エディタ／ブレークポイント／変数・コールスタック／コンソール。補完は `xljs.d.ts` から生成した
  候補による簡易なもの（TypeScript 言語サービスは入れない）。
- 同じフックの上に DAP（Debug Adapter Protocol）を載せれば VS Code からも使える（後回し）。

## 6. テスト方針

- WebView2 は Linux/CI で動かないので、**ウィンドウ層をインターフェース化**し、mock 実装で
  画面側の操作をシナリオとして流せるようにする。例（answers JSON）:
  ```json
  { "app": [
      { "invoke": "aggregate", "args": ["2026-04"], "expect": { "ok": true } },
      { "event": "click", "selector": "#run" },
      { "close": true } ] }
  ```
  mock は `win.state` の変更・`win.$` 操作・`emit` をログに出す → `testdata/scripts/*.test.js` で検証。
- コーデック（Date/ハンドル/Error）とメタデータのパース、pack v2 は Go の単体テスト。
- Windows CI では WebView2 ランタイムがあれば実ウィンドウを開いて自動で閉じる統合テスト
  （既存の `testAutoClose` と同じ考え方）。
- 型定義（`types/xljs.d.ts`）を更新し、`examples/` が tsc で通ることを確認。
- i18n: 新しい文言は `en.json`/`ja.json` に追加（テストがキーとプレースホルダを検査）。

## 7. 実装フェーズと完了条件

1. **基盤**: スレッド分離、WebView2 組み込み（ランタイム有無の判定と案内）、メタデータのパース、pack v2。
   完了条件: 既存テストがすべて通る／空の `@mode app` スクリプトでウィンドウが開いて閉じると終了する。
2. **`ui.table` と `ui.form`**: 完了条件: mock とコンソールで動作、Windows で実ウィンドウ確認、サンプル追加。
3. **アプリモード**: 相互呼び出し → データバインド → 簡易 DOM → evaluate、ファイル実パス受け渡し、
   pack v2 でアセット同梱。完了条件: サンプルアプリ（フォルダ選択→集計→進捗表示→結果表示）が pack 後も動く。
4. **デバッガ**: goja フォークの調査・判断 → フック → エディタ画面。

## 8. 未決事項・リスク

- go-webview2 で WebResourceRequested / postMessageWithAdditionalObjects が使えるか（使えなければ pkg/edge 拡張かフォーク）。
- ファイル実パス取得に必要な WebView2 のバージョンと、古いランタイムでの代替（ピッカー経由のみ）。
- スレッド分離後のダイアログの親ウィンドウ・フォーカス・Ctrl+C の扱い。
- フォームの前回値保存の場所と、パスなど機密になり得る値の扱い（保存対象を選べるようにするか）。
- デバッガ用 goja フォークの保守コスト（上流追従）。
