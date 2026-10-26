"use strict";
// Built-in form page (ui.form). The script sends "init" with the fields,
// validates "submit"ted values and, with onSubmit, reports the run.
const $ = (id) => document.getElementById(id);
const el = (tag, props = {}, ...children) => {
  const e = Object.assign(document.createElement(tag), props);
  for (const c of children) if (c !== null && c !== undefined) e.append(c);
  return e;
};

let texts = {};
let fields = [];
let tabButtons = []; // one per tab; fields name their tab by index (-1 = above the tabs)
let state = "input"; // input | busy (waiting for the script) | running | stopping
let ran = false;
const controls = new Map(); // name -> { field, row, get(), set(value, append), focus() }

host.on((m) => {
  switch (m.type) {
    case "init": init(m); break;
    case "errors": setState("input"); showErrors(m.errors || {}, m.error || ""); break;
    case "picked": { const c = controls.get(m.field); if (c) c.set(m.value, m.append); break; }
    case "table": { const c = controls.get(m.field); if (c && c.setTable) c.setTable(m); break; }
    case "running": startRun(m); break;
    case "progress": progress(m); break;
    case "log": appendLog(m.lines, m.level); break;
    case "done": finishRun(m); break;
  }
});

function init(m) {
  texts = m.texts || {};
  fields = m.fields || [];
  if (m.description) {
    $("description").textContent = m.description;
    $("description").hidden = false;
  }
  $("submit").textContent = m.submit || texts.ok;
  $("cancel").textContent = texts.cancel;
  $("open").textContent = texts.openFolder;
  const box = $("fields");
  box.textContent = "";
  controls.clear();
  const panels = buildTabs(m.tabs || []);
  fields.forEach((f, i) => {
    const id = "f" + i;
    const c = buildControl(f, id);
    const wide = f.type === "table" ? " wide" : "";
    const label = el("label", { className: "lbl" + wide, htmlFor: id }, f.type === "checkbox" ? "" : f.label);
    if (f.required && f.type !== "checkbox") label.append(el("span", { className: "req", textContent: "*" }));
    const ctl = el("div", { className: "ctl" + wide }, c.node);
    if (f.help) ctl.append(el("div", { className: "help", textContent: f.help }));
    c.err = ctl.appendChild(el("div", { className: "err", hidden: true }));
    c.field = f;
    c.ctl = ctl;
    (f.tab >= 0 && panels[f.tab] ? panels[f.tab] : box).append(label, ctl);
    controls.set(f.name, c);
    c.set(f.value);
  });
  setState("input");
  if (tabButtons.length) showTab(0, false);
  const first = fields.find((f) => f.type !== "checkbox" && f.type !== "radio" && f.type !== "table" && !(f.tab > 0));
  if (first) controls.get(first.name).focus();
  host.send({ type: "shown" });
}

// ---- tabs

/** Builds the tab bar and one panel per tab (after the fields above the tabs); returns the panels. */
function buildTabs(labels) {
  $("tabs").textContent = "";
  $("panels").textContent = "";
  $("tabs").hidden = labels.length === 0;
  tabButtons = labels.map((label, k) => {
    const b = el("button", { type: "button", className: "tab", textContent: label, id: "tab" + k });
    b.setAttribute("role", "tab");
    b.onclick = () => showTab(k, true);
    $("tabs").append(b);
    return b;
  });
  return labels.map((_, k) => {
    const p = el("div", { className: "fields panel", id: "panel" + k });
    p.setAttribute("role", "tabpanel");
    $("panels").append(p);
    return p;
  });
}

function showTab(k, focus) {
  tabButtons.forEach((b, i) => {
    b.classList.toggle("active", i === k);
    b.setAttribute("aria-selected", String(i === k));
    $("panel" + i).hidden = i !== k;
  });
  if (focus) tabButtons[k].focus();
}

const activeTab = () => tabButtons.findIndex((b) => b.classList.contains("active"));

function input(type, f, id) {
  const i = el("input", { type, id, name: f.name, placeholder: f.placeholder || "" });
  for (const k of ["min", "max", "step"]) if (f[k] !== undefined && f[k] !== null) i[k] = f[k];
  return i;
}

