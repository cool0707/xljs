# xljs — 開発メモ（Claude Code 向け）

JavaScript/TypeScript のスクリプトで Excel・CSV・JSON を処理する Windows 用シングルバイナリ。
利用者は「VS Code も入れられないような」一般の業務ユーザー。Excel 未インストール環境で動くこと、
日本語環境での使い勝手（CP932、游ゴシック、日本語メッセージ）を重視する。

進行中の大きな作業（HTML UI・フォーム・表表示・デバッガ）は `docs/handoff-ui.md` を参照。

## 構成

| パス | 役割 |
|---|---|
| `cmd/xljs` | CLI。グローバルフラグ（`--ui` `--ui-answers` `--lang` `--debug`）→ サブコマンド（pack/check/init/vba/version/help。スクリプトの代わりにブックを渡すと VBA の取り出し。引数なしは `launch.go` の開始画面＝実行・デバッグ・exe 作成・VBA 取り出しの選択、`ui.Chooser`）→ メタデータを読んで `ui.Run` でスクリプト実行。`rsrc_windows_*.syso` はアイコン＋マニフェスト |
| `internal/engine` | esbuild でバンドル（TS 変換、async generator 変換、インラインソースマップ）→ `(async () => {...})` で包んで goja で実行。`loop.go` はタイマーと他 goroutine からの post。`debug.go` は `--debug` のデバッグセッション（後述） |
| `internal/api` | Go 側ネイティブ関数（`natives` に `n.set("name", fn)` で登録）と `prelude.js`（公開 API をすべて JS で組み立てる）。Go→JS は `native.xxx` 経由のみ。`dayjs/` は npm の Day.js をそのまま置いたもの（`dayjs.go` が prelude 末尾の `/*@dayjs@*/null` に埋め込む。グローバルの `dayjs`、常に日本語ロケール） |
| `internal/ui` | `UI` インターフェースと実装: `Native`（Win32/COM 直叩き、cgo なし。HTML ウィンドウは `webview_windows.go`＝`wailsapp/go-webview2` の `pkg/edge`）、`Console`、`Mock`（応答 JSON＋ログ。テスト用）。`thread.go` の `ui.Run` がスクリプトを専用スレッドで動かし、UI 呼び出しを呼び出し元（main スレッド）へ転送する。`web/` は `ui.form` / `ui.table` の組み込みページ（`https://app.xljs/_xljs/` で配信）と、全ページに注入する画面側ランタイム `xljs.js`（アプリモードの `xljs` オブジェクト。スクリプト側は prelude の `AppWindow`） |
| `internal/meta` | エントリ先頭の `// ==xljs==` メタデータのパースと、アセット（`@html`＋`@assets`）だけを見せる `AssetFS` |
| `internal/vba` | マクロブックの VBA ソースの読み取り（`xljs vba`・ドロップ時の `cmd/xljs/vba.go` と `workbooks.readVBA`）。zip の `vbaProject.bin` か .xls の `_VBA_PROJECT_CUR` を mscfb で開き、`dir` ストリーム（MS-OVBA）を解凍してモジュールを読む。テスト用の .xlsm は `fixture_test.go` が組み立てる（`go test ./internal/vba -run TestWriteFixture -update` で `testdata/scripts/macro.xlsm` を更新） |
| `internal/i18n` | メッセージ。`locales/<lang>.json` を go:embed。`en.json` が基準・フォールバック |
| `internal/pack` | exe 末尾へのスクリプト埋め込み。v2 = `[zip(manifest.json, main.js, assets/...)][len u64]["XLJSPAK2"]`、v1（`XLJSPAK1`）も読める |
| `types/xljs.d.ts` | 公開 API の型定義（`xljs init` で出力）。API を変えたら必ず更新 |
| `testdata/scripts/*.test.{js,ts}` | モック UI で実行される結合テスト。`<name>.answers.json` がモック応答 |
| `examples/` | サンプル。`xljs init` の d.ts で tsc の型チェックが通る状態を保つ |
| `docs/ai/` | AI チャットにスクリプトを書かせるためのプロンプト（`prompt.md`、15,000 字以内）と詳しいリファレンス。API を変えたら d.ts と一緒に更新。`TestAIDocs` がコードブロックの構文と長さを検査 |

## 規約

