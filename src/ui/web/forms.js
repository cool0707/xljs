"use strict";
// A set of forms (ui.forms): the side menu, and each form in a frame of
// form.html (?frame) made when it is first shown. The frames' messages go
// to the script as { type: "from", form, msg } (with the files dropped);
// the script's { type: "to", form, msg } go to the frame.
const $ = (id) => document.getElementById(id);
const el = (tag, props = {}, ...children) => {
  const e = Object.assign(document.createElement(tag), props);
  for (const c of children) if (c !== null && c !== undefined) e.append(c);
  return e;
};

let forms = [], items = [], frames = [], current = -1;
// What the script sent a frame before its page was there.
const waiting = [];

host.on((m) => {
  switch (m.type) {
    case "init": init(m); break;
    case "to": {
      const f = frames[m.form];
      if (!f) return;
      if (f.ready) f.win.postMessage("\x02" + JSON.stringify(m.msg), location.origin);
      else (waiting[m.form] ||= []).push(m.msg);
      break;
    }
  }
});

function init(m) {
  forms = m.forms || [];
  applyColors(m.accent || {}, m.background || {}, m.radius || "");
  const shell = $("shell");
  shell.classList.toggle("right", m.side === "right");
  shell.classList.toggle("collapsed", !!m.collapsed);
  const burger = $("burger");
  burger.title = (m.texts && m.texts.menu) || "";
  burger.setAttribute("aria-label", burger.title);
  burger.setAttribute("aria-expanded", String(!m.collapsed));
  burger.onclick = () => {
    const closed = shell.classList.toggle("collapsed");
    burger.setAttribute("aria-expanded", String(!closed));
  };
  $("items").textContent = "";
  items = forms.map((f, k) => {
    const b = el("button", { type: "button", className: "item", title: f.label },
      f.icon ? el("i", { className: f.icon + " item-icon" }) : el("span", { className: "item-letter", textContent: Array.from(f.label)[0] || "" }),
      el("span", { className: "item-text", textContent: f.label }));
    b.setAttribute("role", "tab");
    b.onclick = () => show(k, true);
    $("items").append(b);
    return b;
  });
  frames = forms.map(() => null);
  show(Math.min(Math.max(m.start || 0, 0), forms.length - 1), false);
  requestAnimationFrame(() => requestAnimationFrame(() => shell.classList.add("ready")));
}

/** Shows the k-th form (made on its first showing); chosen: by the user (the window may take its size). */
function show(k, chosen) {
  if (k === current) return;
  current = k;
  items.forEach((b, i) => {
    b.classList.toggle("active", i === k);
    b.setAttribute("aria-selected", String(i === k));
  });
  if (!frames[k]) {
    const src = "form.html?frame" + (host.canAttach ? "&attach" : "");
    const iframe = el("iframe", { src, title: forms[k].label });
    frames[k] = { iframe, win: null, ready: false };
    $("frames").append(iframe);
    frames[k].win = iframe.contentWindow;
  }
  frames.forEach((f, i) => { if (f) f.iframe.hidden = i !== k; });
  if (chosen) {
    host.send({ type: "show", form: k, side: $("side").getBoundingClientRect().width });
    frames[k].iframe.focus();
  }
}

// Messages of the frames: "\x01" + JSON, or { text, files } with the files dropped.
window.addEventListener("message", (e) => {
  const k = frames.findIndex((f) => f && f.iframe.contentWindow === e.source);
  if (k < 0) return;
  const d = e.data;
  const text = typeof d === "string" ? d : d && typeof d.text === "string" ? d.text : "";
  if (text[0] !== "\x01") return;
  let msg;
  try { msg = JSON.parse(text.slice(1)); } catch { return; }
  const f = frames[k];
  if (msg.type === "ready") {
    f.ready = true;
    f.win = e.source;
  }
  // A cancel is noted by the window as soon as it arrives (cancelRequested), so it goes as it is too.
  if (msg.type === "cancel") host.send({ type: "cancel" });
  host.send({ type: "from", form: k, msg }, d && d.files);
  if (msg.type === "ready") for (const w of (waiting[k] || []).splice(0)) f.win.postMessage("\x02" + JSON.stringify(w), location.origin);
});

// Ctrl+Shift+Up / Down (or the arrows in the menu) move between the forms.
document.addEventListener("keydown", (e) => {
  if (!items.length) return;
  const inMenu = e.target.closest && e.target.closest("#items");
  const step = (inMenu || (e.ctrlKey && e.shiftKey)) && (e.key === "ArrowDown" ? 1 : e.key === "ArrowUp" ? -1 : 0);
  if (!step) return;
  e.preventDefault();
  const k = (current + step + items.length) % items.length;
  show(k, true);
  if (inMenu) items[k].focus();
});

host.send({ type: "ready" });