function buildControl(f, id) {
  switch (f.type) {
    case "checkbox": {
      const i = el("input", { type: "checkbox", id, name: f.name });
      return {
        node: el("label", { className: "check" }, i, f.label),
        get: () => i.checked, set: (v) => { i.checked = !!v; }, focus: () => i.focus(), inputs: [i],
      };
    }
    case "radio": {
      const inputs = (f.options || []).map((o, k) => el("input", { type: "radio", name: f.name, value: o.value, id: k ? "" : id }));
      const node = el("div", { className: "choices" },
        ...inputs.map((i, k) => el("label", {}, i, f.options[k].label)));
      return {
        node, inputs,
        get: () => { const c = inputs.find((i) => i.checked); return c ? c.value : ""; },
        set: (v) => { for (const i of inputs) i.checked = i.value === String(v ?? ""); },
        focus: () => (inputs.find((i) => i.checked) || inputs[0])?.focus(),
      };
    }
    case "select": {
      const s = el("select", { id, name: f.name }, ...(f.options || []).map((o) => el("option", { value: o.value, textContent: o.label })));
      return {
        node: s, inputs: [s],
        get: () => s.value,
        set: (v) => { s.value = String(v ?? ""); if (s.selectedIndex < 0 && s.options.length) s.selectedIndex = 0; },
        focus: () => s.focus(),
      };
    }
    case "textarea": {
      const t = el("textarea", { id, name: f.name, rows: f.rows || 4, placeholder: f.placeholder || "" });
      return { node: t, inputs: [t], get: () => t.value, set: (v) => { t.value = v ?? ""; }, focus: () => t.focus() };
    }
    case "file": case "folder": case "saveFile": {
      const i = input("text", f, id);
      i.spellcheck = false;
      if (!f.placeholder && host.canAttach) i.placeholder = f.type === "folder" ? texts.dropFolder : texts.dropHere;
      const b = el("button", { type: "button", className: "small", textContent: texts.browse });
      b.onclick = () => pick(f, i.value);
      const node = el("div", { className: "row" }, i, b);
      dropTarget(node, f, () => i.value.trim());
      return { node, inputs: [i, b], get: () => i.value.trim(), set: (v) => { i.value = v ?? ""; }, focus: () => i.focus() };
    }
    case "files": {
      const t = el("textarea", { id, name: f.name, rows: f.rows || 3, spellcheck: false, placeholder: f.placeholder || (host.canAttach ? texts.dropHere : "") });
      const b = el("button", { type: "button", className: "small", textContent: texts.browse });
      const clear = el("button", { type: "button", className: "small", textContent: texts.clear });
      const lines = () => t.value.split(/\r?\n/).map((s) => s.trim()).filter(Boolean);
      b.onclick = () => pick(f, lines());
      clear.onclick = () => { t.value = ""; t.focus(); };
      const node = el("div", { className: "row" }, t, el("div", { className: "files" }, b, clear));
      dropTarget(node, f);
      return {
        node, inputs: [t, b, clear],
        get: lines,
        set: (v, append) => {
          const add = Array.isArray(v) ? v : v ? [String(v)] : [];
          const all = append ? lines().concat(add.filter((p) => !lines().includes(p))) : add;
          t.value = all.join("\n");
        },
        focus: () => t.focus(),
      };
    }
    case "number": case "date": case "month": case "time": case "password": {
      const i = input(f.type, f, id);
      return { node: i, inputs: [i], get: () => i.value, set: (v) => { i.value = v ?? ""; }, focus: () => i.focus() };
    }
    case "table":
      return tableControl(f, id);
    default: {
      const i = input("text", f, id);
      return { node: i, inputs: [i], get: () => i.value, set: (v) => { i.value = v ?? ""; }, focus: () => i.focus() };
    }
  }
}

// ---- table fields: sortable; rows are chosen by clicking (selectable) and the value is their indices.

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });
function compareCells(a, b) {
  const ea = a === null || a === "", eb = b === null || b === "";
  if (ea || eb) return ea === eb ? 0 : ea ? 1 : -1;
  if (typeof a === "number" && typeof b === "number") return a - b;
  return collator.compare(String(a), String(b));
}