- **ユーザー向けメッセージはすべて `internal/i18n/locales/*.json`**。Go は `i18n.T(key, args...)` / `i18n.Err(...)` / `i18n.Error(err)`、prelude は `T(key, ...)`。新しいキーは `en.json` → `ja.json` の順に追加。プレースホルダは `{0}` `{1}`。`go test ./internal/i18n` がキーの存在・プレースホルダの一致・ソース中で使われているキーの定義を検査する。
- **ライブラリの英語エラー**（goja・esbuild・excelize）は `ja.json` の `translations`（正規表現）で表示時に訳す。`catch` で受け取る `e.message` は訳さない。
- **API の流儀**: ブラウザの File System Access API に寄せる（`showOpenFilePicker` はキャンセルで `[]`、`showSaveFilePicker`/`showDirectoryPicker` は `null`）。ファイル系は同期を基本にし `await` も可。パスを受け取る引数は文字列・ハンドル・`File` のいずれも可（prelude の `toPath`）。
- **Excel**: 値は number/string/boolean/Date/null（Day.js のオブジェクトは prelude の `plainDate` で Date にしてから natives へ渡す）。Date はローカルの壁時計時刻として扱う（`time.UTC` に詰めて excelize へ）。書式はマージが既定（`applyStyle`）、範囲端の罫線は書式済みの隣接セルにも反映。
- **Windows ネイティブ部分**は `//go:build windows`。COM の vtable 呼び出しは `comObject.call(idx, ...)`。`unsafe.Pointer` 変換は go vet が通る書き方にする（uintptr→Pointer 変換は RtlMoveMemory などで回避）。
- 新規ブックの既定フォントはロケールの `excel.font`（日本語は游ゴシック）。
- **スレッド**: main スレッド＝UI スレッド（Win32 メッセージループ、ダイアログ、WebView2）、スクリプト＝`ui.Run` が作る専用 goroutine（`LockOSThread`）。`UI` の実装は UI スレッドからだけ呼ばれる。UI → スクリプトは `Env.Post`（＝`loop.post`、事前に `Env.AddPending`）。開いているウィンドウは pending として数え、閉じるまでイベントループ（プロセス）が終わらない。
- **デバッガ（`--debug`）**: prelude とスクリプトをデバッグウィンドウ（`web/debug.html`、WebView2 の V8）のワーカー（`debug-worker.js`）で実行し、開発者ツールでデバッグする。natives は Go 側の goja に残り、`api.Remote` が JSON のワイヤ形式（`remote.go` 冒頭）で中継する。ワーカー→Go の呼び出しは、ページ経由の WebView2 メッセージ（`call:` ＋要求、応答は `R` ＋応答）で、ワーカーは SharedArrayBuffer を `Atomics.wait` で待つ（ページは `WindowOptions.Isolated` で COOP/COEP 付き）。同期 XHR を WebResourceRequested の deferral で答える方式は CI の WebView2 で応答が返らなかったので使わない。Go→ページはイベント（ページ側関数の呼び出し、Promise の settle）。終了判定は Go 側（ページの idle 報告で処理済みイベント数が一致し、デバッグウィンドウ以外に pending がない）。`--debug=window` は従来どおり goja で実行して画面の開発者ツールだけ有効。natives を足すときは値がワイヤ形式で表せること（関数・Promise・Go オブジェクトは参照になる）。Linux では `debug_test.go` の `fakeDebugUI` が goja でページ役をし、`TestScriptsInDebugger` が全結合テストをこの経路でも流す。
- コミットメッセージは英語、ドキュメント・UI 文言は日本語（英語はロケールファイル側）。

## 開発

```
go test ./...                       # Linux でも全ロジックをテスト可（ネイティブ UI 以外）
GOOS=windows go vet ./...           # Windows 専用コードのコンパイル確認
make windows / make dist            # dist/x64/{xljs,xljsc}.exe と配布 zip（x64/arm64）
xljs --ui=mock --ui-answers=a.json --lang=ja script.js   # ダイアログなしで実行
xljs --debug script.ts                                    # デバッグウィンドウ＋開発者ツール（Windows）
```

- Go 1.26（go.mod）。cgo 不要。`-H=windowsgui` で GUI 版（xljs.exe）。付けないとコンソール版（xljsc.exe）。
- CI（.github/workflows/ci.yml）: ubuntu と windows で vet+test、Windows ではダイアログ生成テスト（`internal/ui/native_windows_test.go`、`testAutoClose` で自動クローズ）も走る。失敗時はテスト出力をアノテーションに出す。タグ `v*` で Release に zip を添付。
- 出力の見た目確認: LibreOffice があれば `soffice --headless --convert-to pdf` → `pdftoppm` で画像化できる。
- アイコン再生成: `python3 tools/make_icon.py <logo>` → rsrc で syso を作り直す（README 参照）。
