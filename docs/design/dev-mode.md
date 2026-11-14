# 開発モード（`xljs dev`）の設計

AI チャットと並べてスクリプトを作る・直すための画面。本格的な AI 連携（API）や Git 連携はせず、AI とのやりとりは「チャット画面」「ダウンロード」「クリップボード」だけで行う。

## 構成

| 場所 | 役割 |
|---|---|
| `src/dev/mod.rs` | セッション。開発ウィンドウを開き、ページ（`dev.js`）の要求 `{op, ...}` に `{type, ...}` のイベントで答える。ブラウザ枠・実行のイベントもここに集まる（`Event`） |
| `src/dev/project.rs` | 開いているスクリプト（`Project`）。編集できるファイルの一覧、ダウンロードの置き換え先の決定（`match_download`）、テキストの読み書き（UTF-8、だめなら Shift_JIS） |
| `src/dev/versions.rs` | 簡易バージョニング。置き換える前の内容を `<フォルダ>/.xljs/versions/<相対パス>/NNNN.<拡張子>` に、メモを同じフォルダの `log.json` に |
| `src/dev/notify.rs` | 自分で直したところを AI に伝える（後述）。AI が最後に見た内容の記録と、行の差分（Myers）・依頼文 |
| `src/dev/report.rs` | 失敗した実行の stderr から、エラーの部分（`last_error`）と場所（`find_location`）を取り出し、AI への依頼文（`prompt`）を作る。`intro` は `docs/ai/prompt.md` と今のスクリプト（どちらか片方も） |
| `src/dev/agent.rs` | AI チャットのサービスの判定（`classify`）と、xljs 用のエージェント・Gem・GPT・プロジェクトに入れるもの（`kit`。`docs/ai` の文書を exe に埋め込んだもの）。後述の「xljs 用のエージェントの案内」 |
| `src/dev/runner.rs` | スクリプトを別プロセス（この exe 自身）で実行し、stdout/stderr と終了をイベントで返す（`crate::process` 経由。Windows はジョブオブジェクトで、停止するとスクリプトが起動したプログラムも終わる） |
| `src/ui/web/dev.html` `dev.js` `dev.css` `dev-diff.js` | ページ。キャンバス（タブ）、差分表示（Myers の行差分）、エディタ、プレビュー |
| `src/ui/web/dev-editor.js` `dev-types.js` | エディタ（後述）。`vendor/codemirror/codemirror.js` は `tools/codemirror` で作る CodeMirror 6 のバンドル |
| `src/ui/native/browser.rs` | AI チャットの枠: 同じウィンドウの 2 つ目の WebView2（`WindowOptions::browser` / `Window::browser`） |

`ui::run` のスクリプトスレッドでセッションのループ（`mpsc` の受信）を回し、ダイアログ・ウィンドウは通常のスクリプトと同じく UI スレッドへ転送する。

## 新規作成ウィザード（`src/dev/wizard.rs`）

ページの `#wizard`（キャンバスの中）で答えを集め、`wizard::Answers` として送る。

