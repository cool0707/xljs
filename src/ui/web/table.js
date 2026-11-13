"use strict";
// Built-in table page (ui.table): virtual scrolling, sort, filter (the box
// above and per column), column resize, pinned / frozen / hidden columns
// (gridkit.js), conditional formats, copy as TSV, save as CSV/Excel (done by
// the script) and row selection. Cells are numbers, strings, booleans or null.
const $ = (id) => document.getElementById(id);

let texts = {};
let columns = [];   // [{ label, width }]
let widths = [];    // px per column (by index in columns)
let cols = new GridKit.Columns(0); // hidden, pinned and frozen columns
let order = [];     // the columns shown: [{ c, side }] (GridKit.Columns.order)
let sticky = [];    // per entry of order: its sticky offset, or null
const filters = new Map(); // column -> filter (gridkit.js)
const kinds = new Map();   // column -> "number" | "date" | "text", found when filtering
let formats = [];   // conditional formats (gridkit.js)
let hits = [];      // per row: the "rule:column" keys of the formats with functions that matched
let fmtStats = null;
let rows = [];      // arrays of cells; the index is the row's index in the script
let total = 0;      // number of rows the script will send
let select = "";    // "" | "single" | "multiple"
let dblclick = false; // the script handles double clicks (onDoubleClick)
let view = [];      // row indices shown, filtered and sorted
let sortCol = -1, sortDir = 0;
let filterTokens = [];
let searchText = null; // lazily built lower-case text per row
const selected = new Set();
let cursor = -1;    // position in view
let anchor = -1;    // position in view where a shift-selection starts

const grid = $("grid"), head = $("head"), body = $("body"), rowsEl = $("rows");
const rowH = () => parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--row-h")) || 26;

host.on((m) => {
  switch (m.type) {
    case "init": init(m); break;
    case "rows": addRows(m.rows, m.hits); break;
    case "saved": status(fmt(texts.saved, m.path)); break;
    case "error": status(m.message, true); break;
  }
});

function init(m) {
  texts = m.texts || {};
  texts.grid = texts.grid || {};
  columns = m.columns || [];
  total = m.total || 0;
  select = m.select || "";
  dblclick = !!m.dblclick;
  $("filter").placeholder = texts.filter;
  $("copy").textContent = texts.copy;
  $("save-csv").textContent = texts.saveCsv;
  $("save-xlsx").textContent = texts.saveXlsx;
  $("ok").textContent = texts.ok;
  $("ok").hidden = !select;
  $("cancel").textContent = select ? texts.cancel : texts.close;
  $("empty").textContent = texts.empty;
  $("clear-filters").textContent = texts.grid.clearFilters;
  formats = m.formats || [];
  rows = [];
  hits = [];
  widths = columns.map((c) => c.width || 0);
  cols = new GridKit.Columns(columns.length);
  buildHead();
  update();
  grid.focus();
  host.send({ type: "shown" });
}

// ---- data

let rebuildTimer = 0;
function addRows(chunk, chunkHits) {
  const autosize = rows.length === 0;
  for (let i = 0; i < chunk.length; i++) {
    rows.push(chunk[i]);
    hits.push(chunkHits ? chunkHits[i] : null);
  }
  kinds.clear();
  if (searchText) for (const r of chunk) searchText.push(rowText(r));
  if (autosize) autosizeColumns();
  // Re-filter and re-sort at most a few times a second while loading.
  if (!rebuildTimer) rebuildTimer = setTimeout(() => {
    rebuildTimer = 0;
    if (rows.length >= total) buildHead(); // numbers' headings are right-aligned
    update();
  }, rows.length >= total ? 0 : 150);
}

function cellText(v) {
  if (v === null || v === undefined) return "";
  return String(v);
}

