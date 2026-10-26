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
let state = "input"; // input | busy (waiting for the script) | running | stopping
let ran = false;
const controls = new Map(); // name -> { field, row, get(), set(value, append), focus() }

host.on((m) => {
  switch (m.type) {
    case "init": init(m); break;
    case "errors": setState("input"); showErrors(m.errors || {}, m.error || ""); break;
    case "picked": { const c = controls.get(m.field); if (c) c.set(m.value, m.append); break; }
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
  fields.forEach((f, i) => {
    const id = "f" + i;
    const c = buildControl(f, id);
    const label = el("label", { className: "lbl", htmlFor: id }, f.type === "checkbox" ? "" : f.label);
    if (f.required && f.type !== "checkbox") label.append(el("span", { className: "req", textContent: "*" }));
    const ctl = el("div", { className: "ctl" }, c.node);
    if (f.help) ctl.append(el("div", { className: "help", textContent: f.help }));
    c.err = ctl.appendChild(el("div", { className: "err", hidden: true }));
    c.field = f;
    c.ctl = ctl;
    box.append(label, ctl);
    controls.set(f.name, c);
    c.set(f.value);
  });
  setState("input");
  const first = fields.find((f) => f.type !== "checkbox" && f.type !== "radio");
  if (first) controls.get(first.name).focus();
  host.send({ type: "shown" });
}

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
      const b = el("button", { type: "button", className: "small", textContent: texts.browse });
      b.onclick = () => pick(f, i.value);
      const node = el("div", { className: "row" }, i, b);
      if (f.type !== "saveFile") dropTarget(node, f);
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
    default: {
      const i = input("text", f, id);
      return { node: i, inputs: [i], get: () => i.value, set: (v) => { i.value = v ?? ""; }, focus: () => i.focus() };
    }
  }
}

function pick(f, current) {
  if (state !== "input") return;
  host.send({ type: "pick", field: f.name, value: current });
}

function dropTarget(node, f) {
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
    if (state === "input" && files && files.length) host.send({ type: "drop", field: f.name }, files);
  });
}

function values() {
  const v = {};
  for (const [name, c] of controls) v[name] = c.get();
  return v;
}

function showErrors(errors, general) {
  let first = null;
  for (const [name, c] of controls) {
    const msg = errors[name] || "";
    c.err.textContent = msg;
    c.err.hidden = !msg;
    c.ctl.classList.toggle("invalid", !!msg);
    if (msg && !first) first = c;
  }
  $("form-error").textContent = general;
  $("form-error").hidden = !general;
  if (first) first.focus();
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
