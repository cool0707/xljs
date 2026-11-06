"use strict";
// Built-in form page (ui.form). The script sends "init" with the fields,
// validates "submit"ted values and, with onSubmit, reports the run.
const $ = (id) => document.getElementById(id);
const el = (tag, props = {}, ...children) => {
  const e = Object.assign(document.createElement(tag), props);
  for (const c of children) if (c !== null && c !== undefined) e.append(c);
  return e;
};

/** A Font Awesome icon (cls: its classes) followed by text; just the text without cls. */
const withIcon = (cls, text) => (cls ? [el("i", { className: cls + " icon" }), text] : [text]);

let texts = {};
let fields = [];
let tabButtons = []; // one per tab; fields name their tab by index (-1 = above the tabs)
let state = "input"; // input | busy (waiting for the script) | running | stopping
let ran = false;
let noSubmit = false;
let extraButtons = []; // the form's buttons (beside the submit button), with the toggles and items of their menus
let extraNodes = []; // what init put in the footer for them
let pressed = null, spinTimer = 0; // the button whose press runs (it shows a spinner), and the timer that shows it
let seq = 0; // the last value set by the script (ctx.setValue) applied here; sent back with the values
let chartTexts = {}, lang = "ja"; // for chart fields (chart.js)
const controls = new Map(); // name -> { field, row, get(), set(value, append), focus() }

host.on((m) => {
  switch (m.type) {
    case "init": init(m); break;
    case "errors": setState("input"); showErrors(m.errors || {}, m.error || ""); break;
    case "picked": {
      // From a picker or a drop (a change by the user), or quiet: set by the script.
      const c = controls.get(m.field);
      if (m.seq) seq = Math.max(seq, m.seq);
      if (c) { c.set(m.value, m.append); liveCheck(m.field, true, !m.quiet); }
      break;
    }
    case "fieldError": { const c = controls.get(m.field); if (c) { setError(c, m.error || ""); markTabs(); } break; }
    case "table": { const c = controls.get(m.field); if (c && c.setTable) c.setTable(m); break; }
    case "chart": { const c = controls.get(m.field); if (c && c.setChart) c.setChart(m); break; }
    case "cellErrors": { const c = controls.get(m.field); if (c && c.setCellErrors) c.setCellErrors(m.errors || [], m.row); break; }
    case "running": startRun(m); break;
    case "progress": progress(m); break;
    case "log": appendLog(m.lines, m.level); break;
    case "done": finishRun(m); break;
  }
});

function init(m) {
  texts = m.texts || {};
  fields = m.fields || [];
  chartTexts = m.chartTexts || {};
  lang = m.lang || "ja";
  applyColors(m.accent || {}, m.background || {}, m.radius || "");
  if (m.description) {
    $("description").textContent = m.description;
    $("description").hidden = false;
  }
  $("submit").replaceChildren(...withIcon(m.submitIcon, m.submit || texts.ok));
  noSubmit = !!m.noSubmit;
  $("submit").hidden = noSubmit;
  for (const n of extraNodes) n.remove();
  extraButtons = [];
  menus.length = 0;
  extraNodes = (m.buttons || []).map((label, k) => {
    const variant = (m.buttonVariants || [])[k] || "default";
    const b = el("button", { type: "button", className: variantClass(variant), title: (m.buttonTooltips || [])[k] || "" },
      ...withIcon((m.buttonIcons || [])[k], label));
    extraButtons.push(b);
    const menu = (m.buttonMenus || [])[k];
    let node = b;
    if (menu && (menu.items.length || !menu.split)) node = menuButton(b, k, menu, variant);
    else b.onclick = () => submit(k, undefined, b);
    $("submit").before(node);
    return node;
  });
  // Without the submit button the last button is the main one (unless it has its own look).
  if (noSubmit && extraNodes.length && ((m.buttonVariants || [])[extraNodes.length - 1] || "default") === "default") {
    const last = extraNodes[extraNodes.length - 1];
    for (const b of last.matches("button") ? [last] : last.querySelectorAll(":scope > button")) b.classList.add("primary");
  }
  $("cancel").textContent = texts.cancel;
  $("open").textContent = texts.openFolder;
  const box = $("fields");
  box.textContent = "";
  controls.clear();
  const panels = buildTabs(m.tabs || [], m.tabIcons || []);
  const parent = (tab) => (tab >= 0 && panels[tab] ? panels[tab] : box);
  // Groups: a heading and a box for its fields, placed before the field at its index.
  const groups = m.groups || [];
  const groupBoxes = groups.map(() => null);
  const startGroups = (i) => groups.forEach((g, k) => {
    if (g.at !== i || groupBoxes[k]) return;
    const head = el("h2", { className: "group-head" }, ...withIcon(g.icon, g.label));
    const sec = el("section", { className: "group" }, head);
    if (g.help) sec.append(el("div", { className: "group-help", textContent: g.help }));
    if (g.collapsible) collapsible(sec, head, g.collapsed);
    parent(g.tab).append(sec);
    groupBoxes[k] = sec;
  });
  fields.forEach((f, i) => {
    startGroups(i);
    const id = "f" + i;
    const c = buildControl(f, id);
    const check = f.type === "checkbox" || f.type === "switch";
    const wide = f.type === "table" || f.type === "chart" || f.type === "note" ? " wide" : "";
    // A note has no label beside it (its label is its heading).
    const label = f.type === "note" ? null : el("label", { className: "lbl" + wide, htmlFor: id }, ...(check ? [] : withIcon(f.icon, f.label)));
    if (label && f.required && !check) label.append(el("span", { className: "req", textContent: "*" }));
    if (f.tooltip) { if (label) label.title = f.tooltip; c.node.title = f.tooltip; }
    const ctl = el("div", { className: "ctl" + wide }, c.node);
    const counter = f.maxLength && c.text ? el("div", { className: "counter" }) : null;
    const help = f.help ? el("div", { className: "help", textContent: f.help }) : null;
    if (counter) ctl.append(el("div", { className: "meta" }, help || el("div", { className: "help" }), counter));
    else if (help) ctl.append(help);
    c.err = ctl.appendChild(el("div", { className: "err", hidden: true }));
    c.field = f;
    c.ctl = ctl;
    c.group = f.group >= 0 ? groupBoxes[f.group] : null;
    (c.group || parent(f.tab)).append(...(label ? [label, ctl] : [ctl]));
    controls.set(f.name, c);
    if (counter) countChars(c, counter);
    watch(c);
    c.set(f.value);
  });
  startGroups(fields.length);
  setState("input");
  if (tabButtons.length) showTab(0, false);
  const typing = ["text", "password", "number", "date", "month", "time", "select", "textarea", "file", "files", "folder", "saveFile", "combobox", "tags", "dateRange"];
  const hidden = (f) => f.tab > 0 || !!controls.get(f.name).group?.classList.contains("collapsed");
  const first = fields.find((f) => typing.includes(f.type) && !hidden(f));
  if (first) controls.get(first.name).focus();
  host.send({ type: "shown" });
}

