---
title: ファイル・フォルダ
api: Fs, fs, FileSystemHandle, FileSystemFileHandle, FileSystemDirectoryHandle, FileSystemWritableFileStream, FileStat, WalkOptions, ListOptions, WalkResult, WriteData, Blob, File, PathLike, Encoding, Path, path
---

```js
const h = fs.file("C:\\data\\a.xlsx");        // FileSystemFileHandle（存在確認なし）
const d = fs.dir("C:\\data");                  // FileSystemDirectoryHandle
const file = await h.getFile();                // File（name, size, path, text(), arrayBuffer()）

for (const f of d.walk({ glob: "*.xlsx", exclude: ["old"] })) {
  console.log(f.relativePath);                 // サブフォルダも走査
}
```

- `walk()` のパターンは `"/"` を含まなければ任意の階層のファイル名に一致し、含めば起点からの相対パスに一致します（`**` 可、大文字小文字は区別しない）。除外パターンにフォルダが一致すると中身ごと除外します。
- `walk()` `list()` `fs.list()` `fs.glob()` `entries()` / `values()` は、既定で隠しファイル・システムファイル（`desktop.ini`、`Thumbs.db` など）、`.` で始まる名前、Office のロックファイル（`~$集計.xlsx`）を除きます（隠しフォルダは中も見ません）。含めるときは `{ hidden: true }` を渡します（`fs.list(p, { hidden: true })`、`d.list({ hidden: true })`）。
- ハンドルの独自拡張: `path`, `relativePath`, `exists()`, `readText()`, `writeText()`, `readBytes()` など
- `fs`: `exists` `stat` `readText` `writeText` `appendText` `readBytes` `writeBytes` `readJSON` `writeJSON` `list` `walk` `glob` `mkdir` `remove` `rename` `copy` `cwd` `chdir`
- `path`: `join` `resolve` `dirname` `basename` `extname` `relative` `normalize` `isAbsolute` `expandEnv`
- パスを受け取る API（`fs.*`・`path.*`・`workbooks.open`・`save()`・ピッカーの `startIn`・フォームの `file`/`folder` 項目など）は、文字列中の `%USERPROFILE%` のような環境変数を展開します（`fs.readText("%USERPROFILE%\\Desktop\\a.txt")`）。未定義の変数はそのまま残ります。展開した文字列だけ欲しいときは `path.expandEnv(s)`
- パスを受け取る API には文字列・ハンドル・`File`（ディスク上のもの）のどれでも渡せます。
- 相対パスはカレントフォルダが基準です。エクスプローラーから起動した場合（ダブルクリック、exe やスクリプトへのドロップ）はスクリプト（埋め込み exe なら exe）のあるフォルダがカレントフォルダになります。コマンドプロンプトから起動した場合はそのままです。
- `writeText` / `appendText` の既定は UTF-8 で、`.csv` / `.tsv` のファイルだけは BOM 付き（Excel で文字化けしない）にします。
- 文字コード: `"auto"`（BOM → UTF-8 → CP932 の順に判定）, `"utf8"`, `"utf8bom"`, `"cp932"`（= `"shift_jis"`）, `"euc-jp"`, `"utf16le"` など。CP932 で表せない文字は `?` になります。