- `wizardBooks`: ブックを選ばせ、マクロのモジュール数を返す（`wizard::books`）。
- `wizardPrompt`: 依頼文（`wizard::prompt`）。先頭に `docs/ai/prompt.md`（選んだとき）、条件（形式＝フォームアプリ・カスタムアプリ（`@mode app`）・フォームなしのスクリプト、デザイン、ファイル名・ブック・入出力ファイル）、やりたいこと、移植ならマクロ（`vba::text`）とシートの構成（`api::workbook_outline`: シートごとの大きさ、先頭 n 行の表、数式。参照を相対化した形が同じ数式は 1 行にまとめる。共有数式もまとめる。.xls はシートを読めない旨だけ）、回答のしかた（移植ならまず説明、仮定を書く、ファイル名を指定してファイルで出力、アプリなら画面も）。
- 使い方（`users`＝自分だけ・複数の人・おまかせ、`useAd`＝実行している人の氏名・所属などを使う、`data`＝保存しない・続けて使うデータを保存する・おまかせ）: おまかせ以外を選ぶと条件に「使う人」「実行している人の情報」「データ」を足す（複数の人なら書き込むデータは SQLite の共有モードで場所は固定、氏名などは入力させず `ad.me()`、続けて使うデータは SQLite）。AI は何も言わないと氏名を入力欄にし、データの場所を毎回選ばせるので、条件として書く。フォームなしのスクリプト以外では、回答のしかたに「コードの前に画面の構成と、入力させるもの・調べるものを書く」を足す（画面の方針は `docs/ai/reference.md` の「画面の設計」）。
- 見た目（`titleBar` `accent` `background` `radius`。フォームを使わないスクリプトでは隠す）: 既定以外を選ぶと条件に「見た目」を足す。スクリプトなら `ui.form` のオプション、アプリならメタデータの `@titlebar custom` と画面の CSS（色・角丸の px）で依頼する（おまかせは両方）。「プレビュー」は Rust を通さずページだけで、見本のフォーム（項目 3 つ＋実行ボタン）の `formInit` を組み立てて `form.html?preview` の iframe に渡す（色・角丸は form.js の `applyColors` がそのまま効く。フォームの文言は `init` の `formTexts`）。タイトルバーは iframe の上に dev.css で描いた似姿（システム標準か titlebar.js と同じ形。自前のバーは背景色とアクセントを反映）。アプリでは画面が AI の HTML なので、完全に同じにはならない旨を出す。色の入力の初期値は組み込みページの既定（アクセント `#1261d1`、背景 `#ffffff`）。プレビューはボタンをもう一度押す・見本のキャンセル／閉じる（Esc も）・描いたタイトルバーの × で閉じる。
- 移植のマクロとシートの構成は、既定では依頼文に入れず添付ファイル `<最初のブック名>_xljs.md`（`wizard::attachment_name`）にし、依頼文にはその名前と「添付を読む」ことだけを書く（たいていチャットのメッセージの長さに収まらないため）。「プロンプトに直接含める」（`inlineWorkbook`）を選んだときは依頼文に入れる。
- `wizardSave`: マクロとシートの構成だけをテキスト（.md）に、選んだ場所へ保存。
- `wizardCreate`: フォルダに仮のスクリプト（アプリなら雛形と `ui/index.html`）を作って開き、依頼文をコピーさせる。名前が「おまかせ」なら `.xljs/dev.json` の `unnamed` に入れる。添付ファイルがあれば同じフォルダに書き（同じ名前は上書き。依頼文の名前と合わせるため）、`wizardAttach` でページに知らせる。ページはコードの上に添付ファイルのバー（`#attach-bar`）を出し、そのハンドルからチャットへドラッグできる（`dragFile`。スクリプトを開き直すと消える）。
- `unnamed` の入口のスクリプトに AI のスクリプトのファイルが来たら、`download` に `rename`（ブラウザの ` (1)` を除いた名前）を付ける。ページで「名前を変える」がオンのまま承認すると、置き換えたあと名前を変え、履歴のフォルダとチャットの紐付けも移す（`Session::rename_entry`）。

## 画面の配置（`dev.js` の layout）