/** Makes a group's heading open and close it (collapsed: closed at first). */
function collapsible(sec, head, collapsed) {
  head.classList.add("toggle");
  head.prepend(el("i", { className: "fa-solid fa-chevron-right chevron" }));
  head.tabIndex = 0;
  head.setAttribute("role", "button");
  const set = (closed) => {
    sec.classList.toggle("collapsed", closed);
    head.setAttribute("aria-expanded", String(!closed));
  };
  set(collapsed);
  sec.open = () => set(false);
  head.onclick = () => set(!sec.classList.contains("collapsed"));
  head.addEventListener("keydown", (e) => {
    if (e.key === "Enter" || e.key === " ") { e.preventDefault(); head.click(); }
  });
}

/** Shows "12 / 100" under a field with maxLength (red past it). */
function countChars(c, counter) {
  const max = c.field.maxLength;
  const update = () => {
    const n = Array.from(c.text.value).length;
    counter.textContent = `${n} / ${max}`;
    counter.classList.toggle("over", n > max);
  };
  c.text.addEventListener("input", update);
  const set = c.set;
  c.set = (v, append) => { set(v, append); update(); };
}

// ---- tabs

/** Builds the tab bar and one panel per tab (after the fields above the tabs); returns the panels. */
// The accent option: the accent color (buttons, tabs, focus, checkboxes,
// selected rows, the title bar icon) per theme, with its hover color, the
// text on it (black or white, whichever reads better) and the selection
// color derived from it. The background option: the page's (and the custom
// title bar's) background per theme; a dark background in the light theme
// (or a light one in the dark theme) takes the other theme's text, input and
// border colors so the page stays readable. An unknown color keeps the default.
function applyColors(accent, background, radius) {
  const luminance = (c) => {
    const probe = document.body.appendChild(el("span", { hidden: true }));
    probe.style.color = c;
    const [r, g, b] = (getComputedStyle(probe).color.match(/[\d.]+/g) || [0, 0, 0]).map(Number);
    probe.remove();
    const lin = (v) => ((v /= 255) <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4);
    return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
  };
  const valid = (c) => !!c && CSS.supports("color", c);
  const accentVars = (c, dark) => {
    if (!valid(c)) return "";
    const mix = dark ? "black" : "white";
    return `--accent: ${c}; --accent-hover: color-mix(in srgb, ${c} 88%, ${mix});` +
      ` --accent-fg: ${luminance(c) > 0.18 ? "#000000" : "#ffffff"}; --xljs-titlebar-accent: ${c};` +
      ` --sel: color-mix(in srgb, ${c} ${dark ? "45%" : "22%"}, ${dark ? "#18181b" : "white"});`;
  };
  // The text and control colors of base.css for a light and a dark background.
  const palettes = {
    light: { fg: "#18181b", muted: "#71717a", border: "#e4e4e7", inputBorder: "#d4d4d8", input: "#ffffff" },
    dark: { fg: "#fafafa", muted: "#a1a1aa", border: "#2e2e33", inputBorder: "#3f3f46", input: "#1c1c20" },
  };
  const backgroundVars = (c, dark) => {
    if (!valid(c)) return "";
    const darkBg = luminance(c) <= 0.18;
    const p = palettes[darkBg ? "dark" : "light"];
    // The footer, table heads, striped rows and the tab bar: tints of the background.
    const tint = (k) => `color-mix(in srgb, ${c} ${k}, ${p.fg})`;
    let vars = `--bg: ${c}; --xljs-titlebar-bg: ${c}; --xljs-titlebar-fg: ${p.fg};` +
      ` --hover: ${tint(darkBg ? "88%" : "94%")}; --subtle: ${tint(darkBg ? "88%" : "94%")};` +
      ` --panel: ${tint(darkBg ? "95%" : "98%")}; --alt: ${tint(darkBg ? "97%" : "98.5%")};`;
    if (darkBg !== dark) {
      vars += ` --fg: ${p.fg}; --muted: ${p.muted}; --border: ${p.border}; --input-border: ${p.inputBorder}; --input: ${p.input};` +
        ` --sel-fg: ${p.fg}; color-scheme: ${darkBg ? "dark" : "light"};`;
    }
    return vars;
  };
  // The radius option: the corners of buttons, inputs and boxes (a CSS length; "" keeps the default).
  if (radius && CSS.supports("border-radius", radius)) document.documentElement.style.setProperty("--radius", radius);
  else document.documentElement.style.removeProperty("--radius");
  const light = backgroundVars(background.light, false) + " " + accentVars(accent.light, false);
  const dark = backgroundVars(background.dark, true) + " " + accentVars(accent.dark, true);
  let css = "";
  if (light.trim()) css += `:root { ${light.trim()} }\n`;
  if (dark.trim()) css += `@media (prefers-color-scheme: dark) { :root { ${dark.trim()} } }\n`;
  let style = $("accent-style");
  if (!style) style = document.head.appendChild(el("style", { id: "accent-style" }));
  style.textContent = css;
}

