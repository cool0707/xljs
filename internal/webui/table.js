// The table of ui.table: virtual scrolling, sort, filter, column resize,
// selection, copy as TSV and save as CSV/Excel (done by the script).
"use strict";

const $ = (id) => document.getElementById(id);
const ROW_H = 26;
const OVERSCAN = 8;

let def, text;
let cols = [], rows = [];
let kinds = [];          // per column: "num" | "bool" | "text"
let widths = [];         // per column, px
let indexWidth = 56;
let view = [];           // row indices in display order
let lower = [];          // cached lower-case row text for filtering
let sortCol = -1, sortDir = 1;
let selected = new Set();
let anchor = -1;         // view position where a shift-selection starts
let cursor = -1;         // view position of the keyboard cursor
let rendered = { first: -1, last: -1 };
const collator = new Intl.Collator("ja", { numeric: true, sensitivity: "base" });

function fmt(tpl, ...args) { return String(tpl || "").replace(/\{(\d+)\}/g, (_, i) => args[i]); }
function esc(s) {
  return String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));
}
function cellText(v) {
  if (v === null || v === undefined) return "";
  if (typeof v === "boolean") return v ? "TRUE" : "FALSE";
  return String(v);
}

let toastTimer = null;
function toast(msg) {
  const t = $("toast");
  t.textContent = msg;
  t.classList.add("show");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => t.classList.remove("show"), 2200);
}

// ---------------------------------------------------------------- layout

function measure() {
  const canvas = document.createElement("canvas").getContext("2d");
  const cs = getComputedStyle(document.body);
  canvas.font = `${cs.fontSize} ${cs.fontFamily}`;
  // Measure a spread of rows (sorting can bring any of them to the top).
  const sample = [];
  const step = Math.max(1, Math.floor(rows.length / 2000));
  for (let i = 0; i < rows.length; i += step) sample.push(rows[i]);
  widths = cols.map((c, j) => {
    canvas.font = `600 ${cs.fontSize} ${cs.fontFamily}`;
    let w = canvas.measureText(c).width + 36;
    canvas.font = `${cs.fontSize} ${cs.fontFamily}`;
    for (const r of sample) w = Math.max(w, canvas.measureText(cellText(r[j])).width + 24);
    // Numbers must never be cut off: find the longest one in all rows.
    let longest = "";
    for (const r of rows) {
      const v = r[j];
      if (typeof v === "number") {
        const t = String(v);
        if (t.length > longest.length) longest = t;
      }
    }
    if (longest) w = Math.max(w, canvas.measureText(longest.replace(/\d/g, "0")).width + 28);
    return Math.round(Math.min(Math.max(w, 56), 420));
  });
  indexWidth = Math.round(Math.max(48, canvas.measureText(String(rows.length)).width + 24));
  kinds = cols.map((c, j) => {
    let num = 0, bool = 0, other = 0;
    for (const r of sample) {
      const v = r[j];
      if (v === null || v === undefined || v === "") continue;
      if (typeof v === "number") num++;
      else if (typeof v === "boolean") bool++;
      else other++;
    }
    return other === 0 && num > 0 && bool === 0 ? "num" : other === 0 && bool > 0 && num === 0 ? "bool" : "text";
  });
}

function totalWidth() { return indexWidth + widths.reduce((a, b) => a + b, 0); }

function renderHead() {
  const head = $("head");
  let html = `<div class="cell index" style="width:${indexWidth}px">#</div>`;
  cols.forEach((c, j) => {
    const mark = j === sortCol ? `<span class="sort">${sortDir > 0 ? "▲" : "▼"}</span>` : "";
    html += `<div class="cell ${kinds[j] === "num" ? "num" : ""}" data-col="${j}" style="width:${widths[j]}px" title="${esc(c)}">${esc(c)}${mark}<span class="resize" data-col="${j}"></span></div>`;
  });
  head.innerHTML = html;
  head.style.width = totalWidth() + "px";
}

