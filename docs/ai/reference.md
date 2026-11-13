# xljs API リファレンス（AI チャット向け）

`prompt.md` の要点リファレンスより詳しい版です。AI に資料（ナレッジ）として添付するか、`prompt.md` のあとに貼ります。
ここに書いていない関数・オプションは存在しないものとして扱ってください。

## 実行環境

- ES2023 の JavaScript（`async`/`await`、トップレベル `await`、`?.`、`??`、クラス、分割代入、`Object.groupBy`、`Array.prototype.toSorted` など）。TypeScript（`.ts`）もそのまま実行できる。
- 使えないもの: `require`、npm パッケージ、`process`、`Buffer`、Node.js の `fs`/`path` モジュール、`XMLHttpRequest`・`WebSocket`、`document`/`window`（DOM）、Excel の COM/VBA。
- `import` は同じフォルダの自作ファイル（`import { f } from "./lib.js"`）と JSON（`import master from "./master.json"`）だけ。
- 使えるグローバル: `console`、`setTimeout`/`setInterval`、`sleep(ms)`、`exit(code)`、`structuredClone`、`TextEncoder`/`TextDecoder`（Shift_JIS 対応）、`atob`/`btoa`、`Blob`/`File`、`fetch`（HTTP 通信。「Web API（fetch）」の節）、`URL`/`URLSearchParams`、`AbortController`、`Intl`（簡易）、数値・日付の `toLocaleString()`（日本語形式）。
- ファイル系の API は同期（`await` 不要）。`await` が必要なのはダイアログ（`showOpenFilePicker` など）、`ui.form`、`ui.table`（選択するとき）、`sleep`、`fetch`、`h.getFile()` など Promise を返すものだけ（`ui.chart` はすぐ戻る。閉じるまで待つなら `.closed`）。
- パスを受け取る引数には、文字列のパス・ハンドル・`File` のどれでも渡せる。相対パスはカレントフォルダ基準。エクスプローラーから起動した（ダブルクリック・ドロップ）ときはスクリプトのあるフォルダ（exe ならその exe のフォルダ）がカレントフォルダになる。
- エラーが起きると、行番号付きのエラーダイアログを表示して終了する。

### 実行方法（利用者向けの説明に使う）

- `xljs.exe` にスクリプトファイルをドラッグ＆ドロップする。コマンドなら `xljsc 集計.js`（`xljsc.exe` はコンソール版）。
- `xljs pack 集計.js` で `集計.exe` ができる。その exe にファイルをドロップすると、パスが `xljs.args` に入る。
- 先頭にメタデータを書くとタイトルを変えられる:

```js
// ==xljs==
// @title 売上集計ツール
// ==/xljs==
```

## 全体の情報・終了

| API | 説明 |
|---|---|
| `xljs.args` | 引数の配列（exe にドロップしたファイルのパスなど） |
| `xljs.script` / `xljs.scriptDir` | スクリプトのパス / フォルダ |
| `xljs.env(name)` | 環境変数（無ければ `null`）。例 `xljs.env("USERPROFILE")` |
| `path.expandEnv(s)` | `%NAME%` を環境変数で置き換えた文字列。パスを受け取る API は自動で展開する（`fs.readText("%USERPROFILE%\\Desktop\\a.txt")`） |
| `xljs.title` | ダイアログのタイトル（代入で変更可） |
| `exit(code?)` | スクリプトを終了する |

## ダイアログ

| API | 戻り値 |
|---|---|
| `alert(msg)` | なし（同期） |
| `confirm(msg)` | `true` / `false`（同期） |
| `prompt(msg, 既定値)` | 文字列。キャンセルで `null`（同期） |
| `ui.error(msg)` | エラーのダイアログ |
| `ui.notify(msg, { title })` | 画面右下の通知。待たずに戻る |
| `await showOpenFilePicker({ multiple, types, startIn, title })` | ファイルハンドルの配列。キャンセルで `[]` |
| `await showSaveFilePicker({ suggestedName, types, startIn })` | ファイルハンドル。キャンセルで `null` |
| `await showDirectoryPicker({ startIn, title })` | フォルダハンドル。キャンセルで `null` |

- `types`: `[{ description: "Excel", accept: [".xlsx", ".xlsm"] }]`
- `startIn`: `"desktop"` `"documents"` `"downloads"`、パス、ハンドル（そのファイルのフォルダから開く）
- 他のアプリで作業中に `alert` が開くと、同じ内容の通知が自動で出る（`ui.autoNotify = false` で止める）。長い処理の終わりは `alert` で十分。
- `showSaveFilePicker` は保存先を決めるだけで、ファイルはまだ作られない。`types` を省略すると `suggestedName` の拡張子が付く（利用者が `集計` と入力しても `集計.xlsx`）。

## ファイル・フォルダ

### ハンドル

| プロパティ・メソッド | 説明 |
|---|---|
| `h.name` / `h.path` / `h.kind` | 名前 / フルパス / `"file"` か `"directory"` |
| `h.relativePath` | `walk()` の起点からの相対パス（`"2026/4月.xlsx"`） |
| `h.exists()` | 存在するか |
| `h.stat()` | `{ size, mtimeMs, isFile, isDirectory }`（無ければ `null`）。更新日時は `new Date(h.stat().mtimeMs)` |
| `h.readText(enc?)` / `h.writeText(text, enc?)` / `h.appendText(text, enc?)` | テキストの読み書き（書き込みは UTF-8、`.csv` / `.tsv` は BOM 付き） |
| `dir.walk(options)` | サブフォルダも含めて走査。`.toArray()` で配列、`for...of` でも回せる |
| `dir.list()` | 直下の項目のハンドルの配列 |
| `dir.file(name)` / `dir.dir(name)` | 直下のファイル / フォルダのハンドル（存在確認なし） |

`walk` のオプション: `{ glob: "*.xlsx" | ["*.csv", "*.txt"], exclude: ["old"], directories: false, hidden: false }`。
`/` を含まないパターンはどの階層のファイル名にも一致し、含むときは起点からの相対パスに一致する（`"2026/**/*.xlsx"`）。大文字小文字は区別しない。除外パターンに一致したフォルダは中身ごと除外。
`walk` と `list` は、隠しファイル・システムファイル、`.` で始まる名前、Excel の一時ファイル（`~$*.xlsx`）を最初から除く（含めるときは `{ hidden: true }`）。

### fs

| API | 説明 |
|---|---|
| `fs.file(p)` / `fs.dir(p)` | パスからハンドルを作る |
| `fs.exists(p)` / `fs.isFile(p)` / `fs.isDirectory(p)` / `fs.stat(p)` | 確認 |
| `fs.readText(p, enc?)` / `fs.writeText(p, text, enc?)` / `fs.appendText(p, text, enc?)` | テキスト |
| `fs.readBytes(p)` / `fs.writeBytes(p, data)` | バイナリ（`ArrayBuffer`） |
| `fs.readJSON(p, { dates })` / `fs.writeJSON(p, value, { space, encoding })` | JSON |
| `fs.readJSONL(p)` / `fs.writeJSONL(p, rows, { append })` | JSON Lines |
| `fs.list(p)` | 直下の名前（文字列）の配列（隠しファイルも含めるなら `fs.list(p, { hidden: true })`） |
| `fs.walk(p, options)` / `fs.glob(pattern, { cwd })` | 走査 |
| `fs.mkdir(p)` | フォルダ作成（途中のフォルダも作る） |
| `fs.copy(from, to)` / `fs.rename(from, to)` / `fs.move(from, to)` | コピー・名前変更・移動 |
| `fs.remove(p, { recursive })` | 削除（ごみ箱には入らない） |
| `fs.cwd()` / `fs.chdir(p)` | カレントフォルダ |

文字コード `enc`: `"auto"`（読み込みの既定。BOM → UTF-8 → Shift_JIS の順に判定）、`"utf8"`（書き込みの既定）、`"utf8bom"`、`"cp932"`（= `"shift_jis"`）、`"euc-jp"`、`"utf16le"`。

### path

