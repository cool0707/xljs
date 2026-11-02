import { ok, equal, deepEqual, throws } from "./_assert.js";

// ---- Excel-compatible functions (every connection has them)
const db = sqlite.open("calc.db");
db.exec(`CREATE TABLE 受注(
  id INTEGER PRIMARY KEY, 品名 TEXT, 単価 REAL, 数量 INTEGER, 受注日 DATE,
  金額 INTEGER AS (ROUNDDOWN(単価 * 数量 * 1.1)) STORED,
  請求月 TEXT AS (TEXT(受注日, 'yyyy"年"m"月"')) VIRTUAL,
  支払期限 DATE AS (EOMONTH(受注日, 1)) VIRTUAL)`);
db.exec("CREATE VIEW 月別 AS SELECT 請求月, sum(金額) AS 合計 FROM 受注 GROUP BY 請求月");
db.insert("受注", [
  { 品名: "ねじ", 単価: 12.5, 数量: 3, 受注日: new Date(2026, 3, 15) },
  { 品名: "ボルト", 単価: 1.8, 数量: 100, 受注日: new Date(2026, 3, 30) },
]);
const first = db.get("SELECT * FROM 受注 WHERE id = 1");
equal(first.金額, 41);
equal(first.請求月, "2026年4月");
ok(first.支払期限 instanceof Date && first.支払期限.getTime() === new Date(2026, 4, 31).getTime(), "EOMONTH as a Date");
equal(db.get("SELECT 金額 FROM 受注 WHERE id = 2").金額, 198);
deepEqual(db.all("SELECT * FROM 月別"), [{ 請求月: "2026年4月", 合計: 239 }]);
// Rows read back carry the generated columns; insert leaves them out.
db.insert("受注", [{ ...first, id: 3 }]);
equal(db.get("SELECT 金額 FROM 受注 WHERE id = 3").金額, 41);
deepEqual(db.values("SELECT ROUNDHALF(2.675, 2), round(2.675, 2), ROUNDHALF(1234.5, -2), LEFT('東京都千代田区', 3), DATEDIF('2000-04-02', '2026-04-01', 'Y')"),
  [[2.68, 2.67, 1200, "東京都", 25]]);
equal(db.get("SELECT YEAR('abc') AS y").y, null, "NULL for values that are not dates");
db.close();
// Another connection (and the check) computes the generated columns too.
equal(sqlite.check("calc.db").ok, true);
const again = sqlite.open("calc.db", { readonly: true });
equal(again.get("SELECT 支払期限 FROM 受注 WHERE id = 2").支払期限.getMonth(), 4);
again.close();

