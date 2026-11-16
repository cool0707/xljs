"use strict";
// Built-in page of the print preview (book.printPreview, toPdf's preview;
// prelude.js printPreview). The script sends "init" with the settings to
// show (fields) and their values; at each change the page sends "options"
// with the values and the script answers "layout" (the document of
// book.toHtml with its charts) or "layoutError". The pages are print.html in
// a frame (print.html?frame: messages through window.postMessage), scaled
// with a transform so that the charts are drawn at the paper's size.
// "ok" sends the values chosen, "close" cancels.

const $ = (id) => document.getElementById(id);
let texts = {}, fields = [], values = {}, seq = 0, shown = -1;
let frame = null;     // the frame shown
let size = null;      // { w, h, pw, ph, tops } of the frame's document (CSS px)
let timer = 0;

const el = (tag, props, ...kids) => {
  const e = Object.assign(document.createElement(tag), props || {});
  for (const k of kids) if (k !== null && k !== undefined) e.append(k);
  return e;
};

// ---------------------------------------------------------------- settings

/** Whether field f is shown with the values now (showIf: { name: value }). */
const visible = (f) => !f.showIf || Object.entries(f.showIf).every(([k, v]) => values[k] === v);

/** The value of a number field's input, or undefined when it is not valid. */
function numberOf(f, input) {
  const t = input.value.trim();
  const x = t === "" ? NaN : Number(t);
  if (!Number.isFinite(x) || (f.min !== undefined && x < f.min) || (f.max !== undefined && x > f.max)) return undefined;
  if (f.step === 1 && x !== Math.floor(x)) return undefined;
  return x;
}

function changed(now) {
  clearTimeout(timer);
  for (const f of fields) {
    const box = document.querySelector(`[data-field="${f.name}"]`);
    if (box) box.hidden = !visible(f);
  }
  if (now) layout(); else timer = setTimeout(layout, 400);
}

function numberField(f, sub) {
  const input = el("input", { type: "number", id: "f-" + f.name, value: String(values[f.name] ?? "") });
  if (f.min !== undefined) input.min = f.min;
  if (f.max !== undefined) input.max = f.max;
  if (f.step !== undefined) input.step = f.step;
  const box = el("div", { className: "field" + (sub ? " sub" : "") });
  const row = el("div", { className: "inline" }, el("label", { className: "label", htmlFor: input.id, textContent: f.label }), input,
    f.suffix ? el("span", { className: "suffix", textContent: f.suffix }) : null);
  box.append(row);
  if (f.help) box.append(el("div", { className: "help", textContent: f.help }));
  input.addEventListener("input", () => {
    const x = numberOf(f, input);
    box.classList.toggle("invalid", x === undefined);
    if (x === undefined) { clearTimeout(timer); return; }
    values[f.name] = x;
    changed(false);
  });
  input.addEventListener("change", () => { if (numberOf(f, input) !== undefined) changed(true); });
  return box;
}

function fieldBox(f) {
  switch (f.type) {
    case "number":
      return numberField(f, !!f.showIf);
    case "checkbox": {
      const input = el("input", { type: "checkbox", id: "f-" + f.name, checked: !!values[f.name] });
      input.addEventListener("change", () => { values[f.name] = input.checked; changed(true); });
      return el("div", { className: "field" }, el("label", { className: "check" }, input, el("span", { textContent: f.label })));
    }
    case "checkboxes": {
      const list = el("div", { className: "checks" });
      for (const o of f.options) {
        const input = el("input", { type: "checkbox", checked: (values[f.name] || []).includes(o) });
        input.addEventListener("change", () => {
          const on = new Set(Array.from(list.querySelectorAll("input")).filter((x) => x.checked).map((x) => x.dataset.v));
          values[f.name] = f.options.filter((x) => on.has(x));
          changed(true);
        });
        input.dataset.v = o;
        list.append(el("label", { title: o }, input, el("span", { textContent: o })));
      }
      return el("div", { className: "field" }, el("span", { className: "label", textContent: f.label }), list);
    }
    case "segmented": {
      const bar = el("div", { className: "segmented", role: "radiogroup" });
      for (const o of f.options) {
        const b = el("button", { type: "button", className: "seg" + (values[f.name] === o.value ? " on" : ""), textContent: o.label });
        b.setAttribute("role", "radio");
        b.addEventListener("click", () => {
          values[f.name] = o.value;
          for (const x of bar.children) x.classList.toggle("on", x === b);
          changed(true);
        });
        bar.append(b);
      }
      return el("div", { className: "field" }, el("span", { className: "label", textContent: f.label }), bar);
    }
    default: { // select
      const sel = el("select", { id: "f-" + f.name });
      for (const o of f.options) sel.append(el("option", { value: o.value, textContent: o.label }));
      sel.value = values[f.name] ?? "";
      sel.addEventListener("change", () => { values[f.name] = sel.value; changed(true); });
      return el("div", { className: "field" }, el("label", { className: "label", htmlFor: sel.id, textContent: f.label }), sel);
    }
  }
}

