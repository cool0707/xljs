# xljs

JavaScript / TypeScript で書いたスクリプトで Excel・CSV の集計やデータ変換を行う、Windows 用のシングルバイナリです。Excel のインストールは不要です。

```js
const dir = await showDirectoryPicker();
if (!dir) exit();

const rows = [];
for (const h of ui.withProgress(dir.walk({ glob: "*.xlsx", exclude: ["~$*"] }), { title: "集計中" })) {
  const book = workbooks.open(h);
  for (const r of book.sheets["明細"]?.toObjects() ?? []) rows.push({ ...r, ファイル: h.relativePath });
}

const dest = await showSaveFilePicker({ suggestedName: "集計結果.xlsx" });
if (dest) {
  const out = workbooks.create();
  out.addSheet("明細").write(rows);
  out.save(dest);
  alert(`${rows.length} 件を集計しました`);
}
```

- **言語**: ES2023 相当（オプショナルチェーン、`??`、async/await、トップレベル await、クラスフィールドなど）と TypeScript。`import` でファイル分割可能
- **ファイル操作**: ブラウザの File System Access API 風（`showOpenFilePicker` / `FileSystemFileHandle` など）＋ フォルダ走査などの独自拡張
- **Excel**: xlsx の読み書き（[excelize](https://github.com/xuri/excelize)）。値は数値・文字列・真偽値・`Date` で受け渡し
- **CSV**: Shift_JIS(CP932) / UTF-8 の自動判定、BOM 付き UTF-8 での書き出し
- **UI**: Windows ネイティブのダイアログ（メッセージボックス、ファイル選択、進捗ダイアログ）。コンソールでも動作

## 使い方

```
xljs [オプション] <script.js|script.ts> [引数...]
xljs pack <script> [-o 出力.exe]   スクリプトを埋め込んだ exe を作成
xljs check <script>                構文と import を検査（実行しない）
xljs init [フォルダ]               型定義 (xljs.d.ts) と jsconfig.json を作成
xljs version
```

| オプション | 説明 |
|---|---|
| `--ui=auto` | 既定。Windows ではネイティブダイアログ、それ以外はコンソール |
| `--ui=native` / `--ui=console` | UI を固定 |
| `--ui=mock --ui-answers=answers.json` | ダイアログを出さず、ファイルに書いた応答を返す（自動実行・テスト用） |
| `--lang=auto` / `ja` / `en` | メッセージの言語（既定はシステムの表示言語） |

- `xljs.exe` はコンソール版、`xljsw.exe` は GUI 版（ダブルクリックしても黒い窓が出ない）です。中身は同じで、GUI 版をコマンドプロンプトから起動した場合はそのコンソールに出力します。GUI 版で出力先がないときのエラーはダイアログで表示し、文字列を選択したり「コピー」ボタンでクリップボードにコピーしたりできます。
- エラー・警告はシステムの表示言語に合わせて日本語または英語で表示します。`--lang=ja|en`（または環境変数 `XLJS_LANG`）で切り替えられます。JavaScript エンジンや構文チェック、Excel 読み込み、OS のエラーのうちよく出るものは日本語に訳して表示します（`catch` で受け取る `e.message` は元のままです）。
- 引数なしで起動すると、実行するスクリプトを選ぶダイアログが開きます。exe にスクリプトをドラッグ＆ドロップしても実行できます。
- コンソール版をダブルクリックで起動した場合は、終了時に Enter 待ちになります。
- 進捗表示中の Ctrl+C はキャンセル要求（`cancelled` が true になる）、2 回目で強制終了です。

### exe にする

```
xljs pack 集計.js            → 集計.exe
xljsw pack 集計.js -o 集計.exe  （GUI 版をベースにする場合）
```

作成した exe に渡した引数（ドロップしたファイルのパスなど）は `xljs.args` で受け取れます。スクリプトはバンドル済みの状態で埋め込まれるので、`import` しているファイルも一緒に入ります。

### エディタの補完

```
xljs init
```

`xljs.d.ts` と `jsconfig.json` が作られ、VS Code で補完と型チェックが効くようになります。

## API

### ダイアログ・ピッカー

| API | 戻り値 |
|---|---|
| `alert(msg)` / `confirm(msg)` / `prompt(msg, 既定値)` | 同期。`prompt` はキャンセルで `null` |
| `await showOpenFilePicker({ multiple, types, startIn, title })` | `FileSystemFileHandle[]`（キャンセルで `[]`） |
| `await showSaveFilePicker({ suggestedName, types, startIn })` | `FileSystemFileHandle \| null` |
| `await showDirectoryPicker({ startIn, title })` | `FileSystemDirectoryHandle \| null` |

`types` は `[{ description: "Excel", accept: { "<MIME>": [".xlsx"] } }]` のほか、`accept: [".xlsx", ".xlsm"]` とも書けます。`startIn` には `"desktop"` `"documents"` `"downloads"`、パス、ハンドルを指定できます。

### 進捗表示

```js
const p = ui.progress({ title: "処理中", total: files.length });
for (const [i, f] of files.entries()) {
  if (p.cancelled) break;
  p.update(i, f.name);
  // ...
}
p.close();

// まとめて書く場合（キャンセルされるとループが終わる。ui.cancelled で判定）
for (const f of ui.withProgress(files, { title: "処理中" })) { /* ... */ }
for await (const h of ui.withProgress(dir.values())) { /* 総数不明ならマーキー表示 */ }
```

Windows では標準の進捗ダイアログ（IProgressDialog）、コンソールでは標準エラー出力にバーを表示します。

### ファイル・フォルダ

```js
const h = fs.file("C:\\data\\a.xlsx");        // FileSystemFileHandle（存在確認なし）
const d = fs.dir("C:\\data");                  // FileSystemDirectoryHandle
const file = await h.getFile();                // File（name, size, path, text(), arrayBuffer()）

for (const f of d.walk({ glob: "*.xlsx", exclude: ["~$*", "old"] })) {
  console.log(f.relativePath);                 // サブフォルダも走査
}
```

- `walk()` のパターンは `"/"` を含まなければ任意の階層のファイル名に一致し、含めば起点からの相対パスに一致します（`**` 可、大文字小文字は区別しない）。除外パターンにフォルダが一致すると中身ごと除外します。
- ハンドルの独自拡張: `path`, `relativePath`, `exists()`, `readText()`, `writeText()`, `readBytes()` など
- `fs`: `exists` `stat` `readText` `writeText` `appendText` `readBytes` `writeBytes` `readJSON` `writeJSON` `list` `walk` `glob` `mkdir` `remove` `rename` `copy` `cwd` `chdir`
- `path`: `join` `resolve` `dirname` `basename` `extname` `relative` `normalize` `isAbsolute`
- パスを受け取る API には文字列・ハンドル・`File`（ディスク上のもの）のどれでも渡せます。
- 文字コード: `"auto"`（BOM → UTF-8 → CP932 の順に判定）, `"utf8"`, `"utf8bom"`, `"cp932"`（= `"shift_jis"`）, `"euc-jp"`, `"utf16le"` など。CP932 で表せない文字は `?` になります。

### CSV

```js
const rows = csv.read(h);                         // 見出し行をキーにしたオブジェクトの配列
const raw = csv.read("a.tsv", { header: false }); // 2 次元配列（.tsv はタブ区切り）
const typed = csv.read(h, { cast: true });        // 数値・真偽値に変換、空欄は null

csv.write("out.csv", rows);                       // 既定は BOM 付き UTF-8・CRLF
csv.write("out.csv", rows, { encoding: "cp932", columns: ["日付", "金額"] });
csv.parse(text) / csv.stringify(rows)
```

### Excel

```js
const book = workbooks.open(h);              // パス・ハンドル・File・Blob
const sheet = book.sheets["売上"] ?? book.sheets[0];
const rows = sheet.toObjects();              // [{ 日付: Date, 金額: 1200, ... }]
const grid = sheet.toArrays({ text: true }); // 表示形式を適用した文字列

sheet.cell("B2").value = 100;
sheet.cell(3, 2).value = new Date();         // (行, 列)
sheet.cell("C10").formula = "=SUM(C2:C9)";
sheet.append([{ 日付: new Date(), 金額: 500 }]);
book.save();                                 // 上書き（save(パスやハンドル) で別名保存）

const out = workbooks.create();
out.addSheet("集計").write(summary).setColumnWidth("A:C", 16);
out.save("集計.xlsx");
```

- 日付の書式が付いた数値セルは `Date` になります（Excel の日付はタイムゾーンを持たないので、ローカル時刻として扱います）。
- 数式は保存時に「開いたときに再計算」フラグを立てるので、Excel で開けば計算結果が表示されます。スクリプト内で値が必要なら `cell.calc()`（対応関数は一部）。
- xlsx / xlsm に対応しています（旧形式の .xls は不可）。マクロ、ピボットテーブル、グラフなどは読み書きの過程で失われることがあります。

### その他

- `console.log/error/table/time` など、`setTimeout` / `setInterval`、`sleep(ms)`
- `xljs.args` `xljs.script` `xljs.scriptDir` `xljs.version` `xljs.title`（ダイアログのタイトル） `xljs.env(name)` `exit(code)`
- 補っている組み込み: `Array.fromAsync`, `structuredClone`, `TextEncoder` / `TextDecoder`（Shift_JIS 対応）, `atob` / `btoa`, `Blob` / `File`, 数値・日付の `toLocaleString`（日本語形式）, 簡易 `Intl`

## ビルド

Go 1.26 以降が必要です（cgo 不要）。

```
# Windows x64 版（どの OS からでもクロスビルド可。Go では x64 を amd64 と呼びます）
GOOS=windows GOARCH=amd64 go build -trimpath -ldflags "-s -w" -o dist/x64/xljs.exe ./cmd/xljs
GOOS=windows GOARCH=amd64 go build -trimpath -ldflags "-s -w -H=windowsgui" -o dist/x64/xljsw.exe ./cmd/xljs

# または
make windows        # dist/x64/
make dist           # dist/xljs-windows-x64.zip と xljs-windows-arm64.zip
```

Windows 上なら PowerShell で `go build -o xljs.exe ./cmd/xljs` だけでも構いません。`cmd/xljs/rsrc_windows_*.syso` はビジュアルスタイルと高 DPI 対応のマニフェストです（`xljs.manifest` から [rsrc](https://github.com/akavel/rsrc) で生成）。

## 開発

```
go test ./...
```

- `testdata/scripts/*.test.{js,ts}` はモック UI で実行される結合テストです。`*.answers.json` に応答を書きます。
- UI はインターフェースで分離しているので、ネイティブダイアログ以外のロジックは Linux でもテストできます。

| パス | 内容 |
|---|---|
| `cmd/xljs` | CLI（引数処理、exe 埋め込み、コンソール処理） |
| `internal/engine` | esbuild でバンドル → goja で実行、イベントループ、エラー表示 |
| `internal/api` | Go 側のネイティブ関数と `prelude.js`（公開 API の JS 実装） |
| `internal/ui` | UI インターフェースと native（Win32）/ console / mock 実装 |
| `internal/pack` | exe へのスクリプト埋め込み |
| `types/xljs.d.ts` | 型定義（`xljs init` で出力） |

スクリプトは esbuild で 1 ファイルにバンドルし（TypeScript の変換、goja 未対応構文の変換、ソースマップ付き）、`async` 関数で包んで [goja](https://github.com/dop251/goja) で実行します。エラーの行番号は元のファイルの位置で表示されます。

## ライセンス

MIT