// ---- functions of the script
const local = sqlite.open("local.db");
let inDebugger = false;
try {
  local.function("税込", (price) => Math.floor(price * 1.1));
} catch (e) {
  // The script runs in the debugger page, which SQLite cannot call back.
  if (!/デバッガ/.test(e.message)) throw e;
  inDebugger = true;
}
if (!inDebugger) {
  local.exec("CREATE TABLE item(name TEXT, price INTEGER, sold DATE, data BLOB)");
  local.run("INSERT INTO item VALUES('ねじ', 980, '2026-04-01', x'0102'), ('ナット', 120, NULL, NULL)");
  deepEqual(local.values("SELECT name, 税込(price) FROM item ORDER BY price"), [["ナット", 132], ["ねじ", 1078]]);
  equal(local.get("SELECT name FROM item WHERE 税込(price) > 1000").name, "ねじ");

  // Arguments: numbers, text (dates too), Uint8Array, null; any number of them.
  const seen = [];
  local.function("show", { deterministic: true }, (...args) => {
    seen.push(args);
    return args.length;
  });
  equal(local.get("SELECT show(sold, data, price, name, 1.5) AS n FROM item WHERE name = 'ねじ'").n, 5);
  equal(seen[0][0], "2026-04-01");
  ok(seen[0][1] instanceof Uint8Array && seen[0][1][1] === 2, "blob as Uint8Array");
  deepEqual(seen[0].slice(2), [980, "ねじ", 1.5]);
  equal(local.get("SELECT show() AS n").n, 0);
  // Results are stored like parameters.
  local.function("conv", (k) => ({ d: new Date(2026, 3, 1, 9, 30), t: true, u: undefined, o: { a: 1 } })[k]);
  deepEqual(local.values("SELECT conv('d'), conv('t'), conv('u'), conv('o')"), [["2026-04-01 09:30:00", 1, null, '{"a":1}']]);
  // Writes of a local database may use them.
  local.run("UPDATE item SET price = 税込(price)");
  equal(local.get("SELECT price FROM item WHERE name = 'ねじ'").price, 1078);
  // Registering a name again replaces the function.
  local.function("税込", (price) => Math.round(price * 1.08));
  equal(local.get("SELECT 税込(100) AS v").v, 108);

  // What the function throws comes out of the call as it is.
  class MyError extends Error {}
  local.function("boom", () => { throw new MyError("ばくはつ"); });
  const e = throws(() => local.all("SELECT boom()"), /ばくはつ/);
  ok(e instanceof MyError, "the error thrown by the function");
  throws(() => local.exec("SELECT boom()"), /ばくはつ/);
  throws(() => local.transaction(() => local.run("UPDATE item SET price = boom()")), /ばくはつ/);
  equal(local.get("SELECT price FROM item WHERE name = 'ねじ'").price, 1078, "rolled back");
  local.function("later", async () => 1);
  throws(() => local.get("SELECT later()"), /async/);
  local.function("inner", () => local.get("SELECT 1 AS x").x);
  throws(() => local.get("SELECT inner()"), /データベースを使えません/);

  // Names
  throws(() => local.function("round", () => 0), /SQLite か xljs の関数/);
  throws(() => local.function("ROUNDUP", () => 0), /SQLite か xljs の関数/);
  throws(() => local.function("a b", () => 0), /名前にできません/);
  throws(() => local.function("f"), /関数を指定/);

  // Not in the schema: other connections do not have the function.
  throws(() => local.exec("CREATE VIEW v AS SELECT 税込(price) AS p FROM item"), /定義には使えません/);
  throws(() => local.exec("CREATE TABLE g(a, b AS (税込(a)))"), /定義には使えません/);
  throws(() => local.exec("CREATE TABLE log(x); CREATE TRIGGER tr AFTER INSERT ON item BEGIN INSERT INTO log VALUES(税込(new.price)); END"), /定義には使えません/);
  throws(() => local.run("ALTER TABLE item ADD COLUMN p2 AS (税込(price))"), /定義には使えません/);
  deepEqual(local.tables, ["item"], "nothing was created");
  // ... and not from a view or trigger made elsewhere (SQLITE_DIRECTONLY).
  const other = sqlite.open("local.db");
  other.exec("CREATE VIEW v2 AS SELECT 税込(price) AS p FROM item");
  other.close();
  throws(() => local.all("SELECT * FROM v2"), /税込/);
  local.close();
  throws(() => local.get("SELECT 税込(1)"), /閉じられています/);

  // ---- shared databases: changes go into the change log, which a repair
  // replays without the function.
  const shared = sqlite.open("shared.db", { shared: true });
  shared.exec("CREATE TABLE t(v INTEGER)");
  shared.run("INSERT INTO t VALUES(10)");
  shared.function("double", (v) => v * 2);
  equal(shared.get("SELECT double(v) AS d FROM t").d, 20, "queries may use it");
  equal(shared.transaction(() => shared.get("SELECT double(v) AS d FROM t").d), 20);
  throws(() => shared.run("UPDATE t SET v = double(v)"), /共有データベースを変更する文には使えません/);
  throws(() => shared.run("INSERT INTO t SELECT double(v) FROM t"), /共有データベースを変更する文には使えません/);
  equal(shared.get("SELECT sum(v) AS s FROM t").s, 10);
  shared.close();
  const nolog = sqlite.open("nolog.db", { shared: true, backup: false });
  nolog.exec("CREATE TABLE t(v INTEGER); INSERT INTO t VALUES(10)");
  nolog.function("double", (v) => v * 2);
  nolog.run("UPDATE t SET v = double(v)");
  equal(nolog.get("SELECT v FROM t").v, 20, "no change log, no limits");
  nolog.close();
}
console.log("sqlite functions ok");
