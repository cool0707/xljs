# 数式の計算（`book.calculate()`・`cell.calc()`）

Excel がない環境で、ブックの数式を計算して結果をセルに入れる仕組みのメモ。
実装は `src/api/xlsx_calc.rs`、計算エンジンは `vendor/gridcraft/`（GridCraft から取り込んだもの）。

## 1. 何ができるか

| API | 動き |
|---|---|
| `book.calculate()` | すべての数式を計算し直し、結果を各セルの保存値に入れる。`{ formulas, changed, skipped }` を返す |
| `cell.calc()` | そのセルの数式を、いまのセルの値で計算した結果を文字列で返す。セルには入れない |
| `book.toPdf()`・`sheet.toPdf()`・`toHtml()`・`printPages()` | 印刷の前に、書き換えの影響を受ける数式だけを計算してセルに入れる |
| `cell.formula = "=XLOOKUP(...)"` | Excel 2007 より新しい関数に、ファイルで必要な接頭辞 `_xlfn.` を付けて保存する（読むときは外す） |

`cell.value`・`toObjects()`・`save()` は計算しない（セルに保存されている値を読む・書く）。理由は 6 章。

## 2. エンジンの選定

候補は 2 つ。

- [storytold/gridcraft](https://github.com/storytold/gridcraft)（MIT OR Apache-2.0）: 表計算アプリ。数式の構文解析、関数 約 490 個＋特殊な 33 個（`IF`・`LET`・`LAMBDA`・`OFFSET`・`INDIRECT`・`SUBTOTAL`・`TEXT` など）、依存グラフによる再計算、スピル、循環参照の検出がクレートに分かれている。
- [clark-labs-inc/libreoffice-rs](https://github.com/clark-labs-inc/libreoffice-rs)（MIT）: 数式の評価は `lo_calc` の約 1,000 行で、関数は `SUM`・`AVERAGE`・`MIN`・`MAX`・`COUNT`・`IF`・`AND`・`OR`・`NOT`・`CONCAT`・`LEN`・`ABS`・`ROUND` の 13 個。xljs がそれまで持っていた簡易計算（約 45 関数）より少ない。

gridcraft を使う。依存（git）ではなく `vendor/gridcraft/` に写した。理由:

- 日本語の Excel に合わせるために中を変える必要がある（7 章）。
- 元のリポジトリが消えても・履歴が書き換わってもビルドできる。
- 計算に要らない部分（数式の組版＝RaTeX、xlsx の読み書き、画面）を持ち込まない。追加の外部クレートはない（`thiserror`・`log`・`sha2`・`chrono`・`serde`・`regex` は xljs がすでに使っている版）。

取り込んだもの・変えたところ・更新の手順は `vendor/gridcraft/README.md`。

## 3. 計算用の写し

ブックは umya-spreadsheet が持っているが、エンジンは自分のモデル（`gridcraft_model::Workbook`）の上で動く。
そこで、はじめて計算が要るときにブックをエンジンのモデルへ写す（`Engine::build`）。写すのは計算に関わるものだけ。

| 写すもの | 元 | 備考 |
|---|---|---|
| セルの値・数式 | `Worksheet::cells()` | 書式は写さない |
| 名前の定義 | ブックと各シートの `defined_names()` | `_xlnm.`（印刷範囲など）は除く。`localSheetId` があるものだけシートの名前 |
| テーブル | `Worksheet::tables()` | 名前・範囲・列名。構造化参照（`売上表[金額]`・`[@数量]`）用。見出し行はあるものとする（umya が `headerRowCount` を持たない） |
| 結合セル | `merge_cells()` | スピルを妨げるかの判定用 |
| 非表示の行 | `row_dimensions()` | `SUBTOTAL`・`AGGREGATE` 用。フィルターの範囲の中で隠れている行は「フィルターで隠れた行」とみなす（umya がフィルターの条件を持たない） |
| 1904 年の日付システム | `Book::date1904` | |

写しは `Book::calc` に持ち、ブックを閉じるか、追えない変更があるまで使い回す。

### 数式の読み方

ファイルの数式は、保存のされ方で意味が変わる。GridCraft の xlsx の読み込みと同じ扱いにしている（`make`）。

- **共有数式**（`t="shared"`）: umya が各セル用にずらした文字列（`CellFormula::text()`）を使う。
- **配列数式**（`t="array"`、`ref` あり）: `cm` 属性がなければ Ctrl+Shift+Enter の配列数式で、`ref` の範囲を埋める。`cm` があればスピルする数式。どちらも、範囲の先頭以外のセルにファイルが持っている値は、エンジンの「スピルした値」として入れる（セルとして入れるとスピルを妨げる）。
- **それ以外の数式**: 動的配列のメタデータ（`cm`）がない数式を、Excel は「暗黙的な交差」で 1 つの値にする。GridCraft の解析（`gridcraft-xlsx-formulas` の `apply_legacy`）で、交差が起きる場所に `@` を入れてから計算する。`=A1:A5` を 3 行目に書けば A3 の値になる。
- スクリプトが `cell.formula` で書いた数式も「それ以外の数式」。umya が `cm` と `metadata.xml` を書けないので、保存したファイルを Excel で開いても 1 つの値になり、それと同じ結果にしている。

### 計算しない数式

次の数式は、エンジンに渡さず、ファイルに保存されていた値を定数として写す（`unsupported`・`Skip`）。
`#NAME?` で上書きするより、Excel が計算した値を残すほうが害が少ないため。その数式を参照する数式は、残した値を使って計算する。

| `reason` | 条件 |
|---|---|
| `function` | エンジンにない関数（マクロ・アドインの関数など）。エンジンに形だけある関数（`CUBE*`・`WEBSERVICE`・`FILTERXML`）と、ふりがなを持てないので正しく返せない `PHONETIC` も |
| `name` | ブックにない名前（別のシートだけの名前も） |
| `external` | ほかのブックへの参照（シート名に `[` がある。`[1]Sheet1!A1` は構文エラーになるので `syntax`） |
| `circular` | 循環参照と、その結果を参照する数式。これだけは計算したあとで分かる（エンジンの結果が `#CIRC!`）ので、数式は写しに残し、結果をセルへ入れるときに飛ばす |
| `dataTable` | データテーブル（`t="dataTable"`） |
| `large` | 範囲の合計が 200 万セルを超える配列数式 |
| `syntax` | 構文解析できない |

`book.calculate()` の戻り値の `skipped` で、どのセルがなぜ計算されなかったかを返す。`cell.calc()` は保存値（なければ `#NAME?` など）を返し、`syntax` だけは例外にする。

## 4. 2 通りの計算

エンジンには「全部を計算し直す」（`Calc::recalc_all`）と「変わったセルに依存する数式だけを計算する」（`Calc::cells_changed`）がある。

| 呼び出し | 計算する数式 |
|---|---|
| `book.calculate()` | すべて |
| `cell.calc()`、印刷の前、条件付き書式の数式 | 書き換えたセルに依存する数式、保存値のない数式、`NOW`・`TODAY`・`RAND`・`OFFSET`・`INDIRECT` を使う数式 |

後者は Excel の自動計算と同じ考え方で、書き換えていないところは Excel が保存した値のままになる。
エンジンの結果が Excel と食い違う数式があっても、関係のない書き換えで値が変わることはない。

### 書き換えの追跡

差分の計算には「どのセルが変わったか」が要る。`Book` の変更の入口を 3 つに分けた。

| 入口 | 使うところ | 計算への影響 |
|---|---|---|
| `Book::cell_mut(sheet, col, row)` | 値・数式の書き込み（`set_value`・`xlsxSetFormula`） | セルを `Touched` に記録 |
| `Book::layout_mut(sheet)` | 書式・列幅・行の高さ・ウィンドウ枠の固定 | なし |
| `Book::sheet_mut(sheet)` | それ以外（結合・フィルターなど） | 写しを捨てて、次は全部を計算（`calc_reset`） |

シートの追加・削除・名前の変更も `calc_reset`。記録が 20 万セルを超えたら、追うのをやめて全部を計算する。
迷ったら `sheet_mut` を使えば、遅くはなるが結果は正しい。

## 5. 結果をセルへ（`store`）

- 数式のセル: `set_formula_result_*` で保存値だけを替える（数式は残る）。エラーは umya が書ける 8 種類に寄せる（`#SPILL!`・`#CALC!` は `#VALUE!`）。
- 配列数式が埋めるセル: 値を普通のセルとして書く。結果が小さくなって埋めなくなったセルは空にし、数式の `ref` を新しい範囲にする。
- `changed` は、保存値が変わったセルの数。

## 6. 自動で計算しないところ

`cell.value` を読むたび・`save()` のたびに自動で計算する案は採らなかった。

- 写しはブックと同じだけのメモリを使い、作るのにセルの数に比例した時間がかかる。数百万セルのデータに数式が少しあるだけのブックを開いて追記して保存する、というスクリプトが急に遅くなる・メモリの見張りで止まることがある。
- 保存したファイルは Excel で開けば計算される（umya が古い `calcId` を書くので、Excel は開いたときに計算し直す。数式を書いたブックには `fullCalcOnLoad` も付けている）。

印刷だけは自動で計算する。PDF は Excel が直す機会がなく、印刷するブックは大きくないため。
数式のないブックでは写しを作らない（`refresh`）。

## 7. 日本語の Excel に合わせたところ

GridCraft は英語の Excel を基準にしている。次のところを変えた・足した（`// xljs:` の印を付けている）。

| 項目 | 内容 |
|---|---|
| `NOW()`・`TODAY()` | GridCraft は UTC。xljs はパソコンのローカル時刻を渡す（`Calc::fixed_now`）。`xljs test` で固定した時刻（`TestRun::now`）にも従う（`set_fixed_now`） |
| `RAND()` | GridCraft は毎回同じ種から始まる。実行のたびに時計から種を取る（テストでは固定） |
| `LENB`・`LEFTB`・`RIGHTB`・`MIDB`・`REPLACEB`・`FINDB`・`SEARCHB` | 全角を 2 バイト、ASCII と半角カナを 1 バイトと数える。文字の途中で切れた半分は空白（Excel と同じ）。表示言語のロケールの `excel.dbcs`（`ja.json` は true）で切り替え、英語では文字数のまま |
| `ASC`・`JIS`・`DBCS` | 全角と半角の変換（英数字・記号・カタカナ。濁点の合成と分解） |
| `YEN`・`DATESTRING` | 日本語の Excel だけにある関数 |
| `TEXT` の表示形式 | `aaa`・`aaaa`（曜日）、`g`・`gg`・`ggg`（元号）、`e`・`ee`（和暦の年）、`[$-ja-JP-x-gannen]`（元年）。`e` は GridCraft では西暦 4 桁だった |

`PHONETIC` は計算しない（3 章）。`[DBNum1]` などの漢数字の表示形式と `NUMBERSTRING` は未対応（`NUMBERSTRING` は「ない関数」として保存値が残る）。

## 8. セルに置いた画像

セルに置いた画像（`xlsx_cellimage.rs`）は、数式から見ると `#VALUE!`。エンジンにもそのまま `#VALUE!` として渡す。
印刷では、画像のセルを参照するだけの数式（`=B2`・`=IF(...,B2)`・`XLOOKUP` など）に同じ画像を出す（`cell_image`）。
`=B2&""` や `=B2+1` のように画像を使って計算する数式は `#VALUE!` のまま。

この区別はエンジンの結果だけでは付かない（どちらも `#VALUE!`）。そこで、結果が `#VALUE!` の数式を、画像のセルを別の値に見せて 2 回計算する（`Pictures`）。
1 回目はどの数式も作らない文字列、2 回目はどの数式も作らない数値。文字列の演算は数値のほうを、数値の演算は文字列のほうを変えるので、両方がそのまま返ってきた数式だけが「渡しているだけ」と分かる。

## 9. 制限

- **スピルする数式の保存**: umya-spreadsheet は `cm` 属性と `xl/metadata.xml` を書かない。配列数式の `t="array"` も書かない。Excel が保存したスピルの数式・配列数式は、xljs で保存すると普通の数式と値のセルになる（計算の追加より前からの制限）。計算結果の値は入るが、Excel で開いたあとは先頭のセルだけが数式として動く。
- 反復計算（循環参照を繰り返し計算する設定）は読まない。循環参照の数式は計算しない（保存値を残す）。
- `CELL("format")`・`CELL("width")`・「表示桁数で計算する」は書式を写していないので既定の結果になる。
- 条件付き書式・印刷範囲の数式は、写しのセルの値で計算する（`eval_at`・`eval_area`）。

## 10. 検証

- `testdata/xlsx/calc.xlsx`（`make_calc.py`）: 139 個の数式とその期待値。LibreOffice に同じブックを計算させた結果と突き合わせて決めた（LibreOffice にない `XLOOKUP`・`FILTER`・`LET` などは、同じ意味の従来の数式と比べた）。配列数式・スピル・テーブル・共有数式・計算しない数式のシートもある。
- `src/api/xlsx_calc/tests.rs`: 上のブックを使う単体テスト。差分の計算、時計、接頭辞、印刷範囲。
- `testdata/scripts/calc.test.js`: スクリプトから見た動き（デバッガ経由でも流れる）。
- 取り込んだクレートのテストは `cargo test -p gridcraft-calc` などで走る（`cargo test --lib` には入らない）。

速さの目安（リリースビルド）: 5 万行・数式 10 万個（前の行を参照する累計を含む）のブックで、`calculate()` は最初が約 1.3 秒、2 回目から約 0.5 秒。