`path.join(...)` `path.resolve(...)` `path.dirname(p)` `path.basename(p, 拡張子?)` `path.extname(p)` `path.relative(from, to)` `path.normalize(p)` `path.isAbsolute(p)` `path.sep`（Windows では `\`）。ハンドルも渡せる。

### shell

| API | 説明 |
|---|---|
| `shell.showItemInFolder(p)` | エクスプローラーでファイルを選択した状態で開く（配列で複数可） |
| `shell.openPath(p)` | 既定のアプリで開く（.xlsx なら Excel、フォルダならエクスプローラー） |
| `shell.openExternal(url)` | ブラウザで開く |
| `shell.writeShortcutLink(p, [op], { target, args, cwd, description, icon, iconIndex, windowState, hotkey, runAsAdmin })` | ショートカット（.lnk。URL なら .url）を作る・編集する。op は "create"（既定）/ "update"（指定した項目だけ変更）/ "replace"。書いたパスを返す |
| `shell.readShortcutLink(p)` | ショートカットを読む（`target` がリンク先。ほかは上と同じ項目） |
| `await shell.run(cmd, [args], { cwd, env, window, input, timeout, encoding, onStdout, onStderr, stdout, stderr })` | プログラムを実行して終わるのを待つ。`{ exitCode, ok, stdout, stderr, timedOut, pid }` を返す（0 以外でも例外にしない）。args（配列）があれば直接起動、なければ cmd.exe /c のコマンドライン。window は "default"（コンソールはウィンドウなし）/ "hidden" / "normal" / "minimized" / "maximized"。出力は既定で受け取り、CP932 も自動で読む |
| `shell.runSync(cmd, [args], opts)` | run の同期版 |
| `shell.spawn(cmd, [args], opts)` | 起動して待たない。`{ pid, running, wait(), kill(), write(s), end() }`（stdin: "pipe" で write） |

## Excel

対応形式は `.xlsx` / `.xlsm`。`.xls` は不可。マクロ・ピボットテーブル・グラフは保存すると失われることがある。

### ブック

| API | 説明 |
|---|---|
| `workbooks.open(p)` | 開く |
| `workbooks.create({ font? })` | 新しいブック。最初の `addSheet` が既定のシートを置き換える |
| `book.sheets[0]` / `book.sheets["名前"]` / `book.sheet(名前か番号)` | シート（無ければ `undefined` / `null`）。名前の大文字小文字・前後の空白・全角半角の違いは許す |
| `book.sheetNames` | シート名の配列 |
| `book.addSheet(name)` / `book.removeSheet(name)` | 追加・削除 |
| `book.save()` / `book.save(p)` | 上書き保存 / 別名保存 |
| `book.close()` | 閉じる（メモリ解放） |
| `await book.toPdf(p, options?)` / `await sheet.toPdf(p, options?)` | Excel で印刷したときのように PDF に書き出す（「PDF 出力」の節） |
| `workbooks.readVBA(p)` | マクロブック（.xlsm / .xlsb など）の VBA コード。`{ name, modules: [{ name, type, code, sheet }], text }`、マクロがなければ `null`。`type` は `"standard"` `"class"` `"document"`（ThisWorkbook とシート）`"form"`。`text` は全モジュールの Markdown |
| `workbooks.columnName(3)` → `"C"`、`workbooks.cellName(2, 3)` → `"C2"` | 番地の変換 |

### シートの読み取り

| API | 説明 |
|---|---|
| `sheet.toObjects({ header, columns, text, skipEmpty })` | 行をオブジェクトに。`header` は見出しの行番号（既定 1）。`header: 0, columns: ["A列名", ...]` で見出しなし |
| `sheet.toArrays({ text })` | 2 次元配列（0 始まり）。`text: true` で表示形式どおりの文字列 |
| `sheet.cell("B2")` / `sheet.cell(行, 列)` | セル（1 始まり） |
| `cell.value` | 値: number / string / boolean / Date / null |
| `cell.text` | 表示どおりの文字列 |
| `cell.formula` | `"=SUM(A1:A3)"`。無ければ `null` |
| `sheet.getValue("B2")` / `sheet.setValue("B2", v)` | セルの値の読み書き |
| `sheet.range("A1:C3").values` | 範囲の 2 次元配列（代入で書き込み） |
| `sheet.rowCount` / `sheet.dimension` | 最終行 / 使用範囲（`"A1:F120"`） |
| `sheet.name` | シート名（代入で変更） |

- 結合セルの値は左上のセルにだけ入っている。
- 数式セルの `value` は Excel が最後に保存した計算結果。スクリプトで書いた数式は Excel で開くまで計算されない（`value` は `null`。`cell.calc()` で一部の関数は計算できる）。

### シートへの書き込み

| API | 説明 |
|---|---|
| `sheet.write(rows, options)` | オブジェクトの配列（見出し＋データ）か 2 次元配列を書く |
| `sheet.append(rows, options)` | 最終行の下に追記（見出しは書かない） |
| `cell.value = v` / `cell.formula = "=..."` | 1 セル |

`write` のオプション:

| オプション | 説明 |
|---|---|
| `start` | 書き始めのセル（既定 `"A1"`） |
| `header` | 見出し行を書くか（既定 `true`） |
| `columns` | 書く列と順番（`["日付", "金額"]`） |
| `table` | 表の体裁（見出しの太字・塗り・罫線、データの罫線、列幅自動、先頭行固定、フィルター） |
| `headerStyle` / `style` / `columnStyles` | 見出し / 全体 / 列ごとの書式（`{ 金額: { format: "#,##0" } }`） |
| `autoFit` / `freeze` / `filter` | 列幅自動 / 見出しの固定 / フィルター（`table` では既定で有効） |

### 書式

```js
sheet.style("A1:D1", { bold: true, color: "#FFFFFF", background: "#305496", align: "center" });
sheet.cell("D10").style({ font: { size: 14, bold: true }, format: "¥#,##0", border: { top: "double" } });
sheet.range("A3:D10").border({ outline: "medium", inside: { style: "thin", color: "#BFBFBF" } });
sheet.merge("A1:D1");             // セル結合（unmerge で解除）
sheet.freeze("A2");               // 1 行目を固定（"B2" で 1 行目と A 列）
sheet.autoFilter("A3:D10");       // フィルター
sheet.autoFit();                  // 列幅を内容に合わせる
sheet.setColumnWidth("A:C", 16);  // 列幅（文字数）
sheet.setRowHeight(1, 30);        // 行の高さ（ポイント）
```

| 項目 | 指定 |
|---|---|
| 文字 | `bold` `italic` `underline` `strike` `color` `fontSize` `fontName`、または `font: { name, size, bold, italic, color }` |
| 塗り | `background: "#DDEBF7"`（`null` で塗りなし） |
| 配置 | `align`（`left` `center` `right`）、`valign`（`top` `center` `bottom`）、`wrap: true`（折り返し） |
| 表示形式 | `format`: `"#,##0"` `"#,##0.00"` `"0.0%"` `"¥#,##0"` `"yyyy/m/d"` `"yyyy年m月d日"` `"h:mm"` `"@"`（文字列） |
| 罫線 | `border: "thin"`（全セルの上下左右）、または `{ outline, inside, horizontal, vertical, top, bottom, left, right }`。線種 `"thin"` `"medium"` `"thick"` `"double"` `"dashed"` `"dotted"`、色付きは `{ style: "thin", color: "#999999" }` |

色は `"#RRGGBB"` か `red` などの色名。書式は既存の書式に重ねて適用される（`{ replace: true }` で置き換え）。

## PDF 出力

Excel がなくても、印刷したときと同じページで PDF にできる（Windows の WebView2 を使う。コンソール・mock では使えない）。

```js
const book = workbooks.open("請求書.xlsx");
await book.toPdf("請求書.pdf");                                  // 非表示でないすべてのシート
await book.toPdf("一覧.pdf", { sheets: ["一覧"], fitToWidth: 1 }); // 横 1 ページに収める
await book.sheets["明細"].toPdf("明細.pdf", { paper: "A4", landscape: true, margin: 10 });
await xljs.htmlToPdf("<h1>報告書</h1><p>本文</p>", "報告書.pdf", { paper: "A4" }); // HTML から
```

- ブックのページ設定に従う: 用紙・向き・余白・拡大縮小と「次のページ数に合わせて印刷」・印刷範囲・印刷タイトル・改ページ・ページの順序・ヘッダー/フッター（`&P` `&N` `&D` `&A` `&F` など）・左右中央。
- `options` で上書きできる: `paper`（`"A3"` `"A4"` `"B5"` `"Letter"` など）`landscape` `scale`（%）`zoom`（%。シートの倍率や「次のページ数に合わせて」にさらに掛ける。はみ出す分だけ縮めたいとき）`fitToWidth` `fitToHeight` `fitToPage` `margin`（mm）`gridLines` `blackAndWhite`（`false` でシートの白黒印刷を無視） `centerHorizontally` `charts: false` `sheets` `skipBlankPages`（`true` で何も印刷されないページ、`"values"` で値のないページ（罫線・塗りだけのページも）を飛ばす） `condense`（`true` か 0〜20 の %。セルと図形の文字の間隔を詰める。Excel の印刷は画面より文字が広く、折り返しが増えたり右端が切れたりするのを画面の見た目に近づける）。
- `book.printPages(options)` / `sheet.printPages(options)`: PDF を作らずに、ページの分かれ方を返す（シートごとに `{ sheet, pages, across, down, scale, areas: [{ range, across, down, pages }] }`。`across` が横、`down` が縦のページ数、`scale` は %）。はみ出しの確認と `zoom` の調整に使う（例: `if (sheet.printPages().across > 1) await sheet.toPdf(p, { zoom: 90 })`）。
- 値・表示形式・フォント・塗り・罫線・結合・折り返し・回転・画像・図形とテキストボックス（Excel の既定の形すべてとフリーフォーム。影・3-D は描かない）・線と矢印・グラフ（`sheet.charts` で描けるもの）を描く。条件付き書式・SmartArt・Excel 2016 以降のグラフは描かない。自動の改ページの位置は Excel と少し違うことがある。
- `toHtml(options?)` は同じページを HTML の文字列で返す（グラフは空の枠）。

## PDF の編集（pdf）

PDF の結合・ページの削除・挿入・回転・文字の追加ができる（Excel・Acrobat・WebView2 は不要。mock・コンソールでも動く）。ページ番号は 1 始まり、位置は mm、文字の大きさは pt。

```js
const doc = pdf.open("報告書.pdf");                    // パスワード付きは pdf.open(p, { password: "..." })
doc.append("別紙.pdf");                                 // 最後に結合（{ pages: "1-3" } で一部だけ）
doc.insert(2, "差し込み.pdf", { pages: [1] });          // 2 ページ目の前に挿入
doc.insertBlankPage(1, { paper: "A4" });                // 白紙のページ（表紙）
doc.removePages("5-6,-1");                              // 削除（-1 は最後のページ）
doc.rotatePages(3, 90);                                 // 時計回りに回転
doc.addText("all", (p) => `${p.number} / ${p.count}`, { position: "bottom-center", size: 9 }); // ページ番号
doc.addText("all", "資料番号 A-001", { position: "top-right", size: 9 }); // 右上に資料番号
doc.addText(1, "提出用", { x: 20, y: 15, size: 14, color: "red" });     // 位置を mm で
doc.save("報告書_提出用.pdf");                          // 省略で上書き
doc.close();
const p = pdf.merge(["a.pdf", "b.pdf", "c.pdf"], "まとめ.pdf"); // 結合して保存（保存先を省くと文書を返す）
```

- ページの指定: 番号・番号の配列・`"1-3,5"`・`"3-"`（3 ページ目から最後まで）・`"all"`。負の番号は後ろから。
- `doc.pageCount`、`doc.pages`（`[{ number, width, height, rotation }]`。表示したときの大きさ（mm）と回転）。
- `insert` / `append` の取り込み元は PDF ファイル（パス・ハンドル・`File`・`Blob`・`ArrayBuffer`）か別の `PdfDocument`。自分自身を渡すとページの複製になる。しおり（アウトライン）とフォームの定義は取り込まない。
- `addText` の置き場所は `position` が簡単: `"top-left"` `"top-center"` `"top-right"` `"center"` `"bottom-left"` `"bottom-center"` `"bottom-right"`（`"右下"` `"下中央"` なども可）。ページの端から `margin`（mm、既定 10。`{ x, y }` で別々に）だけ内側に置き、複数行は寄せた側にそろう。
- ほかのオプション: `x` `y`（左上からの位置。`y` は文字の上端）、`right` `bottom`（右端・下端からの位置。指定した向きは `position` より優先）、`size`（既定 11）、`color`（`"#rrggbb"` か色名）、`opacity`（0〜1）、`align`（行の揃え）、`lineHeight`（既定 1.3）、`rotate`（度、反時計回り）、`font`（`"游ゴシック"` `"メイリオ"` `"ＭＳ 明朝"` などの名前かフォントファイルのパス。配列なら文字ごとに前から探す）。`"\n"` で改行。回転したページでも見た目の向きで置く。
- 文字はフォントの使った字だけを埋め込むので、検索・コピーできる。既定のフォントは游ゴシック（Windows）。
- 保存した PDF は暗号化しない（パスワードは外れる）。電子署名は無効になる。

## CSV

| API | 説明 |
|---|---|
| `csv.read(p, options)` | 読む。既定は見出しをキーにしたオブジェクトの配列 |
| `csv.write(p, rows, options)` | 書く。既定は BOM 付き UTF-8・CRLF（Excel で文字化けしない） |
| `csv.parse(text, options)` / `csv.stringify(rows, options)` | 文字列との変換 |

- 値は既定で変換する: 数値（`"1,234"` も）、日付（`2026/4/1`、`2026-04-01 09:30` → `Date`）、`TRUE`/`FALSE`、空欄は `null`。先頭が 0 のコード（`"00123"`）、16 桁以上の数字、`"123-4567"` などは文字列のまま。見出しの前後の空白は取り除く。
- 読むオプション: `header: false`（2 次元配列）、`cast: false`（変換せずすべて文字列）、`encoding`（既定 `"auto"`）、`delimiter`（`.tsv` はタブが既定）、`columns`、`trim`、`skipEmptyLines`。
- 書くオプション: `encoding: "cp932"`（Shift_JIS。古いシステム向け）、`columns`、`header: false`、`delimiter: "\t"`、`append: true`、`quoteAll: true`。
- `Date` は `"2026/04/01"` の形式で書き出す。

## JSON

```js
const data = fs.readJSON(h);                       // BOM・Shift_JIS も自動判定
const withDates = fs.readJSON(h, { dates: true }); // ISO 形式の日時文字列を Date に
fs.writeJSON(dest, rows);                          // 2 スペースでインデント、UTF-8
fs.writeJSON(dest, rows, { space: 0 });            // 1 行で
fs.writeJSONL(dest, rows);                         // 1 行 1 レコード
```

設定ファイルを既定値と合わせて読むときは、既定値を複製してから使う（既定値の配列をそのまま返して `push` すると、既定値も増える）:

```js
const defaults = { 担当: [{ 区: "北", 名前: "" }], 行数: 20 };
function loadConfig(path) {
  let saved = {};
  try { if (fs.exists(path)) saved = fs.readJSON(path) ?? {}; }
  catch (e) { alert(`設定を読めなかったため、初期値を使います。\n${e.message}`); }
  const c = { ...structuredClone(defaults), ...saved };
  const blank = Math.max(0, c.行数 - c.担当.length);  // 足す数を先に決める
  c.担当 = [...c.担当, ...Array.from({ length: blank }, () => ({ 区: "", 名前: "" }))];
  return c;
}
```

## SQLite データベース

```js
const db = sqlite.open("受注.db");          // なければ作る
db.exec("CREATE TABLE IF NOT EXISTS 受注 (番号 TEXT PRIMARY KEY, 日付 DATE, 得意先 TEXT, 金額 INTEGER)");
db.run("INSERT INTO 受注 VALUES (?, ?, ?, ?)", "A001", new Date(2026, 3, 1), "山田商店", 12000);
db.run("UPDATE 受注 SET 金額 = :金額 WHERE 番号 = :番号", { 番号: "A001", 金額: 15000 });
const rows = db.all("SELECT * FROM 受注 WHERE 日付 >= ?", new Date(2026, 3, 1)); // オブジェクトの配列
const one = db.get("SELECT sum(金額) AS 合計 FROM 受注");                         // 最初の行か null
db.insert("受注", workbooks.open(h).sheet(0).toObjects(), { replace: true });   // まとめて挿入（1 トランザクション）
db.transaction(() => {                     // 例外なら取り消し。async にはできない
  db.run("DELETE FROM 受注 WHERE 金額 = 0");
});
db.close();
```

- 値: `Date` はローカル時刻の文字列（0 時ちょうどなら `"2026-04-01"`、それ以外は `"2026-04-01 09:30:00"`）で保存し、型を `DATE`・`DATETIME` と宣言した列は `Date` で読める。`BOOLEAN` の列は `true`/`false`。BLOB は `Uint8Array`。`CURRENT_TIMESTAMP` は UTC なので、日時の既定値は `datetime('now', 'localtime')` を使う。
- `db.insert(table, rows)` はテーブルがなければ値から列の型を決めて作る（オブジェクトの生成列のキーは無視する）。`values(sql)` は配列の配列、`tables`・`columns(table)`・`check()`・`backup(dest)` もある。
- **Excel 互換の関数**が SQL で使える。生成列・ビュー・インデックス・トリガーにも使え、ほかの人のスクリプトや修復でも同じ結果になる。引数が NULL や数・日付として読めない値なら NULL。日付は `"2026-04-01"` などの文字列（Excel のシリアル値も可）で、日付を返す関数は `"2026-04-01"` の文字列を返す（`DATE` 型の列なら `Date` で読める）。
  - 数: `ROUNDHALF(x, 桁)`（Excel の ROUND。SQLite の `round` は 2.675 → 2.67 になり負の桁も使えないので、金額の丸めには使わない）・`ROUNDUP`・`ROUNDDOWN`・`INT`・`QUOTIENT`
  - 文字列: `LEFT`・`RIGHT`・`MID`・`LEN`・`FIND`（見つからなければ NULL）・`TEXT(値, 表示形式)`
  - 日付: `YEAR`・`MONTH`・`DAY`・`WEEKDAY(日付, 種類)`・`EDATE(日付, 月数)`・`EOMONTH(日付, 月数)`・`DATEDIF(開始, 終了, "Y"|"M"|"D"|"YM"|"YD"|"MD")`
  - Excel の TRUNC は `ROUNDDOWN`、IF は `iif`、CONCAT は `||`、MOD は `%`（負の数のときの符号が違う）。SUMIFS・VLOOKUP にあたる計算は `GROUP BY`・`JOIN`（またはビュー）で書く。
- **JavaScript の関数を SQL から呼ぶ**: `db.function(name, fn)`。そのスクリプトの文からだけ呼べるので、テーブル（生成列・CHECK・DEFAULT）・ビュー・トリガー・インデックスの定義には使えず、共有モードでデータベースを変更する文（`UPDATE ... SET x = f(y)` など）にも使えない（どちらもエラー）。引数は number・string（日付も文字列）・`Uint8Array`・null。fn は async 不可、fn の中でデータベースは使えない。デバッガ（`--debug`）では使えない。

```js
db.exec(`CREATE TABLE IF NOT EXISTS 明細 (品名 TEXT, 単価 REAL, 数量 INTEGER, 受注日 DATE,
  金額 INTEGER AS (ROUNDDOWN(単価 * 数量 * 1.1)) STORED,      -- Excel の数式の列は生成列に
  支払期限 DATE AS (EOMONTH(受注日, 1)) VIRTUAL)`);