- 枠（グループ）を左右に最大 4 つ並べ、それぞれにタブを持つ（エディタの分割表示と同じ形）。タブは AI チャット（`chat`）とキャンバスの各パネル（`code` `diff` `preview` `history` `output`）。AI チャットだけのグループはタブ列を出さず、チャットのバー（ホーム・アドレス）がそのまま見出しになる。
- タブ（とチャットのバー。アドレス欄を除く）を押したまま 6px 動かすとドラッグ。落とす先は、タブ列の中（並べ替え・別のグループへ）、グループの中央（そのグループへ）、グループの左端・右端の 1/4（その横に新しいグループ。グループが 4 つあるときは不可）。グループを丸ごと動かしたときはそのグループの幅を保ち、タブから作ったグループは割ったグループの半分の幅。空になったグループは消える。Esc で取りやめ。ドラッグの終わりのクリックは捨てる。
- ドラッグ中は AI チャットの枠（ネイティブの WebView2）を隠す。ページの上に重なって落とす先の表示を隠し、マウスも取るため。
- 要素は所属するグループが変わったときだけ DOM を移す（iframe は移すと読み込み直す。プレビューは `ready` で同じフォームを返すので元に戻る）。並びはフレックスの `order`。
- タブを右クリックするとメニュー（「閉じる」）。AI チャットだけのグループはチャットのバー（アドレス欄を除く）を右クリック。最後の 1 つのタブは閉じられない（右クリックしてもメニューを出さない）。空になったグループは消え、幅は隣（左、なければ右）のグループへ。
- 閉じたタブがあるときだけ、各グループのタブ列の右端に「+」（AI チャットだけのグループはチャットのバーの右端）。押すと閉じたタブのメニューが出て、選んだタブをそのグループの最後に開く。閉じたタブを `setTab` で出そうとしたとき（実行で「出力」、ダウンロードで「変更」、新規作成で「コード」など）は、コードのグループ（コードも閉じていればチャットだけではない最初のグループ）に開き直す。ようこそ・ウィザードも同じグループを覆う。
- メニューはページの要素なので、チャットの枠と重なるあいだは枠を隠す。
- ウィンドウは自前のタイトルバー（`WindowOptions::custom_title_bar`）。バーは `dev.html` の `#titlebar`（titlebar.js が見た目とボタンを受け持つ）で、xljs のアイコン（`web/xljs-logo.png`）と「ファイル名 - xljs 開発モード」を出す。同じ文字列を `\x01window:title:` でウィンドウのテキスト（タスクバー）にも設定する。開いているファイルのフォルダはコードタブのファイルの選択の隣に出す。
- ツールバーの配置のボタンは「配置を元に戻す」だけ。既定（左にチャット、右にキャンバス、閉じたタブなし）に戻す。
- 配置は `localStorage` の `xljs.dev.layout`（`{groups: [{id, tabs, active, size}], closed: [tab]}`）。以前の形（`{mode, swap, split}`）は読み込み時に変換し、`mode`（チャットだけ / キャンバスだけ）は捨てる。

## AI チャットの枠

