# 保存で落ちるもの（優先対応事項）

ブックを開いてそのまま `book.save` するだけで、元のファイルにあったものが落ちることがある。原因はどれも同じで、
umya-spreadsheet はモデルに持っていない要素・部品を書き戻さない。利用者から見ると「何もしていないのに壊れた」になるので、
ここにあるものは優先して直す。

- 直したら、項目をここから消す。そして `docs/design/pdf-design.md` 7 章（フォークで直したもの）と、フォークの `FORK.md` 3 章に書く。
- xljs と RAS は、フォーク `cool0707/umya-spreadsheet` の同じブランチ `excel-compat` を使う。保存で落ちるものも同じになる。
- フォークの `FORK.md` に、両方から見た一覧がある。足したもの・まだ足りないもの・ブランチの決まり。

## 1. 調べ方

`tools/excel-verify/features.ps1`（`docs/design/excel-verify.md`）は、Excel 自身に機能を 1 つずつ入れたブックを作らせる。
それを xljs で開いて保存したものについて、次の 3 つを見る。

- Excel で開けるか（修復が要るファイルは開けないものとして数える）
- Excel から見て同じか: スパークライン・ピボットテーブル・スライサー・図形とフォームコントロール・ハイパーリンク・メモ・入力規則・
  条件付き書式・テーブル・保護・印刷の設定・アウトライン・名前・外部リンク・カスタム XML・表示の設定・ふりがなを、COM で読んで比べる
- 部品・要素・属性（`partdiff.py`）: 部品は名前の数字を除いて数える。要素と属性（`sheetView@showZeros`）は、同じ種類の部品の
  どれにもなくなったものを出す

調べている機能は `excel-make-features.ps1` の `$All`。
スレッド形式のコメントは、COM（`CommentsThreaded`・`AddCommentThreaded`）が使える Excel でないと作れず読めない。手元の Excel では
「VBA オブジェクト モデルは、使用している Excel のバージョンではサポートされていません」となって使えないので、`$All` にない。
代わりに `testdata/scripts/comments.test.js` が、Excel の書く形のブック（`testdata/xlsx/make_comments.py`）で見る。

記録:

- 2026-11-16 の時点で、`features.ps1` ではフォームコントロール以外はすべて PASS（Excel 2021）。
- 2026-11-22 に、手元のブック 107 冊を属性まで比べた（`partdiff.py`）。
  - 対象は xljs の `testdata`・`examples/data`、RAS のテスト用のブック、フォークの `tests/test_files`。
  - それまでの要素だけの比較では見えなかったものが、属性の比較で見つかった。例: テーブルの見出し行の数、シートの表示、ヘッダー・フッター、`quotePrefix`、セルのスタイル。
  - 見つかったものはフォークで直した（`pdf-design.md` 7 章）。
  - 同じ日に、ActiveX コントロールの部品が落ちて壊れた参照が残ること、SmartArt と VBA の署名が落ちることも見つけて直した。
- 2026-11-23 に、2 章の表にあったものをフォームコントロールの VML と `.xlsb` を除いて直した（`pdf-design.md` 7 章）。同じ 107 冊で、
  残ったのはフォームコントロールの VML と 3 章の害のないものだけ。
  - 表になかったもので、コードを読んで見つけたもの（シートの範囲の編集の許可・エラーチェックを無視するセル・並べ替え・シナリオなど、
    `fileVersion`・`fileSharing`（書き込みのパスワード）、ヘッダー・フッターの画像、Word や PowerPoint の文書の埋め込み）も直した。
  - 図形・画像・テーマ・メモの枠を読んだまま書くようにしたので、Excel の実機で `features.ps1` と `run.ps1` を流し直すこと（この日は Linux だけで確かめた）。

## 2. 落ちるもの

優先度は、起きやすさと、気づかずに業務のブックを壊す度合いで付けた。

| 優先 | 落ちるもの | 確かめたブック | 中身 |
|---|---|---|---|
| 低 | フォームコントロールの VML | `features.ps1` の `formcontrol` | `xl/drawings/vmlDrawing*.vml`・シートの `legacyDrawing`・描画の `a14:compatExt` など。Excel 2010 以降はシートの `controls` と `ctrlProps` と描画から読むので、Excel では元どおり動く（リンクするセル・リスト・値も同じ）。Excel 2007 以前や LibreOffice では見えない |
| — | `.xlsb` | — | 読めない（RAS の `crates/ras-ovba` が VBA の部分だけを読む） |