db.function("ランク", (金額) => (金額 >= 100000 ? "A" : "B"));   // 定義には使えない。クエリで使う
const list = db.all("SELECT 品名, 金額, ランク(金額) AS ランク FROM 明細 WHERE MONTH(受注日) = ?", 4);
```
- **共有モード**（ファイルサーバーに置いて複数人で使う）: `sqlite.open(p, { shared: true })`。ネットワークドライブ・`\\server\share` 上なら自動で共有モードになる。呼び出しのたびにファイルを開いてロックを取り、すぐ閉じる（遅いが壊れにくい）。他の人の書き込み中は最大 `timeout`（既定 30 秒）待つ。書き込みの前に 1 時間ごとに検査と自動バックアップをし、変更をすべて記録する。トランザクションの中で `confirm` など人を待つ処理をしない。
- 壊れたときは、次の呼び出しで「修復しますか？」と聞かれ、はいなら修復してやり直す（`onCorrupt: "repair"` で確認なし、`"error"` で例外）。修復は 索引の作り直し → 自動バックアップ＋その後の変更の再実行 → 読める行だけで作り直し の順に試し、壊れたファイルは残す。`sqlite.repair(p)` / `db.repair()` で明示的にも実行でき、データベースのファイルを xljs.exe にドロップしても修復できる。

## Web API（fetch）

ブラウザと同じ `fetch` で社内・社外の Web API を呼べる（Windows の WinHTTP を使う。プロキシは Windows の設定（PAC も）のまま、HTTPS の証明書は Windows の証明書ストアで確かめる）。

```js
// GET して JSON を受け取る
const res = await fetch("https://api.example.com/items?" + new URLSearchParams({ 分類: "部品", 件数: 100 }));
if (!res.ok) throw new Error(`取得に失敗しました: ${res.status} ${res.statusText}`);  // 404・500 では reject しない
const items = await res.json();

// JSON を POST する
const r2 = await fetch("https://api.example.com/orders", {
  method: "POST",
  headers: { "Content-Type": "application/json", Authorization: "Bearer " + xljs.env("API_TOKEN") },
  body: JSON.stringify({ 番号: "A001", 金額: 12000 }),
  timeout: 30000,                          // xljs 拡張: 30 秒で応答がなければ TimeoutError
});

// フォームとファイルを送る（multipart/form-data）
const form = new FormData();
form.append("title", "月次報告");
form.append("file", await fs.file("報告.xlsx").getFile());  // ディスクのファイル
await fetch("https://intra.example.com/upload", { method: "POST", body: form });

