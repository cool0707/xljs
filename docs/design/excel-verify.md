# Excel での検証（xlsx の読み書き・数式の計算・表示形式）

xljs の xlsx の扱いは、それまで LibreOffice との突き合わせでしか確かめていなかった。LibreOffice は Excel が開けないファイルも開き
（`_xlfn.` の付いていない新しい関数、`G1#` のままのスピル参照など）、表示形式の解釈も Excel と違うので、互換の確認には使えない。
2026-11-15 に Windows 実機の Excel（Office Professional Plus 2021、16.0.20326）を COM から動かして確かめ、見つかったものを直した。

## 1. 道具（`tools/excel-verify/`）

```
cargo build
pwsh -File tools/excel-verify/run.ps1        # 各項目の PASS / FAIL。差の詳細は %TEMP%\xljs-excel-verify\*.txt
pwsh -File tools/excel-verify/features.ps1   # セル以外の機能を 1 つずつ（docs/design/save-fidelity.md）。-Features sparkline,pivot で絞れる
```

| ファイル | 役割 |
|---|---|
| `run.ps1` | 全項目を流す。失敗があれば終了コード 1 |
| `dump.js` | xljs でブックの全セル（値・数式・表示）を JSON に出す。`calc` で `book.calculate()` してから、保存先を渡せば保存も |
| `excel-dump.ps1` | Excel で同じ形の JSON を出す。`-Recalc`（CalculateFull）、`-Manual`（手動計算で開き、ファイルの保存値を読む）、`-Wide`（列幅を広げてから表示を読む） |
| `excel-shapes.ps1` | Excel から見た図形（名前・種類・文字・文字の変形・向き・線・大きさ）の一覧 |
| `compare.py` | 2 つの JSON を比べる（値は相対誤差 1e-15。日付は Date を通るのでミリ秒まで） |
| `excel-make.ps1` | Excel 自身にブックを作らせる（標準の表示の数値、表示形式、Excel で入力した数式・テーブル・書式） |
| `make_builtin.py` | 組み込みの表示形式 0〜81 を定義なしで参照するブック（Excel が保存するのと同じ形） |
| `write.js` | xljs でいろいろ書いたブック |
| `open-test.ps1` | Excel で開けるかだけを見る（開けないファイルは修復のダイアログを出さずに失敗する） |
| `features.ps1` | セル以外の機能を 1 つずつ入れたブックを Excel に作らせ（`excel-make-features.ps1`）、xljs で保存したものを、開けるか・Excel から見て同じか（`excel-features.ps1`）・部品と要素（`partdiff.py`）で比べる |

項目は、数式の計算（`calc`・`calc-saved`）、Excel が作ったブックの読み込み（`read`・`builtin-formats`）、xljs が書いたブック（`write`・`write-recalc`）、
開いてそのまま保存したブックと元のブックの比較（`roundtrip-*`。セルと図形 `shapes-*`）。

Excel の COM から読む `Text` は列幅で変わる（`####`、桁の切り詰め、`*` の繰り返し）。表示は `-Wide` で比べ、`#` だけの表示と `*` の埋めの空白は同じとみなす。
このマシンの Excel は 2021 なので、Microsoft 365 にしかない関数（`LAMBDA`・`TAKE`・`TEXTSPLIT` など）は Excel 側が `#NAME?` になる。`write.js` はそれらを使わない。

原因を絞るときは、ファイルの部品を 1 つずつ元のものに差し替えたり、描画のアンカーを 1 つずつ抜いたりした版を `open-test.ps1` で開くのが早い。

## 2. 確かめられたこと

- **数式の計算**: `testdata/xlsx/calc.xlsx` の全数式（139 個の関数の例、配列数式・スピル・テーブル・共有数式・名前）の値が、Excel の再計算と相対誤差 1e-15 で一致する。
  違うのは「保存値」シートの、Excel にも計算できない数式（マクロ関数・外部参照・未定義の名前）だけ（xljs は保存値を残す設計）。
- **計算して保存した値**: xljs が `calculate()` して保存したブックを Excel が再計算せずに読むと、全セルの値が xljs の結果と同じ。
- **読み込み**: Excel が作ったブックの値・数式・表示（標準の表示、組み込みと独自の表示形式、和暦、漢数字、経過時間、分数）が Excel と同じ。
- **書き込み**: 値・数式（`_xlfn.`・`_xlpm.`・`ANCHORARRAY`・動的配列のメタデータ）・結合・フォント・塗り・配置・罫線・表示形式・列幅・行の高さ・
  ウィンドウ枠の固定・フィルター・既定フォント（游ゴシック）が Excel で意図どおりに見える。テーブルの `[@数量]` を書いた数式も Excel がそのまま読む。
