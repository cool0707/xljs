// The form of ui.form. The script sends the field definitions ("init"),
// opens pickers ("pick"), turns dropped files into paths ("drop"),
// validates and runs the submission ("submit"); while onSubmit runs it
// sends "progress", "log" and "finished" events.
"use strict";

const $ = (id) => document.getElementById(id);
let def = null;
let text = {};
const controls = new Map(); // name -> { get(), set(v), focus() }

function el(tag, attrs, ...children) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v === undefined || v === null || v === false) continue;
    if (k === "class") e.className = v;
    else if (k === "text") e.textContent = v;
    else if (k in e && typeof e[k] !== "function" && k !== "list") e[k] = v;
    else e.setAttribute(k, v === true ? "" : v);
  }
  for (const c of children) if (c !== null && c !== undefined) e.append(c);
  return e;
}

function labelFor(f, id) {
  const l = el("label", { class: "label", for: id, text: f.label });
  if (f.required) l.append(el("span", { class: "required", text: text.required || "*" }));
  return l;
}

function buildField(f, value, index) {
  const id = "f" + index;
  const wrap = el("div", { class: "field", "data-name": f.name });
  let ctl;
  switch (f.type) {
    case "checkbox": {
      wrap.classList.add("check");
      const input = el("input", { type: "checkbox", id, checked: !!value });
      const label = el("label", { for: id }, input, el("span", { text: f.label }));
      if (f.required) label.append(el("span", { class: "required", text: text.required || "*" }));
      wrap.append(label);
      ctl = { get: () => input.checked, set: (v) => { input.checked = !!v; }, focus: () => input.focus() };
      break;
    }
    case "radio": {
      wrap.append(el("span", { class: "label", text: f.label }, f.required ? el("span", { class: "required", text: text.required || "*" }) : null));
      const box = el("div", { class: "radios", role: "radiogroup" });
      const inputs = f.options.map((o, i) => {
        const input = el("input", { type: "radio", name: id, value: o, id: id + "_" + i, checked: o === value });
        box.append(el("label", { for: id + "_" + i }, input, el("span", { text: o })));
        return input;
      });
      wrap.append(box);
      ctl = {
        get: () => (inputs.find((i) => i.checked) || { value: "" }).value,
        set: (v) => inputs.forEach((i) => { i.checked = i.value === v; }),
        focus: () => (inputs.find((i) => i.checked) || inputs[0]).focus(),
      };
      break;
    }
    case "select": {
      wrap.append(labelFor(f, id));
      const sel = el("select", { id });
      if (!f.required || !f.options.includes(value)) sel.append(el("option", { value: "", text: "" }));
      for (const o of f.options) sel.append(el("option", { value: o, text: o, selected: o === value }));
      wrap.append(sel);
      ctl = { get: () => sel.value, set: (v) => { sel.value = v; }, focus: () => sel.focus() };
      break;
    }
    case "textarea": {
      wrap.append(labelFor(f, id));
      const ta = el("textarea", { id, rows: f.rows || 4, placeholder: f.placeholder, value: value || "" });
      wrap.append(ta);
      ctl = { get: () => ta.value, set: (v) => { ta.value = v; }, focus: () => ta.focus() };
      break;
    }
    case "files": {
      wrap.append(labelFor(f, id));
      wrap.classList.add("droppable");
      let paths = Array.isArray(value) ? value.slice() : [];
      const list = el("div", { class: "file-list", id, tabindex: 0, "data-placeholder": f.placeholder || text.drop });
      const render = () => {
        list.replaceChildren(...paths.map((p) => el("div", { text: p, title: p })));
      };
      const browse = el("button", { type: "button", class: "small", text: text.browse });
      const clear = el("button", { type: "button", class: "small", text: text.clear });
      browse.addEventListener("click", () => pick(f.name));
      clear.addEventListener("click", () => { paths = []; render(); });
      wrap.append(el("div", { class: "path" }, list, browse, clear));
      render();
      ctl = { get: () => paths.slice(), set: (v) => { paths = Array.isArray(v) ? v.slice() : []; render(); }, focus: () => list.focus() };
      break;
    }
    case "file":
    case "folder":
    case "saveFile": {
      wrap.append(labelFor(f, id));
      wrap.classList.add("droppable");
      const input = el("input", { type: "text", id, value: value || "", placeholder: f.placeholder || (f.type === "saveFile" ? "" : text.drop), spellcheck: false });
      const browse = el("button", { type: "button", class: "small", text: text.browse });
      browse.addEventListener("click", () => pick(f.name));
      wrap.append(el("div", { class: "path" }, input, browse));
      ctl = { get: () => input.value.trim().replace(/^"(.*)"$/, "$1"), set: (v) => { input.value = v || ""; }, focus: () => input.focus() };
      break;
    }
    default: {
      wrap.append(labelFor(f, id));
      const input = el("input", { type: f.type, id, value: value === undefined ? "" : value, placeholder: f.placeholder, min: f.min, max: f.max, step: f.step, spellcheck: false });
      wrap.append(input);
      ctl = { get: () => input.value, set: (v) => { input.value = v; }, focus: () => input.focus() };
    }
  }
  if (f.help) wrap.append(el("p", { class: "help", text: f.help }));
  wrap.append(el("p", { class: "field-error", hidden: true }));
  if (wrap.classList.contains("droppable")) enableDrop(wrap, f.name);
  controls.set(f.name, ctl);
  return wrap;
}

async function pick(name) {
  try {
    const v = await xljs.invoke("pick", name);
    if (v !== null && v !== undefined) controls.get(name).set(v);
  } catch (e) {
    showFormError(e.message);
  }
}

