# サンプル

ファイル名の先頭が、そのサンプルで主に見せている機能です。どれも `xljs <ファイル名>` で実行できます。
ファイルやフォルダを選ぶサンプルは、`data/` の中のサンプルデータですぐに試せます（下の表の「試すデータ」）。

| サンプル | 機能 | 内容 | 試すデータ |
|---|---|---|---|
| `dialog-aggregate.js` | ダイアログ・ピッカー | フォルダを選び、Excel ファイルを集計して保存する（`showDirectoryPicker`・`ui.withProgress`・`showSaveFilePicker`・`alert`） | `data/売上`（フォルダ） |
| `progress-cancel.js` | 進捗表示 | 進捗ダイアログとキャンセル（`ui.progress`） |  |
| `form-aggregate.js` | 入力フォーム（`ui.form`） | HTML なしのフォームで条件を入力し、開いたままのウィンドウで進捗・ログを表示しながら集計する（見出し付きのグループ・`onSubmit`・`remember`） | `data/売上`（フォルダ） |
| `form-tabs-table.js` | 入力フォーム（`ui.form`） | タブで項目を切り替え、表から対象を選び、結果を表に表示する（`tabs`・`type: "table"`・`ctx.table`） | `data/売上`（フォルダ） |
| `table-file-list.js` | 表表示（`ui.table`） | ファイル一覧を表で表示し、選んだファイルを処理する（`selectable`・`onDoubleClick`） | `data`（フォルダ） |
| `chart-sales.js` | グラフ（`ui.chart`） | Excel ブックのグラフをそのまま表示し、表から組み合わせグラフを作る（`sheet.charts`・`toSpec`・`onClick`） | `data/売上レポート.xlsx` |
| `chart-gantt.js` | ガントチャート | 工程表をフォームの中にガントチャートで表示し、ドラッグで変えた日付をブックに保存する（`type: "chart"`・`editable`・`holidays`・進捗線・出来高累計曲線） | `data/工程表.xlsx` |
| `app-hello/` | アプリ（HTML の画面） | 最小のアプリ。フォームの値を export した関数に渡し、結果を表示する（`@mode app`・`data-action`・`data-bind`） |  |
| `app-aggregate/` | アプリ（HTML の画面） | フォルダ選択 → 集計 → 進捗 → 結果の表 → 保存。ドロップ・中止・画面の関数呼び出しも使う（`win.state`・`win.invoke`・`xljs.invoke`） | `data/売上`（フォルダ） |
| `sqlite-shared-orders.js` | SQLite（共有モード） | ファイルサーバー上のデータベースに Excel の受注を取り込み、集計して表示する（`sqlite.open`・`db.insert`・`db.all`・自動バックアップ） | `data/売上/*.xlsx` |
| `csv-to-xlsx.js` | CSV → Excel | CSV（Shift_JIS / UTF-8 自動判定）を Excel ブックに変換する | `data/顧客一覧.csv`・`data/商品一覧.csv` |
| `xlsx-to-json.js` | Excel → JSON | Excel の先頭シートを JSON / JSON Lines に書き出す | `data/売上/*.xlsx` |
| `xlsx-to-pdf.js` | Excel → PDF | Excel ファイルを印刷したときと同じページ設定・グラフ付きで PDF にする（`book.toPdf`） | `data/売上レポート.xlsx`・`data/売上/*.xlsx` |
| `pdf-merge-stamp.js` | PDF の編集（`pdf`） | 複数の PDF を結合し、表紙・ページ番号・透かしを入れる（`pdf.merge`・`insertBlankPage`・`addText`） | `data/資料/*.pdf` |
| `xlsx-style-report.js` | Excel の書式 | フォント・罫線・塗りつぶし・表示形式・セル結合・ウィンドウ枠の固定などを使った帳票 |  |
| `ad-account-report.js` | Active Directory（`ad`） | 自分の所属するグループの中の、無効・ロックアウト中・パスワード期限切れのアカウントと使われていないコンピューターを Excel に書き出す（`ad.me`・`ad.search` の `memberOf` と `limit`・`ui.withProgress`。ドメイン全体は検索しない） |  |
| `pack-drop-xlsx-to-csv.js` | exe 化（`xljs pack`） | exe にドロップした Excel を CSV に変換する（`xljs.args`） | `data/売上/*.xlsx` |

## サンプルデータ（`data/`）

| ファイル | 中身 |
|---|---|
| `売上/2026-04.xlsx`〜`2026-06.xlsx` | 月ごとの売上明細。「明細」シートに 伝票番号・日付・支店・顧客名・商品名・数量・単価・金額 の表。フォルダの集計・変換・取り込みのサンプルで共通に使う |
| `売上レポート.xlsx` | 「月別」シート（月・売上・粗利と、棒＋折れ線の組み合わせグラフ）と「支店別」シート（円グラフ）。印刷の設定（A4 横・ヘッダー/フッター）付き |
| `工程表.xlsx` | 作業・開始・終了・進捗・前提・金額 の工程表（2026 年 10〜12 月） |
| `顧客一覧.csv` | Shift_JIS の CSV。顧客コードは `00001` のような先頭が 0 の文字列 |
| `商品一覧.csv` | UTF-8（BOM 付き）の CSV |
| `資料/*.pdf` | 結合用の 2 ページずつの PDF が 3 つ |

- 変換するサンプル（`xlsx-to-json.js`・`xlsx-to-pdf.js`・`pack-drop-xlsx-to-csv.js`）は、元のファイルと同じフォルダに結果を書き出します。
  集計のサンプルは「明細」シートのないブックを飛ばすので、`売上` フォルダに変換結果があっても集計には混ざりません。
- 元に戻したいときは、配布の zip（またはリポジトリ）の `examples/data` をもう一度コピーしてください。
  サンプルデータはリポジトリの `tools/examples/make_data.py` で作っています。