function tableControl(f, id) {
  let cols = f.table.columns, rows = f.table.rows, select = f.table.select;
  let order = [], sortCol = -1, sortDir = 1, boxes = [];
  const selected = new Set();
  const head = el("thead");
  const body = el("tbody");
  const all = el("input", { type: "checkbox", tabIndex: -1 });
  const wrap = el("div", { className: "ftable", id, tabIndex: 0 }, el("table", {}, head, body));
  wrap.style.maxHeight = (30 + 27 * (f.rows || 8)) + "px";

  function header() {
    head.textContent = "";
    const tr = el("tr");
    if (select) tr.append(el("th", { className: "sel" }, select === "multiple" ? all : ""));
    cols.forEach((label, k) => {
      const th = el("th", { textContent: label, title: label });
      if (k === sortCol) th.dataset.sort = sortDir > 0 ? "asc" : "desc";
      th.onclick = () => {
        sortDir = k === sortCol ? -sortDir : 1;
        sortCol = k;
        sort();
        header();
        render();
      };
      tr.append(th);
    });
    head.append(tr);
  }

  function sort() {
    order = rows.map((_, i) => i);
    if (sortCol >= 0) order.sort((a, b) => sortDir * compareCells(rows[a][sortCol], rows[b][sortCol]) || a - b);
  }

  function render() {
    body.textContent = "";
    boxes = [];
    if (!rows.length) {
      body.append(el("tr", { className: "empty" }, el("td", { colSpan: cols.length + (select ? 1 : 0), textContent: texts.noRows })));
    }
    for (const i of order) {
      const tr = el("tr");
      tr.dataset.i = i;
      if (select) {
        const box = el("input", { type: select === "multiple" ? "checkbox" : "radio", name: id + "-sel", tabIndex: -1 });
        box.disabled = state !== "input";
        boxes.push(box);
        tr.append(el("td", { className: "sel" }, box));
      }
      rows[i].forEach((v) => tr.append(el("td", { className: "cell" + (typeof v === "number" ? " num" : ""), textContent: v === null ? "" : String(v) })));
      body.append(tr);
    }
    mark();
  }

  function mark() {
    body.querySelectorAll("tr[data-i]").forEach((tr, k) => {
      const on = selected.has(Number(tr.dataset.i));
      tr.classList.toggle("selected", on);
      if (boxes[k]) boxes[k].checked = on;
    });
    all.checked = rows.length > 0 && selected.size === rows.length;
    all.indeterminate = selected.size > 0 && selected.size < rows.length;
  }

  body.addEventListener("click", (e) => {
    const tr = e.target.closest("tr[data-i]");
    if (!tr || !select || state !== "input") return;
    const i = Number(tr.dataset.i);
    if (select === "single") {
      selected.clear();
      selected.add(i);
    } else if (selected.has(i)) selected.delete(i);
    else selected.add(i);
    mark();
  });
  all.onclick = (e) => {
    e.stopPropagation();
    if (state !== "input") { e.preventDefault(); return; }
    if (selected.size === rows.length) selected.clear();
    else rows.forEach((_, i) => selected.add(i));
    mark();
  };

  sort();
  header();
  render();
  return {
    node: wrap,
    get inputs() { return select ? [all, ...boxes] : []; },
    get: () => Array.from(selected).sort((a, b) => a - b),
    set: (v) => {
      selected.clear();
      for (const i of Array.isArray(v) ? v : []) if (i >= 0 && i < rows.length) selected.add(i);
      mark();
    },
    setTable: (m) => {
      cols = m.columns; rows = m.rows; select = m.select;
      selected.clear();
      sortCol = -1;
      sort();
      header();
      render();
    },
    focus: () => wrap.focus(),
  };
}

function pick(f, current) {
  if (state !== "input") return;
  host.send({ type: "pick", field: f.name, value: current });
}

/** Lets files be dropped on node; current() is the field's value (a save field keeps its file name). */
function dropTarget(node, f, current) {
  if (!host.canAttach) return;
  node.classList.add("drop");
  let depth = 0;
  node.addEventListener("dragenter", (e) => { e.preventDefault(); depth++; node.classList.add("over"); });
  node.addEventListener("dragleave", () => { if (--depth <= 0) { depth = 0; node.classList.remove("over"); } });
  node.addEventListener("dragover", (e) => { e.preventDefault(); e.dataTransfer.dropEffect = state === "input" ? "copy" : "none"; });
  node.addEventListener("drop", (e) => {
    e.preventDefault();
    depth = 0;
    node.classList.remove("over");
    const files = e.dataTransfer && e.dataTransfer.files;
    if (state === "input" && files && files.length) host.send({ type: "drop", field: f.name, value: current ? current() : "" }, files);
  });
}

