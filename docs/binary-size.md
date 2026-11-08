# 実行ファイルのサイズ

xljs.exe（と、`xljs pack` で作る exe）のサイズの内訳、すでに入れた対策、今後の方針のメモ。
数字は Linux から `cargo build --release --target x86_64-pc-windows-gnu` でクロスビルドした x64 版（mingw）のもの。
配布物の MSVC 版とは多少ずれるが、傾向は同じ。

## 1. 内訳（2026-11、対策前 15.98MB）

| 区分 | サイズ | 中身 |
|---|---|---|
| コード（`.text` と例外処理の表 `.pdata`/`.xdata`） | 約 9.0MB | 下の表 |
| 埋め込みファイル | 4.5MB（無圧縮） | 組み込みページ・同梱ライブラリ・prelude など |
| その他の定数・リソース | 約 1.6MB | 文字列、Unicode の表、アイコン |

### コード（`cargo bloat --crates`、Linux ビルドでの概算）

| クレート | サイズ | 備考 |
|---|---|---|
| xljs 本体 | 2.3MB | natives はすべて `api::install_natives` で登録するので、使わない機能も必ずリンクされる |
| oxc（TS 変換・バンドル） | 1.4MB | パック済み exe では使わない（中身はバンドル済み。実行時はソースマップの読み取り＝`oxc_sourcemap` だけ） |
| umya-spreadsheet | 0.9MB | ほかに jiff・fancy-regex・暗号系（aes・cbc・md5・sha2・hmac・rand）・html5gum を引き込む |
| SQLite（C）＋rusqlite | 約 0.8〜1.0MB | |
| regex 系（regex・fancy-regex・aho-corasick） | 0.7MB | |
| std | 0.56MB | |
| QuickJS（C） | 約 0.4MB | |
| lopdf・subsetter・ttf-parser | 0.3MB | |

### 埋め込みファイル

| 中身 | 無圧縮 | deflate |
|---|---|---|
| 開発モード（CodeMirror・Prettier・`dev*`） | 1.60MB | 0.49MB |
| グラフ（AG Charts・Frappe Gantt・`chart*`） | 1.48MB | 0.45MB |
| Font Awesome | 0.35MB | 0.28MB（woff2 は圧縮済み） |
| 図形の定義（`preset_shapes.xml` ほか） | 0.30MB | 0.03MB |
| prelude.js | 0.25MB | 0.07MB |
| d.ts・メッセージ・フォーム／表・Day.js ほか | 0.5MB | 0.2MB |
| 合計 | 4.5MB | 1.5MB |

## 2. 入れた対策: 埋め込みファイルの圧縮

`build.rs` が埋め込むファイルを deflate で圧縮して `OUT_DIR/embed/` に書き、`src/embed.rs` の `File` が初めて使うときに展開して保持する
（小さくならないもの＝フォント・アイコンは無圧縮のまま）。対象は次のとおり。

- `src/ui/web`（`WEB_FILES`）
- `src/api/dayjs`（`DAYJS_FILES`）
- `src/i18n/locales/*.json`（`LOCALE_FILES`。フォルダの `.json` はすべて入る）
- `prelude.js`、`preset_shapes.xml`、`preset_text_warps.xml`、`types/xljs.d.ts`、`docs/ai/prompt.md`

結果: 15.98MB → 12.94MB（−3.0MB）。展開は zip と同じ flate2（zlib-rs）で、コードはほとんど増えない。
展開にかかる時間は prelude（250KB）で 1ms 程度。
大きいファイルを埋め込むときは `include_bytes!` / `include_str!` ではなく `build.rs` の一覧に足す。

## 3. 今後の方針

### 3.1 パック専用のランタイムを別ビルドで同梱する（次にやる）

`pack::write` は xljs.exe 自身をコピーして末尾にスクリプトを付けるので、パック済み exe にも次のものが入っている。

- TS 変換・バンドル（oxc）
- 開発モード（`src/dev` と開発用の Web ファイル）
- サブコマンド（`init`・`check`・`pack`・`vba`・`dev`）

これらを cargo の feature（例: `runtime`）で外したビルドをパックの土台にする。

- 試作の実測（oxc・開発モード・`init` を止めたもの。圧縮前）: 15.98MB → 12.10MB（−3.9MB）。
  圧縮と合わせると約 10MB の見込み。