- ページの WebView2 とは別に、子ウィンドウ（`xljsBrowserHost`）に 2 つ目の WebView2 を作る。外部サイトは iframe に入れられない（X-Frame-Options）ため。環境（ユーザーデータフォルダ）はページと同じなので、サインインは残る。環境のオプション `AllowSingleSignOnUsingOSPrimaryAccount` を有効にしているので、Microsoft のサイト（Copilot）には Edge と同じく Windows にサインインしているアカウントで自動的にサインインする（ローカルアカウントの Windows では効かない）。
- 位置はページが決める。`dev.js` が `#chat-view` の矩形（チャットのタブが表示中のときだけ。ほかのタブの裏やドラッグ中は `hide`）（CSS ピクセル）と `devicePixelRatio` を `{op:"browser", path:"show"}` で送り、Rust が子ウィンドウを `HWND_TOP` でそこに置く。親ウィンドウは `WS_CLIPCHILDREN`。ページ側の要素は枠（子ウィンドウ）の下に隠れるので、ページは枠の上に重なる要素を基本的に作らない（確認は `confirm()` のネイティブダイアログ）。例外はメニュー（依頼文のコピーの選択肢など）で、`show` にメニューと重なる部分の矩形（`holes`、枠の左上からの CSS ピクセル）を付けて送り、Rust が子ウィンドウのリージョン（`SetWindowRgn`）からその部分を抜いて下のページを見せる（以前は枠ごと隠していたので、メニューを開くと枠が灰色になっていた）。
- ダウンロード（`ICoreWebView2_4::DownloadStarting`）は保存先を一時フォルダ（`%TEMP%\xljs-dev\<pid>`）に変え、ダウンロードの表示を出さない（`Handled`）。完了（`StateChanged` が COMPLETED）で `BrowserEvent::Downloaded`。
- `.js` などは WebView2 の「危険なファイルの種類」の確認（保存するかを毎回尋ねる）にかかるため、`ICoreWebView2_26::SaveFileSecurityCheckStarting` で、開発モードが読むテキストの拡張子（`project::TEXT_EXTS`。`BrowserOptions::trusted_extensions`）だけ既定の確認を外す（`SuppressDefaultPolicy`）。受け取ったファイルはテキストとして読むだけで実行しない。.exe などはそのまま確認される。SmartScreen（評判の確認）は切らない。ランタイムが 1.0.2903 より古いと、このイベントがないので従来どおり尋ねる。
- 表示中のページ（`SourceChanged`）を `BrowserEvent::Url` で知らせ、スクリプトごとに `.xljs/dev.json` の `chat` に覚える。スクリプトを開くとその URL を開く。チャットを開いていない状態でスクリプトを開いたときは、今のチャットをそのスクリプトに紐付ける。ホームボタンはスプリットボタン。本体はデフォルトのチャット（ユーザーの設定 `%APPDATA%\xljs\dev.json` の `home`。チャットを選ぶ画面のチェック「選んだチャットをデフォルトにする」か、メニューの「今のページをデフォルトにする」で決める。既定はなし）があればそこを開き、なければチャットを選び直す（紐付けを消し、`about:blank` にして隠す）。メニューは「AI チャットを選び直す」「今のページをデフォルトにする」「デフォルトを解除する」。デフォルトのチャットは、チャットのないスクリプトを開いたときと起動時にも、exe の隣の `xljs.json` の `dev.chat` より先に使う。
- User-Agent（`src/net/agent.rs`。`fetch` と共通）: 既定はこのパソコンの Edge（版は EdgeUpdate の `Clients\{56EB18F8-…}` の `pv`、なければ `HKCU\Software\Microsoft\Edge\BLBeacon`、Edge がなければ WebView2 ランタイムの版）。`xljs.json` の `userAgent`（環境変数 `XLJS_USER_AGENT` が優先）で `"chrome"`（Google Update の `{8A69D345-…}`。なければ Edge）・`"xljs"`（枠は WebView2 のまま、`fetch` は `xljs/<版>`）・任意の文字列。`BrowserOptions::agent` で枠を作ったときに `ICoreWebView2Settings2::UserAgent` と DevTools プロトコルの `Emulation.setUserAgentOverride`（`userAgentMetadata`: ブランドを Chromium と同じ並びと GREASE で、`platformVersion` は Windows のビルド番号から）を設定する。WebView2 のままだと Sec-CH-UA に「Microsoft Edge WebView2」が入り、埋め込みのブラウザを断るサイトがあるため。ポップアップ（`window.open` のサインイン画面など）は WebView2 の既定のまま。
- 配布用の既定（`DevOptions::exe_settings`＝exe と同じフォルダの `xljs.json` の `dev`。`load_exe_settings`）: `chat` は既定のチャット。`ready` でスクリプトが開かれず、チャットも開いていなければそれを開き（`url` を送ってページの `st.chat` にする）、`open` で紐付けのないスクリプトに今のチャットもなければそれを開く（紐付けは表示されたときの `BrowserEvent::Url` で覚える）。`services` はチャットの選択肢の先頭に、`builtinServices: false` で `SERVICES` を出さない。`chat` が選択肢になければ先頭に足す。http(s) 以外の URL は捨てる。読めない JSON は `dev.exeSettingsInvalid` を `error` で知らせ、既定なしで続ける。

## xljs 用のエージェントの案内（`agent.rs`）

指示を持たせた AI（Microsoft 365 Copilot のエージェント、Gemini の Gem、ChatGPT の GPT・プロジェクト、Claude のプロジェクト）を作れば毎回プロンプトを貼らなくてよいので、それを使っていない人にチャットの上で案内する。

