// xljs dump.js <in.xlsx> <out.json> [calc] [saveTo.xlsx]
// Dumps every cell of every sheet: value as Excel's Value2 (dates as serials).
const [src, out, mode, saveTo] = xljs.args;
const book = workbooks.open(src);
let calc = null;
if (mode === "calc") {
  const r = book.calculate();
  calc = { formulas: r.formulas, changed: r.changed, skipped: r.skipped };
}
const base = Date.UTC(1899, 11, 30);
function v2(v) {
  if (v instanceof Date) {
    const ms = Date.UTC(v.getFullYear(), v.getMonth(), v.getDate(), v.getHours(), v.getMinutes(), v.getSeconds(), v.getMilliseconds());
    return { d: (ms - base) / 86400000 };
  }
  return v;
}
const sheets = {};
for (const ws of book.sheets) {
  const cells = {};
  const dim = ws.dimension;
  const m = /^([A-Z]+)(\d+)(?::([A-Z]+)(\d+))?$/.exec(dim);
  if (m) {
    const col = (s) => [...s].reduce((n, c) => n * 26 + c.charCodeAt(0) - 64, 0);
    const c1 = col(m[1]), r1 = +m[2], c2 = m[3] ? col(m[3]) : c1, r2 = m[4] ? +m[4] : r1;
    for (let r = r1; r <= r2; r++) for (let c = c1; c <= c2; c++) {
      const cell = ws.cell(r, c);
      const v = cell.value, f = cell.formula;
      if (v === null && f === null) continue;
      cells[cell.address] = { v: v2(v), f, t: cell.text };
    }
  }
  sheets[ws.name] = { dim, cells };
}
fs.writeJSON(out, { calc, sheets });
if (saveTo) book.save(saveTo);
