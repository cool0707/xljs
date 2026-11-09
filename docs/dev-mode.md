# 開発モード（`xljs dev`）の設計

AI チャットと並べてスクリプトを作る・直すための画面。本格的な AI 連携（API）や Git 連携はせず、AI とのやりとりは「チャット画面」「ダウンロード」「クリップボード」だけで行う。

## 構成

| 場所 | 役割 |
|---|---|
| `src/dev/mod.rs` | セッション。開発ウィンドウを開き、ページ（`dev.js`）の要求 `{op, ...}` に `{type, ...}` のイベントで答える。ブラウザ枠・実行のイベントもここに集まる（`Event`） |
| `src/dev/project.rs` | 開いているスクリプト（`Project`）。編集できるファイルの一覧、ダウンロードの置き換え先の決定（`match_download`）、テキストの読み書き（UTF-8、だめなら Shift_JIS） |
| `src/dev/versions.rs` | 簡易バージョニング。置き換える前の内容を `<フォルダ>/.xljs/versions/<相対パス>/NNNN.<拡張子>` に、メモを同じフォルダの `log.json` に |
| `src/dev/notify.rs` | 自分で直したところを AI に伝える（後述）。AI が最後に見た内容の記録と、行の差分（Myers）・依頼文 |
| `src/dev/report.rs` | 失敗した実行の stderr から、エラーの部分（`last_error`）と場所（`find_location`）を取り出し、AI への依頼文（`prompt`）を作る。`intro` は `docs/ai/prompt.md`＋今のスクリプト |
| `src/dev/runner.rs` | スクリプトを別プロセス（この exe 自身）で実行し、stdout/stderr と終了をイベントで返す |
| `src/ui/web/dev.html` `dev.js` `dev.css` `dev-diff.js` | ページ。キャンバス（タブ）、差分表示（Myers の行差分）、エディタ、プレビュー |
| `src/ui/web/dev-editor.js` `dev-types.js` | エディタ（後述）。`vendor/codemirror/codemirror.js` は `tools/codemirror` で作る CodeMirror 6 のバンドル |
| `src/ui/native/browser.rs` | AI チャットの枠: 同じウィンドウの 2 つ目の WebView2（`WindowOptions::browser` / `Window::browser`） |

`ui::run` のスクリプトスレッドでセッションのループ（`mpsc` の受信）を回し、ダイアログ・ウィンドウは通常のスクリプトと同じく UI スレッドへ転送する。

## 新規作成ウィザード（`src/dev/wizard.rs`）

ページの `#wizard`（キャンバスの中）で答えを集め、`wizard::Answers` として送る。

- `wizardBooks`: ブックを選ばせ、マクロのモジュール数を返す（`wizard::books`）。
- `wizardPrompt`: 依頼文（`wizard::prompt`）。先頭に `docs/ai/prompt.md`（選んだとき）、条件（形式＝フォームアプリ・カスタムアプリ（`@mode app`）・フォームなしのスクリプト、デザイン、ファイル名・ブック・入出力ファイル）、やりたいこと、移植ならマクロ（`vba::text`）とシートの構成（`api::workbook_outline`: シートごとの大きさ、先頭 n 行の表、数式。参照を相対化した形が同じ数式は 1 行にまとめる。共有数式もまとめる。.xls はシートを読めない旨だけ）、回答のしかた（移植ならまず説明、仮定を書く、ファイル名を指定してファイルで出力、アプリなら画面も）。
- 見た目（`titleBar` `accent` `background` `radius`。フォームを使わないスクリプトでは隠す）: 既定以外を選ぶと条件に「見た目」を足す。スクリプトなら `ui.form` のオプション、アプリならメタデータの `@titlebar custom` と画面の CSS（色・角丸の px）で依頼する（おまかせは両方）。「プレビュー」は Rust を通さずページだけで、見本のフォーム（項目 3 つ＋実行ボタン）の `formInit` を組み立てて `form.html?preview` の iframe に渡す（色・角丸は form.js の `applyColors` がそのまま効く。フォームの文言は `init` の `formTexts`）。タイトルバーは iframe の上に dev.css で描いた似姿（システム標準か titlebar.js と同じ形。自前のバーは背景色とアクセントを反映）。アプリでは画面が AI の HTML なので、完全に同じにはならない旨を出す。色の入力の初期値は組み込みページの既定（アクセント `#1261d1`、背景 `#ffffff`）。プレビューはボタンをもう一度押す・見本のキャンセル／閉じる（Esc も）・描いたタイトルバーの × で閉じる。
- `wizardSave`: マクロとシートの構成だけをテキスト（.md）に保存（長さに上限のあるチャットで添付するため）。
- `wizardCreate`: フォルダに仮のスクリプト（アプリなら雛形と `ui/index.html`）を作って開き、依頼文をコピーさせる。名前が「おまかせ」なら `.xljs/dev.json` の `unnamed` に入れる。
- `unnamed` の入口のスクリプトに AI のスクリプトのファイルが来たら、`download` に `rename`（ブラウザの ` (1)` を除いた名前）を付ける。ページで「名前を変える」がオンのまま承認すると、置き換えたあと名前を変え、履歴のフォルダとチャットの紐付けも移す（`Session::rename_entry`）。