// ダウンロードして保存する
const file = await fetch("https://intra.example.com/files/master.xlsx");
const data = await file.arrayBuffer();
fs.writeBytes("master.xlsx", data);
const book = workbooks.open(data);         // 保存せずに開くこともできる
```

- `res.text()` は Content-Type の charset（`Shift_JIS` なども）で読む。`res.text("cp932")` で文字コードを指定できる（xljs 拡張）。本文は 1 回しか読めない（2 回読むなら先に `res.clone()`）。
- オプション: `method`、`headers`（オブジェクト・`Headers`）、`body`（文字列は UTF-8、`URLSearchParams` はフォーム、`FormData`、`Blob`、`ArrayBuffer`）、`signal`（`AbortController` で中止、`AbortSignal.timeout(ms)`）、`redirect`（`"follow"` 既定・`"manual"`・`"error"`）、`timeout`（ミリ秒、xljs 拡張）。
- 社内サーバーの Windows 認証（Negotiate / NTLM）はサインイン中のユーザーで自動で通る（イントラネットのサーバーだけ。ほかのサーバーにも使うなら `credentials: "include"`）。Cookie はその実行の間だけ保たれる。
- 認証付きのプロキシも通る（Windows 認証はそのまま、Basic 認証は初回にユーザー名とパスワードを尋ねて Windows の資格情報マネージャーに保存する）。プロキシの ID・パスワードをスクリプトに書かない。
- つながらない・サーバーが見つからない・証明書の問題は `TypeError`、中止は `AbortError`、時間切れは `TimeoutError` で reject する。ヘッダーの値に日本語は使えない（必要なら `encodeURIComponent`）。
- `URL` / `URLSearchParams` は URL の組み立てに使える（`new URL("/api/a", base)`、`url.searchParams.set("q", "日本語")`）。

## Active Directory（ad。ドメイン参加の Windows のみ）

サインイン中のユーザーの権限で読むだけ。結果のキーは LDAP の属性名（`sAMAccountName` `displayName` `mail` `department` `title` `manager` `memberOf` `lastLogonTimestamp` `whenCreated` など）。日時は `Date`（無期限・なしは `null`）、`memberOf` は識別名の配列。`type`（"user" / "computer" / "group" ...）、`enabled` `lockedOut` `passwordExpired` も付く。

| API | 説明 |
|---|---|
| `ad.available()` | ドメインに参加しているか |
| `xljs.computerName` / `xljs.userName` | コンピューター名 / ログオン中のユーザー名（AD なしでも使える） |
| `ad.me()` / `ad.computer()` | 自分 / このコンピューター |
| `ad.get(id)` | 1 件（ログオン名・識別名・UPN・メール・SID・名前。なければ `null`） |
| `ad.search("山田*")` | 名前・ログオン名・表示名・メールで検索（既定はユーザー） |
| `ad.search({ type, filter, disabled, lockedOut, expired, expiringDays, inactiveDays, passwordExpired, passwordNeverExpires, memberOf, recursive, searchBase, properties, limit })` | 条件はすべて AND。`type`: "user"（既定）/ "computer" / "group" / "all"。`filter`: `{ department: "営業部", title: ["課長", "部長"], mail: true }` か LDAP のフィルタ文字列。`inactiveDays: 90` は 90 日ログオンなし。`memberOf` はグループ名か識別名 |

## 進捗表示

```js
// まとめて書く（キャンセルされるとスクリプトがその場で終わる）
for (const h of ui.withProgress(files, { title: "処理中" })) { /* ... */ }

// キャンセル後も続けたいとき（ループが終わるだけ）
for (const h of ui.withProgress(files, { title: "処理中", onCancel: "break" })) { /* ... */ }
if (ui.cancelled) alert("途中で中止しました");

// 細かく制御する
const p = ui.progress({ title: "処理中", total: files.length });
for (const [i, f] of files.entries()) {
  if (p.cancelled) break;
  p.update(i, f.name);    // 完了件数と表示する文字
}
p.close();
```

## 画面の設計（フォームアプリ・カスタムアプリ）

画面のあるスクリプトは、書く前にここで形を決める。細かい API は「入力フォーム（ui.form）」「表の表示（ui.table）」「アプリモード」。

### 形を選ぶ

| 形 | 使うとき |
|---|---|
| フォームアプリ（`ui.form` を 1 つ） | 既定。「入力 → 実行 → 結果」「一覧を見て、直して、登録」「条件を変えて一覧を見る」は、どれも 1 つのフォームで作れる |
| フォームのまとまり（`ui.forms`） | 登録・設定・照会など、切り替えて使う画面がいくつかあるとき。1 つのウィンドウのサイドメニューでフォームを切り替える |
| カスタムアプリ（`@mode app` と HTML） | 利用者が頼んだとき。または `ui.form` / `ui.forms` にないものが要るとき: 行を追加・削除しながら入力する明細、一覧と詳細の切り替え、開いたままにして使う業務画面、入力に応じて計算結果がその場で変わる表 |
| ステップごとのスクリプト | 利用者が頼んだとき（バッチ・使い捨て）。画面は標準のダイアログだけ |

迷ったらフォームアプリ。フォームアプリで無理に作って使いにくくなるなら、理由を書いてカスタムアプリを提案する。

### 原則

1. **ウィンドウは 1 つ。** 起動から終了まで同じ画面で作業が終わるようにする。フォームを順に出さない（設定のフォーム → 本体のフォーム、など）。表のウィンドウ（`ui.table`）を続けて出さない。
2. **入力させるのは、プログラムが調べられないことだけ。** 利用者の氏名・所属・メール・役職は Active Directory から取る（下）。今日の日付・今月は初期値にする。ファイルを見れば分かること（シート名・対象の件数）は読んで決める。前回と同じになりやすい値は `remember: true`。
3. **入力は入力欄で受ける。** 値に合った `type` を選ぶ: 決まった選択肢は `select` / `radio` / `segmented`、多い候補から選ぶなら `combobox`、日付は `date` / `month` / `dateRange`、はい・いいえは `checkbox` / `switch`。選べるものを文字で打たせない。
4. **表は「見る・選ぶ・すでにある行を直す」ためのもの。** 新しいデータの入力欄の代わりにしない（フォームの表は行の追加・削除ができない）。1 件を登録するなら入力欄を並べ、何件も続けて入れるならカスタムアプリにする。
5. **結果は同じウィンドウに出す。** 経過と件数は `ctx.log`、一覧は同じフォームの `table` 項目に `ctx.table(name, rows)`、できたファイルは `return`（「フォルダを開く」ボタンが出る）。`ui.table` の別ウィンドウは、利用者が頼んだときと、1 行をダブルクリックして開く明細のような補助の表示だけ。
6. **処理のあとも画面を最新にする。** 登録・更新のあとは `ctx.table` で一覧を読み直す。「閉じてもう一度実行してください」と案内しない（フォームは処理のあとも入力に戻り、続けて実行できる）。
7. **`onSubmit` / `onClick` の中で `alert` を出さない。** 知らせることは `ctx.log`、失敗は `throw new Error("…")`（フォームに赤字で出る）、取り消せない操作の確認はボタンの `confirm`。
8. **機能が複数あるなら** 1 つのフォームの `tabs` と `buttons`（数が多ければ `menu`）にまとめる。めったに使わない管理用の機能は、条件付きで足す（`buttons: [...(admin ? [管理用のボタン] : [])]`）か、別のスクリプトにする。ボタンから別のフォームを開かない。

### 利用者の情報は Active Directory から

「記入者」「担当者」「申請者」「所属」「メール」のように実行している本人の情報は、入力欄にせず `ad.me()` で取る（ドメインに参加していないパソコンでも動くように備える）。ほかの人を選ばせるときは `ad.search` の結果を `combobox` の選択肢にする。

```js
const me = ad.available() ? ad.me() : null;
const user = {
  name: me?.displayName ?? xljs.userName,   // 表示名（AD がなければログオン名）
  mail: me?.mail ?? "",
  department: me?.department ?? "",
  title: me?.title ?? "",
};
// フォームでは確認のために見せるだけ（入力させない）:
// { name: "user", type: "note", label: "記入者", default: `${user.department} ${user.name}` }
// 同じ部署の人から選ばせる:
// { name: "to", label: "承認者", type: "combobox", options: ad.search({ filter: { department: user.department } }).map((u) => u.displayName) }
```

利用者ごとに出すものを変えるとき（自分の担当分だけ、管理者だけの機能）も、名前や役割を入力・選択させず、この `user` と手元のデータ（社員名簿など）を突き合わせて決める。見つからなければ、何と突き合わせて見つからなかったかをエラーで知らせる。

### フォームアプリの型

**入力 → 実行 → 結果**: ファイル・条件・保存先を 1 画面で受け、`onSubmit` で処理する。結果の一覧も見せるなら、空の `table` 項目を置いて `ctx.table` で入れる。

**一覧を見て、直して、登録する**: 起動時に一覧を読んでフォームの表に出し、登録のあと同じ表を読み直す。

```js
// 自分が担当する未出荷の受注を出し、出荷したものにチェックを付けて登録する
const me = ad.available() ? ad.me() : null;
const name = me?.displayName ?? xljs.userName;
const db = sqlite.open(path.join(xljs.scriptDir, "受注.db")); // みんなで使うデータの場所は毎回選ばせない
const load = () => db.all("SELECT 受注番号, 顧客名, 納期 FROM 受注 WHERE 担当者 = ? AND 出荷日 IS NULL ORDER BY 納期", name)
  .map((r) => ({ 出荷: false, ...r, 配送: "" }));

