---
name: xljs-sqlite
description: xljs の SQLite データベース（sqlite.open。共有フォルダで複数人が使う共有モード、自動バックアップ、修復、Excel 互換の SQL 関数）のリファレンス。xljs のスクリプトでデータベースを使うときに使う。
---

# xljs API リファレンス: SQLite

xljs の SQLite データベースです。基本の API（ファイル・Excel・CSV・JSON・フォーム・表・日付など）はスキル `xljs-reference` にあります。ここに書いていない関数・オプションは存在しないものとして扱ってください。

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