## 画面の配置（`dev.js` の layout）

- 枠（グループ）を左右に最大 4 つ並べ、それぞれにタブを持つ（エディタの分割表示と同じ形）。タブは AI チャット（`chat`）とキャンバスの各パネル（`code` `diff` `preview` `history` `output`）。AI チャットだけのグループはタブ列を出さず、チャットのバー（ホーム・アドレス）がそのまま見出しになる。
- タブ（とチャットのバー。アドレス欄を除く）を押したまま 6px 動かすとドラッグ。落とす先は、タブ列の中（並べ替え・別のグループへ）、グループの中央（そのグループへ）、グループの左端・右端の 1/4（その横に新しいグループ。グループが 4 つあるときは不可）。グループを丸ごと動かしたときはそのグループの幅を保ち、タブから作ったグループは割ったグループの半分の幅。空になったグループは消える。Esc で取りやめ。ドラッグの終わりのクリックは捨てる。
- ドラッグ中は AI チャットの枠（ネイティブの WebView2）を隠す。ページの上に重なって落とす先の表示を隠し、マウスも取るため。
- 要素は所属するグループが変わったときだけ DOM を移す（iframe は移すと読み込み直す。プレビューは `ready` で同じフォームを返すので元に戻る）。並びはフレックスの `order`。
- タブを右クリックするとメニュー（「閉じる」）。AI チャットだけのグループはチャットのバー（アドレス欄を除く）を右クリック。最後の 1 つのタブは閉じられない（右クリックしてもメニューを出さない）。空になったグループは消え、幅は隣（左、なければ右）のグループへ。
- 閉じたタブがあるときだけ、各グループのタブ列の右端に「+」（AI チャットだけのグループはチャットのバーの右端）。押すと閉じたタブのメニューが出て、選んだタブをそのグループの最後に開く。閉じたタブを `setTab` で出そうとしたとき（実行で「出力」、ダウンロードで「変更」、新規作成で「コード」など）は、コードのグループ（コードも閉じていればチャットだけではない最初のグループ）に開き直す。ようこそ・ウィザードも同じグループを覆う。
- メニューはページの要素なので、チャットの枠と重なるあいだは枠を隠す。
- ツールバーの配置のボタンは「配置を元に戻す」だけ。既定（左にチャット、右にキャンバス、閉じたタブなし）に戻す。
- 配置は `localStorage` の `xljs.dev.layout`（`{groups: [{id, tabs, active, size}], closed: [tab]}`）。以前の形（`{mode, swap, split}`）は読み込み時に変換し、`mode`（チャットだけ / キャンバスだけ）は捨てる。

## AI チャットの枠