await ui.form({
  title: "出荷の登録",
  description: `${name} さんが担当する未出荷の受注です。出荷したものにチェックを付けて「登録」を押してください。`,
  width: 900,
  fields: [
    { name: "date", label: "出荷日", type: "date", default: new Date(), required: true }, // 今日が初期値。行ごとの列にしない
    {
      name: "items", label: "未出荷", type: "table", rows: load(), editable: true,
      columns: [
        { key: "出荷", type: "checkbox" },
        { key: "受注番号", editable: false },
        { key: "顧客名", editable: false },
        { key: "納期", editable: false },
        { key: "配送", type: "select", options: ["宅配便", "自社便"] } // 先頭の空欄（未選択）は自動で付く。"" を書かない,
      ],
      validateRow: (r) => (r.出荷 && !r.配送 ? { 配送: "配送の方法を選んでください" } : ""),
    },
  ],
  validate: (v) => (v.items.some((r) => r.出荷) ? null : { items: "出荷したものにチェックを付けてください" }),
  submit: "登録",
  onSubmit(v, ctx) {
    const done = v.items.filter((r) => r.出荷);
    db.transaction(() => {
      for (const r of done) db.run("UPDATE 受注 SET 出荷日 = ?, 配送 = ? WHERE 受注番号 = ?", v.date, r.配送, r.受注番号);
    });
    ctx.log(`${done.length} 件を登録しました`);
    ctx.table("items", load()); // 同じ画面のまま、残りの一覧に入れ替える
  },
});
db.close();
```

**条件を変えて一覧を見る**: 条件の項目の `onChange` で `ctx.table` を呼ぶ（実行ボタンを押させない）。保存などの操作は `buttons`、送信ボタンが要らなければ `submit: false`。

```js
const all = workbooks.open(path.join(xljs.scriptDir, "商品台帳.xlsx")).sheets[0].toObjects();
const pick = (v) => all.filter((r) => (v.kind === "すべて" || r.区分 === v.kind) && (!v.word || String(r.品名 ?? "").includes(v.word)));
await ui.form({
  title: "商品の検索",
  fields: [
    { name: "kind", label: "区分", type: "segmented", options: ["すべて", "製品", "部品", "消耗品"], default: "すべて",
      onChange(_, v, ctx) { ctx.table("list", pick(v)); } },
    { name: "word", label: "品名に含む文字",
      onChange(_, v, ctx) { ctx.table("list", pick(v)); } },
    { name: "list", label: "一覧", type: "table", rows: all, height: 15 },
  ],
  submit: false,
  buttons: [{
    label: "Excel に保存", icon: "file-excel", variant: "primary",
    async onClick(v, ctx) {
      const dest = await showSaveFilePicker({ suggestedName: "検索結果.xlsx" });
      if (!dest) return;
      const out = workbooks.create();
      out.addSheet("検索結果").write(pick(v), { table: true });
      out.save(dest);
      return dest;
    },
  }],
});
```

### 続けて使うデータ・設定

- 毎回同じになるものを毎回選ばせない。入力の前回値は `remember: true`。みんなで使うデータ（SQLite・マスターのブック）は、スクリプトと同じフォルダ（`xljs.scriptDir`）に置く決まりにするか、初回だけ選ばせて `fs.writeJSON(path.join(xljs.scriptDir, "設定.json"), …)` に覚える。データベースを選ぶだけのフォームを毎回最初に出さない。
- 複数人が書き込むデータは Excel のブックではなく SQLite（共有フォルダでは自動で共有モード）にする。Excel は取り込み元と出力先に使う。
- テーブルは起動時に `CREATE TABLE IF NOT EXISTS` で完成形を 1 回だけ書く。作っている途中のやり直しで付けた `ALTER TABLE` などの移行のコードは、本番のデータがまだないなら残さない。
- 画面を開く前に必要なもの（データベース・設定・利用者）がそろわないなら、そこでエラーにして何が足りないかを伝える（`ui.error` か `throw`）。足りないまま空の画面を出さない。

### アンチパターン

| してはいけない | 代わりに |
|---|---|
| 氏名・所属・役職・メールの入力欄 | `ad.me()`（上）。見せるだけなら `note` |
| フォームを閉じてから次のフォームや表を出す、の繰り返し | 1 つのフォーム（`tabs`・グループ・`hidden` / `disabled` で出し分け） |
| 結果を `ui.table` で次々に出す（件数の表 → 明細の表 → `alert`） | 件数は `ctx.log`、明細は同じフォームの `table` 項目に `ctx.table` |
| 新しいデータを入力させるために編集できる表を出す | 入力欄を並べる。何行も入力するならカスタムアプリ |
| 選択肢が決まっているのに `text` | `select` / `radio` / `segmented` / `combobox` |
| 全行に同じ値（今日の日付など）を入れる列を表に作る | 表の外に 1 つの項目（`default: new Date()`）を置き、保存のときに全行へ使う |
| 表のセルを変えたら同じ行のほかのセルを書き換えようとする（その仕組みはない） | 保存のときに計算する。その場で見せる必要があるならカスタムアプリ |
| 「閉じて再実行してください」という案内 | `ctx.table` で読み直す |
| 処理の中の `alert`（特にループの中） | `ctx.log` / `ctx.error`、最後に件数をまとめて 1 行 |
| 起動のたびに同じファイル・データベースを選ばせる | `remember: true`、`xljs.scriptDir`、設定ファイル |
| 説明のためだけの `note` を並べる | `description` に 1〜2 行、項目ごとの補足は `help` |
| 進捗の出ない長い処理 | `ctx.withProgress(list)`（中止もできる） |

### カスタムアプリの方針

- 画面（HTML）は見た目と操作、スクリプト（`.js`）はファイル・Excel・データベースの処理、と分ける。画面の JavaScript から xljs の API（`workbooks` など）は呼べないので、スクリプトの `export` した関数を `data-action` か `xljs.invoke` で呼ぶ。
- 1 つのウィンドウの中で画面を切り替える（`data-show` で出し分け）。`app.open` で別のウィンドウを増やすのは、同時に見比べる必要があるときだけ。
- 起動時に見せるデータ（利用者の名前、一覧）はスクリプトの先頭で `win.state` に入れる。利用者にボタンを押させてから読むのではなく、開いたときに出ているようにする。
- ファイルの選択は `<input type="file">` ではなく、スクリプトの関数で `showOpenFilePicker` などを呼ぶ（パスが取れる）。ドロップは `e.dataTransfer.files` をそのまま `xljs.invoke` に渡す。
- 入力の検査は HTML の `required` `min` `max` `pattern`（`data-action` は送信の前に検査する）と、スクリプトの関数の中の両方で行う。エラーは `throw new Error("…")`。
- 長い処理は進捗を `win.state` に入れ、ループの中で `await sleep(0)` して中止のボタンを受け取れるようにする。処理中はボタンを `data-show` / `disabled` で押せなくする（`data-action` のボタンは処理が終わるまで自動で無効になる）。
- 見た目は Windows の業務アプリとして落ち着いたものにする: フォントは `"Yu Gothic UI", "Meiryo UI", sans-serif`、文字は 14px 前後、余白と枠線で区切り、色はアクセント 1 色。`color-scheme: light dark` を付け、色は CSS 変数にして `@media (prefers-color-scheme: dark)` で切り替える。外部の CSS・フォント・CDN は使わない（社内のパソコンで読めない）。アイコンは同梱の Font Awesome。
- 表は `<table>` を自分で描く（何百行もあるなら絞り込みか、明細は `ui.table` に任せる）。グラフは `xljs.chart`。
- `alert` / `confirm` / `ui.form` / `ui.table` / ピッカーはスクリプトの関数から呼べる。確認や簡単な入力はこれで足りる。
- ファイルは `アプリ名.js` と `ui/index.html`（必要なら `ui/style.css`）。回答ではすべてのファイルを省略せずに出す。

## 入力フォーム（ui.form）

入力は原則このフォームで受け取る（バッチ・他のアプリから自動で呼ばれるときは画面を出さず `xljs.args` や設定ファイルから、使い捨ての処理はダイアログで）。`required` `min` `max` `pattern` と `validate` で入力チェックを必ず入れ、ファイルは中身（シート・列）まで `validate` で確かめる。

```js
const v = await ui.form({
  title: "請求書作成",
  description: "一覧の各行から請求書を作ります",
  fields: [
    { name: "list", label: "請求一覧", type: "file", accept: [".xlsx"], required: true },
    { name: "outDir", label: "出力フォルダ", type: "folder", required: true },
    { name: "date", label: "請求日", type: "date", default: new Date() },
    { name: "prefix", label: "ファイル名の先頭", type: "text", default: "請求書_", pattern: "[^\\\\/:*?\"<>|]*", patternMessage: "使えない文字があります" },
    { name: "rate", label: "税率(%)", type: "number", default: 10, min: 0, max: 100 },
  ],
  submit: "作成",
  remember: true,
  validate(v) {                                        // 追加の検証。問題があれば { 項目名: "メッセージ" } を返す
    if (v.rate !== 8 && v.rate !== 10) return { rate: "8 か 10 を入力してください" };
  },
});
if (!v) exit();
```

| type | 値 |
|---|---|
| `text` `password` `textarea` | 文字列 |
| `number` | 数値か `null`（`min` `max` `step`） |
| `date` | `Date` か `null` |
| `month` | `"2026-04"` か `null` |
| `time` | `"09:30"` か `null` |
| `select` `radio` | 選んだ文字列（`options: ["A", "B"]` または `[{ value, label }]`） |
| `checkbox` | `true` / `false` |
| `file` `saveFile` | ファイルハンドルか `null`（`accept: [".xlsx"]`） |
| `files` | ファイルハンドルの配列 |
| `folder` | フォルダハンドルか `null` |
| `table` | 表から選んだ行（`rows` `columns` `selectable: true | "single"`。色分けは `formats`（下の ui.table）） |
| `switch` | `true` / `false`（オン・オフのスイッチ） |
| `checkboxes` | 選んだ文字列の配列（`options` の複数選択。`required` は 1 つ以上） |
| `segmented` | 選んだ文字列（`options` を横に並べたボタン。`multiple: true` なら配列） |
| `combobox` | 選んだ選択肢の `value`（入力で `options` を絞り込む。`allowCustom: true` なら入力した文字列も可） |
| `slider` | 数値（`min` `max` 既定 0〜100、`step`） |
| `dateRange` | `{ from: Date, to: Date }` か `null`（`default: { from, to }`。開始 > 終了はエラー） |
| `tags` | 文字列の配列（Enter かカンマで区切って入力。`pattern` `maxLength` は 1 つずつ） |
| `note` | 値なし。メッセージの表示（`label` が見出し、`default` が本文、`variant: "info" | "success" | "warning" | "error"`） |
| `image` | 値なし。画像の表示（`default` に画像ファイルのパス・バイト列・URL。`width` `height` `align: "left" | "center" | "right"`。`label` なしならフォームの幅いっぱい） |

- 項目の共通オプション: `label` `default` `required` `placeholder`（`select` では未選択の表示になり、初期値が `""`）`help`（説明文）`tooltip`（マウスを重ねたときの説明。ボタンにも付けられる）、`prefix` / `suffix`（入力欄の中の単位。`{ type: "number", prefix: "¥", suffix: "円" }`）、`maxLength`（文字数の上限。「12 / 100」と表示）、`validate(value, values)`（項目ごとの検証。メッセージを返す。入力欄を離れたときにも検証される）。フォームの `validate(values)` は送信時だけ。
- ウィンドウの大きさは `width` `height`（既定は項目数から決める）。`resizable: false` で大きさを固定する（枠のドラッグ・最大化ができない）。`titleBar: "custom"` で OS のタイトルバーの代わりに自前のタイトルバー（`icon` とタイトル、最小化・最大化・閉じる）を表示する（`ui.table` / `app.open` も同じ。メインウィンドウは `@titlebar custom`）。
- `onChange(value, values, ctx)`: ユーザーが値を変え、その項目の検証が通ったときに呼ばれる。`ctx.setValue(name, value)` でほかの項目を埋められる（setValue で変えた項目の onChange は呼ばれない）。

```js
// 1 つ目のファイルを選ぶと、同じフォルダの対になるファイルを自動で設定する
await ui.form({
  title: "突合",
  fields: [
    { name: "sales", label: "売上", type: "file", accept: [".csv"], required: true, tooltip: "売上_YYYYMM.csv",
      onChange(h, v, ctx) {
        const m = h && /^売上_(\d{6})\.csv$/.exec(h.name);
        if (!m) return;
        const dir = path.dirname(h.path);
        for (const [name, file] of [["purchase", `仕入_${m[1]}.csv`], ["stock", `在庫_${m[1]}.csv`]]) {
          if (!v[name] && fs.exists(path.join(dir, file))) ctx.setValue(name, path.join(dir, file));
        }
      } },
    { name: "purchase", label: "仕入", type: "file", accept: [".csv"], required: true },
    { name: "stock", label: "在庫", type: "file", accept: [".csv"], required: true },
  ],
});
```
- 項目の無効化・非表示: `disabled: true` でグレー表示（入力できない）、`hidden: true` で非表示。関数にするとほかの項目の値から決まり、値が変わるたびに見直される（`disabled: (v) => !v.useMail`）。onChange・onSubmit・ボタンの `ctx.setDisabled(name, bool)` / `ctx.setEnabled(name)` / `ctx.setHidden(name, bool)` / `ctx.setVisible(name)` でも切り替えられる（項目の `disabled` / `hidden` を置き換える）。無効・非表示の項目は `required` などの検証をしない（値はそのまま `values` に入る）。コンソール・mock では尋ねない。

```js
// チェックを外している間は宛先をグレーにし、「その他」を選んだときだけ詳細を出す
await ui.form({
  title: "送付",
  fields: [
    { name: "useMail", label: "メールで送る", type: "checkbox", default: true },
    { name: "to", label: "宛先", required: true, disabled: (v) => !v.useMail },
    { name: "kind", label: "種類", type: "select", options: ["請求書", "見積書", "その他"] },
    { name: "detail", label: "詳細", required: true, hidden: (v) => v.kind !== "その他" },
    { name: "lock", label: "宛先を固定", type: "switch",
      onChange(on, v, ctx) { ctx.setDisabled("kind", on); } },
  ],
});
```
- `tabs: [{ label: "基本", fields: [...] }, { label: "詳細", fields: [...] }]` でタブ分け。
- 横に並べる: `{ row: "氏名", fields: [{ name: "last", label: "姓" }, { name: "first", label: "名" }] }` を項目と並べて書く（`row` が左のラベル、各項目の `label` は入力欄の前。`""` で出さない）。値はほかの項目と同じオブジェクト。
- 右上のツールバー: `toolbar: [{ name: "detail", type: "switch", label: "詳細表示" }, { icon: "gear", tooltip: "設定", onClick(values, ctx) { ... } }]`。`name` のあるものは項目（値は `values`）、ないものはボタン（`label` なしならアイコンだけ。検証せずに `onClick` を呼ぶ。`ctx.setValue` で項目を変えられる。`menu: [{ label, onClick }]` も可）。
- 複数のフォームを 1 つのウィンドウに: `await ui.forms({ forms: [{ label: "登録", icon: "pen", fields, onSubmit }, { label: "設定", icon: "gear", fields }], side: "left", size: "fixed" })`。サイドメニュー（ハンバーガーで畳める）で切り替える。`side: "right"` で右、`size: "each"` で切り替えるたびにそのフォームの `width` / `height` にする（`"fixed"` は `ui.forms` の `width` / `height`）。戻り値はフォームごとの最後に送信された値（なければ `null`）の配列。
- `fields` に `{ group: "出力", icon: "file-export", help: "説明", fields: [...] }` を項目と並べて書くと見出し付きのグループになる（タブの中でも可。値は項目と同じく 1 つのオブジェクト）。`collapsed: true` で閉じた状態の「詳細設定」にできる（見出しのクリックで開閉。エラーが出ると開く）。`collapsible: true` は開いた状態で開閉可能。
- アイコン（Font Awesome Free の名前。`"folder-open"` など）: 項目・タブ・ボタンの `icon`、送信ボタンは `submitIcon`。フォームの `icon` は `titleBar: "custom"` のときだけタイトルバーに出る（それ以外は無視）。
- アクセントカラー: `accent: "#c42b1c"`（CSS の色。ライト・ダーク共通）か `accent: { light, dark }`。送信ボタン・タブ・フォーカス枠などの色が変わる。
- 角丸: `radius: "large"`（`"none"` `"small"` `"medium"`（既定）`"large"` `"full"` かピクセル数）。ボタン・入力欄・タブ・表の角の丸み。
- 背景色: `background: "#fff8e1"`（CSS の色。ライト・ダーク共通）か `background: { light, dark }`。暗い色なら文字・入力欄もダークの配色になる。
- `table` に `editable: true` を付けるとセルをその場で編集でき、入力のたびに検証してエラーのセルを赤く表示する（エラーが残っていると送信できない）。検証を通った値は元の行オブジェクトに書き込まれる。

```js
const v = await ui.form({
  title: "エラー行の修正",
  fields: [{
    name: "rows", label: "エラーのある行", type: "table", rows: badRows, editable: true,
    columns: [
      { key: "行", editable: false },
      { key: "コード", pattern: "[A-Z]\\d{2}", patternMessage: "英大文字＋数字2桁" },
      { key: "金額", type: "number", min: 0, required: true },
      { key: "区分", type: "select", options: ["売上", "返品"] },
      { key: "日付", type: "date", validate: (d) => (dayjs(d).isAfter(dayjs(), "day") ? "未来の日付です" : "") },
    ],
    validateRow: (r) => (r.区分 === "返品" && r.金額 > 10000 ? { 金額: "返品は 1 万円まで" } : ""),
  }],
  submit: "修正を反映",
});
if (!v) exit();
// v.rows は（編集済みの）全行。badRows の各オブジェクトも書き換わっている
```
- `onSubmit(v, ctx)` を書くと、送信後もフォームが開いたまま処理し、進捗・ログ・中止ボタンを表示する。出力ファイルを `return` すると「フォルダを開く」ボタンが出る。

```js
await ui.form({
  title: "集計",
  fields: [{ name: "src", label: "フォルダ", type: "folder", required: true }],
  async onSubmit(v, ctx) {
    const files = v.src.walk("*.xlsx").toArray();
    for (const h of ctx.withProgress(files)) {  // 中止ボタンが押されると onSubmit はここで終わる
      ctx.log(h.relativePath);      // ログ欄に表示（ctx.error で赤字）
    }
    // ... 保存
    // return dest;
  },
});
```

- `ctx.table(name, rows, columns?)` で `table` 項目の行（と列）を入れ替えられる（`onSubmit`・ボタンの `onClick`・項目の `onChange` のどれからでも）。結果の一覧を同じウィンドウに出す、登録のあとに一覧を読み直す、条件を変えたら絞り込む、に使う。最初は `rows: []` で空の表を置いておける。
- 処理が終わるとフォームは入力できる状態に戻り、値を変えて続けて実行できる（閉じるのは利用者）。
- フォームの表でできないこと: 行の追加・削除、セルを変えたときにほかのセルを変えること（セルの `onChange` はない。検査は `validate` / `validateRow`）。これらが要るならアプリモード。

- `buttons: [{ label, onClick(v, ctx) }]` で送信ボタンの左にボタンを追加できる。押すと onClick が onSubmit と同じように動く（ウィンドウは開いたまま。押したボタンは終わるまでスピナーを表示）。`validate: false` で検証なし、`close: true` で正常終了後に閉じる。`submit: false` で送信ボタンを出さない。`ctx.setValue(name, value)` で項目の値を変えられる。
- ボタンの `variant`: `"primary"`（塗りつぶし）`"destructive"`（赤。削除など）`"ghost"`（枠なし）。`confirm: "削除しますか？"` で実行前に確認する（検証のあと）。
- `menu: [{ label, onClick, ... }]` でスプリットボタン（本体と ▼ のメニュー）。`onClick` なしで `menu` だけなら、メニューを開くだけのボタン。

```js
await ui.form({
  title: "名簿",
  fields: [{ name: "file", label: "名簿", type: "file", accept: [".xlsx"], required: true }, { name: "count", label: "件数", type: "number" }],
  buttons: [
    { label: "件数を確認", onClick(v, ctx) { ctx.setValue("count", workbooks.open(v.file).sheets[0].toObjects().length); } },
    { label: "その他", menu: [
      { label: "名簿を削除", variant: "destructive", confirm: "名簿を削除しますか？", onClick(v) { fs.remove(v.file); } },
    ] },
  ],
  submit: "作成",
  async onSubmit(v, ctx) { /* ... */ },
});
```

## 表の表示（ui.table）

```js
ui.table(rows, { title: "結果" });                         // 表示するだけ（すぐ次へ進む）
await ui.table(rows, { title: "結果" }).closed;            // 閉じるまで待つ
const picked = await ui.table(rows, { title: "選択", selectable: "multiple" }); // 選んだ行。キャンセルは []
const [one] = await ui.table(rows, { selectable: "single" });
ui.table(rows, {
  columns: ["名前", { key: "size", label: "サイズ", width: 80 }],
  onDoubleClick(e) { shell.openPath(e.row.handle); },      // e: { row, index, column, value, close() }
});
```

表は並べ替え・絞り込み（全体と列ごと）・列の固定・列の表示/非表示・幅の変更・コピー・CSV / Excel への保存が画面上でできる（フォームの `table` 項目も同じ）。スクリプトで作らない。

色分けは `formats`（`ui.table` とフォームの `table` 項目で共通）。先の規則が優先。

```js
ui.table(rows, { formats: [
  { column: "金額", gte: 100000, color: "#9c0006", background: "#ffc7ce" }, // 条件: equals notEquals gt gte lt lte between contains startsWith endsWith empty
  { column: "状態", equals: "未完了", row: true, bold: true },              // row: true で行全体
  { column: "金額", dataBar: true },                                        // データバー（数値の列。太さは { color, height: 0.3 }）
  { column: "達成率", colorScale: ["#f8696b", "#ffeb84", "#63be7b"] },      // カラースケール（2 色か 3 色）
  { when: (row) => row.期限 < new Date(), color: "red" },                    // 関数（column なしは行ごと、行全体に付く）
] });
```

- `ui.table` は表示と選択だけで、セルは編集できない。編集させるときは `ui.form` の `type: "table"` に `editable: true`（上の「入力フォーム」）。
- `onDoubleClick` は `ui.table` だけにある（フォームの表にはない）。`e.column` はダブルクリックした列の見出し（行番号の欄なら `null`）、`e.key` は列のキー。`selectable: "single"` と一緒に指定すると、ダブルクリックでは OK にならない。

## グラフ（ui.chart）

```js
const rows = sheet.toObjects();                              // [{ 月: "4月", 売上: 1200, 原価: 800 }, ...]
ui.chart({ type: "column", data: rows, x: "月", y: ["売上", "原価"], title: "月別売上" }); // 表示するだけ（すぐ次へ進む）
const view = ui.chart({
  type: "column", data: rows, x: "月",
  y: ["売上", { key: "利益率", type: "line", axis: "y2", format: "0%" }], // 組み合わせ・第 2 軸
  axes: { y: { title: "金額", format: "#,##0\"円\"" } },
  onClick(row, info) { alert(`${row.月} ${info.series}`); },
});
view.update({ data: rows.slice(0, 3) });                     // 指定したキーだけ変える
await view.closed;                                           // 閉じるまで待つ