- 判定はチャットを開始した URL（ページの `st.chat`。スクリプトに紐付いたチャット・選んだチャット・デフォルトのチャット）で行う。ページが `st.chat` かデフォルトのチャットが変わるたびに `agentHint`（`url` `home`）を送り、Rust が `agent::classify` で `{service, custom, what}` を返す。M365（`*.cloud.microsoft`・`microsoft365.com`・`office.com`）は `titleId=` かパスの `agent(s)`、Gemini はパスの `gem`、ChatGPT は `/g/`（GPT とプロジェクト）、Claude はパスの `project` があれば作ったもの（`custom`）。Claude のプロジェクトの会話は `/chat/...` になるので、デフォルトのチャットが同じサービスの作ったものなら `custom` とする。Copilot（`copilot.microsoft.com`）はエージェントを作れないので出さない。
- 案内（`#agent-hint`）は `#chat-view` の右上に置くページの要素で、チャットの枠はその矩形を穴として抜く（メニューと同じ `holes`）。文言はサービスで変わる（`dev.agentWhat.*`）。［作成用のプロンプト］・右上の閉じる・「今後表示しない」（`localStorage` の `xljs.dev.agentHint`＝`off`）。閉じたらそのウィンドウでは出さない。
- ［作成用のプロンプト］とホームボタンの ▼ の「xljs 用の…を作る」は `agentKit`（`name`＝サービス）を送り、Rust が `agent::kit` を返す。ページはそれをウィザードと同じくコードのグループを覆うパネル（`#agent`）に出す: 手順（`dev.agentSteps.*`）、作成画面を開くボタン（URL が決まっているサービスだけ）、名前（例）・説明（例）とコピー、指示（`agentCopy` で全文をコピー）、スキル・知識のファイル（つまみをチャットへドラッグ＝`agentDrag`、コピー）。
- 入れるもの: M365 は指示に `copilot/instructions.md`（8,000 字の上限）とスキルに `copilot/skills/*/SKILL.md`（`<スキル名>/SKILL.md` としてドラッグ）。ChatGPT・Gemini・Claude は指示に `<サービス>/instructions.md`（`copilot/instructions.md` のスキルの呼び方を知識のファイル名に替えたもの。3 つとも同じで、`ai_knowledge_instructions` テストが作る・検査する）と、知識にスキルを `<スキル名>.txt` として（Gemini のスキルはファイルを MIME 型で見分け、Windows のレジストリに `.md` の Content Type がない（多くのパソコン）と Chromium がドラッグされた .md の型を空にして「サポートされていないファイル形式」になるため。.txt は Chromium が自前で text/plain にする。ほかのサービスも確かめられないので .txt にそろえる）。
- ドラッグするファイルは埋め込みの文書を `%TEMP%\xljs-dev\<pid>-ai\` に書き出して `BrowserCommand::DragFiles` で渡す（ウィンドウを閉じたら消す）。

## ダウンロードの置き換え先（`Project::match_download`）

1. 同じ名前（拡張子の一致を優先。`.js` と `.ts` などスクリプト同士は同じ種類として扱う）。ブラウザが付ける ` (1)` は除く。
2. 既存のファイル名がダウンロードの名前の先頭に来るもの（AI が `集計_修正版.js` のように後ろに付け足すため）。いちばん長く一致するもの。
3. アプリでない場合のスクリプトは、編集中のファイル（スクリプトなら）、なければ入口のスクリプト。
4. それ以外は決めない（ページで選ぶ。新しいファイルとして保存もできる）。

対象のファイルは、入口のスクリプトと同じフォルダのテキストファイル、およびメタデータの `@assets` / `@html` / `@icon` の下（アプリの画面）。

## AI に変更を通知（`notify.rs`）

AI の回答を取り込んだあとに自分でコードを直すと、AI は自分の前の回答をもとに次の回答を作るので、直したところが元に戻りやすい。コードのバーの［AI に変更を通知］（［整形］と［保存］の間）で、直したところをチャットに貼る依頼文としてコピーする。

- AI が最後に見た内容をファイルごとに `<フォルダ>/.xljs/ai/<相対パス>` に置く。記録するのは、置き換え（`apply`。AI の回答そのもの）、AI への説明のコピー（`intro`。スクリプトを含めたときだけ、入口のスクリプト）、通知したとき（通知した内容）。前の版に戻す（`restore`）・保存では変えない。入口のスクリプトの名前を変えたら記録も移す。
- ボタンはまず保存し（`save`）、`{op:"notify", file}` を送る。Rust はファイル一覧（`Project::files`）のうち記録と今の内容が違うものを unified diff（前後 3 行）で、記録のない編集中のファイルは全体を載せる。差分が多い（1000 編集を超える）か、差分のほうがファイルより長いときも全体。依頼文は「自分で直した → この変更を前提に・元に戻さない → 今回は短い返事でよい」。コピーしたら記録を今の内容にする。
- 伝えることがなければ `info`（`dev.notifyNone`）。

## 実行

- `runner::Run` が `exe [--lang=..] [--debug] script` を、スクリプトのフォルダをカレントにして起動する（Windows では `CREATE_NO_WINDOW`）。標準出力・エラーはパイプ。GUI 版の子プロセスは出力先がある（パイプ）のでエラーダイアログを出さず、stderr に書く。
- 環境変数: `XLJS_DEV=1`（`Options::dev_guard`）、`XLJS_WATCHDOG=on`・`XLJS_STOP_KEY=on`（利用者が指定していなければ）、プレビューでは `XLJS_DEV_PREVIEW=<一時ファイル>`（`Options::preview`）。
- `dev_guard`: `fs.remove`（`removeEntry` も）が、中身のあるフォルダを再帰的に消すときと、その実行で 2 つ目のファイルを消すとき（1 回だけ）に `confirm` で尋ねる。キャンセルでエラー（`dev.removeDeclined`）を投げ、スクリプトは止まる。
- 中止したとき（応答がなく監視で中止＝終了コード 124、停止キー・デバッグウィンドウを閉じた＝130）は、エンジンが中止した例外のスタック（ソースマップで元のファイルに対応付けたもの）を「中止した場所:」に続けて stderr に書く（`engine::finish_caught`）。開発モード以外でも出る。
- 終了時、stderr の最後の「インデントのない行」から後ろ（エラー本文とスタック、構文エラー、中止の場所）をエラーとして扱い、その中の最初のスクリプトのファイル（`node_modules` を除く）を場所とする。

## exe の作成

ツールバーの［exe を作成］（`pack`）は `xljs pack` と同じ（`pack::pack_script`。ベースは開発モードの exe＝`DevOptions::exe`、埋め込み済みのスクリプトは取り除く）。編集中の内容を保存してから、保存先を尋ねて（既定はスクリプトの隣の同名の .exe、2 回目からは前回の保存先＝`.xljs/dev.json` の `exe`）バンドルし、ステータスに結果（メタデータの警告も）を出して、フォルダを開くか尋ねる。バンドルのエラー（構文エラーなど）はステータスに出る。

## フォームのプレビュー

`XLJS_DEV_PREVIEW` を付けて実行すると、prelude の `ui.form` が最初のフォームのページ向けの初期メッセージ（`formInit`。フォームのウィンドウに送るものと同じ）をファイルに書いて `exit(0)` する。開発ウィンドウはそれを `form.html?preview` の iframe に渡す（`bridge.js` は `?preview` 付きで iframe の中にいると、親ページと `window.postMessage` でやりとりする）。送信はしない（「プレビューのため実行しません」）。

アプリの画面のプレビューは、スクリプトのフォルダを `https://app.xljs/_project/` で配信して iframe で表示する（スクリプトは動かないので data-bind などは空）。