読んだまま持つもののうち、行・列の挿入と削除で動かないものがある（ユーザー設定のビューのフィルターと選択範囲、範囲の編集の許可・
エラーチェックを無視するセルの範囲）。図形・画像・メモの枠・グラフのアンカーは動く。

## 3. 落ちても害のないもの

比較には出るが、直さなくてよい（`partdiff.py` も数えない、または見て害がないと分かっているもの）。

- `xl/calcChain.xml`（Excel が作り直す）、サムネイル（`docProps/thumbnail.*`）、何もない描画。
- 空の `definedNames`・`numFmts`・`dxfs`、既定値だけの `sheetPr`（`outlinePr`・`pageSetUpPr`）、空の `evenHeader`・`firstHeader` など。
- 既定値と同じ属性: `<b val="1"/>` が `<b/>` になる、`quotePrefix="0"`、`headerRowCount="1"`、`workbookView` の `firstSheet`、
  LibreOffice が書く `hidden="false"` など。
- 接頭辞の違い（`x:sheet` が `sheet`、`wsDr` が `xdr:wsDr` になる）、部品の名前の番号（`theme.xml` が `theme1.xml` になる）。
- インライン文字列（`<is>`）が共有文字列になる。
- `xr:revisionPtr`・`xcalcf:calcFeatures`・`x15ac:absPath`、共同編集の ID（`xr:uid`、図形の `a16:creationId`）、コメントの `shapeId`、
  数式の `aca`。
- テーマの `thm15:themeFamily`（テーマの系統の ID）。テーマは読んだまま書くので、今は落ちない。
- 中身のないユーザー設定のプロパティ（`docProps/custom.xml`）、`date1904="false"`。
- Id も Target もないリレーション（Google スプレッドシートがピボットキャッシュに書く `externalLinkPath`）、どこからも参照されない部品。

## 4. 直し方の方針

- umya のフォークで直す。xljs や RAS の側で、保存したファイルを書き換えたり、ファイルを開き直して読んだりしない（以前は標準のフォント・
  ページ設定・テーブルスタイル・1904 年方式をそうしていたが、フォークの API に移した。`FORK.md` 2.2）。xljs と RAS が同じものを使えるように、
  読み方もフォークに置く。
- モデルに足せるもの（`outlineLevel`・`blackAndWhite`・`<b val="0"/>`・`ph`・テーブルの `headerRowCount`・シートの表示）は読み書きを足す。
- 値を読まないが残したい属性は、読んだまま持つ（`other_attributes`。`pageSetup`・`sheetView`・`table`・`workbookView`・`headerFooter`・
  描画の文字のラン）。名前空間の接頭辞のある属性は持たない（宣言がない名前空間を書くと Excel で開けない）。
- モデルに読むが、モデルが持たない部分のあるもの（グラフ・図形と画像のアンカー・テーマ・メモの枠）は、読んだものも持っておき、
  モデルが読んだときのままなら読んだものを書く（モデルの `Debug` のハッシュで比べる。行・列の挿入と削除は両方を動かす）。
- 中身を xljs が扱わないもの（外部リンク・スライサー・タイムライン・ピボットテーブル・`ctrlProps`・ActiveX・カスタム XML・SmartArt・VBA の署名、
  外部データの接続とクエリ、パッケージの部品＝秘密度ラベル・リボン、ヘッダー・フッターの画像）は、
  読んだ部品を中身を見ずに持っておいて書き戻す（`kept_parts.rs` の `KeptRelationships`、描画のアンカーは `KeptAnchor`）。それを参照する
  `workbook.xml` の `externalReferences`・`pivotCaches`・`extLst`、シートの `controls`・`phoneticPr`・読まない `ext` も、読んだ XML のまま持つ。
  - リレーションの id は `rIdKept1`… と付け直し、XML の `r:id` を書き換える。
  - 読んだ部品のルートで宣言された名前空間は、書き戻す要素に宣言し直す（`xr2:uid` などが宣言なしになると Excel で開けない）。
  - 参照する部品がファイルにないアンカーは、そのアンカーだけをモデルに読ませる。
- 書いたあとの整合は umya のテストが見る: どの `r:id` もリレーションにあるか（`tests/excel_features.rs`）、使った接頭辞がどれも宣言されているか。