function buildTabs(labels, icons) {
  $("tabs").textContent = "";
  $("panels").textContent = "";
  $("tabs").hidden = labels.length === 0;
  tabButtons = labels.map((label, k) => {
    const b = el("button", { type: "button", className: "tab", id: "tab" + k }, ...withIcon(icons[k], label));
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

/** An input with its prefix and suffix ("¥", "件") inside its box. */
function affix(f, i) {
  if (!f.prefix && !f.suffix) return i;
  return el("div", { className: "affix" },
    f.prefix ? el("span", { className: "affix-text", textContent: f.prefix }) : null, i,
    f.suffix ? el("span", { className: "affix-text", textContent: f.suffix }) : null);
}

/** Tells the field's checks that the user changed it (for controls without a native change event). */
const changedBy = (node) => node.dispatchEvent(new Event("change", { bubbles: true }));

function buildControl(f, id) {
  switch (f.type) {
    case "checkbox": case "switch": {
      const i = el("input", { type: "checkbox", id, name: f.name, className: f.type === "switch" ? "switch" : "" });
      if (f.type === "switch") i.setAttribute("role", "switch");
      return {
        node: el("label", { className: "check" }, i, ...withIcon(f.icon, f.label)),
        get: () => i.checked, set: (v) => { i.checked = !!v; }, focus: () => i.focus(), inputs: [i],
      };
    }
    case "checkboxes": {
      const inputs = (f.options || []).map((o, k) => el("input", { type: "checkbox", name: f.name, value: o.value, id: k ? "" : id }));
      const node = el("div", { className: "choices" }, ...inputs.map((i, k) => el("label", {}, i, f.options[k].label)));
      return {
        node, inputs,
        get: () => inputs.filter((i) => i.checked).map((i) => i.value),
        set: (v) => { const on = new Set((Array.isArray(v) ? v : []).map(String)); for (const i of inputs) i.checked = on.has(i.value); },
        focus: () => inputs[0]?.focus(),
      };
    }
    case "segmented": return segmentedControl(f, id);
    case "combobox": return comboControl(f, id);
    case "tags": return tagsControl(f, id);
    case "slider": {
      const i = el("input", { type: "range", id, name: f.name });
      for (const k of ["min", "max", "step"]) if (f[k] !== undefined && f[k] !== null) i[k] = f[k];
      const out = el("output", { className: "slider-value", htmlFor: id });
      const show = () => { out.textContent = (f.prefix || "") + i.value + (f.suffix || ""); };
      i.addEventListener("input", show);
      return {
        node: el("div", { className: "slider" }, i, out), inputs: [i],
        get: () => i.value, set: (v) => { i.value = v ?? ""; show(); }, focus: () => i.focus(),
      };
    }
    case "dateRange": {
      const a = el("input", { type: "date", id, name: f.name + "-from" });
      const b = el("input", { type: "date", name: f.name + "-to" });
      a.setAttribute("aria-label", texts.rangeFrom);
      b.setAttribute("aria-label", texts.rangeTo);
      // The end's calendar starts at the start date.
      const limit = () => { b.min = a.value; };
      a.addEventListener("change", limit);
      return {
        node: el("div", { className: "range" }, a, el("span", { className: "range-sep", textContent: "～" }), b), inputs: [a, b],
        get: () => [a.value, b.value],
        set: (v) => { const [x, y] = Array.isArray(v) ? v : []; a.value = x || ""; b.value = y || ""; limit(); },
        focus: () => a.focus(),
      };
    }
    case "note": {
      const icons = { info: "fa-solid fa-circle-info", success: "fa-solid fa-circle-check", warning: "fa-solid fa-triangle-exclamation", error: "fa-solid fa-circle-xmark" };
      const variant = icons[f.variant] ? f.variant : "info";
      const body = el("div", { className: "note-body" });
      const node = el("div", { className: "note " + variant, id },
        el("i", { className: (f.icon || icons[variant]) + " note-icon" }),
        el("div", { className: "note-text" }, f.label ? el("div", { className: "note-title", textContent: f.label }) : null, body));
      node.setAttribute("role", variant === "error" || variant === "warning" ? "alert" : "note");
      return { node, inputs: [], get: () => null, set: (v) => { body.textContent = v ?? ""; }, focus: () => {} };
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
      // With a placeholder the select starts on an empty option that shows it.
      const s = el("select", { id, name: f.name },
        ...(f.placeholder ? [el("option", { value: "", textContent: f.placeholder, className: "placeholder" })] : []),
        ...(f.options || []).map((o) => el("option", { value: o.value, textContent: o.label })));
      const empty = () => s.classList.toggle("empty", !!f.placeholder && s.value === "");
      s.addEventListener("change", empty);
      return {
        node: s, inputs: [s],
        get: () => s.value,
        set: (v) => { s.value = String(v ?? ""); if (s.selectedIndex < 0 && s.options.length) s.selectedIndex = 0; empty(); },
        focus: () => s.focus(),
      };
    }
    case "textarea": {
      const t = el("textarea", { id, name: f.name, rows: f.rows || 4, placeholder: f.placeholder || "" });
      return { node: t, inputs: [t], text: t, get: () => t.value, set: (v) => { t.value = v ?? ""; }, focus: () => t.focus() };
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
      clear.onclick = () => {
        t.value = "";
        t.dispatchEvent(new Event("change", { bubbles: true }));
        t.focus();
      };
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
      const text = f.type === "password" ? i : undefined;
      return { node: affix(f, i), inputs: [i], text, get: () => i.value, set: (v) => { i.value = v ?? ""; }, focus: () => i.focus() };
    }
    case "table":
      return tableControl(f, id);
    case "chart":
      return chartControl(f, id);
    default: {
      const i = input("text", f, id);
      return { node: affix(f, i), inputs: [i], text: i, get: () => i.value, set: (v) => { i.value = v ?? ""; }, focus: () => i.focus() };
    }
  }
}

/** Options as buttons side by side (one chosen, or several with multiple). */
function segmentedControl(f, id) {
  const opts = f.options || [];
  const on = new Set();
  const node = el("div", { className: "segmented" });
  node.setAttribute("role", f.multiple ? "group" : "radiogroup");
  const btns = opts.map((o, k) => {
    const b = el("button", { type: "button", className: "seg", id: k ? "" : id, textContent: o.label });
    b.setAttribute("role", f.multiple ? "checkbox" : "radio");
    b.onclick = () => {
      if (f.multiple) on.has(o.value) ? on.delete(o.value) : on.add(o.value);
      else { if (on.has(o.value)) return; on.clear(); on.add(o.value); }
      mark();
      changedBy(node);
    };
    return b;
  });
  node.append(...btns);
  const mark = () => {
    const any = opts.some((o) => on.has(o.value));
    btns.forEach((b, k) => {
      const sel = on.has(opts[k].value);
      b.classList.toggle("on", sel);
      b.setAttribute("aria-checked", String(sel));
      // One tab stop for the group (the chosen option); arrows move between the options.
      b.tabIndex = f.multiple || sel || (!any && k === 0) ? 0 : -1;
    });
  };
  node.addEventListener("keydown", (e) => {
    const step = e.key === "ArrowRight" || e.key === "ArrowDown" ? 1 : e.key === "ArrowLeft" || e.key === "ArrowUp" ? -1 : 0;
    const at = btns.indexOf(document.activeElement);
    if (!step || at < 0) return;
    e.preventDefault();
    const next = btns[(at + step + btns.length) % btns.length];
    next.focus();
    if (!f.multiple) next.click();
  });
  return {
    node, inputs: btns,
    get: () => (f.multiple ? opts.filter((o) => on.has(o.value)).map((o) => o.value) : [...on][0] ?? ""),
    set: (v) => { on.clear(); for (const x of [v].flat()) if (x !== null && x !== undefined && x !== "") on.add(String(x)); mark(); },
    focus: () => (btns.find((b) => b.tabIndex === 0) || btns[0])?.focus(),
  };
}

/**
 * A text input with a list of the options that match what is typed. The
 * value is the option whose label (or value) is typed, or (custom) the text.
 */
function comboControl(f, id) {
  const opts = f.options || [];
  const i = input("text", f, id);
  i.classList.add("combo-input");
  i.spellcheck = false;
  i.setAttribute("role", "combobox");
  i.setAttribute("aria-autocomplete", "list");
  i.setAttribute("aria-expanded", "false");
  const list = el("div", { className: "combo-list", hidden: true });
  list.setAttribute("role", "listbox");
  let shown = [], active = -1;
  const labelOf = (v) => (opts.find((o) => o.value === v) || { label: v }).label;
  const valueOf = (text) => {
    const t = text.trim();
    const o = opts.find((x) => x.label === t) || opts.find((x) => x.value === t);
    return o ? o.value : t;
  };
  function render() {
    list.textContent = "";
    if (!shown.length) list.append(el("div", { className: "combo-empty", textContent: texts.noMatch }));
    shown.slice(0, 200).forEach((o, k) => {
      const item = el("div", { className: "combo-item" + (k === active ? " active" : ""), textContent: o.label });
      item.setAttribute("role", "option");
      item.addEventListener("mousedown", (e) => { e.preventDefault(); choose(o); });
      list.append(item);
    });
    const a = list.children[active];
    if (a) a.scrollIntoView({ block: "nearest" });
  }
  /** Opens the list with the options matching the text (all: every option, the current one active). */
  function open(all) {
    if (i.disabled) return;
    const q = all ? "" : i.value.trim().toLowerCase();
    shown = opts.filter((o) => !q || o.label.toLowerCase().includes(q) || o.value.toLowerCase().includes(q));
    active = all ? shown.findIndex((o) => o.label === i.value.trim()) : shown.length ? 0 : -1;
    list.hidden = false;
    i.setAttribute("aria-expanded", "true");
    render();
  }
  function close() {
    list.hidden = true;
    i.setAttribute("aria-expanded", "false");
  }
  function choose(o) {
    i.value = o.label;
    close();
    changedBy(i);
  }
  i.addEventListener("input", () => open(false));
  i.addEventListener("mousedown", () => { if (list.hidden) open(true); });
  i.addEventListener("blur", close);
  i.addEventListener("keydown", (e) => {
    if (e.isComposing) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (list.hidden) { open(true); return; }
      const n = Math.min(shown.length, 200);
      if (n) { active = (active + (e.key === "ArrowDown" ? 1 : -1) + n) % n; render(); }
    } else if (e.key === "Enter" && !list.hidden) {
      e.preventDefault();
      if (active >= 0 && shown[active]) choose(shown[active]);
      else close();
    } else if (e.key === "Escape" && !list.hidden) {
      e.preventDefault();
      e.stopPropagation();
      close();
    }
  });
  return {
    node: el("div", { className: "combo" }, i, list), inputs: [i],
    get: () => valueOf(i.value), set: (v) => { i.value = labelOf(String(v ?? "")); close(); }, focus: () => i.focus(),
  };
}

/** A list of strings as chips; typing and Enter (or a comma) adds one, Backspace on empty removes the last. */
function tagsControl(f, id) {
  const i = el("input", { type: "text", id, name: f.name, className: "tag-input", placeholder: f.placeholder || texts.addTag, spellcheck: false });
  const box = el("div", { className: "tags" }, i);
  let tags = [], removes = [];
  const split = (s) => String(s).split(/[,、;\r\n\t]+/).map((x) => x.trim()).filter(Boolean);
  function render() {
    for (const n of [...box.querySelectorAll(".tag")]) n.remove();
    removes = tags.map((t, k) => {
      const x = el("button", { type: "button", className: "tag-x", title: texts.removeTag, tabIndex: -1 }, el("i", { className: "fa-solid fa-xmark" }));
      x.setAttribute("aria-label", texts.removeTag + " " + t);
      x.disabled = i.disabled;
      x.onclick = (e) => { e.stopPropagation(); tags.splice(k, 1); render(); changedBy(box); i.focus(); };
      i.before(el("span", { className: "tag" }, el("span", { textContent: t }), x));
      return x;
    });
  }
  /** Adds the tags in text; true if any was added. */
  function add(text) {
    const before = tags.length;
    for (const t of split(text)) if (!tags.includes(t)) tags.push(t);
    if (tags.length === before) return false;
    render();
    return true;
  }
  i.addEventListener("keydown", (e) => {
    if (e.isComposing) return;
    if (e.key === "Enter" && i.value.trim()) {
      e.preventDefault();
      if (add(i.value)) changedBy(box);
      i.value = "";
    } else if (e.key === "Backspace" && i.value === "" && tags.length) {
      tags.pop();
      render();
      changedBy(box);
    }
  });
  i.addEventListener("input", () => {
    if (/[,、;\t]/.test(i.value)) {
      const parts = i.value.split(/[,、;\t]/);
      const rest = parts.pop();
      if (add(parts.join(","))) changedBy(box);
      i.value = rest;
    }
  });
  i.addEventListener("paste", (e) => {
    const t = e.clipboardData && e.clipboardData.getData("text");
    if (!t || !/[\r\n,、;\t]/.test(t)) return;
    e.preventDefault();
    if (add(i.value + t)) changedBy(box);
    i.value = "";
  });
  // Leaving the field adds what is typed (as the text input's own change would only report it).
  i.addEventListener("change", (e) => {
    if (!i.value.trim()) return;
    e.stopPropagation();
    add(i.value);
    i.value = "";
    changedBy(box);
  });
  box.addEventListener("mousedown", (e) => { if (e.target === box) { e.preventDefault(); i.focus(); } });
  return {
    node: box,
    get inputs() { return [i, ...removes]; },
    get: () => tags.concat(split(i.value).filter((t) => !tags.includes(t))),
    set: (v) => { tags = []; i.value = ""; add((Array.isArray(v) ? v : v ? [v] : []).join("\n")); render(); },
    focus: () => i.focus(),
  };
}

// ---- table fields: sortable; rows are chosen by clicking (selectable) and the value is their indices.
// Editable cells are inputs: each change goes to the script, which checks it
// and answers with the errors of the row ("cellErrors").

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });
function compareCells(a, b) {
  const ea = a === null || a === "", eb = b === null || b === "";
  if (ea || eb) return ea === eb ? 0 : ea ? 1 : -1;
  if (typeof a === "number" && typeof b === "number") return a - b;
  return collator.compare(String(a), String(b));
}