プレビューはタブが画面に出た時点で自動で表示する。アプリの画面はそのまま読み込み、フォームはページが `preview`（`auto: true`）を送る。Rust はスクリプトのファイル（`Project::opens_form`。フォルダのスクリプトのどれかに `ui.form(`）がフォームを開かないとき、または実行中のときは実行せずに `previewSkipped`（`busy`）を返す（フォームを開かないスクリプトは［フォームを表示］を押したときだけ実行する）。ページはファイルの保存・置き換えのたびに「変更の世代」を進め、表示中のフォームが古ければ（タブが出ているとき・実行が終わったとき）もう一度実行する。自動で実行したプレビューが失敗したときは、前のフォームを残したままエラーを上に出す（編集の途中のことが多いため）。

［要素を選択］はプレビューの要素を選んで AI に依頼するモード。iframe のページ（`form.html`・`_project/` のアプリの画面）は開発ウィンドウと同じオリジンなので、ページから `contentDocument` に直接キャプチャのリスナーを付け、ポイントした要素に破線、選んだ要素に実線の枠（`data-xljs-hover` / `data-xljs-picked` の属性と注入した style）を付ける。選択中はクリック・mousedown・submit などを止めるのでフォームは動かない。クリックで選び直し、Ctrl / Shift＋クリックで追加・解除。要素ごとに短い名前（タグ・type・name・ラベルか文字）・CSS のパス（id のある祖先から）・HTML（800 字まで）を集め、直し方と一緒に `pick` で送ると Rust（`report::pick`）が依頼文を作ってコピーさせる（フォームならスクリプト、アプリなら `@html` のファイルを直すよう依頼。「スクリプト全体も含める」でそのファイルの全体も付け、`notify` の基準として覚える）。iframe が読み込み直されたときは CSS のパスで要素を探し直す。もう一度ボタンを押すと終わり、選択も消える。