function buildFields() {
  const box = $("fields");
  box.replaceChildren();
  for (const f of fields) {
    const b = fieldBox(f);
    b.dataset.field = f.name;
    box.append(b);
  }
  $("side").hidden = !fields.length;
  changed(true);
}

// ---------------------------------------------------------------- pages

function layout() {
  clearTimeout(timer);
  if (document.querySelector(".field.invalid:not([hidden])")) return;
  seq++;
  $("pages").classList.add("busy");
  $("status").textContent = texts.updating || "";
  host.send({ type: "options", seq, values });
}

/** Opens print.html in a new hidden frame and draws m's document there; resolves to the frame. */
function draw(m) {
  return new Promise((resolve, reject) => {
    const f = el("iframe", { className: "next", src: "print.html?frame" });
    f.setAttribute("tabindex", "-1");
    // Wide enough for any paper (A2 landscape at 96 dpi is about 2245px).
    f.style.width = "2400px";
    f.style.height = "800px";
    const onMessage = (e) => {
      if (e.source !== f.contentWindow || typeof e.data !== "string" || e.data[0] !== "\x01") return;
      const r = JSON.parse(e.data.slice(1));
      if (r.type === "ready") {
        f.contentWindow.postMessage("\x02" + JSON.stringify({ type: "init", html: m.html, charts: m.charts, texts: m.texts, lang: m.lang }), "*");
      } else if (r.type === "rendered") {
        window.removeEventListener("message", onMessage);
        resolve(f);
      } else if (r.type === "error") {
        window.removeEventListener("message", onMessage);
        f.remove();
        reject(new Error(r.message));
      }
    };
    window.addEventListener("message", onMessage);
    $("sheet").append(f);
  });
}

/** The size of the frame's pages (CSS px). */
function measure(f) {
  const doc = f.contentDocument;
  doc.documentElement.style.overflow = "hidden";
  const pages = Array.from(doc.querySelectorAll(".xljs-page"));
  const pw = Math.max(1, ...pages.map((p) => p.offsetWidth));
  const ph = Math.max(1, ...pages.map((p) => p.offsetHeight));
  const w = pw + 24;
  f.style.width = w + "px";
  const h = doc.documentElement.scrollHeight;
  f.style.height = h + "px";
  return { w, h, pw: pw + 24, ph: ph + 24, tops: pages.map((p) => p.offsetTop) };
}

function zoom() {
  if (!frame || !size) return;
  const box = $("pages");
  const aw = Math.max(50, box.clientWidth - 16), ah = Math.max(50, box.clientHeight - 8);
  const v = $("zoom").value;
  const z = v === "page" ? Math.min(aw / size.pw, ah / size.ph) : v === "width" ? aw / size.pw : Number(v) / 100;
  frame.style.transform = `scale(${z})`;
  const sheet = $("sheet");
  sheet.style.width = size.w * z + "px";
  sheet.style.height = size.h * z + "px";
  size.z = z;
  pageNumber();
}

function current() {
  if (!size || !size.tops.length) return 0;
  const y = $("pages").scrollTop + $("pages").clientHeight / 3;
  let k = 0;
  size.tops.forEach((t, i) => { if (t * size.z <= y) k = i; });
  return k;
}

