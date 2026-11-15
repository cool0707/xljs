# 保存で落ちるもの（優先対応事項）

ブックを開いてそのまま `book.save` するだけで、元のファイルにあったものが落ちることがある。原因はどれも同じで、
umya-spreadsheet はモデルに持っていない要素・部品を書き戻さない。利用者から見ると「何もしていないのに壊れた」になるので、
ここにあるものは優先して直す。直したら項目を消し、`docs/design/pdf-design.md` 7 章（フォークで直したもの）に書く。

## 1. 調べ方

2026-11-15 に、手元のブック 14 個（`testdata/xlsx/*.xlsx`、`testdata/book/*.xlsx`、`examples/data/*.xlsx`、
umya のテスト用の `aaa.xlsx`・`aaa.xlsm`）を `workbooks.open(src).save(dst)` で保存し、前後の zip を比べた。

- 部品: 名前の数字を除いて数え、減ったものを出す。
- 要素: `xl/workbook.xml`・`xl/styles.xml`・シートの XML で、保存後に 1 つもなくなった要素名を出す。

見ているのは「部品と要素がなくなったか」だけ。属性の値だけが変わったものは、目に入ったもの（入力規則）しか拾えていない。
スライサー・スパークライン・フォームコントロール・外部リンク・カスタム XML などは手元のブックにないので調べていない。
直すときは、この比較を属性まで見る形にしてテストに入れる（保存の前後で落ちてよいものの一覧を持ち、それ以外が落ちたら失敗）。

## 2. 落ちるもの

優先度は、起きやすさと、気づかずに業務のブックを壊す度合いで付けた。

| 優先 | 落ちるもの | 確かめたブック | 中身 |
|---|---|---|---|
| 中 | 入力規則のメッセージの文字化け | `rules.xlsx` | `error`・`errorTitle`・`prompt`・`promptTitle` が数値文字参照（`&#25968;`）で書かれていると `&amp;#25968;` と二重にエスケープされる。Excel は UTF-8 のまま書くので、openpyxl などが作ったブックで起きる |
| 中 | 独自のテーブルスタイル | `tables.xlsx` | `styles.xml` の `tableStyles` が空になり、`defaultTableStyle`・`defaultPivotStyle` も書き換わる。印刷用には xljs が別に読む（`xlsx_table.rs`）ので PDF には出るが、保存したファイルには残らない |
| 中 | ピボットテーブルのリレーション | `aaa.xlsx` | `xl/pivotTables/_rels/*`・`xl/pivotCache/_rels/*` がない。保存したブックのピボットが Excel で開けるかは確かめていない |
| 中 | 埋め込みの Excel ワークシート | `aaa.xlsm` | `xl/embeddings/Microsoft_Excel_Worksheet*.xlsx` がない |
| 中 | 白黒印刷 | `tables.xlsx` | `<pageSetup blackAndWhite="1"/>` が要素ごと消える（ほかに属性のない `pageSetup`） |
| 低 | 差分書式の「太字を解除」 | `tables.xlsx` | `dxf` の `<b val="0"/>` が消える |
| 低 | 色のパレット | `aaa.xlsx` ほか | `styles.xml` の `colors`（`indexedColors`・`mruColors`）。パレットを変えたブックでは、番号で指定した色が変わりうる |
| 低 | タイムラインのスタイル | `aaa.xlsx` | `x15:timelineStyles` |

## 3. 落ちても害のないもの

比較には出るが、直さなくてよい。

- `xl/calcChain.xml`（Excel が作り直す）。
- 空の `definedNames`・`numFmts`・`dxfs`、既定値だけの `sheetPr`（`outlinePr`・`pageSetUpPr`）、空の `evenHeader`・`firstHeader` など。
- インライン文字列（`<is>`）が共有文字列になる。
- `xr:revisionPtr`・`xcalcf:calcFeatures`・`x15ac:absPath`。

## 4. 直し方の方針

- umya のフォーク（`xljs` ブランチ）で直す。モデルに足せるもの（`blackAndWhite`・`<b val="0"/>`）は読み書きを足す。
- 中身を xljs が扱わないもの（テーブルスタイル・埋め込み・色のパレット）は、セルに置いた画像のリッチ値（`rich_data.rs`）やグラフ（`kept_parts.rs`）と同じく、
  読んだ部品を中身を見ずに持っておいて書き戻すのが早い。