## ページとのメッセージ（主なもの）

| ページ → Rust（`op`） | 内容 |
|---|---|
| `ready` | 初期化。`init`（文言・AI の一覧・最近使ったスクリプト）を返し、起動時に指定されたスクリプトを開く |
| `open` `new` | スクリプトを開く（`path` なしはピッカー）/ テンプレートから新規作成（`template`: `script`＝スクリプト、`form`＝フォームアプリ（`ui.form`）、`app`＝カスタムアプリ（`@mode app`＋`ui/index.html`）） |
| `drop` | ウィンドウにドロップしたファイル（`postMessageWithAdditionalObjects` で渡し、Rust 側でパスにする）。最初のスクリプト（.js・.mjs・.ts）を開く。フォルダならその中でピッカー、スクリプトがなければエラー。チャットの枠（別の WebView2）へのドロップはチャット側のもの |
| `dragFile` | コードタブのファイル名の左のつまみ（`#file-drag`）を動かしたとき、編集中のファイル（`file`）をウィンドウの外へドラッグする。Rust がシェルのデータオブジェクト（`SHParseDisplayName` → `SHCreateShellItemArrayFromIDLists` → `BHID_DataObject`。エクスプローラーと同じ CF_HDROP など）で `SHDoDragDrop` を始める（`BrowserCommand::DragFiles`。届いたときにボタンが離されていれば何もしない）ので、チャットの枠へのドロップはエクスプローラーからのドロップと同じになる。ドラッグ中は自分のページへのドロップを無視する（`st.fileDrag`） |
| `read` `save` `edit` | ファイルの読み書き。`edit` は編集中のファイル（置き換え先の決定用） |
| `apply` `versions` `version` `restore` | 置き換え（履歴に残す）、履歴の一覧・内容、前の版に戻す |
| `run` `stop` `preview` | 実行（`debug`）・停止・フォームのプレビュー（`auto` はタブを開いたときの自動表示） |
| `pick` | プレビューで選んだ要素についての AI への依頼のコピー（`elements` `text` `full`） |
| `pack` | exe の作成（保存先を尋ねる。結果は `packed`（`path`・`warnings`）） |
| `chat` `setHome` `browser` | チャットの URL（空で選ぶ画面）、デフォルトのチャット（空で解除）、枠の表示・位置・戻る・進む・再読み込み |
| `agentHint` `agentKit` `agentCopy` `agentDrag` | xljs 用のエージェントの案内（チャットのサービスの判定、入れるもの、文書のコピー、文書のドラッグ） |
| `intro` `notify` `showFolder` | AI への説明のコピー（［プロンプト］のドロップダウン。`mode` が `prompt`＝指示だけ、`script`＝スクリプトだけ、空＝両方）、自分で直したところのコピー、フォルダを開く |
| `wizardBooks` `wizardFolder` `wizardPrompt` `wizardSave` `wizardCreate` | 新規作成ウィザード（`answers`） |
| `tests` `testApprove` `testRecord` `testPrompt` | 「テスト」タブ（`docs/design/test-design.md`） |