function renderRows(force) {
  const sc = $("scroller");
  const headH = $("head").offsetHeight;
  const top = Math.max(0, sc.scrollTop - headH);
  const first = Math.max(0, Math.floor(top / ROW_H) - OVERSCAN);
  const last = Math.min(view.length, Math.ceil((top + sc.clientHeight) / ROW_H) + OVERSCAN);
  if (!force && first === rendered.first && last === rendered.last) return;
  rendered = { first, last };
  let html = "";
  for (let p = first; p < last; p++) {
    const i = view[p];
    const r = rows[i];
    let cls = "row";
    if (selected.has(i)) cls += " selected";
    if (p === cursor) cls += " cursor";
    html += `<div class="${cls}" data-pos="${p}"><div class="cell index" style="width:${indexWidth}px">${i + 1}</div>`;
    for (let j = 0; j < cols.length; j++) {
      const v = r[j];
      const k = v === null || v === undefined ? "null" : typeof v === "number" ? "num" : typeof v === "boolean" ? "bool" : "";
      const t = cellText(v);
      html += `<div class="cell ${k}" style="width:${widths[j]}px"${t.length > 20 ? ` title="${esc(t)}"` : ""}>${esc(t)}</div>`;
    }
    html += "</div>";
  }
  const box = $("rows");
  box.innerHTML = html;
  box.style.transform = `translateY(${first * ROW_H}px)`;
  box.style.width = totalWidth() + "px";
  $("body").style.height = view.length * ROW_H + "px";
  $("body").style.width = totalWidth() + "px";
}

function renderStatus() {
  let s = view.length === rows.length ? fmt(text.count, rows.length) : fmt(text.filtered, view.length, rows.length);
  if (selected.size) s += " ・ " + fmt(text.selected, selected.size);
  $("status").textContent = s;
  $("empty").hidden = view.length > 0;
  if (def.selectable) $("ok").disabled = def.selectable === "single" ? selected.size !== 1 : selected.size === 0;
}

function refresh() {
  renderRows(true);
  renderStatus();
}

// ---------------------------------------------------------------- filter & sort

function compare(a, b) {
  const an = a === null || a === undefined || a === "", bn = b === null || b === undefined || b === "";
  if (an || bn) return an && bn ? 0 : an ? 1 : -1; // empty cells last in both directions
  const at = typeof a, bt = typeof b;
  if (at === "number" && bt === "number") return (a - b) * sortDir;
  if (at === "number") return -1 * sortDir;
  if (bt === "number") return 1 * sortDir;
  return collator.compare(cellText(a), cellText(b)) * sortDir;
}

function applyView() {
  const terms = $("filter").value.toLowerCase().split(/\s+/).filter(Boolean);
  let v = [];
  if (terms.length) {
    for (let i = 0; i < rows.length; i++) {
      if (lower[i] === undefined) lower[i] = rows[i].map(cellText).join("\t").toLowerCase();
      if (terms.every((t) => lower[i].includes(t))) v.push(i);
    }
  } else {
    v = rows.map((_, i) => i);
  }
  if (sortCol >= 0) {
    const j = sortCol;
    v.sort((x, y) => compare(rows[x][j], rows[y][j]) || x - y);
  }
  view = v;
  cursor = Math.min(cursor, view.length - 1);
  refresh();
}

// ---------------------------------------------------------------- selection

function setCursor(p, extend, toggle) {
  if (!view.length) return;
  p = Math.max(0, Math.min(view.length - 1, p));
  const multi = def.selectable !== "single";
  if (extend && multi && anchor >= 0) {
    selected = new Set();
    const [a, b] = anchor < p ? [anchor, p] : [p, anchor];
    for (let q = a; q <= b; q++) selected.add(view[q]);
  } else if (toggle && multi) {
    const i = view[p];
    if (selected.has(i)) selected.delete(i);
    else selected.add(i);
    anchor = p;
  } else {
    selected = new Set([view[p]]);
    anchor = p;
  }
  cursor = p;
  scrollIntoView(p);
  refresh();
}

function scrollIntoView(p) {
  const sc = $("scroller");
  const headH = $("head").offsetHeight;
  const y = p * ROW_H;
  if (y < sc.scrollTop) sc.scrollTop = y;
  else if (y + ROW_H > sc.scrollTop + sc.clientHeight - headH) sc.scrollTop = y + ROW_H - sc.clientHeight + headH;
}

function selectedInView() {
  return selected.size ? view.filter((i) => selected.has(i)) : view.slice();
}

// ---------------------------------------------------------------- copy & save

