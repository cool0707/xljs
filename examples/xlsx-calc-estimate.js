// 積算ブックの「計算」シートの数式で、選んだ工事の工事費内訳書を作る。
// 計算はすべてブックの数式に任せ、スクリプトには計算式を書かない。
// 「計算」シートの率・数式・項目を Excel で直せば、そのまま内訳書に反映される。
//
// ブックの決まりごと（data/積算.xlsx）
// - 「工事一覧」シート: 1 行が 1 件の工事。見出しが「計算」シートの入力欄の項目名と同じ列を、その入力欄に入れる
// - 「計算」シート: A 列が項目名、B 列が入力欄。A 列が「項目」の行が内訳の見出しで、その下の行が内訳
//   （見出しの「項目」「金額」の列を読む。行・列を足しても、入力欄・内訳の位置を変えてもよい）
const LIST_SHEET = "工事一覧";
const CALC_SHEET = "計算";

const [file] = await showOpenFilePicker({ types: [{ description: "Excel", accept: [".xlsx", ".xlsm"] }] });
if (!file) exit();
const book = workbooks.open(file);
const list = book.sheets[LIST_SHEET];
const calc = book.sheets[CALC_SHEET];
if (!list || !calc) {
  alert(`「${LIST_SHEET}」と「${CALC_SHEET}」のシートがあるブックを選んでください`);
  exit(1);
}

// ---- 「計算」シートのつくりを読む ----
const [, lastCell] = calc.dimension.split(":");
const lastRow = calc.rowCount;
const lastCol = colNumber(lastCell ?? "A1");
const label = (r) => String(calc.getValue(r, 1) ?? "").trim();

const headRow = range(1, lastRow).find((r) => label(r) === "項目");
if (!headRow) {
  alert(`「${CALC_SHEET}」シートの A 列に「項目」（内訳の見出し）がありません`);
  exit(1);
}
const headers = range(1, lastCol).map((c) => String(calc.getValue(headRow, c) ?? "").trim());
const amountCol = headers.indexOf("金額") + 1;
if (!amountCol) {
  alert(`「${CALC_SHEET}」シートの内訳の見出しに「金額」の列がありません`);
  exit(1);
}
// 入力欄: 内訳の見出しより上で、A 列が工事一覧の見出しと同じ行の B 列
const jobs = list.rows();
const listHeaders = Object.keys(jobs[0] ?? {});
const inputRows = new Map();
for (const r of range(1, headRow - 1)) {
  if (listHeaders.includes(label(r))) inputRows.set(label(r), r);
}
// 内訳の行（A 列の空いた行は飛ばす）
const itemRows = range(headRow + 1, lastRow).filter((r) => label(r) !== "");

// ---- 内訳書を作る工事を選ぶ ----
const picked = await ui.table(jobs, {
  title: `内訳書を作る工事を選んでください（入力欄: ${[...inputRows.keys()].join("・")}）`,
  selectable: "multiple",
});
if (!picked.length) exit();

// ---- 工事ごとに入力欄へ値を入れて計算し、計算シートの見た目のまま値を写す ----
const out = workbooks.create();
const summarySheet = out.addSheet("総括");
const summary = [];
const skipped = new Map();
for (const job of picked) {
  for (const [key, r] of inputRows) calc.cell(r, 2).value = job[key] ?? null;
  for (const s of book.calculate().skipped) skipped.set(`${s.sheet}!${s.address}`, s);

  const sheet = out.addSheet(sheetName(job, out));
  copySheet(calc, sheet);
  summary.push({
    ...job,
    ...Object.fromEntries(itemRows.map((r) => [label(r), calc.getValue(r, amountCol)])),
  });
}

// 総括: 工事一覧の列に内訳の金額を並べる
summarySheet.write(summary, {
  table: true,
  columnStyles: Object.fromEntries(
    Object.keys(summary[0]).filter((k) => typeof summary[0][k] === "number").map((k) => [k, { format: "#,##0" }]),
  ),
});

if (skipped.size) {
  const text = [...skipped.values()].map((s) => `${s.sheet}!${s.address} ${s.formula}（${s.reason}）`).join("\n");
  alert(`計算できない数式があり、保存されていた値のまま使いました:\n${text}`);
}

const dest = await showSaveFilePicker({
  suggestedName: `工事費内訳書_${dayjs().format("YYYYMMDD")}.xlsx`,
  types: [{ description: "Excel", accept: [".xlsx"] }],
});
if (!dest) exit();
out.save(dest);
ui.table(summary, "総括（保存しました）");

// ---- 補助 ----

/** 計算シートの値（計算結果）と書式・列幅を、数式を含めずに写す */
function copySheet(src, dst) {
  for (const r of range(1, lastRow)) {
    for (const c of range(1, lastCol)) {
      const from = src.cell(r, c);
      const to = dst.cell(r, c);
      if (from.value !== null) to.value = from.value;
      to.style(from.getStyle(), { replace: true });
    }
  }
  for (const c of range(1, lastCol)) {
    const col = workbooks.columnName(c);
    dst.setColumnWidth(col, src.getColumnWidth(col));
  }
}

/** シート名: 工事番号（なければ連番）。使えない文字を除き、31 文字まで */
function sheetName(job, book) {
  let base = String(job[listHeaders[0]] ?? "").replace(/[\\/?*\[\]:]/g, "").slice(0, 28) || "工事";
  let name = base;
  for (let i = 2; book.sheetNames.includes(name); i++) name = `${base}(${i})`;
  return name;
}

function range(from, to) {
  return Array.from({ length: Math.max(0, to - from + 1) }, (_, i) => from + i);
}

/** "D25" → 4 */
function colNumber(address) {
  return [...address.replace(/\d+$/, "").toUpperCase()].reduce((n, ch) => n * 26 + ch.charCodeAt(0) - 64, 0);
}