- ページの WebView2 とは別に、子ウィンドウ（`xljsBrowserHost`）に 2 つ目の WebView2 を作る。外部サイトは iframe に入れられない（X-Frame-Options）ため。環境（ユーザーデータフォルダ）はページと同じなので、サインインは残る。環境のオプション `AllowSingleSignOnUsingOSPrimaryAccount` を有効にしているので、Microsoft のサイト（Copilot）には Edge と同じく Windows にサインインしているアカウントで自動的にサインインする（ローカルアカウントの Windows では効かない）。
- 位置はページが決める。`dev.js` が `#chat-view` の矩形（チャットのタブが表示中のときだけ。ほかのタブの裏やドラッグ中は `hide`）（CSS ピクセル）と `devicePixelRatio` を `{op:"browser", path:"show"}` で送り、Rust が子ウィンドウを `HWND_TOP` でそこに置く。親ウィンドウは `WS_CLIPCHILDREN`。ページ側のメニューやダイアログが枠に隠れないよう、ページは枠の上に重なる要素を作らない（確認は `confirm()` のネイティブダイアログ）。
- ダウンロード（`ICoreWebView2_4::DownloadStarting`）は保存先を一時フォルダ（`%TEMP%\xljs-dev\<pid>`）に変え、ダウンロードの表示を出さない（`Handled`）。完了（`StateChanged` が COMPLETED）で `BrowserEvent::Downloaded`。
- `.js` などは WebView2 の「危険なファイルの種類」の確認（保存するかを毎回尋ねる）にかかるため、`ICoreWebView2_26::SaveFileSecurityCheckStarting` で、開発モードが読むテキストの拡張子（`project::TEXT_EXTS`。`BrowserOptions::trusted_extensions`）だけ既定の確認を外す（`SuppressDefaultPolicy`）。受け取ったファイルはテキストとして読むだけで実行しない。.exe などはそのまま確認される。SmartScreen（評判の確認）は切らない。ランタイムが 1.0.2903 より古いと、このイベントがないので従来どおり尋ねる。
- 表示中のページ（`SourceChanged`）を `BrowserEvent::Url` で知らせ、スクリプトごとに `.xljs/dev.json` の `chat` に覚える。スクリプトを開くとその URL を開く。チャットを開いていない状態でスクリプトを開いたときは、今のチャットをそのスクリプトに紐付ける。「ホーム」でチャットを選び直す（紐付けを消し、`about:blank` にして隠す）。
- 配布用の既定（`DevOptions::exe_settings`＝exe と同じフォルダの `xljs.json` の `dev`。`load_exe_settings`）: `chat` は既定のチャット。`ready` でスクリプトが開かれず、チャットも開いていなければそれを開き（`url` を送ってページの `st.chat` にする）、`open` で紐付けのないスクリプトに今のチャットもなければそれを開く（紐付けは表示されたときの `BrowserEvent::Url` で覚える）。`services` はチャットの選択肢の先頭に、`builtinServices: false` で `SERVICES` を出さない。`chat` が選択肢になければ先頭に足す。http(s) 以外の URL は捨てる。読めない JSON は `dev.exeSettingsInvalid` を `error` で知らせ、既定なしで続ける。

## ダウンロードの置き換え先（`Project::match_download`）

1. 同じ名前（拡張子の一致を優先。`.js` と `.ts` などスクリプト同士は同じ種類として扱う）。ブラウザが付ける ` (1)` は除く。
2. 既存のファイル名がダウンロードの名前の先頭に来るもの（AI が `集計_修正版.js` のように後ろに付け足すため）。いちばん長く一致するもの。
3. アプリでない場合のスクリプトは、編集中のファイル（スクリプトなら）、なければ入口のスクリプト。
4. それ以外は決めない（ページで選ぶ。新しいファイルとして保存もできる）。

対象のファイルは、入口のスクリプトと同じフォルダのテキストファイル、およびメタデータの `@assets` / `@html` / `@icon` の下（アプリの画面）。

## AI に変更を通知（`notify.rs`）

AI の回答を取り込んだあとに自分でコードを直すと、AI は自分の前の回答をもとに次の回答を作るので、直したところが元に戻りやすい。コードのバーの［AI に変更を通知］（［整形］と［保存］の間）で、直したところをチャットに貼る依頼文としてコピーする。

