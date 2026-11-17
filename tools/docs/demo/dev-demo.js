"use strict";
// The development mode on the reference site (tools/docs/build.mjs puts it
// in front of a copy of web/dev.html): it plays the Rust side of
// src/dev/mod.rs with an in-memory project, shows the fictional AI chat
// (chat.html) where the browser pane would be, and runs the form in a
// window drawn in the page (form.html?preview, answered from the form's
// first message captured from examples/form-master.xljs). Data:
// window.XLJS_DEMO (demo-data.js, made by build.mjs).
(() => {
  const D = window.XLJS_DEMO;
  const ENTRY = "form-master.xljs";
  const DIR = "C:\\Users\\user\\Documents\\xljs";
  const CHAT = "https://chat.example.com/";
  const files = { [ENTRY]: D.before };
  const versions = [];
  const listeners = [];
  const post = (m) => setTimeout(() => { for (const fn of listeners) fn({ data: "\x02" + JSON.stringify(m) }); }, 0);
  const notInDemo = () => post({ type: "info", message: D.texts.demoOnly });

  window.__xljsTitleBar = { title: D.texts.title, icon: "", texts: D.window, nonClient: true };
  window.chrome = window.chrome || {};
  window.chrome.webview = {
    addEventListener(type, fn) { if (type === "message") listeners.push(fn); },
    removeEventListener() {},
    postMessage(s) {
      if (typeof s !== "string" || s[0] === "\x01") return; // the title bar's messages
      let m;
      try { m = JSON.parse(s); } catch { return; }
      handle(m);
    },
  };

  /** The form of the script as it is now (the formats appear with the change). */
  function form() {
    const f = JSON.parse(JSON.stringify(D.form));
    if (!files[ENTRY].includes("formats:")) for (const x of f.fields) if (x.table) delete x.table.formats;
    return f;
  }

  const now = () => {
    const d = new Date();
    const p = (n) => String(n).padStart(2, "0");
    return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
  };

  function handle(m) {
    switch (m.op) {
      case "ready":
        fetch("../../xljs.d.ts").then((r) => (r.ok ? r.text() : "")).catch(() => "").then((dts) => {
          post({ type: "init", lang: "ja", texts: D.texts, formTexts: {}, services: [{ name: D.chatName, url: CHAT, icon: "" }], agents: [], recent: [], home: DIR, homeChat: "", dts });
          post({ type: "project", path: DIR + "\\" + ENTRY, dir: DIR, entry: ENTRY, text: files[ENTRY], app: false, html: "", title: "", files: [ENTRY], chat: CHAT });
          post({ type: "versions", file: ENTRY, versions });
        });
        break;
      case "read": post({ type: "file", file: m.file, text: files[m.file] ?? "" }); break;
      case "save":
        files[m.file] = m.text;
        post({ type: "saved", file: m.file });
        break;
      case "apply": {
        versions.push({ n: versions.length + 1, time: now(), note: D.texts.demoBefore, text: files[m.file] ?? "" });
        files[m.file] = m.text;
        post({ type: "applied", file: m.file, text: m.text });
        post({ type: "versions", file: m.file, versions });
        break;
      }
      case "versions": post({ type: "versions", file: m.file, versions }); break;
      case "version": {
        const v = versions.find((x) => x.n === m.n);
        if (v) post({ type: "version", file: m.file, n: v.n, text: v.text });
        break;
      }
      case "restore": {
        const v = versions.find((x) => x.n === m.n);
        if (!v) break;
        files[m.file] = v.text;
        post({ type: "applied", file: m.file, text: v.text });
        break;
      }
      case "tests": post({ type: "tests", outcomes: [], pass: 0, fail: 0, new: 0, problem: 0 }); break;
      case "preview":
        if (m.auto) post({ type: "previewForm", form: form() });
        else post({ type: "previewForm", form: form() });
        break;
      case "run":
      case "debug":
        run();
        break;
      case "stop": closeRun(null); break;
      case "intro": post({ type: "copy", what: "intro", text: D.intro }); break;
      case "notify": post({ type: "copy", what: "notify", text: D.notify }); break;
      case "chat": post({ type: "url", url: m.url || CHAT }); break;
      case "browser":
        if (m.path === "show") showChat(true);
        else if (m.path === "hide") showChat(false);
        else if (m.path === "reload" && chatFrame) chatFrame.src = "chat.html";
        break;
      case "edit": case "agentHint": case "dragFile": case "agentDrag": case "setHome": break;
      default: notInDemo();
    }
  }

  // ------------------------------------------------------------ the chat

  let chatFrame = null;
  function showChat(on) {
    const view = document.getElementById("chat-view");
    if (!view) return;
    if (on && !chatFrame) {
      chatFrame = document.createElement("iframe");
      chatFrame.src = "chat.html";
      chatFrame.title = D.chatName;
      chatFrame.style.cssText = "position:absolute;inset:0;width:100%;height:100%;border:0;background:#fff";
      view.append(chatFrame);
    }
    if (chatFrame) chatFrame.style.visibility = on ? "visible" : "hidden";
  }

  // ------------------------------------------------------------ the run

  let runWin = null;
  function run() {
    if (runWin) return;
    post({ type: "runStarted", kind: "run" });
    const wrap = document.createElement("div");
    wrap.className = "demo-run";
    wrap.innerHTML = `<div class="demo-win" role="dialog"><div class="demo-tb"><img src="xljs-logo.png" alt=""><span></span><button type="button" aria-label="${D.window.close}">✕</button></div><iframe src="form.html?preview=1"></iframe></div>`;
    wrap.querySelector(".demo-tb span").textContent = D.form.title || "商品マスター";
    wrap.querySelector("button").onclick = () => closeRun(0);
    document.body.append(wrap);
    runWin = { wrap, frame: wrap.querySelector("iframe"), rows: form().fields.find((f) => f.table).table.rows };
  }
  function closeRun(code) {
    if (!runWin) return;
    runWin.wrap.remove();
    runWin = null;
    post({ type: "runEnded", kind: "run", code, stopped: code === null });
  }

  // The form in the run's window (bridge.js in a frame: "\x01" + JSON).
  window.addEventListener("message", (e) => {
    if (chatFrame && e.source === chatFrame.contentWindow && e.data && e.data.demo === "download") {
      post({ type: "download", name: ENTRY, text: D.after, target: ENTRY, old: files[ENTRY], files: Object.keys(files) });
      return;
    }
    if (!runWin || e.source !== runWin.frame.contentWindow || typeof e.data !== "string" || e.data[0] !== "\x01") return;
    let m;
    try { m = JSON.parse(e.data.slice(1)); } catch { return; }
    const reply = (x) => runWin.frame.contentWindow.postMessage("\x02" + JSON.stringify(x), "*");
    const set = (r) => {
      // The fields in the order of the table's columns.
      ["code", "name", "price", "note"].forEach((k, i) => {
        const v = r ? r[i] : "";
        reply({ type: "picked", field: k, value: v === null || v === undefined ? "" : String(v), quiet: true });
      });
    };
    switch (m.type) {
      case "ready": reply(form()); break;
      case "check":
        if (!m.change) break;
        if (m.field === "word") {
          const w = String(m.values.word || "");
          runWin.shown = runWin.rows.filter((r) => String(r[1]).includes(w));
          const t = form().fields.find((f) => f.table).table;
          reply(Object.assign({ type: "table", field: "list" }, t, { rows: runWin.shown }));
          set(null);
        } else if (m.field === "list") {
          const sel = [].concat(m.values.list ?? [])[0];
          const rows = runWin.shown || runWin.rows;
          set(typeof sel === "number" ? rows[sel] : null);
        }
        break;
      case "button":
        if (m.index === 0) set(null);
        reply({ type: "errors", errors: {}, error: m.index === 0 ? "" : D.texts.demoNoSave });
        break;
      case "submit": reply({ type: "errors", errors: {}, error: D.texts.demoNoSave }); break;
      case "cancel": case "close": closeRun(0); break;
    }
  });

  const css = document.createElement("style");
  css.textContent = `
.demo-run { position: fixed; inset: 0; z-index: 3000; background: rgba(0,0,0,.18); display: flex; align-items: center; justify-content: center; }
.demo-win { width: min(820px, 92vw); height: min(640px, 88vh); display: flex; flex-direction: column; background: #fff; border: 1px solid #8a8a8a; border-radius: 8px; overflow: hidden; box-shadow: 0 12px 40px rgba(0,0,0,.35); }
.demo-tb { flex: none; height: 32px; display: flex; align-items: center; gap: 8px; padding-left: 12px; background: #f3f3f3; color: #1b1b1b; font: 12px "Yu Gothic UI", "Meiryo UI", system-ui, sans-serif; }
.demo-tb img { width: 16px; height: 16px; }
.demo-tb span { flex: 1; }
.demo-tb button { width: 46px; height: 32px; min-width: 0; border: 0; border-radius: 0; background: transparent; box-shadow: none; color: inherit; font-size: 13px; cursor: pointer; }
.demo-tb button:hover { background: #c42b1c; color: #fff; }
.demo-win iframe { flex: 1; width: 100%; border: 0; }
.xljs-tb-btn svg { width: 10px; height: 10px; fill: none; stroke: currentColor; stroke-width: 1; }
`;
  document.head.append(css);
  // The title bar's glyphs are those of Windows' icon font: drawn here.
  const glyphs = {
    minimize: '<svg viewBox="0 0 10 10"><path d="M0 5.5h10"/></svg>',
    maximize: '<svg viewBox="0 0 10 10"><rect x=".5" y=".5" width="9" height="9"/></svg>',
    close: '<svg viewBox="0 0 10 10"><path d="M0 0l10 10M10 0L0 10"/></svg>',
  };
  document.addEventListener("DOMContentLoaded", () => {
    for (const b of document.querySelectorAll(".xljs-tb-btn")) b.innerHTML = glyphs[b.dataset.xljsWindow] || "";
  });
})();