// Excel のブックにあるグラフをそのまま表示する（値はセルから読む）
for (const c of workbooks.open("売上.xlsx").sheets[0].charts) {
  if (c.supported) ui.chart(c);                              // c.toSpec() で定義（ChartSpec）になる
  else console.log(c.name, c.warnings[0]);                   // レーダーなど表示できないもの
}

// ガントチャート（使い方は同じ。終了日はその日を含む）
ui.chart({
  type: "gantt", data: tasks, name: "作業", start: "開始", end: "終了", progress: "進捗", dependencies: "前提",
  view: "week", editable: true, holidays: [new Date(2026, 4, 3), { date: "2026-05-04", name: "みどりの日" }],
  onChange(row, changes) { /* row には書き込み済み。changes: { start, end } か { progress } */ },
});

// フォームの中に置く（ctx.chart で変える。editable なガントは値が全行）
await ui.form({
  fields: [{ name: "g", type: "chart", height: 12, chart: { type: "line", data: [], x: "月", y: "合計" } }],
  async onSubmit(v, ctx) { ctx.chart("g", { data: aggregate() }); },
});
```

- 種類（`type`）: `column`（縦棒、既定）`bar`（横棒）`line` `area` `scatter` `bubble`（系列に `size`）`histogram`（`bins`）`pie` `donut` `gantt`。`stacked: true` / `"percent"` で積み上げ。
- `data` は行オブジェクトの配列（`ui.table` と同じ）。`x` を省くと先頭の列、`y` を省くと数値の列すべて。日付の列は時間の軸になる（折れ線・面）。
- 書式は Excel の書式文字列（`format`、`axes.x/y/y2.format`、系列の `format`）。色は `colors`（既定は Excel と同じ Office の色）。
- ガントには進捗線（イナズマ線、`progressLine: true`）と出来高累計曲線（S カーブ、`sCurve: { weight: "金額", actual: [{ date, value }] }`）を重ねられる。基準日は `baseDate`（既定は今日）。
- ガントの列は省くと `名前`/`作業`・`開始`/`開始日`・`終了`/`終了日`・`進捗`・`前提` の列を探す。進捗は 0〜100（すべて 0〜1 なら割合）。前提は名前（`id` 列があればその値）を「,」区切りで。土日は既定で色が付く（`weekends: false` で消す）。
- 関数は `onClick` / `onChange` だけ使える（ほかのオプションは JSON で表せる値のみ）。ライブラリ固有の指定は `options`（AG Charts / Frappe Gantt のオプションに重ねる）。
- `sheet.charts` で読めないもの: レーダー・株価・等高線、Excel 2016 以降のウォーターフォール・ツリーマップ・サンバースト・ヒストグラム・箱ひげ図・じょうご・マップ（`supported: false`）。3-D は平面に、近似曲線・誤差範囲・データテーブル・補助円は省く（`warnings` に出る）。開始日の系列を塗りなしにした積み上げ横棒はガントチャートにする（`toSpec({ gantt: false })` で横棒のまま）。
- 画面（アプリモード）では `await xljs.chart("#el", spec)` で同じ定義を描ける（ハンドラーは画面側で動く）。

## 日付（dayjs）

[Day.js](https://day.js.org/) を組み込んである（`import` 不要。グローバルの `dayjs`）。PC の表示言語のロケール（日本語環境なら日本語。`dayjs.locale("ja")` で固定できる）と次のプラグインを読み込み済み: customParseFormat, isBetween, isSameOrAfter, isSameOrBefore, minMax, quarterOfYear, weekOfYear, isoWeek, dayOfYear, duration, relativeTime, advancedFormat, isLeapYear, isToday, isTomorrow, isYesterday, weekday, localeData。

`Date` を直接計算しない（`new Date("2026-04-01")` が UTC になる、月が 0 始まり、`setMonth` で月末がずれる、など）。Excel・CSV から読んだ `Date` も `dayjs(値)` で包んで扱う。

```js
const d = dayjs(r.日付);                         // Date・"2026/4/1"・"2026-04-01 09:30" など（文字列はローカル時刻）
dayjs("20260401", "YYYYMMDD");                   // 形式を指定して読む
dayjs("2026/02/30", "YYYY/MM/DD", true).isValid(); // false（厳密に読む）
d.format("YYYY/MM/DD");                          // "2026/04/01"
d.format("YYYY年M月D日(ddd)");                  // "2026年4月1日(水)"
d.format("YYYY-MM");                             // 月別集計のキー
dayjs().format("YYYYMMDD_HHmmss");               // ファイル名用のタイムスタンプ
d.add(1, "month"); d.subtract(7, "day");         // 新しい値を返す（d は変わらない）
d.startOf("month"); d.endOf("month");            // 月初・月末（endOf は 23:59:59.999）
d.year(); d.month() + 1; d.date(); d.day();      // 年・月（0 始まり）・日・曜日（0 = 日曜）
d.quarter(); d.isoWeek(); d.daysInMonth();       // 四半期・ISO 週番号・月の日数
dayjs(b).diff(a, "day");                         // 日数の差（b - a）。"month" "year" も
d.isBefore(x); d.isAfter(x); d.isSame(x, "month");
d.isBetween(from, to, "day", "[]");              // 期間内（両端含む）
dayjs.max(list); dayjs.min(list);                // 最新・最古（dayjs の配列）
dayjs.duration(90, "minutes").format("H:mm");    // "1:30"
dayjs.fromExcel(45383);                          // Excel のシリアル値 → 2024/4/1（xljs 拡張）
d.toDate();                                      // Date に戻す
```

- dayjs の値はそのまま `cell.value`・`sheet.write`・`csv.write`・`fs.writeJSON`・`ui.table`・フォームの `default` に渡せる（`Date` として扱う）。
- 和暦（令和など）の解釈・表示はできない。
- 数値: `(1234567).toLocaleString()` → `"1,234,567"`、`Math.round(x * 100) / 100` で小数第 2 位に丸める。

## アプリモード（HTML の画面）

自由なレイアウトの画面が必要なときに使う（使いどころと作り方の方針は「画面の設計」）。スクリプト（`.js`）と HTML ファイルを別に用意する。画面は WebView2（Edge と同じ）で表示される。

```js
// ==xljs==
// @mode   app
// @html   ui/index.html
// @assets ui/
// @title  月次集計
// @size   900x600
// ==/xljs==
const win = app.main;                     // メインウィンドウ（スクリプトの実行前に開いている）
win.state = { status: "月を選んでください", busy: false, rows: 0 }; // 画面と共有する値（起動時の表示）