- AI が最後に見た内容をファイルごとに `<フォルダ>/.xljs/ai/<相対パス>` に置く。記録するのは、置き換え（`apply`。AI の回答そのもの）、AI への説明のコピー（`intro`。入口のスクリプト）、通知したとき（通知した内容）。前の版に戻す（`restore`）・保存では変えない。入口のスクリプトの名前を変えたら記録も移す。
- ボタンはまず保存し（`save`）、`{op:"notify", file}` を送る。Rust はファイル一覧（`Project::files`）のうち記録と今の内容が違うものを unified diff（前後 3 行）で、記録のない編集中のファイルは全体を載せる。差分が多い（1000 編集を超える）か、差分のほうがファイルより長いときも全体。依頼文は「自分で直した → この変更を前提に・元に戻さない → 今回は短い返事でよい」。コピーしたら記録を今の内容にする。
- 伝えることがなければ `info`（`dev.notifyNone`）。

## 実行

- `runner::Run` が `exe [--lang=..] [--debug] script` を、スクリプトのフォルダをカレントにして起動する（Windows では `CREATE_NO_WINDOW`）。標準出力・エラーはパイプ。GUI 版の子プロセスは出力先がある（パイプ）のでエラーダイアログを出さず、stderr に書く。
- 環境変数: `XLJS_DEV=1`（`Options::dev_guard`）、`XLJS_WATCHDOG=on`・`XLJS_STOP_KEY=on`（利用者が指定していなければ）、プレビューでは `XLJS_DEV_PREVIEW=<一時ファイル>`（`Options::preview`）。
- `dev_guard`: `fs.remove`（`removeEntry` も）が、中身のあるフォルダを再帰的に消すときと、その実行で 2 つ目のファイルを消すとき（1 回だけ）に `confirm` で尋ねる。キャンセルでエラー（`dev.removeDeclined`）を投げ、スクリプトは止まる。
- 無限ループで中止したとき（終了コード 130）は、エンジンが中止した例外のスタック（ソースマップで元のファイルに対応付けたもの）を「中止した場所:」に続けて stderr に書く（`engine::finish_caught`）。開発モード以外でも出る。
- 終了時、stderr の最後の「インデントのない行」から後ろ（エラー本文とスタック、構文エラー、中止の場所）をエラーとして扱い、その中の最初のスクリプトのファイル（`node_modules` を除く）を場所とする。

## フォームのプレビュー

`XLJS_DEV_PREVIEW` を付けて実行すると、prelude の `ui.form` が最初のフォームのページ向けの初期メッセージ（`formInit`。フォームのウィンドウに送るものと同じ）をファイルに書いて `exit(0)` する。開発ウィンドウはそれを `form.html?preview` の iframe に渡す（`bridge.js` は `?preview` 付きで iframe の中にいると、親ページと `window.postMessage` でやりとりする）。送信はしない（「プレビューのため実行しません」）。

アプリの画面のプレビューは、スクリプトのフォルダを `https://app.xljs/_project/` で配信して iframe で表示する（スクリプトは動かないので data-bind などは空）。

## ページとのメッセージ（主なもの）

| ページ → Rust（`op`） | 内容 |
|---|---|
| `ready` | 初期化。`init`（文言・AI の一覧・最近使ったスクリプト）を返し、起動時に指定されたスクリプトを開く |
| `open` `new` | スクリプトを開く（`path` なしはピッカー）/ テンプレートから新規作成（`template`: `script`＝スクリプト、`form`＝フォームアプリ（`ui.form`）、`app`＝カスタムアプリ（`@mode app`＋`ui/index.html`）） |
| `drop` | ウィンドウにドロップしたファイル（`postMessageWithAdditionalObjects` で渡し、Rust 側でパスにする）。最初のスクリプト（.js・.mjs・.ts）を開く。フォルダならその中でピッカー、スクリプトがなければエラー。チャットの枠（別の WebView2）へのドロップはチャット側のもの |
| `read` `save` `edit` | ファイルの読み書き。`edit` は編集中のファイル（置き換え先の決定用） |
| `apply` `versions` `version` `restore` | 置き換え（履歴に残す）、履歴の一覧・内容、前の版に戻す |
| `run` `stop` `preview` | 実行（`debug`）・停止・フォームのプレビュー |
| `chat` `browser` | チャットの URL（空でホーム）、枠の表示・位置・戻る・進む・再読み込み |
| `intro` `notify` `showFolder` | AI への説明のコピー、自分で直したところのコピー、フォルダを開く |
| `wizardBooks` `wizardFolder` `wizardPrompt` `wizardSave` `wizardCreate` | 新規作成ウィザード（`answers`） |