function pageNumber() {
  const n = size ? size.tops.length : 0;
  const k = current();
  $("pageno").textContent = n ? fmt(texts.page, k + 1, n) : "";
  $("prev").disabled = !n || k === 0;
  $("next").disabled = !n || k >= n - 1;
}

function go(d) {
  if (!size) return;
  const k = Math.max(0, Math.min(size.tops.length - 1, current() + d));
  // The page's top a little below the view's top (the gray between pages).
  $("pages").scrollTop = Math.max(0, (size.tops[k] - 8) * size.z);
  pageNumber();
}

function showError(message) {
  $("message").textContent = message;
  $("message").hidden = false;
  $("sheet").hidden = true;
  $("status").textContent = "";
  $("status").classList.add("error-text");
  $("ok").disabled = true;
  size = null;
  pageNumber();
}

async function onLayout(m) {
  if (m.seq !== seq) return;
  let f;
  try {
    f = await draw(m);
  } catch (e) {
    if (m.seq === seq) { $("pages").classList.remove("busy"); showError(e.message); }
    return;
  }
  if (m.seq !== seq || m.seq < shown) { f.remove(); return; }
  shown = m.seq;
  const keep = frame ? current() : 0;
  if (frame) frame.remove();
  frame = f;
  $("sheet").hidden = false;
  $("message").hidden = true;
  size = measure(f);
  f.classList.remove("next");
  zoom();
  if (keep) { const k = Math.min(keep, size.tops.length - 1); $("pages").scrollTop = Math.max(0, (size.tops[k] - 8) * size.z); }
  pageNumber();
  $("pages").classList.remove("busy");
  $("status").classList.remove("error-text");
  $("status").textContent = fmt(texts.pages, m.pages);
  $("ok").disabled = false;
}

// ---------------------------------------------------------------- messages

host.on((m) => {
  switch (m.type) {
    case "init": {
      texts = m.texts || {};
      fields = m.fields || [];
      values = Object.assign({}, m.values);
      $("ok").textContent = m.ok || texts.ok;
      $("cancel").textContent = texts.cancel;
      $("side-title").textContent = texts.settings;
      $("prev").title = texts.prev;
      $("next").title = texts.next;
      $("zoom-label").textContent = texts.zoom;
      const zs = $("zoom");
      zs.replaceChildren(el("option", { value: "page", textContent: texts.zoomPage }), el("option", { value: "width", textContent: texts.zoomWidth }),
        ...[50, 75, 100, 125, 150, 200].map((x) => el("option", { value: String(x), textContent: x + "%" })));
      zs.value = "page";
      buildFields();
      break;
    }
    case "layout":
      onLayout(m);
      break;
    case "layoutError":
      if (m.seq !== seq && m.seq !== -1) break;
      $("pages").classList.remove("busy");
      if (m.seq === -1) {
        $("status").classList.add("error-text");
        $("status").textContent = m.message;
      } else showError(m.message);
      break;
  }
});

$("ok").addEventListener("click", () => {
  if (document.querySelector(".field.invalid:not([hidden])")) return;
  host.send({ type: "ok", values });
});
$("cancel").addEventListener("click", () => host.send({ type: "close" }));
$("prev").addEventListener("click", () => go(-1));
$("next").addEventListener("click", () => go(1));
$("zoom").addEventListener("change", () => {
  const k = current();
  zoom();
  if (size) $("pages").scrollTop = Math.max(0, (size.tops[k] - 8) * size.z);
});
$("pages").addEventListener("scroll", pageNumber);
new ResizeObserver(() => zoom()).observe($("pages"));
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") host.send({ type: "close" });
  else if (e.target.tagName !== "INPUT" && e.target.tagName !== "SELECT") {
    if (e.key === "PageDown" || (e.key === "ArrowRight" && !e.altKey)) { e.preventDefault(); go(1); }
    else if (e.key === "PageUp" || (e.key === "ArrowLeft" && !e.altKey)) { e.preventDefault(); go(-1); }
  }
});

host.send({ type: "ready" });