- **往復**: `testdata/xlsx`・`testdata/book`・`examples/data` のブックと Excel が作ったブックを開いて保存しても、値・数式・表示・表示形式・図形が変わらない。

## 3. 直したもの

### 3.1 Excel で開けなくなる・内容が変わる（umya のフォーク。`docs/design/pdf-design.md` 7 章）

- U+0001 などの制御文字を含む文字列を書くと、ブック全体が Excel で開けなかった（`_xHHHH_` で書く）。文字列 `_x000D_` が Excel で CR になっていた。
- 図形を含むブックを保存すると Excel で開けなかった・図形が消えた（空の `<a:ln/>`、空の `<a:prstGeom/>`）。
- ワードアートの文字の変形、縦書き、テキストボックスの印、フリーフォームの形が保存で落ちていた。
- 定義のない組み込みの表示形式（日本語版 Excel が書く ID 5〜8 など）のスタイルが、保存で 1 つにまとめられていた。
- Excel が書いた改行（ファイル上の CR LF）が CR LF のまま読めていた（Excel と同じく LF）。

### 3.2 表示（`cell.text`・`toObjects({ text: true })`・PDF の印刷。`src/api/numfmt.rs`）

- 端数の丸め: 0.5 を 0 から遠いほうへ（`#,##0` で 1234.5 → `1,235`）。値を 15 桁の 10 進にしてから丸めるので `1.005` → `1.01` も Excel と同じ。
- 標準（General）: 符号を除いて 11 文字（`0.333333333`、`142857.1429`）。小数にすると有効桁が減るときは指数（`1.23457E+11`、`3.33333E-05`、`1E+20`）。
  漢数字（`[DBNum1]General`）は Excel と同じく桁をすべて書く。
- 経過時間 `[h]:mm`・`[h]:mm:ss`・`[mm]:ss`・`[s].0`（組み込み ID 46・80 も）。
- 曜日 `aaa`・`aaaa` だけの表示形式を日付として扱う（`xlsx::date_format_code`）。
- 組み込みの表示形式の表を日本語版 Excel に合わせた（`xlsx::builtin_code`。5〜8 と 63〜66 は `"¥"#,##0;"¥"\-#,##0` ほか、23〜26 は `$`、
  37〜40 は `#,##0;\-#,##0` ほか、41〜44 は会計、59〜62・67〜81 は 1〜4・9〜22・45〜47 と同じ）。
- 分数 `# ?/?`・`# ??/??`・`?/?`・`# ?/4`（Excel と同じく連分数の近似分数）。
- 指数 `0.00E+00`（負の数の `--`）、工学表記 `##0.0E+0`（`46.1E+3`）。
- 負の数や 9999-12-31 より後の日付は `########`。1 未満の値の日付は `1900/1/0`。

セルの数値の表示は `numfmt::format_serial`（経過時間 → 日付 → 数値）を通る。

### 3.3 数式の文字列（`cell.formula`。`xlsx_calc::formula_shown`）

- テーブルの同じ行の参照を Excel の表示どおりに読む（`果物[[#This Row],[数量]]` → テーブルの中では `[@数量]`、外では `果物[@数量]`）。

### 3.4 テストのデータ

`testdata/xlsx/calc.xlsx` は Excel で開けなかった（`配列!I4` の `SUM(G1#)` がファイル上で `#` のまま）。また Excel 2010 以降の関数 12 個
（`MAXIFS`・`IFS`・`TEXTJOIN`・`XMATCH`・`LET` など）が `_xlfn.` なしで入っていて、Excel では `#NAME?` になっていた。
xljs と LibreOffice は接頭辞がなくても計算するので気づかなかった。`make_calc.py` の `to_file()` が Excel の保存する形に直す。

## 4. 残っている差

- 範囲に入れた配列数式（Ctrl+Shift+Enter）の左上以外のセルは `formula` が null（スピルの広がり先と同じ扱い）。Excel はどのセルでも同じ数式を返す。
- `'値'!A4` のように不要な引用符を付けて書いた数式はそのまま残る（Excel は `値!A4` にする）。害はない。
- `*` の繰り返し（会計の表示形式）は埋めない（列幅で決まるため）。
- `setColumnWidth("E", 30)` が Excel では 29.38 になる（ファイルの `width` は文字幅に余白を足した値。Excel の「列の幅 30」は `width="30.625"` 前後）。
- 改行を含む文字列を書いても「折り返して全体を表示」が付かず、Excel では 1 行に見える（Excel で Alt+Enter で入力すると付く）。
- 1900-01-01〜1900-02-28 の日付と、Excel にしかない 1900-02-29（シリアル値 60）の表示は確かめていない。
