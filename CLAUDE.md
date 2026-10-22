# xljs — 開発メモ（Claude Code 向け）

JavaScript/TypeScript のスクリプトで Excel・CSV・JSON を処理する Windows 用シングルバイナリ。
利用者は「VS Code も入れられないような」一般の業務ユーザー。Excel 未インストール環境で動くこと、
日本語環境での使い勝手（CP932、游ゴシック、日本語メッセージ）を重視する。

進行中の大きな作業（HTML UI・フォーム・表表示・デバッガ）は `docs/handoff-ui.md` を参照。

## 構成

| パス | 役割 |
|---|---|
| `cmd/xljs` | CLI。グローバルフラグ（`--ui` `--ui-answers` `--lang`）→ サブコマンド（pack/check/init/version/help）→ スクリプト実行。`rsrc_windows_*.syso` はアイコン＋マニフェスト |
| `internal/engine` | esbuild でバンドル（TS 変換、async generator 変換、インラインソースマップ）→ `(async () => {...})` で包んで goja で実行。`loop.go` はタイマーと他 goroutine からの post |
| `internal/api` | Go 側ネイティブ関数（`natives` に `n.set("name", fn)` で登録）と `prelude.js`（公開 API をすべて JS で組み立てる）。Go→JS は `native.xxx` 経由のみ |
| `internal/ui` | `UI` インターフェースと実装: `Native`（Win32/COM 直叩き、cgo なし）、`Console`、`Mock`（応答 JSON＋ログ。テスト用） |
| `internal/i18n` | メッセージ。`locales/<lang>.json` を go:embed。`en.json` が基準・フォールバック |
| `internal/pack` | exe 末尾へのスクリプト埋め込み（`[nameLen][name][code][len u64]["XLJSPAK1"]`） |
| `types/xljs.d.ts` | 公開 API の型定義（`xljs init` で出力）。API を変えたら必ず更新 |
| `testdata/scripts/*.test.{js,ts}` | モック UI で実行される結合テスト。`<name>.answers.json` がモック応答 |
| `examples/` | サンプル。`xljs init` の d.ts で tsc の型チェックが通る状態を保つ |

## 規約

- **ユーザー向けメッセージはすべて `internal/i18n/locales/*.json`**。Go は `i18n.T(key, args...)` / `i18n.Err(...)` / `i18n.Error(err)`、prelude は `T(key, ...)`。新しいキーは `en.json` → `ja.json` の順に追加。プレースホルダは `{0}` `{1}`。`go test ./internal/i18n` がキーの存在・プレースホルダの一致・ソース中で使われているキーの定義を検査する。
- **ライブラリの英語エラー**（goja・esbuild・excelize）は `ja.json` の `translations`（正規表現）で表示時に訳す。`catch` で受け取る `e.message` は訳さない。
- **API の流儀**: ブラウザの File System Access API に寄せる（`showOpenFilePicker` はキャンセルで `[]`、`showSaveFilePicker`/`showDirectoryPicker` は `null`）。ファイル系は同期を基本にし `await` も可。パスを受け取る引数は文字列・ハンドル・`File` のいずれも可（prelude の `toPath`）。
- **Excel**: 値は number/string/boolean/Date/null。Date はローカルの壁時計時刻として扱う（`time.UTC` に詰めて excelize へ）。書式はマージが既定（`applyStyle`）、範囲端の罫線は書式済みの隣接セルにも反映。
- **Windows ネイティブ部分**は `//go:build windows`。COM の vtable 呼び出しは `comObject.call(idx, ...)`。`unsafe.Pointer` 変換は go vet が通る書き方にする（uintptr→Pointer 変換は RtlMoveMemory などで回避）。
- 新規ブックの既定フォントはロケールの `excel.font`（日本語は游ゴシック）。
- コミットメッセージは英語、ドキュメント・UI 文言は日本語（英語はロケールファイル側）。

## 開発

```
go test ./...                       # Linux でも全ロジックをテスト可（ネイティブ UI 以外）
GOOS=windows go vet ./...           # Windows 専用コードのコンパイル確認
make windows / make dist            # dist/x64/{xljs,xljsw}.exe と配布 zip（x64/arm64）
xljs --ui=mock --ui-answers=a.json --lang=ja script.js   # ダイアログなしで実行
```

- Go 1.26（go.mod）。cgo 不要。`-H=windowsgui` で GUI 版（xljsw.exe）。
- CI（.github/workflows/ci.yml）: ubuntu と windows で vet+test、Windows ではダイアログ生成テスト（`internal/ui/native_windows_test.go`、`testAutoClose` で自動クローズ）も走る。失敗時はテスト出力をアノテーションに出す。タグ `v*` で Release に zip を添付。
- 出力の見た目確認: LibreOffice があれば `soffice --headless --convert-to pdf` → `pdftoppm` で画像化できる。
- アイコン再生成: `python3 tools/make_icon.py <logo>` → rsrc で syso を作り直す（README 参照）。