| Rust → ページ（`type`） | 内容 |
|---|---|
| `init` `project` `meta` `files` | 初期化、開いたスクリプト、メタデータの変化、ファイル一覧 |
| `file` `saved` `applied` `versions` `version` | ファイル・保存・置き換え・履歴 |
| `download` | ダウンロードしたファイル（`name` `text` `target` `old` `files`） |
| `runStarted` `output` `runEnded` | 実行。`runEnded` は失敗時に `message` `location` `prompt` |
| `previewForm` `previewFailed` `previewSkipped` | フォームのプレビュー（`previewSkipped` は自動表示をしなかったとき） |
| `testsStarted` `tests` | テストケースの実行と結果 |
| `agentHint` `agentKit` | チャットのサービスと作ったものかどうか、エージェントなどに入れるもの |
| `url` `copy` `error` `info` `browserFailed` | チャットの URL、クリップボードへのコピー、エラー・お知らせ |
| `wizardBooks` `wizardFolder` `wizardPrompt` `wizardAttach` | ウィザードの答え（`wizardPrompt` の `attachment`、`wizardAttach` の `file` は添付ファイルの名前） |

ページの配置（グループとタブの並び、閉じたタブ、境目の位置）はページの `localStorage` に保存する。最近使ったスクリプトはユーザーの設定（`%APPDATA%\xljs\dev.json`）。

## テスト

- `src/dev/tests.rs`: 置き換え先の決定、履歴、エラーの場所と依頼文、セッションの流れ（記録するだけのウィンドウとモック UI）、別プロセスの実行（Linux の `sh`）。
- `src/engine/tests.rs`: `dev_guard`、フォームのプレビュー、無限ループで中止した場所。
- ネイティブの枠（2 つ目の WebView2、ダウンロード）は Windows 実機で確認する（`docs/design/gui-testing.md` の手順）。ページは `XLJS_DEV_DEBUG=1` で開発者ツールが使える。

## エディタ（`dev-editor.js`・`dev-types.js`）

- CodeMirror 6。必要なモジュールだけを `tools/codemirror/index.js` から esbuild で 1 つの IIFE（グローバルの `CM`）にまとめて `src/ui/web/vendor/codemirror/codemirror.js` に置く（`cd tools/codemirror && npm ci && npm run build`。版は `package.json` と `vendor/VERSIONS.md`）。
- 言語はファイルの拡張子で決める（`.js`・`.ts`・`.json`・`.html`・`.css`。ほかは色分けなし）。ファイルを開くたびに新しい `EditorState`（元に戻す履歴もファイルごと）。エラーの行は `markField`（行の装飾と行番号）。
- 色はページの CSS 変数（`dev.css` の `--hl-*`・`--editor-*`。ライト・ダーク）。CodeMirror の UI 文言（検索パネルなど）は `devPage.cm*`（`dev-editor.js` の `PHRASES` が英語の文言 → キー）。
- 補完とホバー（`dev-types.js`）: セッションの `init` が `types/xljs.d.ts`（`crate::DTS`）を `dts` で渡す。型検査器ではなく、d.ts を読む小さなパーサー（interface・class・type・グローバルの const/var/function。JSDoc を説明に）と、構文木（Lezer）をたどる型の解決。
  - 解決できるもの: グローバル、メンバー・メソッドの戻り値（`Promise` は `await` で外す。オーバーロードは最初のもの）、`x[i]` と `for (... of x)` の要素、`const x = 式`、TS の型注釈、型引数（既定値・制約で置き換え。推論はしない）、引数・プロパティの位置で期待される型（`ui.form({ ` のオプション名、`onSubmit: (v, ctx) => ctx.` や `rows.map((r) => r.` の引数）。
  - JS の組み込み（Array・String・Date・Map・Math・JSON など）は `dev-types.js` の `BUILTINS` に最小限の宣言を持つ。
  - 分からない型は何も出さない（CodeMirror の JS の補完＝ローカルの名前・キーワード・スニペットは残る）。
- 整形（［整形］・Shift+Alt+F）: Prettier の standalone 版（`vendor/prettier`。初めて使うときに `<script>` で読み込む）。パーサーは拡張子で `babel`・`babel-ts`（TS も babel のプラグインで足りるので、大きい typescript のプラグインは入れない）・`json`・`html`・`css`。設定は既定から `printWidth: 100` だけ変える。結果は変わった中ほどだけを 1 回の変更として置き換える（Ctrl+Z で戻せる、表示の位置が飛ばない）。構文エラーはその行へ移動してステータスに出す。