function tsvField(s) { return /[\t\r\n"]/.test(s) ? '"' + s.replace(/"/g, '""') + '"' : s; }

async function copy() {
  const idx = selectedInView();
  const lines = [cols.map(tsvField).join("\t")];
  for (const i of idx) lines.push(rows[i].map((v) => tsvField(cellText(v))).join("\t"));
  const tsv = lines.join("\r\n") + "\r\n";
  try {
    await navigator.clipboard.writeText(tsv);
  } catch (e) {
    const ta = document.createElement("textarea");
    ta.value = tsv;
    document.body.append(ta);
    ta.select();
    document.execCommand("copy");
    ta.remove();
  }
  toast(fmt(text.copied, idx.length));
}

async function save(format) {
  try {
    const name = await xljs.invoke("save", format, view.slice());
    if (name) toast(fmt(text.saved, name));
  } catch (e) {
    toast(e.message);
  }
}

function ok() {
  if ($("ok").hidden || $("ok").disabled) return;
  xljs.invoke("ok", selectedInView());
}

// ---------------------------------------------------------------- events

function bind() {
  const sc = $("scroller");
  let frame = 0;
  sc.addEventListener("scroll", () => {
    if (!frame) frame = requestAnimationFrame(() => { frame = 0; renderRows(false); });
  });
  window.addEventListener("resize", () => renderRows(true));

  let filterTimer = null;
  $("filter").addEventListener("input", () => {
    clearTimeout(filterTimer);
    filterTimer = setTimeout(applyView, rows.length > 50000 ? 250 : 80);
  });
  $("filter").addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown" || e.key === "Enter") { e.preventDefault(); sc.focus(); if (cursor < 0) setCursor(0); }
  });

  // Sorting and column resizing.
  $("head").addEventListener("mousedown", (e) => {
    const handle = e.target.closest(".resize");
    if (!handle) return;
    e.preventDefault();
    e.stopPropagation();
    const j = Number(handle.dataset.col);
    const startX = e.clientX, startW = widths[j];
    const move = (ev) => {
      widths[j] = Math.max(40, startW + ev.clientX - startX);
      renderHead();
      renderRows(true);
    };
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      setTimeout(() => { resizing = false; }, 0);
    };
    resizing = true;
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  });
  let resizing = false;
  $("head").addEventListener("click", (e) => {
    if (resizing || e.target.closest(".resize")) return;
    const cell = e.target.closest(".cell[data-col]");
    if (!cell) return;
    const j = Number(cell.dataset.col);
    if (sortCol !== j) { sortCol = j; sortDir = 1; }
    else if (sortDir > 0) sortDir = -1;
    else sortCol = -1;
    renderHead();
    applyView();
  });

  $("rows").addEventListener("mousedown", (e) => {
    const row = e.target.closest(".row");
    if (!row) return;
    sc.focus();
    setCursor(Number(row.dataset.pos), e.shiftKey, e.ctrlKey || e.metaKey);
  });
  $("rows").addEventListener("dblclick", (e) => {
    if (e.target.closest(".row") && def.selectable) ok();
  });

  sc.addEventListener("keydown", (e) => {
    const page = Math.max(1, Math.floor(sc.clientHeight / ROW_H) - 2);
    const move = (p) => { e.preventDefault(); setCursor(p, e.shiftKey, false); };
    switch (e.key) {
      case "ArrowDown": return move(cursor + 1);
      case "ArrowUp": return move(cursor < 0 ? 0 : cursor - 1);
      case "PageDown": return move(cursor + page);
      case "PageUp": return move(cursor - page);
      case "Home": if (e.ctrlKey) return move(0); break;
      case "End": if (e.ctrlKey) return move(view.length - 1); break;
      case " ":
        if (cursor >= 0) { e.preventDefault(); setCursor(cursor, false, true); }
        return;
      case "Enter": e.preventDefault(); return ok();
    }
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a" && def.selectable !== "single") {
      e.preventDefault();
      selected = new Set(view);
      refresh();
    }
  });
  document.addEventListener("keydown", (e) => {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "c" && document.activeElement !== $("filter")) {
      e.preventDefault();
      copy();
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "f") {
      e.preventDefault();
      $("filter").focus();
      $("filter").select();
    } else if (e.key === "Escape") {
      if (document.activeElement === $("filter") && $("filter").value) {
        $("filter").value = "";
        applyView();
      } else {
        xljs.close();
      }
    }
  });

  $("copy").addEventListener("click", copy);
  $("save-csv").addEventListener("click", () => save("csv"));
  $("save-xlsx").addEventListener("click", () => save("xlsx"));
  $("ok").addEventListener("click", ok);
  $("cancel").addEventListener("click", () => xljs.close());
  $("close").addEventListener("click", () => xljs.close());
}

async function start() {
  def = await xljs.invoke("init");
  text = def.text || {};
  cols = def.columns;
  rows = def.rows;
  document.title = def.title;
  $("filter").placeholder = text.filter;
  $("copy").textContent = text.copy;
  $("save-csv").textContent = text.saveCSV;
  $("save-xlsx").textContent = text.saveExcel;
  $("empty").textContent = "—";
  if (def.selectable) {
    $("ok").textContent = text.ok;
    $("cancel").textContent = text.cancel;
    $("ok").hidden = false;
    $("cancel").hidden = false;
  } else {
    $("close").textContent = text.close;
    $("close").hidden = false;
  }
  measure();
  renderHead();
  bind();
  applyView();
  $("scroller").focus();
}

start().catch((e) => {
  document.body.textContent = String(e && e.message ? e.message : e);
});