/** Cell edits waiting to be sent (sent before a button press). */
const pendingCells = new Map();
function flushCells() {
  for (const send of pendingCells.values()) send();
  pendingCells.clear();
}

const fill = (t, ...a) => String(t || "").replace(/\{(\d)\}/g, (_, i) => a[i]);

/** The input of an editable cell (e: the column's { type, options, min, max, step }). */
function cellInput(e, v) {
  let i;
  if (e.type === "select") {
    i = el("select", {}, el("option", { value: "", textContent: "" }), ...(e.options || []).map((o) => el("option", { value: o.value, textContent: o.label })));
    i.value = v === null ? "" : String(v);
    if (i.selectedIndex < 0) i.append(el("option", { value: String(v), textContent: String(v), selected: true }));
  } else if (e.type === "checkbox") {
    i = el("input", { type: "checkbox", checked: v === true });
  } else {
    // Numbers are typed as text so that what does not parse stays visible (and full-width digits work).
    i = el("input", { type: e.type === "text" || e.type === "number" ? "text" : e.type, spellcheck: false });
    if (e.type === "number") i.inputMode = "decimal";
    for (const k of ["min", "max", "step"]) if (e[k] !== undefined && e[k] !== null) i[k] = e[k];
    i.value = v === null ? "" : String(v);
  }
  return i;
}

