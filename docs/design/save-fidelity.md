# 保存で落ちるもの（優先対応事項）

ブックを開いてそのまま `book.save` するだけで、元のファイルにあったものが落ちることがある。原因はどれも同じで、
umya-spreadsheet はモデルに持っていない要素・部品を書き戻さない。利用者から見ると「何もしていないのに壊れた」になるので、
ここにあるものは優先して直す。直したら項目を消し、`docs/design/pdf-design.md` 7 章（フォークで直したもの）に書く。

## 1. 調べ方

`tools/excel-verify/features.ps1`（`docs/design/excel-verify.md`）で、Excel 自身に機能を 1 つずつ入れたブックを作らせ、
xljs で開いて保存したものについて、次の 3 つを見る。

- Excel で開けるか（修復が要るファイルは開けないものとして数える）
- Excel から見て同じか（スパークライン・ピボットテーブル・スライサー・図形とフォームコントロール・ハイパーリンク・メモ・入力規則・条件付き書式・
  テーブル・保護・印刷の設定・アウトライン・名前・外部リンク・カスタム XML・表示の設定・ふりがなを、COM で読んで比べる）
- 部品と要素（`partdiff.py`。部品は名前の数字を除いて数え、要素は同じ種類の部品のどれにもなくなった要素名を出す）

調べている機能は `excel-make-features.ps1` の `$All`。

2026-11-16 の時点で、フォームコントロール以外はすべて PASS（Excel 2021）。
2026-11-15 に、手元のブック 14 個（`testdata/xlsx`・`testdata/book`・`examples/data`、umya の `aaa.xlsx`・`aaa.xlsm`）の部品と要素も比べた。

## 2. 落ちるもの

優先度は、起きやすさと、気づかずに業務のブックを壊す度合いで付けた。

| 優先 | 落ちるもの | 確かめたブック | 中身 |
|---|---|---|---|
| 中 | 入力規則のメッセージの文字化け | `rules.xlsx` | `error`・`errorTitle`・`prompt`・`promptTitle` が数値文字参照（`&#25968;`）で書かれていると `&amp;#25968;` と二重にエスケープされる。Excel は UTF-8 のまま書くので（Excel のブックでは起きないことを確かめた）、openpyxl などが作ったブックで起きる |
| 中 | 埋め込みの Excel ワークシート | `aaa.xlsm` | `xl/embeddings/Microsoft_Excel_Worksheet*.xlsx` がない |
| 低 | フォームコントロールの VML | `features.ps1` の `formcontrol` | `xl/drawings/vmlDrawing*.vml`・シートの `legacyDrawing`・描画の `a14:compatExt` など。Excel 2010 以降はシートの `controls` と `ctrlProps` と描画から読むので、Excel では元どおり動く（リンクするセル・リスト・値も同じ）。Excel 2007 以前や LibreOffice では見えない |
| 低 | 色のパレット | `aaa.xlsx` ほか | `styles.xml` の `colors`（`indexedColors`・`mruColors`）。パレットを変えたブックでは、番号で指定した色が変わりうる |

## 3. 落ちても害のないもの

比較には出るが、直さなくてよい（`partdiff.py` も数えない）。

- `xl/calcChain.xml`（Excel が作り直す）。
- 空の `definedNames`・`numFmts`・`dxfs`、既定値だけの `sheetPr`（`outlinePr`・`pageSetUpPr`）、空の `evenHeader`・`firstHeader` など。
- インライン文字列（`<is>`）が共有文字列になる。
- `xr:revisionPtr`・`xcalcf:calcFeatures`・`x15ac:absPath`。
- テーマの `thm15:themeFamily`（テーマの系統の ID）、図形の `a16:creationId`（共同編集で使う ID）。

## 4. 直し方の方針

- umya のフォークで直す。モデルに足せるもの（`outlineLevel`・`blackAndWhite`・`<b val="0"/>`・`ph`）は読み書きを足す。
- 中身を xljs が扱わないもの（外部リンク・スライサー・タイムライン・ピボットテーブル・`ctrlProps`・カスタム XML）は、読んだ部品を中身を見ずに持っておいて
  書き戻す（`kept_parts.rs` の `KeptRelationships`。それを参照する `workbook.xml` の `externalReferences`・`pivotCaches`・`extLst`、シートの `controls`・
  読まない `ext` も読んだ XML のまま）。リレーションの id は `rIdKept1`… と付け直し、XML の `r:id` を書き換える。読んだ部品のルートで宣言された名前空間は、
  書き戻す要素に宣言し直す（`xr2:uid` などが宣言なしになると Excel で開けない）。
- 書いたあとの整合（どの `r:id` もリレーションにあるか、使った接頭辞がどれも宣言されているか）は umya の `tests/excel_features.rs` が見る。