function rowText(r) {
  return r.map(cellText).join("\t").normalize("NFKC").toLowerCase();
}

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });
function compare(a, b) {
  const ea = a === null || a === "", eb = b === null || b === "";
  if (ea || eb) return ea === eb ? 0 : ea ? 1 : -1;
  const na = typeof a === "number", nb = typeof b === "number";
  if (na && nb) return a - b;
  if (na !== nb) return na ? -1 : 1;
  return collator.compare(String(a), String(b));
}

function kindOf(c) {
  if (!kinds.has(c)) kinds.set(c, GridKit.kindOf(rows, c));
  return kinds.get(c);
}

function rebuildView() {
  let v = [];
  const byColumn = [...filters].filter(([c, f]) => c < columns.length && GridKit.filterOn(f));
  if (filterTokens.length || byColumn.length) {
    if (filterTokens.length && !searchText) searchText = rows.map(rowText);
    for (let i = 0; i < rows.length; i++) {
      if (filterTokens.length) {
        const t = searchText[i];
        if (!filterTokens.every((tok) => t.includes(tok))) continue;
      }
      if (byColumn.length && !byColumn.every(([c, f]) => GridKit.match(f, rows[i][c]))) continue;
      v.push(i);
    }
  } else {
    v = new Array(rows.length);
    for (let i = 0; i < rows.length; i++) v[i] = i;
  }
  if (sortCol >= 0 && sortDir) {
    const c = sortCol, d = sortDir;
    v.sort((x, y) => {
      const a = rows[x][c], b = rows[y][c];
      const ea = a === null || a === "", eb = b === null || b === "";
      if (ea || eb) return ea === eb ? x - y : ea ? 1 : -1; // empty cells last either way
      return compare(a, b) * d || x - y;
    });
  }
  view = v;
}

function update() {
  const curRow = cursor >= 0 ? view[cursor] : -1;
  rebuildView();
  fmtStats = formats.length ? GridKit.stats(formats, rows, columns.length) : null;
  cursor = curRow >= 0 ? view.indexOf(curRow) : -1;
  if (cursor < 0 && view.length) cursor = Math.min(Math.max(cursor, 0), view.length - 1);
  anchor = cursor;
  body.style.height = view.length * rowH() + "px";
  $("empty").hidden = view.length > 0 || rows.length < total;
  updateCount();
  render();
}

function updateCount() {
  const shown = view.length, all = rows.length;
  let s = shown === all ? fmt(texts.count, all) : fmt(texts.countFiltered, shown, all);
  if (all < total) s = fmt(texts.loading, all, total);
  if (selected.size) s += "　" + fmt(texts.selected, selected.size);
  $("count").textContent = s;
  $("ok").disabled = selected.size === 0;
  $("clear-filters").hidden = ![...filters.values()].some(GridKit.filterOn);
}

// ---- columns

/** Widths (px, padding included) of texts laid out as cells of the grid. */
function measure(texts, header) {
  const box = document.createElement("div");
  box.className = "measure" + (header ? " head" : "");
  const spans = texts.map((t) => {
    const s = document.createElement("span");
    s.className = "cell";
    s.textContent = t;
    return box.appendChild(s);
  });
  grid.append(box);
  const w = spans.map((s) => s.offsetWidth);
  box.remove();
  return w;
}

function columnWidth(i, list) {
  const texts = new Set();
  for (const ri of list) {
    const t = cellText(rows[ri][i]);
    if (t.length > 80) return 360;
    texts.add(t);
  }
  return Math.max(...measure([columns[i].label], true), ...measure(Array.from(texts)), 0) + 2;
}

function autosizeColumns() {
  const sample = [];
  for (let i = 0; i < Math.min(rows.length, 300); i++) sample.push(i);
  widths = columns.map((c, i) => c.width || Math.round(Math.min(Math.max(columnWidth(i, sample), 56), 360)));
  applyWidths();
}

let gutterW = 0;
function gutterWidth() {
  return measure([String(Math.max(total, 1))])[0] + 2;
}