function tableControl(f, id) {
  let cols = f.table.columns, rows = f.table.rows, select = f.table.select, edit = f.table.editable || null;
  let order = [], sortCol = -1, sortDir = 1, boxes = [], cellInputs = [];
  const selected = new Set();
  const errors = new Map(); // "row:col" -> message (col -1: the row)
  const head = el("thead");
  const body = el("tbody");
  const all = el("input", { type: "checkbox", tabIndex: -1 });
  const wrap = el("div", { className: "ftable" + (edit ? " editable" : ""), id, tabIndex: 0 }, el("table", {}, head, body));
  const list = el("ul", { className: "cell-errors", hidden: true });
  wrap.style.maxHeight = (32 + 29 * (f.rows || 8)) + "px";

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

  /** Sends the value of an editable cell; text inputs wait until typing pauses. */
  function changed(i, k, input, now) {
    const e = edit[k];
    const v = e.type === "checkbox" ? input.checked : input.value;
    rows[i][k] = e.type === "number" && v.trim() !== "" && isFinite(v) ? Number(v) : v;
    const key = i + ":" + k;
    // Enter sends at once and the change on leaving the cell would again.
    if (now && !pendingCells.has(key) && input._sent === v) return;
    const send = () => {
      pendingCells.delete(key);
      input._sent = v;
      host.send({ type: "cell", field: f.name, row: i, col: k, value: v });
    };
    clearTimeout(input._timer);
    if (now) send();
    else {
      pendingCells.set(key, send);
      input._timer = setTimeout(send, 250);
    }
  }

  function render() {
    body.textContent = "";
    boxes = [];
    cellInputs = [];
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
      cols.forEach((_, k) => {
        const v = rows[i][k] === undefined ? null : rows[i][k];
        if (edit && edit[k]) {
          const input = cellInput(edit[k], v);
          input.disabled = state !== "input";
          input.dataset.c = k;
          const text = input.tagName === "INPUT" && input.type !== "checkbox";
          input.addEventListener(text ? "input" : "change", () => changed(i, k, input, !text));
          if (text) input.addEventListener("change", () => changed(i, k, input, true));
          cellInputs.push(input);
          tr.append(el("td", { className: "edit " + edit[k].type }, input));
        } else {
          tr.append(el("td", { className: "cell" + (typeof v === "number" ? " num" : ""), textContent: v === null ? "" : String(v) }));
        }
      });
      body.append(tr);
    }
    mark();
    markErrors();
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

  /** Marks the cells and rows with errors and lists the first ones below the table. */
  function markErrors() {
    body.querySelectorAll("tr[data-i]").forEach((tr) => {
      const i = Number(tr.dataset.i);
      const rowMsg = errors.get(i + ":-1") || "";
      tr.classList.toggle("bad-row", rowMsg !== "" || [...errors.keys()].some((x) => x.startsWith(i + ":")));
      tr.title = rowMsg;
      tr.querySelectorAll("td").forEach((td, n) => {
        const k = n - (select ? 1 : 0);
        if (k < 0) return;
        const msg = errors.get(i + ":" + k) || "";
        td.classList.toggle("bad", msg !== "");
        td.title = msg;
      });
    });
    list.textContent = "";
    const entries = [...errors].map(([key, msg]) => [...key.split(":").map(Number), msg]).sort((a, b) => a[0] - b[0] || a[1] - b[1]);
    for (const [i, k, msg] of entries.slice(0, 5)) {
      const li = el("li", { textContent: k >= 0 ? fill(texts.cellError, i + 1, cols[k], msg) : fill(texts.rowError, i + 1, msg) });
      li.onclick = () => focusCell(i, k);
      list.append(li);
    }
    if (entries.length > 5) list.append(el("li", { className: "more", textContent: fill(texts.moreErrors, entries.length - 5) }));
    list.hidden = entries.length === 0;
  }

  function focusCell(i, k) {
    const tr = body.querySelector(`tr[data-i="${i}"]`);
    if (!tr) return;
    const input = tr.querySelector(`[data-c="${k}"]`) || tr.querySelector("[data-c]");
    tr.scrollIntoView({ block: "nearest" });
    if (input && !input.disabled) input.focus();
  }

  body.addEventListener("click", (e) => {
    const tr = e.target.closest("tr[data-i]");
    if (!tr || !select || state !== "input" || e.target.closest("td.edit")) return;
    const i = Number(tr.dataset.i);
    if (select === "single") {
      selected.clear();
      selected.add(i);
    } else if (selected.has(i)) selected.delete(i);
    else selected.add(i);
    mark();
    if (control.onChange) control.onChange();
  });
  // Enter in a cell goes to the same column of the next row (Shift+Enter: the row above), as in Excel.
  body.addEventListener("keydown", (e) => {
    const input = e.target.closest("[data-c]");
    if (!input || e.key !== "Enter" || e.isComposing) return;
    e.preventDefault();
    const trs = [...body.querySelectorAll("tr[data-i]")];
    const next = trs[trs.indexOf(input.closest("tr")) + (e.shiftKey ? -1 : 1)];
    const target = next && next.querySelector(`[data-c="${input.dataset.c}"]`);
    if (target) target.focus();
    if (input.tagName === "INPUT" && input.type !== "checkbox") changed(Number(input.closest("tr").dataset.i), Number(input.dataset.c), input, true);
  });
  all.onclick = (e) => {
    e.stopPropagation();
    if (state !== "input") { e.preventDefault(); return; }
    if (selected.size === rows.length) selected.clear();
    else rows.forEach((_, i) => selected.add(i));
    mark();
    if (control.onChange) control.onChange();
  };

  sort();
  header();
  render();
  const control = {
    node: el("div", {}, wrap, list),
    get inputs() { return [...(select ? [all, ...boxes] : []), ...cellInputs]; },
    get: () => Array.from(selected).sort((a, b) => a - b),
    set: (v) => {
      selected.clear();
      for (const i of Array.isArray(v) ? v : []) if (i >= 0 && i < rows.length) selected.add(i);
      mark();
    },
    setTable: (m) => {
      cols = m.columns; rows = m.rows; select = m.select; edit = m.editable || null;
      wrap.classList.toggle("editable", !!edit);
      selected.clear();
      errors.clear();
      sortCol = -1;
      sort();
      header();
      render();
    },
    /** Errors of the whole table, or (row) of one row. */
    setCellErrors: (list, row) => {
      for (const key of [...errors.keys()]) if (row === undefined || key.startsWith(row + ":")) errors.delete(key);
      for (const e of list) errors.set(e.row + ":" + e.col, e.message);
      markErrors();
    },
    focus: () => wrap.focus(),
  };
  return control;
}

