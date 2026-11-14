# 開発者向けメモ

xljs 本体を開発する人向けのメモです。スクリプトを書く人向けのリファレンスは [GitHub Pages](https://cool0707.github.io/xljs/)（原稿は `docs/site/`）にあります。

## ビルド

Rust（stable、1.91 以降）が必要です。JavaScript エンジンの QuickJS は C で書かれているので、C コンパイラも使います（Windows では Visual Studio Build Tools の MSVC、Linux からのクロスビルドでは MinGW-w64）。

```
# Windows 上（MSVC）: x64 の GUI 版とコンソール版
cargo build --release                                  # target\release\xljs.exe（GUI 版）
cargo build --release --features console --target-dir target\console   # コンソール版（xljsc.exe として配布）

# x64 / arm64 の両方を dist/ に（Git Bash などの bash で）
make windows
make dist           # dist/xljs-windows-x64.zip と xljs-windows-arm64.zip

# Linux から x64 版をクロスビルド（apt install mingw-w64）
make windows-gnu    # dist/x64/
```

- GUI 版（xljs.exe）は `windows_subsystem = "windows"` でビルドし、ターミナルから起動したときはそのコンソールに接続します。`console` フィーチャーを付けるとコンソール版（xljsc.exe）になります。
- exe のアイコン（`assets/xljs.ico`）とマニフェスト（`src/cli/xljs.manifest`、ビジュアルスタイル・高 DPI 対応）は `build.rs` が `src/cli/xljs.rc` をリソースとして埋め込みます。アイコンを作り直すときは `python3 tools/make_icon.py ロゴ.png`（`assets/xljs.png` と `assets/xljs.ico` を生成）。
- WebView2 は WebView2Loader.dll を使わず、インストール済みのランタイムを直接読み込みます（`src/ui/native/loader.rs`）。配布物は exe 1 つです。

## テスト

```
cargo test --lib                                   # Linux でも全ロジックをテスト可（ネイティブ UI 以外）
cargo check --target x86_64-pc-windows-gnu --all-targets   # Windows 専用コードのコンパイル確認
```

- `testdata/scripts/*.test.{js,ts}` はモック UI で実行される結合テストです。`*.answers.json` に応答を書きます。同じスクリプトをデバッガ経由（偽のデバッグページ）でも実行します。
- UI はトレイトで分離しているので、ネイティブダイアログ以外のロジックは Linux でもテストできます。Windows では `src/ui/native/tests.rs` がダイアログを実際に開いて自動で閉じます。
- スクリプトは専用のスレッドで動き、メインスレッドが UI（ダイアログ・ウィンドウのメッセージループ）を受け持ちます（`ui::run`）。重い処理の間もウィンドウが固まりません。

| パス | 内容 |
|---|---|
| `src/main.rs`, `src/cli` | CLI（引数処理、サブコマンド、開始画面、コンソール処理） |
| `src/engine` | バンドラ（oxc）、QuickJS での実行、イベントループ、エラー表示、デバッガ |
| `src/api` | Rust 側のネイティブ関数と `prelude.js`（公開 API の JS 実装） |
| `src/i18n` | メッセージの多言語化（`locales/*.json` を埋め込み） |
| `src/ui` | UI トレイトと native（Win32・WebView2）/ console / mock 実装、UI スレッド |
| `src/pack` | exe へのスクリプト・アセット埋め込み（zip） |
| `src/meta` | スクリプト先頭のメタデータの読み取りとアセットの範囲 |
| `src/vba` | マクロブックの VBA コードの読み取り |
| `src/book` | ブックの数式・入力規則・名前の定義・条件付き書式・グラフ・表の列の特徴をテキストに（`xljs vba`） |
| `types/xljs.d.ts` | 型定義（`xljs init` で出力） |

スクリプトは [oxc](https://oxc.rs/) で TypeScript を変換し、ES モジュールを async 関数に変えて（oxc の module runner 変換）1 ファイルにまとめ、[QuickJS](https://github.com/quickjs-ng/quickjs)（[rquickjs](https://github.com/DelSkayn/rquickjs)）で実行します。インラインのソースマップでエラーの行番号は元のファイルの位置になります。`--debug` のときだけはデバッグウィンドウ（WebView2）のワーカーで実行し、ネイティブ関数を Rust 側からメッセージで提供します（`src/api/remote.rs`、`src/engine/debug.rs`、`src/ui/web/debug*.js`）。

## Go 版との違い

- JavaScript エンジンが goja から QuickJS に、xlsx のライブラリが excelize から umya-spreadsheet に変わりました。エンジンの英語のエラーメッセージの文面が違います（日本語訳は `ja.json` の `translations` で付けています）。
- `cell.calc()` は xljs 内蔵の簡易計算です（四則演算・比較・文字列連結・セル参照、SUM・AVERAGE・MIN・MAX・COUNT・ROUND・IF・SUMIF・COUNTIF などの代表的な関数）。
- 組み込みの書式コードの `getStyle().numberFormat` は番号ではなく書式の文字列で返ります（`"#,##0"` など）。
- `--debug` の偽ページのテストは QuickJS で行います。実機の WebView2 での動作確認は Windows の CI（`webview_window` テスト）で行います。

## ドキュメント

| 場所 | 読む人 | 内容 |
|---|---|---|
| `README.md` | 初めて見る人 | 簡単な紹介とリンクだけ |
| `types/xljs.d.ts` | エディタ・リファレンス | **API の正**。シグネチャとドキュメントコメント。`xljs init` で利用者に配る |
| `docs/site/` | スクリプトを書く人 | リファレンスサイト（GitHub Pages）の原稿。ページの並びと、インラインコードの変数名から型へのリンク（`sheet.toPdf` → `Worksheet.toPdf`）は `site.json` |
| `docs/ai/` | AI チャット | プロンプトとリファレンス（配布 zip の `prompt/`）。文字数の上限に合わせて手で要約している |
| `docs/design/` | 本体の開発者・Claude Code | 機能ごとの設計メモ |
| `examples/` | スクリプトを書く人 | サンプルと、それで試すサンプルデータ `examples/data/`（`tools/examples/make_data.py` で生成）。配布 zip の `examples/`。サイトの「サンプル」ページは `examples/README.md` をそのまま表示 |

サイトは `tools/docs/build.mjs` が生成し、`.github/workflows/docs.yml` が main への push で GitHub Pages に公開します（リポジトリの Settings → Pages の Source を「GitHub Actions」にしておく）。

```
cd tools/docs && npm ci
node build.mjs            # _site/ に生成
node build.mjs --serve    # 生成して http://localhost:8080/ で見る
```

ビルドは次のどれかがあると失敗します（PR でも CI が検査する）。

- d.ts の宣言（グローバルの interface・class・type・関数・変数）が、どのページの `api:` にもない、または 2 つのページにある。API を足したら、該当する `docs/site/api/*.md` のフロントマターの `api:` に名前を足す。
- コード例（```` ```js ```` / ```` ```ts ````）が d.ts に対して型エラーになる。対象はサイトの原稿・`README.md`・`examples/README.md`・`docs/ai/` のすべて。前後の文章に出てくる変数（`book`、`rows` など）が未定義なのは許すが、存在しないメソッド・オプションや型の合わない引数はエラーになる。検査したくないブロックは ```` ```js nocheck ````。
- ページ間のリンク切れ、リポジトリにないファイルへのリンク。

ページの書き方:

- フロントマターは `title`・`description`・`api`（このページで説明する d.ts の宣言、カンマ区切り）・`source`（本文として表示するリポジトリ内の別の Markdown。`docs/ai/README.md` など）。
- 本文のあとに「リファレンス」として `api` の宣言が d.ts から並ぶ。本文中に `<!-- api -->` の行を置くと、そこに入る。
- 本文の `` `Workbook` `` `` `ui.form` `` `` `Worksheet.toPdf` `` のようなインラインコードは、d.ts の宣言・メンバーなら自動でリファレンスへのリンクになる。
- 説明はなるべく d.ts のドキュメントコメントに書く（エディタの補完にも出る）。ページの本文には使い方の流れ・例・注意を書く。

API を変えたときの手順: `types/xljs.d.ts` を直す → `docs/site/api/*.md`（例と説明、新しい宣言は `api:`）→ `docs/ai/`（`docs/ai/README.md` の「資料を更新するとき」）。