/** Lays the columns out again: the grid's columns and the sticky offsets. */
function applyWidths() {
  if (!gutterW) gutterW = gutterWidth();
  order = cols.order();
  sticky = GridKit.stickyLayout(order, (c) => widths[c] || 120, gutterW);
  // The columns pinned to the right stay at the right edge: an empty column before them takes the rest.
  const tpl = [gutterW + "px", ...order.map((o, i) => (i === firstRight() ? "minmax(0, 1fr) " : "") + (widths[o.c] || 120) + "px")].join(" ");
  document.documentElement.style.setProperty("--cols", tpl);
  head.querySelectorAll(".cell[data-c]").forEach((cell, i) => GridKit.stick(cell, sticky[i]));
}

/** The position in order of the first column pinned to the right, or -1. */
const firstRight = () => order.findIndex((o) => o.side === "right");
/** The empty cell before the columns pinned to the right. */
const spacer = () => Object.assign(document.createElement("div"), { className: "cell spacer-cell" });

function buildHead() {
  order = cols.order();
  head.textContent = "";
  const g = Object.assign(document.createElement("div"), { className: "cell gutter gk-stk" });
  g.style.left = "0px";
  head.append(g);
  for (const { c: i, side } of order) {
    if (side === "right" && !head.querySelector(".spacer-cell")) head.append(spacer());
    const c = columns[i];
    const cell = document.createElement("div");
    cell.className = "cell";
    cell.dataset.c = i;
    const f = filters.get(i);
    GridKit.heading(cell, {
      label: c.label,
      sort: sortCol === i ? sortDir : 0,
      filtered: GridKit.filterOn(f),
      numeric: rows.length >= total && rows.length > 0 && kindOf(i) === "number",
      texts: texts.grid,
      onSort: () => sortBy(i),
      onFilter: (btn) => GridKit.filterPopup({
        anchor: btn, kind: kindOf(i), filter: filters.get(i), texts: texts.grid,
        onChange: (nf) => {
          if (nf) filters.set(i, nf); else filters.delete(i);
          cell.classList.toggle("gk-filtered", !!nf);
          update();
        },
      }),
      onMenu: (btn) => GridKit.columnMenu({
        anchor: btn, c: i, labels: columns.map((x) => x.label), cols, texts: texts.grid,
        onChange: () => { buildHead(); render(); },
        onReset: resetColumns,
      }),
      onResize: (e) => GridKit.resize(e, widths[i] || 120, (w) => { widths[i] = w; applyWidths(); render(); }),
      onFit: () => fitColumn(i),
    });
    head.append(cell);
  }
  applyWidths();
}

/** The menu's "reset": all columns shown and not pinned, their widths, no sort and no column filters. */
function resetColumns() {
  cols.reset();
  filters.clear();
  sortCol = -1;
  sortDir = 0;
  if (rows.length) autosizeColumns();
  buildHead();
  update();
}

function sortBy(i) {
  if (sortCol !== i) { sortCol = i; sortDir = 1; }
  else if (sortDir === 1) sortDir = -1;
  else { sortCol = -1; sortDir = 0; }
  buildHead();
  update();
}

function fitColumn(i) {
  const first = Math.floor(grid.scrollTop / rowH());
  widths[i] = Math.round(Math.min(columnWidth(i, view.slice(first, first + 200)), 800));
  applyWidths();
  render();
}

// ---- rendering