// ---- chart fields: drawn by chart.js (the libraries are loaded with the first chart).
// Clicks and edits of a Gantt chart go to the script, which runs the spec's onClick / onChange.

/** Dates for the script: { $date: ms }. */
const wireDates = (o) => {
  const out = {};
  for (const [k, v] of Object.entries(o || {})) out[k] = v instanceof Date ? { $date: v.getTime() } : v;
  return out;
};

function chartControl(f, id) {
  const box = el("div", { className: "xljs-chart fchart", id, tabIndex: -1 });
  const height = (f.chart && f.chart.height) || 300;
  box.style.height = height + "px";
  let view = null, current = f.chart, failed = false;
  function draw() {
    const c = current;
    XljsChart.load().then(() => {
      if (c !== current) return;
      if (view) view.destroy();
      view = null;
      try {
        view = XljsChart.create(box, XljsChart.revive(c.spec), {
          lang, texts: chartTexts, height: height - 2,
          onClick: c.click ? (index, info) => host.send({ type: "chartClick", field: f.name, index, series: info.series ?? null }) : null,
          onChange: c.change ? (index, changes) => host.send({ type: "chartChange", field: f.name, index, changes: wireDates(changes) }) : null,
        });
        if (failed) { setError(control, ""); failed = false; }
      } catch (e) {
        failed = true;
        setError(control, e.message);
      }
    }, (e) => { failed = true; setError(control, e.message); });
  }
  const control = {
    node: box, inputs: [],
    get: () => null, set: () => {},
    setChart: (m) => { current = m; draw(); },
    focus: () => box.focus(),
  };
  // Drawn once the field is on the page (it needs its size).
  requestAnimationFrame(draw);
  return control;
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

// ---- checks while typing: a changed field is checked by the script
// (its checks and its own validate, then its onChange); a field showing an
// error is checked again as it is typed in, so that the error goes away
// once fixed.

const checkTimers = new Map();
/** change: the user changed the field (not just typing in it, or the script setting it). */
function liveCheck(name, now, change) {
  if (state !== "input") return;
  clearTimeout(checkTimers.get(name));
  const send = () => {
    checkTimers.delete(name);
    if (state === "input") host.send({ type: "check", field: name, values: values(), seq, change: !!change });
  };
  if (now) send();
  else checkTimers.set(name, setTimeout(send, 250));
}

function watch(c) {
  const name = c.field.name;
  if (c.field.type === "chart") return;
  if (c.field.type === "table") {
    c.onChange = () => { if (!c.err.hidden || c.field.change) liveCheck(name, true, true); };
    return;
  }
  if (c.field.type === "dateRange") {
    // Not halfway: once both dates are entered (or none), or when the field is left.
    const done = () => { const [a, b] = c.get(); return (a === "") === (b === ""); };
    c.ctl.addEventListener("change", () => { if (done() || !c.err.hidden) liveCheck(name, true, true); });
    c.ctl.addEventListener("focusout", (e) => { if (!c.ctl.contains(e.relatedTarget) && !done()) liveCheck(name, true, true); });
    return;
  }
  c.ctl.addEventListener("change", () => liveCheck(name, true, true));
  c.ctl.addEventListener("input", () => { if (!c.err.hidden) liveCheck(name, false); });
}

function setError(c, msg) {
  if (msg && c.group && c.group.open) c.group.open(); // a closed group opens to show it
  c.err.textContent = msg;
  c.err.hidden = !msg;
  c.ctl.classList.toggle("invalid", !!msg);
}

/** Marks the tabs with fields that show an error. */
function markTabs() {
  tabButtons.forEach((b, k) => b.classList.toggle("has-error",
    [...controls.values()].some((c) => c.field.tab === k && !c.err.hidden)));
}

function showErrors(errors, general) {
  let first = null;
  for (const t of checkTimers.values()) clearTimeout(t);
  checkTimers.clear();
  for (const [name, c] of controls) {
    const msg = errors[name] || "";
    setError(c, msg);
    if (msg && !first) first = c;
  }
  markTabs();
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
  for (const b of extraButtons) b.disabled = !editing;
  if (!editing) closeMenus();
  if (editing) {
    clearTimeout(spinTimer);
    if (pressed) pressed.classList.remove("loading");
    pressed = null;
  }
  $("cancel").disabled = s === "stopping";
  $("cancel").textContent = s === "running" ? texts.abort : s === "stopping" ? texts.aborting : ran ? texts.close : texts.cancel;
}

/**
 * Sends the values for the submit button or, with k, the form's k-th
 * button (item: the item of its menu). The button pressed (b) shows a
 * spinner until the form takes input again, unless that is right away.
 */
function submit(k, item, b) {
  if (state !== "input") return;
  flushCells();
  closeMenus();
  setState("busy");
  pressed = b || (k === undefined ? $("submit") : null);
  clearTimeout(spinTimer);
  spinTimer = setTimeout(() => { if (pressed) pressed.classList.add("loading"); }, 150);
  const m = { type: "submit", values: values(), seq };
  if (k !== undefined) m.button = k;
  if (item !== undefined) m.item = item;
  host.send(m);
}

const variantClass = (v) => (v === "primary" ? "primary" : v === "destructive" ? "destructive" : v === "ghost" ? "ghost" : "");

// ---- menus of the buttons: a split button (the button, and a toggle for
// its menu) or a menu button (no action of its own). The menu opens above
// the footer; arrow keys move in it, Escape closes it.

const menus = []; // { menu, toggle } of each button with a menu

/** The footer node of button b (the k-th) with its menu. */
function menuButton(b, k, spec, variant) {
  const menu = el("div", { className: "menu", hidden: true });
  menu.setAttribute("role", "menu");
  const items = spec.items.map((x, j) => {
    const it = el("button", { type: "button", className: "menu-item" + (x.variant === "destructive" ? " destructive" : ""), title: x.tooltip || "" },
      ...withIcon(x.icon, x.label));
    it.setAttribute("role", "menuitem");
    it.tabIndex = -1;
    it.onclick = () => submit(k, j, b);
    return it;
  });
  menu.append(...items);
  let toggle = b;
  if (spec.split) {
    b.onclick = () => submit(k, undefined, b);
    toggle = el("button", { type: "button", className: "split-toggle " + variantClass(variant), title: texts.moreActions },
      el("i", { className: "fa-solid fa-chevron-down" }));
    toggle.setAttribute("aria-label", texts.moreActions);
    extraButtons.push(toggle);
  } else {
    b.append(el("i", { className: "fa-solid fa-chevron-down caret" }));
  }
  toggle.setAttribute("aria-haspopup", "menu");
  toggle.setAttribute("aria-expanded", "false");
  extraButtons.push(...items);
  const entry = { menu, toggle };
  menus.push(entry);
  const open = (focusItem) => {
    closeMenus();
    if (state !== "input" || !items.length) return;
    menu.hidden = false;
    // Under the button's right end, or its left end when that would leave the window.
    menu.style.left = menu.style.right = "";
    if (menu.getBoundingClientRect().left < 0) { menu.style.left = "0"; menu.style.right = "auto"; }
    toggle.setAttribute("aria-expanded", "true");
    if (focusItem !== undefined) items[(focusItem + items.length) % items.length].focus();
  };
  toggle.onclick = () => (menu.hidden ? open() : closeMenus());
  toggle.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") { e.preventDefault(); open(e.key === "ArrowDown" ? 0 : -1); }
  });
  menu.addEventListener("keydown", (e) => {
    const at = items.indexOf(document.activeElement);
    const step = e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0;
    if (step) { e.preventDefault(); items[(at + step + items.length) % items.length].focus(); }
    else if (e.key === "Home" || e.key === "End") { e.preventDefault(); items[e.key === "Home" ? 0 : items.length - 1].focus(); }
    else if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); closeMenus(); toggle.focus(); }
    else if (e.key === "Tab") closeMenus();
  });
  return el("div", { className: "menu-wrap" + (spec.split ? " split" : "") }, ...(spec.split ? [b, toggle] : [b]), menu);
}

function closeMenus() {
  for (const { menu, toggle } of menus) {
    menu.hidden = true;
    toggle.setAttribute("aria-expanded", "false");
  }
}
const menuOpen = () => menus.some((x) => !x.menu.hidden);
document.addEventListener("mousedown", (e) => { if (!e.target.closest(".menu-wrap")) closeMenus(); });

// Enter submits (not when the submit button is hidden).
$("form").addEventListener("submit", (e) => { e.preventDefault(); if (!noSubmit) submit(undefined, undefined, $("submit")); });
$("submit").onclick = () => submit();
$("cancel").onclick = cancel;
$("open").onclick = () => host.send({ type: "open" });
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") {
    e.preventDefault();
    if (menuOpen()) closeMenus();
    else cancel();
  }
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