function enableDrop(wrap, name) {
  let depth = 0;
  wrap.addEventListener("dragenter", (e) => { e.preventDefault(); depth++; wrap.classList.add("dragover"); });
  wrap.addEventListener("dragleave", () => { if (--depth <= 0) { depth = 0; wrap.classList.remove("dragover"); } });
  wrap.addEventListener("dragover", (e) => { e.preventDefault(); e.dataTransfer.dropEffect = "copy"; });
  wrap.addEventListener("drop", async (e) => {
    e.preventDefault();
    e.stopPropagation();
    depth = 0;
    wrap.classList.remove("dragover");
    const files = e.dataTransfer.files;
    if (!files || !files.length) return;
    try {
      const v = await xljs.invoke("drop", name, xljs.files(files));
      if (v === null || v === undefined) showFormError(text.noDrop);
      else controls.get(name).set(v);
    } catch (err) {
      showFormError(err.message);
    }
  });
}

// Dropping a file anywhere else must not navigate away from the form.
window.addEventListener("dragover", (e) => e.preventDefault());
window.addEventListener("drop", (e) => e.preventDefault());

function values() {
  const v = {};
  for (const [name, c] of controls) v[name] = c.get();
  return v;
}

function showFormError(msg) {
  const p = $("form-error");
  p.textContent = msg || "";
  p.hidden = !msg;
}

function showErrors(errors) {
  let first = null;
  for (const wrap of document.querySelectorAll(".field")) {
    const msg = errors[wrap.dataset.name];
    const p = wrap.querySelector(".field-error");
    p.textContent = msg || "";
    p.hidden = !msg;
    wrap.classList.toggle("invalid", !!msg);
    if (msg && !first) first = wrap.dataset.name;
  }
  showFormError(errors[""]);
  if (first) controls.get(first).focus();
}

function setBusy(busy) {
  $("submit").disabled = busy;
  $("cancel").disabled = busy;
}

async function submit() {
  setBusy(true);
  try {
    const r = await xljs.invoke("submit", values());
    if (r && r.errors && Object.keys(r.errors).length) {
      showErrors(r.errors);
      setBusy(false);
      return;
    }
    showErrors({});
    if (def.run) startRun();
  } catch (e) {
    showFormError(e.message);
    setBusy(false);
  }
}

// ---------------------------------------------------------------- run view

let output = null;

function startRun() {
  $("form").hidden = true;
  $("run").hidden = false;
  $("run-status").textContent = text.running;
  $("run-status").className = "run-status";
  $("progress").removeAttribute("value");
  $("progress").removeAttribute("max");
  $("progress-count").textContent = "";
  $("progress-message").textContent = "";
  $("log").textContent = "";
  $("run-message").hidden = true;
  $("run-cancel").hidden = false;
  $("run-cancel").disabled = false;
  $("run-cancel").textContent = text.cancel;
  for (const id of ["back", "open", "close"]) $(id).hidden = true;
  output = null;
}

xljs.on("progress", (p) => {
  const bar = $("progress");
  if (p.total > 0) {
    bar.max = p.total;
    bar.value = Math.min(p.done, p.total);
    $("progress-count").textContent = `${p.done} / ${p.total}`;
  } else {
    bar.removeAttribute("value");
    $("progress-count").textContent = p.done ? String(p.done) : "";
  }
  $("progress-message").textContent = p.message || "";
});

xljs.on("log", (line) => {
  const log = $("log");
  const atEnd = log.scrollTop + log.clientHeight >= log.scrollHeight - 4;
  log.append(line + "\n");
  if (atEnd) log.scrollTop = log.scrollHeight;
});

xljs.on("finished", (r) => {
  const st = $("run-status");
  st.textContent = text[r.state] || r.state;
  st.className = "run-status " + r.state;
  // Stop an indeterminate bar; a determinate one keeps the last progress.
  const bar = $("progress");
  if (!bar.hasAttribute("value")) {
    bar.max = 1;
    bar.value = r.state === "done" ? 1 : 0;
  }
  if (r.message) {
    $("run-message").textContent = r.message;
    $("run-message").hidden = false;
  }
  output = r.output;
  $("run-cancel").hidden = true;
  $("back").hidden = false;
  $("close").hidden = false;
  if (output) {
    $("open").textContent = output.dir ? text.openFolder : text.openFile;
    $("open").hidden = false;
  }
  $("close").focus();
});

// ---------------------------------------------------------------- start

async function start() {
  def = await xljs.invoke("init");
  text = def.text || {};
  document.title = def.title;
  $("title").textContent = def.title;
  if (def.description) {
    $("description").textContent = def.description;
    $("description").hidden = false;
  }
  const box = $("fields");
  def.fields.forEach((f, i) => box.append(buildField(f, def.values[f.name], i)));
  $("submit").textContent = def.submit;
  $("cancel").textContent = text.cancel;
  $("back").textContent = text.back;
  $("close").textContent = text.close;

  $("form").addEventListener("submit", (e) => { e.preventDefault(); submit(); });
  $("cancel").addEventListener("click", () => xljs.close());
  $("close").addEventListener("click", () => xljs.close());
  $("run-cancel").addEventListener("click", () => {
    xljs.emit("cancel");
    $("run-cancel").disabled = true;
    $("run-cancel").textContent = text.cancelling;
  });
  $("back").addEventListener("click", () => {
    $("run").hidden = true;
    $("form").hidden = false;
    setBusy(false);
  });
  $("open").addEventListener("click", () => { if (output) xljs.invoke("open", output.path, output.dir); });
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && !$("form").hidden && !$("cancel").disabled) xljs.close();
  });

  $("form").hidden = false;
  const first = def.fields[0];
  if (first) controls.get(first.name).focus();
}

start().catch((e) => {
  document.body.textContent = String(e && e.message ? e.message : e);
});