| Rust → ページ（`type`） | 内容 |
|---|---|
| `init` `project` `meta` `files` | 初期化、開いたスクリプト、メタデータの変化、ファイル一覧 |
| `file` `saved` `applied` `versions` `version` | ファイル・保存・置き換え・履歴 |
| `download` | ダウンロードしたファイル（`name` `text` `target` `old` `files`） |
| `runStarted` `output` `runEnded` | 実行。`runEnded` は失敗時に `message` `location` `prompt` |
| `previewForm` `previewFailed` | フォームのプレビュー |
| `url` `copy` `error` `info` `browserFailed` | チャットの URL、クリップボードへのコピー、エラー・お知らせ |
| `wizardBooks` `wizardFolder` `wizardPrompt` | ウィザードの答え |

ページの配置（グループとタブの並び、閉じたタブ、境目の位置）はページの `localStorage` に保存する。最近使ったスクリプトはユーザーの設定（`%APPDATA%\xljs\dev.json`）。

## テスト

- `src/dev/tests.rs`: 置き換え先の決定、履歴、エラーの場所と依頼文、セッションの流れ（記録するだけのウィンドウとモック UI）、別プロセスの実行（Linux の `sh`）。
- `src/engine/tests.rs`: `dev_guard`、フォームのプレビュー、無限ループで中止した場所。
- ネイティブの枠（2 つ目の WebView2、ダウンロード）は Windows 実機で確認する（`docs/gui-testing.md` の手順）。ページは `XLJS_DEV_DEBUG=1` で開発者ツールが使える。

## エディタ（`dev-editor.js`・`dev-types.js`）

- CodeMirror 6。必要なモジュールだけを `tools/codemirror/index.js` から esbuild で 1 つの IIFE（グローバルの `CM`）にまとめて `src/ui/web/vendor/codemirror/codemirror.js` に置く（`cd tools/codemirror && npm ci && npm run build`。版は `package.json` と `vendor/VERSIONS.md`）。
- 言語はファイルの拡張子で決める（`.js`・`.ts`・`.json`・`.html`・`.css`。ほかは色分けなし）。ファイルを開くたびに新しい `EditorState`（元に戻す履歴もファイルごと）。エラーの行は `markField`（行の装飾と行番号）。
- 色はページの CSS 変数（`dev.css` の `--hl-*`・`--editor-*`。ライト・ダーク）。CodeMirror の UI 文言（検索パネルなど）は `devPage.cm*`（`dev-editor.js` の `PHRASES` が英語の文言 → キー）。
- 補完とホバー（`dev-types.js`）: セッションの `init` が `types/xljs.d.ts`（`crate::DTS`）を `dts` で渡す。型検査器ではなく、d.ts を読む小さなパーサー（interface・class・type・グローバルの const/var/function。JSDoc を説明に）と、構文木（Lezer）をたどる型の解決。
  - 解決できるもの: グローバル、メンバー・メソッドの戻り値（`Promise` は `await` で外す。オーバーロードは最初のもの）、`x[i]` と `for (... of x)` の要素、`const x = 式`、TS の型注釈、型引数（既定値・制約で置き換え。推論はしない）、引数・プロパティの位置で期待される型（`ui.form({ ` のオプション名、`onSubmit: (v, ctx) => ctx.` や `rows.map((r) => r.` の引数）。
  - JS の組み込み（Array・String・Date・Map・Math・JSON など）は `dev-types.js` の `BUILTINS` に最小限の宣言を持つ。
  - 分からない型は何も出さない（CodeMirror の JS の補完＝ローカルの名前・キーワード・スニペットは残る）。
- 整形（［整形］・Shift+Alt+F）: Prettier の standalone 版（`vendor/prettier`。初めて使うときに `<script>` で読み込む）。パーサーは拡張子で `babel`・`babel-ts`（TS も babel のプラグインで足りるので、大きい typescript のプラグインは入れない）・`json`・`html`・`css`。設定は既定から `printWidth: 100` だけ変える。結果は変わった中ほどだけを 1 回の変更として置き換える（Ctrl+Z で戻せる、表示の位置が飛ばない）。構文エラーはその行へ移動してステータスに出す。