function values() {
  const v = {};
  for (const [name, c] of controls) v[name] = c.get();
  return v;
}

function showErrors(errors, general) {
  let first = null;
  tabButtons.forEach((b) => b.classList.remove("has-error"));
  for (const [name, c] of controls) {
    const msg = errors[name] || "";
    c.err.textContent = msg;
    c.err.hidden = !msg;
    c.ctl.classList.toggle("invalid", !!msg);
    if (msg && c.field.tab >= 0) tabButtons[c.field.tab].classList.add("has-error");
    if (msg && !first) first = c;
  }
  $("form-error").textContent = general;
  $("form-error").hidden = !general;
  if (first) {
    if (first.field.tab >= 0) showTab(first.field.tab, false);
    first.focus();
  }
}

function setState(s) {
  state = s;
  const editing = s === "input";
  for (const c of controls.values()) for (const i of c.inputs) i.disabled = !editing;
  $("submit").disabled = !editing;
  $("cancel").disabled = s === "stopping";
  $("cancel").textContent = s === "running" ? texts.abort : s === "stopping" ? texts.aborting : ran ? texts.close : texts.cancel;
}

function submit() {
  if (state !== "input") return;
  setState("busy");
  host.send({ type: "submit", values: values() });
}

$("form").addEventListener("submit", (e) => { e.preventDefault(); submit(); });
$("submit").onclick = submit;
$("cancel").onclick = cancel;
$("open").onclick = () => host.send({ type: "open" });
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") { e.preventDefault(); cancel(); }
  // Ctrl+Tab / Ctrl+PageDown and back switch tabs.
  const step = e.ctrlKey && (e.key === "Tab" ? (e.shiftKey ? -1 : 1) : e.key === "PageDown" ? 1 : e.key === "PageUp" ? -1 : 0);
  if (step && tabButtons.length) {
    e.preventDefault();
    showTab((activeTab() + step + tabButtons.length) % tabButtons.length, true);
  }
});

function cancel() {
  if (state === "running") {
    setState("stopping");
    host.send({ type: "cancel" });
  } else if (state === "input" || state === "busy") {
    host.send({ type: "close" });
  }
}

// ---- run (onSubmit)

function startRun() {
  showErrors({}, "");
  ran = true;
  $("run").hidden = false;
  $("open").hidden = true;
  $("status").textContent = texts.running;
  $("status").className = "status";
  $("run-message").textContent = "";
  $("run-count").textContent = "";
  $("run-progress").removeAttribute("value");
  $("log").textContent = "";
  $("log").hidden = true;
  setState("running");
  $("run").scrollIntoView({ block: "nearest" });
  $("cancel").focus();
}

function progress(m) {
  const bar = $("run-progress");
  if (m.total > 0) {
    bar.max = m.total;
    bar.value = Math.min(m.done, m.total);
    $("run-count").textContent = `${m.done} / ${m.total}`;
  } else {
    bar.removeAttribute("value");
    $("run-count").textContent = m.done > 0 ? String(m.done) : "";
  }
  if (m.message !== undefined) $("run-message").textContent = m.message;
}

function appendLog(lines, level) {
  const log = $("log");
  log.hidden = false;
  const atEnd = log.scrollTop + log.clientHeight >= log.scrollHeight - 4;
  const text = lines.join("\n") + "\n";
  if (level === "error") log.append(el("span", { className: "error", textContent: text }));
  else log.append(text);
  // Keep the log from growing without bound.
  while (log.childNodes.length > 5000) log.firstChild.remove();
  if (atEnd) log.scrollTop = log.scrollHeight;
}

function finishRun(m) {
  const bar = $("run-progress");
  if (m.ok && !m.cancelled) {
    if (!bar.hasAttribute("value")) { bar.max = 1; bar.value = 1; }
    else bar.value = bar.max;
  }
  if (m.error) appendLog([m.error], "error");
  const status = $("status");
  status.textContent = m.error ? texts.failed : m.cancelled ? texts.aborted : texts.done;
  status.className = "status " + (m.error ? "error" : m.cancelled ? "" : "ok");
  $("open").hidden = !m.output;
  setState("input");
  (m.output ? $("open") : $("cancel")).focus();
}

host.send({ type: "ready" });
