---
title: SQLite
api: Sqlite, sqlite, SqliteOpenOptions, Database, SqlValue, SqlParams, SqlRow, SqliteRunResult, SqliteColumn, SqliteCheckResult, SqliteRepairReport, SqliteBackup
---

```js
const db = sqlite.open("受注.db");                   // なければ作る
db.exec("CREATE TABLE IF NOT EXISTS 受注 (番号 TEXT PRIMARY KEY, 日付 DATE, 金額 INTEGER)");
db.run("INSERT INTO 受注 VALUES (?, ?, ?)", "A1", new Date(2026, 3, 1), 1000);  // { changes, lastInsertRowid }
db.all("SELECT * FROM 受注 WHERE 金額 > :min", { min: 500 });                   // オブジェクトの配列
db.get("SELECT count(*) AS n FROM 受注");                                      // 最初の行か null
db.insert("受注", rows, { replace: true });          // まとめて挿入（テーブルがなければ値から型を決めて作る）
db.transaction(() => { /* ... */ });                        // 例外なら取り消し（入れ子は SAVEPOINT）
db.check();                                           // 整合性の検査
db.backup("copy.db");                                // 使用中でもコピーを作る
db.close();
```

- `Date` はローカル時刻の文字列（`"2026-04-01"`・`"2026-04-01 09:30:00"`）で保存し、`DATE`・`DATETIME` と宣言した列は `Date` で読めます。`BOOLEAN` の列は真偽値、BLOB は `Uint8Array`。
- **ファイルサーバーで複数人が使う（共有モード）**: `sqlite.open(p, { shared: true })`、ネットワークドライブ・`\\server\share` 上なら自動。データベースの隣に `<ファイル名>.xljs-shared` フォルダを作り、以後は誰が開いても共有モードになります。呼び出しのたびにファイルを開いてロック（読み取りは共有、書き込みは排他）を取り、終わったらすぐ閉じます（ローカルより遅くなりますが壊れにくくなります）。書き込みの前に 1 時間ごとに検査と自動バックアップ（10 世代）を取り、以降の変更をすべて記録します。
- **壊れたとき**: 次の呼び出しで「修復しますか？」と確認し、はいなら修復して処理をやり直します。修復は 中断された書き込みの取り消し・索引の作り直し → 自動バックアップ＋その後の変更の再実行 → 読める行だけで作り直し の順に試し、壊れたファイルは `broken` フォルダに残します。データベースのファイルを xljs.exe にドロップするか `xljs repair 受注.db` でも修復できます。詳しくは `docs/design/sqlite-design.md`。