export async function run({ month }) {    // export した関数は画面から呼べる
  win.state.busy = true;
  try {
    const rows = aggregate(month);
    await win.invoke("render", rows);     // 画面の関数（xljs.handle）を呼ぶ
    win.state = { status: `${month} を集計しました`, rows: rows.length };
    return rows.length;                   // 呼び出し元（data-result・xljs.invoke の戻り値）へ
  } finally {
    win.state.busy = false;
  }
}

function aggregate(month) {
  return [{ 顧客名: "A 社", 合計: 1200 }]; // ここでファイルやデータベースを読む
}
```

```html
<!-- ui/index.html -->
<!doctype html>
<html lang="ja">
<head>
  <meta charset="utf-8">
  <link rel="stylesheet" href="/_xljs/fontawesome/css/all.min.css">
  <link rel="stylesheet" href="style.css">
</head>
<body>
  <form data-action="run" data-result="count">
    <input name="month" type="month" required>
    <button><i class="fa-solid fa-play"></i> 実行</button>
  </form>
  <p data-bind="status"></p>
  <p data-show="rows">件数: <span data-bind="rows"></span></p>
  <table id="result"><tbody></tbody></table>
  <script>
    xljs.handle("render", (rows) => {
      document.querySelector("#result tbody").replaceChildren(...rows.map((r) => {
        const tr = document.createElement("tr");
        for (const v of [r.顧客名, r.合計.toLocaleString()]) tr.append(Object.assign(document.createElement("td"), { textContent: v }));
        return tr;
      }));
    });
  </script>