let frame = 0;
function render() {
  frame = 0;
  const h = rowH();
  const first = Math.max(0, Math.floor(grid.scrollTop / h) - 4);
  const count = Math.ceil(grid.clientHeight / h) + 8;
  const last = Math.min(view.length, first + count);
  rowsEl.style.top = first * h + "px";
  const frag = document.createDocumentFragment();
  for (let p = first; p < last; p++) {
    const ri = view[p];
    const r = rows[ri];
    const tr = document.createElement("div");
    tr.className = "trow" + (selected.has(ri) ? " sel" : "") + (p === cursor ? " cur" : "");
    tr.dataset.pos = p;
    const g = document.createElement("div");
    g.className = "cell gutter gk-stk";
    g.style.left = "0px";
    g.textContent = ri + 1;
    tr.append(g);
    const fm = formats.length ? GridKit.rowFormats(formats, r, hits[ri], fmtStats) : null;
    const sel = selected.has(ri);
    const fr = firstRight();
    order.forEach(({ c }, k) => {
      if (k === fr) {
        const sp = tr.appendChild(spacer());
        if (fm) GridKit.paint(sp, fm[0], null, sel);
      }
      const v = r[c];
      const td = document.createElement("div");
      td.className = "cell" + (typeof v === "number" ? " num" : "");
      td.dataset.c = c;
      const t = cellText(v);
      td.textContent = t;
      if (t.length > 20) td.title = t;
      if (sticky[k]) GridKit.stick(td, sticky[k]);
      if (fm) GridKit.paint(td, fm[0], fm[1].get(c), sel);
      tr.append(td);
    });
    frag.append(tr);
  }
  // Keep the alternating stripes on the same rows while scrolling.
  if (first % 2) frag.prepend(Object.assign(document.createElement("div"), { hidden: true }));
  rowsEl.replaceChildren(frag);
}
const scheduleRender = () => { if (!frame) frame = requestAnimationFrame(render); };
grid.addEventListener("scroll", scheduleRender);
window.addEventListener("resize", scheduleRender);

// ---- selection

function rowAt(target) {
  const tr = target.closest && target.closest("#rows .trow");
  return tr ? Number(tr.dataset.pos) : -1;
}

function selectTo(pos, extend, toggle) {
  if (pos < 0 || pos >= view.length) return;
  const multi = select !== "single";
  if (extend && multi && anchor >= 0) {
    if (!toggle) selected.clear();
    const [a, b] = anchor < pos ? [anchor, pos] : [pos, anchor];
    for (let p = a; p <= b; p++) selected.add(view[p]);
  } else if (toggle && multi) {
    const ri = view[pos];
    if (selected.has(ri)) selected.delete(ri); else selected.add(ri);
    anchor = pos;
  } else {
    selected.clear();
    selected.add(view[pos]);
    anchor = pos;
  }
  cursor = pos;
  scrollIntoView(pos);
  updateCount();
  render();
}

function scrollIntoView(pos) {
  const h = rowH(), top = pos * h, headH = head.offsetHeight;
  const viewH = grid.clientHeight - headH;
  if (top < grid.scrollTop) grid.scrollTop = top;
  else if (top + h > grid.scrollTop + viewH) grid.scrollTop = top + h - viewH;
}

// Rows are re-rendered on every mousedown, so the browser's dblclick event
// would not reach them; the click count of mousedown is used instead.
rowsEl.addEventListener("mousedown", (e) => {
  const p = rowAt(e.target);
  if (p < 0 || e.button !== 0) return;
  // The column's index; -1 for the row number.
  const cell = e.target.closest(".cell");
  const column = cell && cell.dataset.c !== undefined ? Number(cell.dataset.c) : -1;
  grid.focus();
  if (e.detail === 2 && !e.shiftKey && !e.ctrlKey && !e.metaKey) {
    e.preventDefault();
    if (dblclick) {
      status("");
      host.send({ type: "dblclick", index: view[p], column });
    } else if (select === "single") {
      ok();
    }
    return;
  }
  selectTo(p, e.shiftKey, e.ctrlKey || e.metaKey);
});