- 配布: 配布 zip にランタイム（GUI 版・コンソール版）を同梱し、`xljs pack` は xljs.exe と同じフォルダのランタイムを土台にする。
  見つからなければ今どおり自分自身を使う。
  xljs.exe の中にランタイムを持たせる案は、xljs.exe が約 4MB 太るので採らない。
  - パック済み exe の GUI 版／コンソール版は、今は `pack` を実行した exe（xljs.exe / xljsc.exe）に従う。ランタイムも同じ対応にする。
- ランタイムでも残すもの:
  - ソースマップによるスタックの対応付け（`oxc_sourcemap`）
  - `XLJS_DEBUG` でのデバッガ（`web/debug*`。バンドル済みのコードで動く）
  - 中止・メモリの見張り
- 実装の要点:
  - `engine::bundle` / `bundle_with` と `src/dev` を `#[cfg(not(feature = "runtime"))]` で外し、CLI のサブコマンドも外す（ランタイムはペイロードがないとき使い方のエラーにする）。
  - `build.rs` で開発用の Web ファイル（`dev*`・`vendor/codemirror`・`vendor/prettier`）を外す。
  - Makefile の `windows` / `windows-gnu` / `dist` にランタイムのビルドと同梱を足す。
  - CI でランタイムのビルドと、パック済み exe の結合テスト（ペイロードを付けて mock UI で実行）を流す。

### 3.2 機能別の軽量ランタイム（必要になったら）

3.1 のランタイムから、さらに使われにくい機能を feature で外した「lite」を 1 種類だけ用意する案。
natives は機能ごとに `sqlite::install` などで登録しているので、グループ単位で `#[cfg]` にしやすい。

- 試作の実測（3.1 の試作から SQLite・PDF 編集・AD を外したもの。圧縮前）: 12.10MB → 9.80MB（−2.3MB）。
  圧縮と合わせると約 8MB の見込み。
- 候補ごとの目安:
  - SQLite（`src/db`。C の SQLite 込み）: 約 1MB
  - PDF 編集（`src/pdf`。lopdf・subsetter・ttf-parser）: 約 1MB
  - AD（`src/ad`）: 約 0.1MB
  - グラフの同梱ライブラリ: 圧縮後 0.45MB
- 課題:
  - どのランタイムを使うかの選び方。バンドル後のコードを静的に調べると、動的なアクセスや `ad` のような短い名前で誤判定する。
    メタデータ（例: `@features`）か `xljs pack --lite` のような明示の指定にする。
    グラフはフォームの `chart` 項目や `@html` のページの `xljs.chart` からも使われるので、静的には読み切れない。
  - lite で外した API（`sqlite.open` など）を呼んだら、「この exe には含まれていない」と分かる日本語のエラーにする（prelude で natives の有無を見る）。
  - `Host` が持つ `dbs`・`pdfs` などの型も cfg で外さないと、後始末のコードから SQLite などがリンクされ続ける（試作で確認）。
  - 組み合わせを増やすと、ビルドが GUI／コンソール × x64／arm64 の倍々で増える。lite は 1 種類までにとどめる。

### 3.3 見送ったもの

- **グラフの同梱ライブラリだけをパック時に選ぶ**（Web ファイルをペイロードの zip に入れる）: 圧縮後は 0.45MB と小さく、使用の判定も難しい（3.2）。
- **`opt-level = "z"`**: 全体に掛けると −1.5MB（14.47MB）だが、Excel の読み書きや QuickJS の速度が落ちるおそれがある。
  oxc など実行速度に関係しないクレートだけ `[profile.release.package.*]` で `z` にする案は未計測。
- **`panic = "abort"`**: 例外処理の表が減るが、`catch_unwind` を使っている箇所があるので確認が要る。
- **UPX での圧縮**: 半分ほどになるが、ウイルス対策ソフトの誤検知が多く、業務の配布には向かない。
- **umya-spreadsheet の依存を feature で外す**（暗号系・html5gum。フォークで可能）: 効果は 100KB 程度。

## 4. 測り方

```
cargo build --release --target x86_64-pc-windows-gnu
x86_64-w64-mingw32-objdump -h target/x86_64-pc-windows-gnu/release/xljs.exe    # セクションごと
CARGO_PROFILE_RELEASE_STRIP=false cargo bloat --release --crates -n 40 --target x86_64-unknown-linux-gnu
```

`cargo bloat` は mingw の PE を読めないので、クレート別の内訳は Linux ビルドで測る。
埋め込みファイルの圧縮前後の合計は、生成される `OUT_DIR/embedded.rs` の最後の行に出る。