</body>
</html>
```

メタデータ（スクリプトの先頭のコメント）: `@mode app`、`@html`（最初に開く HTML）、`@assets`（画面が使うフォルダ。複数行可）、`@title`、`@size 幅x高さ`、`@icon app.ico`、`@titlebar custom`（OS のタイトルバーの代わりに自前のタイトルバー）。

画面側（HTML。`xljs` は自動で使える。`<script src>` で読み込まない）:

| 書き方 | 働き |
|---|---|
| `data-bind="key"` | `win.state.key` を表示する（`<progress>` は値、入力欄は値、ほかは文字。`"a.b"` で中の値、`"!key"` で否定） |
| `data-model="key"` | 入力欄と `state.key` を双方向につなぐ（入力するとスクリプトの `win.state.key` も変わる） |
| `data-show="key"` | 値が真のときだけ表示する（`"!busy"` で逆） |
| `data-action="関数名"` | クリック（`<form>` に付ければ送信）でスクリプトの `export` した関数を呼ぶ。フォームの中なら引数は入力値のオブジェクト（`name` 属性がキー）で、呼ぶ前に `required` などを検査する。`data-args='[1, "a"]'` で引数を指定、`data-result="key"` で戻り値を `state.key` に入れる。処理中は押せなくなる |
| `await xljs.invoke("関数名", ...引数)` | スクリプトの関数を呼んで戻り値を受け取る（エラーは reject） |
| `xljs.handle("名前", fn)` | スクリプトの `win.invoke("名前", ...)` から呼ばれる関数を登録する |
| `xljs.on("名前", fn)` / `xljs.emit("名前", ...)` | スクリプトの `win.emit` を受け取る / スクリプトの `win.on` へ送る |
| `xljs.state` | `win.state` と同じ値（画面の JavaScript から読み書き）。`await xljs.ready` で最初の値が届くのを待てる |
| `await xljs.chart("#el", spec)` | `ui.chart` と同じ定義のグラフを要素に描く |

スクリプト側:

| API | 説明 |
|---|---|
| `export function 名前(...)` | 画面から呼べる関数（エントリのファイルの export だけ）。`app.handle(name, fn)` でも登録できる |
| `win.state.key = 値` / `win.state = { ... }` | 画面へ送る（代入したキーだけが変わる）。同期するのは最上位のキーだけなので、中身を変えたら `win.state.list = [...list]` のように代入し直す |
| `await win.invoke(name, ...args)` / `win.emit(name, ...args)` / `win.on(name, fn)` | 画面の関数を呼ぶ / イベントを送る / 画面の `xljs.emit` を受け取る |
| `win.$(selector).text(v)` `.html(v)` `.val(v)` `.attr(n, v)` `.addClass(c)` `.show()` `.hide()` `.on("click", fn)` | 画面の要素を直接変える・イベントを受ける簡易 DOM（読み取りは `await win.$(sel).val()`） |
| `app.open({ html, title, width, height })` | 別のウィンドウを開く（`await w.closed` で閉じるまで待つ） |
| `await win.closed` / `win.close()` | メインウィンドウが閉じるとスクリプトも終わる |

- 受け渡せる値: JSON の値・`Date`・ファイル / フォルダのハンドル・`ArrayBuffer`。画面でドロップ・選択した `File` はスクリプト側でハンドル（パス付き）になる。
- スクリプトの関数で起きたエラーは、画面で `catch` しなければ xljs のエラーダイアログに出る。
- 画面から呼べるのは同梱のファイルだけ（`@assets` のフォルダ）。外部の URL・CDN は読み込まない。アイコンは同梱の Font Awesome Free 7（上の `<link>`。`fa-solid` `fa-regular` `fa-brands`）。

## レシピ

### 1. 複数の CSV を 1 つの Excel に（ファイルごとにシート）

```js
const handles = await showOpenFilePicker({ multiple: true, types: [{ description: "CSV", accept: [".csv", ".tsv", ".txt"] }] });
if (handles.length === 0) exit();

const book = workbooks.create();
for (const h of handles) {
  const rows = csv.read(h);
  const name = path.basename(h.name, path.extname(h.name)).replace(/[\\\/?*\[\]:]/g, "_").slice(0, 31);
  book.addSheet(name).write(rows, { table: true });
}
const dest = await showSaveFilePicker({ suggestedName: "CSVまとめ.xlsx", startIn: handles[0], types: [{ description: "Excel", accept: [".xlsx"] }] });
if (!dest) exit();
book.save(dest);
alert(`${handles.length} ファイルを変換しました`);
shell.showItemInFolder(dest);
```

### 2. 列の値ごとに別ファイルに分ける

```js
const [h] = await showOpenFilePicker({ types: [{ description: "Excel", accept: [".xlsx"] }] });
if (!h) exit();
const book = workbooks.open(h);
const sheet = book.sheets[0];
const rows = sheet.toObjects();
const key = prompt("分ける列の名前", Object.keys(rows[0] ?? {})[0] ?? "");
if (!key) exit();

const outDir = path.join(path.dirname(h), "分割");
fs.mkdir(outDir);
const groups = Object.groupBy(rows, (r) => String(r[key] ?? "(空欄)"));
for (const [name, list] of ui.withProgress(Object.entries(groups), "分割中")) {
  const out = workbooks.create();
  out.addSheet(sheet.name).write(list, { table: true });
  out.save(path.join(outDir, `${name.replace(/[\\\/:*?"<>|]/g, "_")}.xlsx`));
}
alert(`${Object.keys(groups).length} ファイルに分けました`);
shell.openPath(outDir);
```

### 3. ひな形（テンプレート）に 1 行ずつ差し込んで、行の数だけファイルを作る

```js
const v = await ui.form({
  title: "差し込み作成",
  fields: [
    { name: "list", label: "一覧（1 行目が見出し）", type: "file", accept: [".xlsx"], required: true },
    { name: "template", label: "ひな形", type: "file", accept: [".xlsx"], required: true },
    { name: "outDir", label: "出力フォルダ", type: "folder", required: true },
  ],
  remember: true,
});
if (!v) exit();

const rows = workbooks.open(v.list).sheets[0].toObjects();
const safe = (s) => String(s ?? "").replace(/[\\\/:*?"<>|]/g, "_");  // ファイル名に使えない文字を置き換える
let count = 0;
for (const r of ui.withProgress(rows, "作成中")) {
  const book = workbooks.open(v.template);   // 毎回ひな形を開き直す
  const ws = book.sheets[0];
  ws.cell("B3").value = r.顧客名;
  ws.cell("B4").value = r.住所;
  ws.cell("E3").value = new Date();
  ws.cell("D10").value = Number(r.金額) || 0;
  book.save(path.join(v.outDir.path, `請求書_${safe(r.顧客名)}.xlsx`));  // 別名で保存（ひな形は変わらない）
  book.close();
  count++;
}
alert(`${count} 件作成しました`);
shell.openPath(v.outDir);
```

### 4. 2 つの一覧を突き合わせる（VLOOKUP の代わり）

```js
const v = await ui.form({
  title: "突き合わせ",
  fields: [
    { name: "a", label: "一覧 A", type: "file", accept: [".xlsx", ".csv"], required: true },
    { name: "b", label: "一覧 B（マスタ）", type: "file", accept: [".xlsx", ".csv"], required: true },
    { name: "key", label: "照合する列", type: "text", default: "社員番号", required: true },
  ],
});
if (!v) exit();

const load = (h) => h.name.toLowerCase().endsWith(".csv") ? csv.read(h) : workbooks.open(h).sheets[0].toObjects();
const a = load(v.a);
const master = new Map(load(v.b).map((r) => [String(r[v.key]).trim(), r]));

const result = a.map((r) => {
  const m = master.get(String(r[v.key]).trim());
  return { ...r, 照合結果: m ? "あり" : "なし", マスタ_氏名: m?.氏名 ?? null };
});
const missing = result.filter((r) => r.照合結果 === "なし");

const dest = await showSaveFilePicker({ suggestedName: "突き合わせ結果.xlsx", startIn: v.a });
if (!dest) exit();
const out = workbooks.create();
out.addSheet("結果").write(result, { table: true });
out.addSheet("見つからない").write(missing, { table: true });
out.save(dest);
alert(`${a.length} 件中 ${missing.length} 件がマスタにありませんでした`);
shell.showItemInFolder(dest);
```

### 5. 月別・担当別のクロス集計

```js
const [h] = await showOpenFilePicker({ types: [{ description: "Excel", accept: [".xlsx"] }] });
if (!h) exit();
const rows = workbooks.open(h).sheets[0].toObjects().filter((r) => r.日付 instanceof Date);

const ym = (d) => dayjs(d).format("YYYY-MM");
const months = [...new Set(rows.map((r) => ym(r.日付)))].sort();
const table = Object.entries(Object.groupBy(rows, (r) => r.担当 ?? "(未設定)")).map(([who, list]) => {
  const line = { 担当: who };
  for (const m of months) line[m] = 0;
  for (const r of list) line[ym(r.日付)] += Number(r.金額) || 0;
  line.合計 = list.reduce((s, r) => s + (Number(r.金額) || 0), 0);
  return line;
});

const dest = await showSaveFilePicker({ suggestedName: "月別集計.xlsx", startIn: h });
if (!dest) exit();
const out = workbooks.create();
const style = Object.fromEntries([...months, "合計"].map((m) => [m, { format: "#,##0" }]));
out.addSheet("月別").write(table, { table: true, columnStyles: style });
out.save(dest);
shell.openPath(dest);
```

### 6. exe にドロップしたファイルを処理する（`xljs pack` 用）

```js
// xljs.args にドロップしたファイルのパスが入る。無ければ選ばせる
let targets = xljs.args.map((p) => fs.file(p));
if (targets.length === 0) targets = await showOpenFilePicker({ multiple: true, types: [{ description: "Excel", accept: [".xlsx"] }] });
if (targets.length === 0) exit();

const created = [];
for (const h of ui.withProgress(targets, "変換中")) {
  const book = workbooks.open(h);
  for (const sheet of book.sheets) {
    const dest = path.join(path.dirname(h), `${path.basename(h.name, path.extname(h.name))}_${sheet.name}.csv`);
    csv.write(dest, sheet.toArrays({ text: true }), { encoding: "cp932" });
    created.push(dest);
  }
  book.close();
}
shell.showItemInFolder(created);
```