grid.addEventListener("keydown", (e) => {
  const page = Math.max(1, Math.floor((grid.clientHeight - head.offsetHeight) / rowH()) - 1);
  const moves = { ArrowDown: 1, ArrowUp: -1, PageDown: page, PageUp: -page };
  if (e.key in moves || e.key === "Home" || e.key === "End") {
    e.preventDefault();
    if (!view.length) return;
    let p = e.key === "Home" ? 0 : e.key === "End" ? view.length - 1 : Math.max(0, cursor) + moves[e.key];
    p = Math.min(Math.max(p, 0), view.length - 1);
    if (e.ctrlKey && !e.shiftKey) { cursor = p; scrollIntoView(p); render(); return; }
    selectTo(p, e.shiftKey, false);
  } else if (e.key === " " && cursor >= 0) {
    e.preventDefault();
    selectTo(cursor, false, true);
  } else if (e.key === "a" && e.ctrlKey) {
    e.preventDefault();
    if (select === "single") return;
    for (const ri of view) selected.add(ri);
    updateCount();
    render();
  } else if (e.key === "Enter" && select) {
    e.preventDefault();
    ok();
  }
});

document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") {
    if (document.activeElement === $("filter") && $("filter").value) { $("filter").value = ""; applyFilter(); return; }
    e.preventDefault();
    host.send({ type: "close" });
  } else if ((e.key === "f" || e.key === "F") && e.ctrlKey) {
    e.preventDefault();
    $("filter").focus();
    $("filter").select();
  } else if ((e.key === "c" || e.key === "C") && e.ctrlKey && !/^(INPUT|SELECT|TEXTAREA)$/.test(document.activeElement.tagName)) {
    e.preventDefault();
    copy();
  }
});

// ---- filter

let filterTimer = 0;
function applyFilter() {
  filterTokens = $("filter").value.normalize("NFKC").toLowerCase().split(/\s+/).filter(Boolean);
  update();
}
$("filter").addEventListener("input", () => {
  clearTimeout(filterTimer);
  filterTimer = setTimeout(applyFilter, rows.length > 50000 ? 250 : 80);
});
$("filter").addEventListener("keydown", (e) => {
  if (e.key === "ArrowDown" || e.key === "Enter") {
    e.preventDefault();
    clearTimeout(filterTimer);
    applyFilter();
    grid.focus();
    if (view.length && cursor < 0) selectTo(0, false, false);
  }
});

// ---- copy / save / buttons

function tsvCell(s) {
  return /[\t\n\r"]/.test(s) ? '"' + s.replace(/"/g, '""') + '"' : s;
}

function chosenRows() {
  if (!selected.size) return view;
  return view.filter((ri) => selected.has(ri));
}

async function copy() {
  const list = chosenRows();
  const shown = order.map((o) => o.c);
  const lines = [shown.map((c) => tsvCell(columns[c].label)).join("\t")];
  for (const ri of list) lines.push(shown.map((c) => tsvCell(cellText(rows[ri][c]))).join("\t"));
  const text = lines.join("\r\n") + "\r\n";
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const ta = Object.assign(document.createElement("textarea"), { value: text });
    document.body.append(ta);
    ta.select();
    document.execCommand("copy");
    ta.remove();
    grid.focus();
  }
  status(fmt(texts.copied, list.length));
}

function isIdentity() {
  if (view.length !== rows.length) return false;
  for (let i = 0; i < view.length; i++) if (view[i] !== i) return false;
  return true;
}

function save(format) {
  status("");
  // The columns shown, in their order (null: all of them as they are).
  const shown = order.map((o) => o.c);
  const all = shown.length === columns.length && shown.every((c, i) => c === i);
  host.send({ type: "save", format, indices: isIdentity() ? null : view, columns: all ? null : shown });
}

function ok() {
  if (!select || !selected.size) return;
  host.send({ type: "ok", indices: Array.from(selected).sort((a, b) => a - b) });
}

function status(text, error) {
  $("status").textContent = text || "";
  $("status").className = "status" + (error ? " error-text" : "");
}

$("copy").onclick = copy;
$("clear-filters").onclick = () => { filters.clear(); buildHead(); update(); };
$("save-csv").onclick = () => save("csv");
$("save-xlsx").onclick = () => save("xlsx");
$("ok").onclick = ok;
$("cancel").onclick = () => host.send({ type: "close" });

host.send({ type: "ready" });
