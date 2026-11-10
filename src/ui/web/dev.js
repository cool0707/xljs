"use strict";
// The development mode's window (xljs dev; the other side is src/dev/mod.rs):
// an AI chat (a browser pane that Rust puts over #chat-view) beside the
// canvas: the code, the differences of a downloaded answer, the preview,
// earlier versions and the output of runs. Messages to Rust are JSON
// {op, ...}; Rust answers with {type, ...} prefixed by \x02.
(() => {
  const wv = window.chrome && window.chrome.webview;
  const $ = (id) => document.getElementById(id);
  const el = (tag, props = {}, ...children) => {
    const e = Object.assign(document.createElement(tag), props);
    for (const c of children) if (c !== null && c !== undefined) e.append(c);
    return e;
  };
  const icon = (cls) => el("i", { className: cls });
  const fmt = (s, ...args) => String(s).replace(/\{(\d+)\}/g, (m, i) => (args[i] === undefined ? m : args[i]));
  const send = (m) => { if (wv) wv.postMessage(JSON.stringify(m)); };
  // Dropped files reach Rust as paths (WebView2 113+).
  const canAttach = !!(wv && typeof wv.postMessageWithAdditionalObjects === "function");

  let L = {}; // texts
  const st = {
    project: null, // { path, dir, entry, app, html, title, files }
    file: "", // the file in the editor
    attach: "", // the wizard's attachment offered over the code (a file of the project)
    saved: "", // its text as saved
    dirty: false,
    chat: "", // the URL of the chat pane ("" = the start)
    running: false,
    pending: null, // a downloaded answer: { name, text, target, old }
    versionView: null, // { file, n, text } compared in the diff tab
    failure: null, // the last failed run: { prompt, location }
    layout: null, // { groups: [{ id, tabs, active, size }], closed: [tab] } (layout)
    tabDrag: false, // a tab is being dragged
    fileDrag: false, // the file in the editor is dragged out (initFileDrag)
    menu: null, // the open menu's element (menu)
  };

  // ------------------------------------------------------------ drop

  // A script dropped on the window is opened (Rust checks what it is).
  function initDrop() {
    const hasFiles = (e) => canAttach && !st.fileDrag && e.dataTransfer && Array.from(e.dataTransfer.types || []).includes("Files");
    let depth = 0;
    const over = (on) => document.body.classList.toggle("drop-over", on);
    document.addEventListener("dragenter", (e) => { if (hasFiles(e)) { depth++; over(true); } });
    document.addEventListener("dragleave", (e) => { if (hasFiles(e) && --depth <= 0) { depth = 0; over(false); } });
    document.addEventListener("dragover", (e) => {
      // Dropping must never replace the page.
      e.preventDefault();
      e.dataTransfer.dropEffect = hasFiles(e) ? "copy" : "none";
    });
    document.addEventListener("drop", (e) => {
      e.preventDefault();
      depth = 0;
      over(false);
      if (!hasFiles(e)) return;
      const files = Array.from(e.dataTransfer.files || []);
      if (!files.length) return;
      save();
      wv.postMessageWithAdditionalObjects(JSON.stringify({ op: "drop" }), files);
    });
  }

  // The handle by the file's name drags the file out of the window (Rust
  // starts a drag as from Explorer): dropped on the chat, it is attached.
  // Its own drop on this page is not one to open.
  function initFileDrag() {
    dragHandle($("file-drag"), () => (st.project && st.file) || "");
    // The attachment of the wizard (the macros and sheets of a port).
    dragHandle($("attach-drag"), () => (st.project && st.attach) || "");
    $("attach-close").onclick = () => showAttach("");
    // The drag has ended once the mouse moves with no button down.
    const end = (e) => { if (st.fileDrag && !(e.buttons & 1)) st.fileDrag = false; };
    document.addEventListener("pointermove", end, true);
    document.addEventListener("pointerdown", () => { st.fileDrag = false; }, true);
    document.addEventListener("drop", () => { st.fileDrag = false; });
  }

  // A handle that drags a file of the project (file() gives its name, ""
  // for none) out of the window.
  function dragHandle(h, file) {
    h.addEventListener("pointerdown", (e) => {
      if (e.button !== 0 || !file()) return;
      const x0 = e.clientX;
      const y0 = e.clientY;
      const move = (ev) => {
        if (Math.abs(ev.clientX - x0) + Math.abs(ev.clientY - y0) < 6) return;
        done();
        save();
        st.fileDrag = true;
        send({ op: "dragFile", file: file() });
      };
      const done = () => {
        h.removeEventListener("pointermove", move);
        h.removeEventListener("pointerup", done);
        h.removeEventListener("pointercancel", done);
      };
      try { h.setPointerCapture(e.pointerId); } catch { /* not down any more */ }
      h.addEventListener("pointermove", move);
      h.addEventListener("pointerup", done);
      h.addEventListener("pointercancel", done);
    });
  }

  // The bar over the code offering the wizard's attachment ("" hides it).
  function showAttach(file) {
    st.attach = file || "";
    $("attach-bar").hidden = !st.attach;
    $("attach-name").textContent = st.attach;
  }

  // ------------------------------------------------------------ layout

  // The panes are groups side by side (like an editor's split view), each
  // with its tabs: the AI chat ("chat") and the canvas' panels. A tab is
  // dragged to reorder it, into another group, or to a group's left or
  // right edge (a new group there); the chat is also dragged by its bar.
  // A group with the chat alone shows no tabs (the chat's bar instead).
  // A tab is closed from its menu (right click) and opened again with the
  // "+" at the end of a group's tabs (or of the chat's bar); a closed tab
  // that something shows (setTab) opens again by itself.
  // The layout is kept per user (localStorage).
  const TABS = ["chat", "code", "diff", "preview", "history", "output"];
  const MAX_GROUPS = 4;
  const tabButton = (tab) => document.querySelector(`button[data-tab="${tab}"]`);
  const panelOf = (tab) => $("panel-" + tab);

  function defaultLayout() {
    return {
      groups: [
        { id: "g1", tabs: ["chat"], active: "chat", size: 42 },
        { id: "g2", tabs: ["code", "diff", "preview", "history", "output"], active: "code", size: 58 },
      ],
      closed: [],
    };
  }

  // Every tab in exactly one group or closed, no empty group, sizes adding
  // up to 100.
  function normalizeLayout(l) {
    const out = { groups: [], closed: [] };
    const seen = new Set();
    for (const g of (l && Array.isArray(l.groups) ? l.groups : [])) {
      const tabs = (Array.isArray(g.tabs) ? g.tabs : []).filter((t) => TABS.includes(t) && !seen.has(t));
      for (const t of tabs) seen.add(t);
      if (!tabs.length) continue;
      out.groups.push({ id: String(g.id || ""), tabs, active: tabs.includes(g.active) ? g.active : tabs[0], size: Number(g.size) > 0 ? Number(g.size) : 50 });
    }
    if (!out.groups.length) return defaultLayout();
    out.closed = TABS.filter((t) => !seen.has(t) && l && Array.isArray(l.closed) && l.closed.includes(t));
    const rest = TABS.filter((t) => !seen.has(t) && !out.closed.includes(t));
    if (rest.length) (out.groups.find((g) => g.tabs.includes("code")) || out.groups[out.groups.length - 1]).tabs.push(...rest);
    while (out.groups.length > MAX_GROUPS) {
      const g = out.groups.pop();
      out.groups[out.groups.length - 1].tabs.push(...g.tabs);
    }
    // New groups (and repeated ids) get ids that no other group has.
    const ids = new Set();
    for (const g of out.groups) {
      if (g.id && !ids.has(g.id)) ids.add(g.id);
      else g.id = "";
    }
    let n = 0;
    for (const g of out.groups) {
      if (g.id) continue;
      while (ids.has("g" + ++n));
      g.id = "g" + n;
      ids.add(g.id);
    }
    const sum = out.groups.reduce((a, g) => a + g.size, 0);
    for (const g of out.groups) g.size = (g.size / sum) * 100;
    return out;
  }

  function loadLayout() {
    let l = null;
    try { l = JSON.parse(localStorage.getItem("xljs.dev.layout") || "null"); } catch { /* defaults */ }
    if (l && !l.groups && (l.split !== undefined || l.swap !== undefined)) {
      // The layout before the groups: the chat and the canvas side by side.
      const d = defaultLayout();
      const split = Math.min(0.8, Math.max(0.2, Number(l.split) || 0.42));
      d.groups[0].size = split * 100;
      d.groups[1].size = (1 - split) * 100;
      if (l.swap) d.groups.reverse();
      l = d;
    }
    st.layout = normalizeLayout(l || defaultLayout());
  }
  function saveLayout() {
    try { localStorage.setItem("xljs.dev.layout", JSON.stringify(st.layout)); } catch { /* not kept */ }
  }

  const groupOf = (tab) => st.layout.groups.find((g) => g.tabs.includes(tab));
  const isSoloChat = (g) => g.tabs.length === 1 && g.tabs[0] === "chat";
  // The group of the code (or, when the code is closed, the first group
  // that is not the chat alone): the welcome and the wizard cover it and a
  // closed tab opens there again.
  const homeGroup = () => groupOf("code") || st.layout.groups.find((g) => !isSoloChat(g)) || st.layout.groups[0];
  const openTabs = () => st.layout.groups.reduce((n, g) => n + g.tabs.length, 0);
  const activeOf = (g) => (g.tabs.includes(g.active) ? g.active : g.tabs[0]);
  // The welcome or the wizard covers the group of the code.
  function covered() {
    const a = activeOf(homeGroup());
    if (st.wizard) return "wizard";
    if (st.showWelcome || (!st.project && (a === "code" || a === "history" || a === "preview"))) return "welcome";
    return "";
  }
  // Whether a tab's panel is on screen.
  function shown(tab) {
    const g = groupOf(tab);
    if (!g || activeOf(g) !== tab) return false;
    return g !== homeGroup() || !covered();
  }

  const groupEls = new Map(); // group id -> element
  function groupEl(g) {
    let e = groupEls.get(g.id);
    if (!e) {
      const add = el("button", { type: "button", className: "tab-add" }, icon("ic ic-plus"));
      add.onclick = () => addMenu(add, e.dataset.gid);
      e = el("section", { className: "pane tab-group" }, el("nav", { className: "tabs" }, add), el("div", { className: "group-body" }));
      e.dataset.gid = g.id;
      $("panes").append(e);
      groupEls.set(g.id, e);
    }
    return e;
  }

  // Moves an element to a new parent, keeping what it scrolled to (moving
  // an element out of the document resets its scrolling).
  function moveTo(parent, e) {
    if (e.parentNode === parent) return;
    const scrolled = [e, ...e.querySelectorAll(".cm-scroller, .diff, .output, .history, .start")].filter((x) => x.scrollTop || x.scrollLeft)
      .map((x) => [x, x.scrollTop, x.scrollLeft]);
    parent.append(e);
    for (const [x, top, left] of scrolled) { x.scrollTop = top; x.scrollLeft = left; }
  }

  // Puts the tabs and the panels into their groups (an element only moves
  // when its group changes: a moved frame reloads) and shows the active ones.
  function applyLayout() {
    const groups = st.layout.groups;
    const home = homeGroup();
    // Closed tabs wait in the stash.
    for (const t of st.layout.closed) {
      if (tabButton(t).parentNode !== $("tabs")) $("tabs").append(tabButton(t));
      moveTo($("stash"), panelOf(t));
    }
    groups.forEach((g, i) => {
      const e = groupEl(g);
      const nav = e.querySelector(".tabs");
      const add = nav.querySelector(".tab-add");
      const body = e.querySelector(".group-body");
      g.tabs.forEach((t, j) => {
        const b = tabButton(t);
        if (nav.children[j] !== b) nav.insertBefore(b, nav.children[j] || null);
        moveTo(body, panelOf(t));
      });
      if (nav.lastElementChild !== add) nav.append(add);
      add.hidden = !st.layout.closed.length;
      add.title = L.tabAdd || "";
      if (g === home) { moveTo(body, $("welcome")); moveTo(body, $("wizard")); }
      e.style.order = String(i * 2);
      e.style.flex = `${g.size} 1 0`;
      e.classList.toggle("solo-chat", isSoloChat(g));
    });
    const chat = groupOf("chat");
    $("chat-add").hidden = !st.layout.closed.length || !chat || !groupEl(chat).classList.contains("solo-chat");
    for (const [id, e] of groupEls) {
      if (groups.some((g) => g.id === id)) continue;
      e.remove();
      groupEls.delete(id);
    }
    // A splitter between each two visible groups.
    for (const s of $("panes").querySelectorAll(":scope > .splitter")) s.remove();
    for (let i = 1; i < groups.length; i++) {
      const s = el("div", { className: "splitter" });
      s.style.order = String(i * 2 - 1);
      initSplitter(s, groups[i - 1], groups[i]);
      $("panes").append(s);
    }
    showPanels();
  }

  function showPanels() {
    const cover = covered();
    const codeGroup = homeGroup();
    for (const g of st.layout.groups) {
      const active = activeOf(g);
      for (const t of g.tabs) {
        const b = tabButton(t);
        b.classList.toggle("on", t === active);
        panelOf(t).hidden = t !== active || (g === codeGroup && !!cover);
      }
    }
    $("welcome").hidden = cover !== "welcome";
    $("wizard").hidden = cover !== "wizard";
    if (shown("preview")) showPreviewHint();
    syncBrowser();
  }

  function setTab(tab) {
    const reopened = st.layout.closed.includes(tab);
    if (reopened) openTab(tab, tab === "chat" ? st.layout.groups[0] : homeGroup());
    const g = groupOf(tab);
    g.active = tab;
    if (tab !== "code" && g === homeGroup()) { st.showWelcome = false; st.wizard = false; }
    saveLayout();
    if (reopened) applyLayout();
    else showPanels();
    if (tab === "code" && shown("code")) editor.focus();
  }

  // Puts a closed tab at the end of a group's tabs (the caller saves and
  // lays out).
  function openTab(tab, g) {
    st.layout.closed = st.layout.closed.filter((t) => t !== tab);
    g.tabs.push(tab);
    g.active = tab;
  }

  // Closes a tab (not the last one). A group left empty goes away and its
  // width goes to its neighbour.
  function closeTab(tab) {
    const g = groupOf(tab);
    if (!g || openTabs() <= 1) return;
    const i = g.tabs.indexOf(tab);
    g.tabs.splice(i, 1);
    if (g.active === tab) g.active = g.tabs[Math.min(i, g.tabs.length - 1)];
    const groups = st.layout.groups;
    if (!g.tabs.length) {
      const k = groups.indexOf(g);
      groups.splice(k, 1);
      groups[Math.max(0, k - 1)].size += g.size;
    }
    st.layout.closed.push(tab);
    st.layout = normalizeLayout(st.layout);
    saveLayout();
    applyLayout();
  }

  // The "+" of a group: a menu of the closed tabs; the one chosen opens in
  // that group.
  function addMenu(anchor, gid) {
    const r = anchor.getBoundingClientRect();
    openMenu(r.left, r.bottom, st.layout.closed.map((tab) => ({
      label: tabLabel(tab),
      action: () => {
        const g = st.layout.groups.find((x) => x.id === gid);
        if (!g || !st.layout.closed.includes(tab)) return;
        openTab(tab, g);
        if (tab !== "code" && g === homeGroup()) { st.showWelcome = false; st.wizard = false; }
        st.layout = normalizeLayout(st.layout);
        saveLayout();
        applyLayout();
        if (tab === "code" && shown("code")) editor.focus();
      },
    })));
  }

  // The menu of the "Prompt" button: what to copy for the AI chat.
  function introMenu(anchor) {
    const r = anchor.getBoundingClientRect();
    const item = (label, mode, disabled) => ({
      label: el("span", {}, icon("ic ic-clipboard"), el("span", { textContent: label || "" })),
      action: () => send({ op: "intro", mode }),
      disabled,
    });
    openMenu(r.left, r.bottom, [
      item(L.introPrompt, "prompt", false),
      item(L.introBoth, "", false),
      item(L.introScript, "script", !st.project),
    ]);
  }

  // The menu of a tab (right click); the last tab has none (it cannot be
  // closed).
  function tabMenu(tab, x, y) {
    if (openTabs() <= 1) return;
    openMenu(x, y, [
      { label: el("span", {}, icon("ic ic-x"), el("span", { textContent: L.tabClose || "" })), action: () => closeTab(tab) },
    ]);
  }

  // ------------------------------------------------------------ menu

  // A small menu at (x, y): items {label (an element), action, disabled}.
  // It closes on a choice, a press elsewhere, Esc or when the window loses
  // the focus. The browser pane leaves out the part under the menu.
  function closeMenu() {
    if (!st.menu) return;
    st.menu.remove();
    st.menu = null;
    document.removeEventListener("pointerdown", outside, true);
    document.removeEventListener("keydown", menuKey, true);
    window.removeEventListener("blur", closeMenu);
    window.removeEventListener("resize", closeMenu);
    syncBrowser();
  }
  function outside(e) { if (st.menu && !st.menu.contains(e.target)) closeMenu(); }
  function menuKey(e) {
    if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); closeMenu(); return; }
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    const items = Array.from(st.menu.querySelectorAll("button:not(:disabled)"));
    if (!items.length) return;
    const i = items.indexOf(document.activeElement);
    const n = e.key === "ArrowDown" ? (i + 1) % items.length : (i <= 0 ? items.length - 1 : i - 1);
    items[n].focus();
  }
  function openMenu(x, y, items) {
    closeMenu();
    if (!items.length) return;
    const m = el("div", { className: "menu" }, ...items.map((it) => {
      const b = el("button", { type: "button", disabled: !!it.disabled }, it.label);
      b.onclick = () => { closeMenu(); it.action(); };
      return b;
    }));
    m.addEventListener("contextmenu", (e) => e.preventDefault());
    document.body.append(m);
    const r = m.getBoundingClientRect();
    m.style.left = Math.max(0, Math.min(x, window.innerWidth - r.width - 2)) + "px";
    m.style.top = Math.max(0, Math.min(y, window.innerHeight - r.height - 2)) + "px";
    st.menu = m;
    document.addEventListener("pointerdown", outside, true);
    document.addEventListener("keydown", menuKey, true);
    window.addEventListener("blur", closeMenu);
    window.addEventListener("resize", closeMenu);
    const first = m.querySelector("button:not(:disabled)");
    if (first) first.focus();
    syncBrowser();
  }
  // The part of the open menu over an element ([x, y, width, height] from
  // the element's top left), or null. The browser pane leaves it out so
  // that the menu shows (the pane is a window over the page).
  function menuOver(e) {
    if (!st.menu) return null;
    const a = st.menu.getBoundingClientRect();
    const b = e.getBoundingClientRect();
    const l = Math.max(a.left, b.left), t = Math.max(a.top, b.top);
    const r = Math.min(a.right, b.right), btm = Math.min(a.bottom, b.bottom);
    if (l >= r || t >= btm) return null;
    // A little more for the menu's shadow.
    const m = 6;
    return [l - b.left - m, t - b.top - m, r - l + 2 * m, btm - t + 2 * m];
  }

  // The native browser pane follows #chat-view; it is hidden while the
  // start of the chat pane shows, the chat is not on screen or a tab is
  // dragged (the pane would hide the drop places and take the mouse). A
  // menu over it is cut out of it.
  let syncQueued = false;
  function syncBrowser() {
    if (syncQueued) return;
    syncQueued = true;
    requestAnimationFrame(() => {
      syncQueued = false;
      const view = $("chat-view");
      const show = st.chat && shown("chat") && !st.tabDrag && !document.hidden;
      $("chat-start").hidden = !!st.chat;
      view.hidden = !st.chat;
      if (!show) { send({ op: "browser", path: "hide" }); return; }
      const r = view.getBoundingClientRect();
      if (r.width < 2 || r.height < 2) { send({ op: "browser", path: "hide" }); return; }
      const hole = menuOver(view);
      send({ op: "browser", path: "show", x: r.left, y: r.top, width: r.width, height: r.height, scale: window.devicePixelRatio || 1, holes: hole ? [hole] : [] });
    });
  }

  // A splitter between two groups shares their sizes.
  function initSplitter(sp, a, b) {
    sp.addEventListener("pointerdown", (e) => {
      if (e.button !== 0) return;
      sp.setPointerCapture(e.pointerId);
      document.body.classList.add("dragging");
      const ra = groupEl(a).getBoundingClientRect();
      const rb = groupEl(b).getBoundingClientRect();
      const left = Math.min(ra.left, rb.left);
      const width = ra.width + rb.width;
      const total = a.size + b.size;
      const min = Math.min(0.45, 160 / Math.max(1, width));
      const move = (ev) => {
        const f = Math.min(1 - min, Math.max(min, (ev.clientX - left) / width));
        a.size = total * f;
        b.size = total - a.size;
        groupEl(a).style.flex = `${a.size} 1 0`;
        groupEl(b).style.flex = `${b.size} 1 0`;
        syncBrowser();
      };
      const up = () => {
        sp.removeEventListener("pointermove", move);
        sp.removeEventListener("pointerup", up);
        sp.removeEventListener("pointercancel", up);
        document.body.classList.remove("dragging");
        saveLayout();
      };
      sp.addEventListener("pointermove", move);
      sp.addEventListener("pointerup", up);
      sp.addEventListener("pointercancel", up);
    });
  }

  // ------------------------------------------------------------ dragging tabs

  // Where a dragged tab would go at (x, y): before a tab in a group's tabs
  // ({kind: "tab"}), into a group ({kind: "into"}) or beside it, as a new
  // group ({kind: "split", side}).
  function dropTarget(tab, x, y) {
    const at = document.elementFromPoint(x, y);
    const ge = at && at.closest("#panes > .tab-group");
    if (!ge) return null;
    const g = st.layout.groups.find((x) => x.id === ge.dataset.gid);
    if (!g) return null;
    const src = groupOf(tab);
    const nav = ge.querySelector(".tabs");
    const nr = nav.getBoundingClientRect();
    if (!ge.classList.contains("solo-chat") && y >= nr.top && y <= nr.bottom) {
      const btns = Array.from(nav.children).filter((b) => b.dataset.tab && !b.hidden);
      let before = null;
      for (const b of btns) {
        const r = b.getBoundingClientRect();
        if (x < r.left + r.width / 2) { before = b; break; }
      }
      const ref = before || btns[btns.length - 1];
      const r = ref.getBoundingClientRect();
      const mark = { left: (before ? r.left : r.right) - 1, top: nr.top + 2, width: 3, height: nr.height - 4 };
      return { kind: "tab", group: g, before: before ? before.dataset.tab : null, mark };
    }
    const r = ge.getBoundingClientRect();
    const body = ge.querySelector(".group-body").getBoundingClientRect();
    const fx = (x - r.left) / r.width;
    const alone = src.tabs.length === 1;
    const canSplit = !(alone && g === src) && (st.layout.groups.length < MAX_GROUPS || alone);
    if (canSplit && (fx < 0.25 || fx > 0.75)) {
      const side = fx < 0.25 ? "left" : "right";
      const half = r.width / 2;
      return { kind: "split", group: g, side, mark: { left: side === "left" ? r.left : r.left + half, top: body.top, width: half, height: body.bottom - body.top, area: true } };
    }
    if (g === src) return null;
    return { kind: "into", group: g, mark: { left: r.left, top: body.top, width: r.width, height: body.bottom - body.top, area: true } };
  }

  function moveTab(tab, t) {
    const groups = st.layout.groups;
    const src = groupOf(tab);
    src.tabs.splice(src.tabs.indexOf(tab), 1);
    if (src.active === tab) src.active = src.tabs[0];
    // A group that moves as a whole keeps its size; a group made from a
    // tab takes half of the group it splits.
    const whole = !src.tabs.length && src !== t.group;
    if (whole) groups.splice(groups.indexOf(src), 1);
    let dst = t.group;
    if (t.kind === "split") {
      dst = { id: whole ? src.id : "", tabs: [], active: tab, size: whole ? src.size : t.group.size / 2 };
      if (!whole) t.group.size /= 2;
      groups.splice(groups.indexOf(t.group) + (t.side === "right" ? 1 : 0), 0, dst);
    } else if (whole) {
      dst.size += src.size;
    }
    const i = t.kind === "tab" && t.before ? dst.tabs.indexOf(t.before) : -1;
    if (i >= 0) dst.tabs.splice(i, 0, tab);
    else dst.tabs.push(tab);
    dst.active = tab;
    if (tab !== "code" && dst === homeGroup()) { st.showWelcome = false; st.wizard = false; }
    st.layout = normalizeLayout(st.layout);
    saveLayout();
    applyLayout();
  }

  // A press on a tab (or the chat's bar) that moves past a few pixels drags
  // it; the click that ends a drag is not a click.
  let dragEnded = false;
  function startTabDrag(e, tab, handle, label) {
    if (e.button !== 0) return;
    const x0 = e.clientX;
    const y0 = e.clientY;
    const id = e.pointerId;
    let ghost = null;
    let hint = null;
    let target = null;
    const move = (ev) => {
      if (ev.pointerId !== id) return;
      if (!ghost) {
        if (Math.abs(ev.clientX - x0) + Math.abs(ev.clientY - y0) < 6) return;
        ghost = el("div", { className: "tab-ghost" }, label());
        hint = el("div", { className: "drop-hint" });
        hint.hidden = true;
        document.body.append(ghost, hint);
        document.body.classList.add("tab-dragging");
        st.tabDrag = true;
        syncBrowser();
      }
      ghost.style.left = ev.clientX + 12 + "px";
      ghost.style.top = ev.clientY + 12 + "px";
      target = dropTarget(tab, ev.clientX, ev.clientY);
      hint.hidden = !target;
      if (target) {
        const m = target.mark;
        Object.assign(hint.style, { left: m.left + "px", top: m.top + "px", width: m.width + "px", height: m.height + "px" });
        hint.classList.toggle("area", !!m.area);
      }
    };
    const end = (ev, cancel) => {
      if (ev && ev.pointerId !== undefined && ev.pointerId !== id) return;
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", up);
      handle.removeEventListener("pointercancel", cancelled);
      document.removeEventListener("keydown", key, true);
      if (!ghost) return;
      dragEnded = true;
      ghost.remove();
      hint.remove();
      document.body.classList.remove("tab-dragging");
      st.tabDrag = false;
      if (!cancel && target) moveTab(tab, target);
      syncBrowser();
    };
    const up = (ev) => end(ev, false);
    const cancelled = (ev) => end(ev, true);
    const key = (ev) => { if (ev.key === "Escape" && ghost) { ev.preventDefault(); ev.stopPropagation(); end(null, true); } };
    // Moves come to the handle while the button is down (a click on it
    // still is a click).
    try { handle.setPointerCapture(id); } catch { /* not down any more */ }
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", up);
    handle.addEventListener("pointercancel", cancelled);
    document.addEventListener("keydown", key, true);
  }

  // The icon and the name of a tab (what a drag shows by the mouse).
  function tabLabel(tab) {
    const b = tabButton(tab);
    return el("span", {}, ...Array.from(b.children).filter((c) => !c.classList.contains("badge")).map((c) => c.cloneNode(true)));
  }

  function initTabs() {
    for (const tab of TABS) {
      const b = tabButton(tab);
      b.onclick = () => { st.wizard = false; st.showWelcome = false; setTab(tab); };
      b.addEventListener("pointerdown", (e) => startTabDrag(e, tab, b, () => tabLabel(tab)));
      b.addEventListener("contextmenu", (e) => { e.preventDefault(); tabMenu(tab, e.clientX, e.clientY); });
    }
    // The chat's bar: its empty space, the grip and its buttons (not the
    // address, where a press selects text).
    const bar = $("chat-bar");
    bar.addEventListener("pointerdown", (e) => {
      if (e.target.closest("input, select, textarea")) return;
      const handle = e.target.closest("button") || bar;
      startTabDrag(e, "chat", handle, () => tabLabel("chat"));
    });
    // The chat alone in its group has no tab: its bar has the menu and the "+".
    bar.addEventListener("contextmenu", (e) => {
      if (e.target.closest("input, select, textarea")) return;
      e.preventDefault();
      tabMenu("chat", e.clientX, e.clientY);
    });
    $("chat-add").onclick = () => addMenu($("chat-add"), groupOf("chat").id);
    document.addEventListener("pointerdown", () => { dragEnded = false; }, true);
    document.addEventListener("click", (e) => {
      if (dragEnded) { dragEnded = false; e.preventDefault(); e.stopPropagation(); }
    }, true);
  }

  // ------------------------------------------------------------ status

  function status(text, cls) {
    $("status").textContent = text || "";
    $("status").className = "status " + (cls || "");
  }

  function updateButtons() {
    const has = !!st.project;
    $("btn-run").disabled = !has || st.running;
    $("btn-debug").disabled = !has || st.running;
    $("btn-stop").disabled = !st.running;
    $("btn-folder").disabled = !has;
    $("btn-pack").disabled = !has;
    $("btn-save").disabled = !has;
    $("btn-format").disabled = !has;
    $("btn-notify").disabled = !has;
    document.title = has ? `${st.project.entry} - ${L.title}` : L.title || "";
    $("app-title").textContent = document.title;
    // The window's text in the taskbar (web/titlebar.js's messages).
    if (window.__xljsTitleBar && window.chrome && chrome.webview) chrome.webview.postMessage("\x01window:title:" + document.title);
    showFileDir();
  }

  // The folder of the file in the editor, next to the file's choice.
  function showFileDir() {
    let dir = "";
    if (st.project) {
      const sep = st.project.dir.includes("\\") ? "\\" : "/";
      const sub = (st.file || "").split("/").slice(0, -1).join(sep);
      dir = sub ? st.project.dir.replace(/[\\/]+$/, "") + sep + sub : st.project.dir;
    }
    $("file-dir").textContent = dir;
    $("file-dir").title = dir;
  }

  // ------------------------------------------------------------ editor

  let editor = null;

  function setEditor(file, text) {
    st.file = file;
    st.saved = text;
    st.dirty = false;
    editor.set(file, text);
    $("file-select").value = file;
    showFileDir();
    $("save-state").textContent = "";
    $("history-title").textContent = fmt(L.historyOf, file);
    send({ op: "edit", file });
    send({ op: "versions", file });
  }

  function fillFiles(files) {
    const s = $("file-select");
    s.replaceChildren(...files.map((f) => el("option", { value: f, textContent: f })));
    s.value = st.file;
  }

  let saveTimer = 0;
  function edited() {
    st.dirty = editor.value() !== st.saved;
    $("save-state").textContent = st.dirty ? L.unsaved : "";
    clearTimeout(saveTimer);
    if (st.dirty) saveTimer = setTimeout(save, 1500);
  }

  function save() {
    clearTimeout(saveTimer);
    if (!st.project || !st.dirty) return;
    const text = editor.value();
    send({ op: "save", file: st.file, text });
    st.saved = text;
    st.dirty = false;
    $("save-state").textContent = L.saving;
  }

  /** Puts the cursor on line (1-based) of the editor and marks it. */
  function gotoLine(line, col) {
    setTab("code");
    editor.goto(line, col);
  }

  /** Formats the file in the editor (Prettier). */
  async function formatCode() {
    if (!st.project || !editor.canFormat()) { status(L.formatUnsupported, "error"); return; }
    try {
      await editor.format();
      status("");
    } catch (e) {
      const line = e && e.loc && e.loc.start && e.loc.start.line;
      const msg = String((e && e.message) || e).split("\n")[0].replace(/\s*\(\d+:\d+\)\s*$/, "");
      status(line ? fmt(L.formatFailedAt, line, msg) : fmt(L.formatFailed, msg), "error");
      if (line) editor.goto(line, e.loc.start.column);
    }
    editor.focus();
  }

  function initEditor() {
    editor = devEditor.create($("code"), { onChange: edited, onFormat: formatCode });
    $("file-select").addEventListener("change", () => {
      save();
      send({ op: "read", file: $("file-select").value });
      st.wantEditor = $("file-select").value;
    });
    $("btn-save").onclick = save;
    $("btn-format").onclick = formatCode;
    // Saves first: what is told is the files as they are on disk.
    $("btn-notify").onclick = () => { save(); send({ op: "notify", file: st.file }); };
  }

  // ------------------------------------------------------------ diff

  function renderDiff(oldText, newText) {
    const a = devDiff.split(oldText), b = devDiff.split(newText);
    const edits = devDiff.lines(a, b);
    let add = 0, del = 0;
    for (const e of edits) { if (e.op === "+") add++; else if (e.op === "-") del++; }
    $("diff-stats").replaceChildren(el("span", { className: "add" }, "+" + add), " ", el("span", { className: "del" }, "−" + del));
    const rows = [];
    for (const h of devDiff.hunks(edits)) {
      if (h.skip) { rows.push(el("div", { className: "skip" }, fmt(L.sameLines, h.skip))); continue; }
      const text = h.op === "+" ? b[h.b] : a[h.a];
      rows.push(el("div", { className: "line " + (h.op === "+" ? "add" : h.op === "-" ? "del" : "") },
        el("span", { className: "num" }, h.a >= 0 ? String(h.a + 1) : ""),
        el("span", { className: "num" }, h.b >= 0 ? String(h.b + 1) : ""),
        el("span", { className: "sign" }, h.op === "=" ? " " : h.op === "-" ? "−" : "+"),
        el("span", { className: "text" }, text)));
    }
    if (!add && !del) rows.push(el("div", { className: "skip" }, L.noChanges));
    $("diff").replaceChildren(...rows);
    $("diff").scrollTop = 0;
  }

  function showDiff() {
    const p = st.pending, v = st.versionView;
    $("diff-empty").hidden = !!(p || v);
    $("diff-badge").hidden = !p;
    $("diff-apply").hidden = !(p || v);
    $("diff-discard").hidden = !(p || v);
    $("diff-target").hidden = !p;
    $("diff-rename-label").hidden = true;
    if (!p && !v) {
      $("diff").replaceChildren();
      $("diff-title").textContent = "";
      $("diff-stats").replaceChildren();
      return;
    }
    if (p) {
      $("diff-title").textContent = fmt(L.diffFrom, p.name);
      const t = $("diff-target");
      t.replaceChildren(el("option", { value: "", textContent: L.chooseTarget }),
        ...p.files.map((f) => el("option", { value: f, textContent: "→ " + f })),
        el("option", { value: "\u0000new", textContent: fmt(L.asNewFile, p.name) }));
      t.value = p.target === null ? "" : p.target;
      $("diff-apply-label").textContent = L.apply;
      $("diff-apply").disabled = !p.target;
      const canRename = !!(p.rename && st.project && p.target === st.project.entry);
      $("diff-rename-label").hidden = !canRename;
      if (canRename) $("diff-rename-text").textContent = fmt(L.renameTo, p.rename);
      if (p.target) renderDiff(p.old || "", p.text);
      else $("diff").replaceChildren(el("div", { className: "skip" }, L.chooseTargetHint));
    } else {
      $("diff-title").textContent = fmt(L.diffVersion, v.file, v.n);
      $("diff-apply-label").textContent = fmt(L.restoreThis, v.n);
      $("diff-apply").disabled = false;
      // From the current text to the version: what restoring would change.
      renderDiff(v.current, v.text);
    }
  }

  function initDiff() {
    $("diff-target").addEventListener("change", () => {
      const p = st.pending;
      if (!p) return;
      const v = $("diff-target").value;
      if (v === "\u0000new") {
        p.target = p.name.replace(/[\\/:*?"<>|]/g, "_");
        p.old = "";
        showDiff();
        return;
      }
      p.target = v || null;
      p.old = null;
      if (v) send({ op: "read", file: v });
      showDiff();
    });
    $("diff-apply").onclick = () => {
      if (st.pending && st.pending.target) {
        if (st.pending.target === st.file) save();
        const rename = st.pending.rename && st.pending.target === st.project.entry && $("diff-rename").checked ? st.pending.rename : "";
        send({ op: "apply", file: st.pending.target, text: st.pending.text, note: st.pending.name, rename });
        st.pending = null;
      } else if (st.versionView) {
        const v = st.versionView;
        if (v.file === st.file) save();
        send({ op: "restore", file: v.file, n: v.n });
        st.versionView = null;
      }
      showDiff();
    };
    $("diff-discard").onclick = () => {
      st.pending = null;
      st.versionView = null;
      showDiff();
      status(L.discarded);
    };
  }

  // ------------------------------------------------------------ history

  function renderHistory(file, list) {
    if (file !== st.file) return;
    const ul = $("history");
    if (!list.length) { ul.replaceChildren(el("li", { className: "muted" }, L.noVersions)); return; }
    ul.replaceChildren(...list.slice().reverse().map((v) => el("li", {},
      el("span", { className: "n" }, "#" + v.n),
      el("span", { className: "time" }, v.time),
      el("span", { className: "note" }, v.note),
      el("button", { type: "button", className: "small", onclick: () => send({ op: "version", file, n: v.n }) }, L.compare),
      el("button", { type: "button", className: "small", onclick: () => {
        if (confirm(fmt(L.confirmRestore, file, v.n))) {
          if (file === st.file) save();
          send({ op: "restore", file, n: v.n });
        }
      } }, L.restore))));
  }

  // ------------------------------------------------------------ preview

  // The preview shows by itself when its tab comes on screen: the app's
  // page, or the form of a script that opens one (ui.form; Rust checks,
  // previewSkipped otherwise). A form runs again when a file was saved
  // since (gen: the changes, done: the changes the shown form has).
  const pv = { gen: 1, done: 0, sent: 0, asked: false, auto: false, noForm: false };

  function showPreviewHint() {
    const p = st.project;
    if (!p) return;
    if (p.app) {
      $("preview-title").textContent = fmt(L.previewApp, p.html);
      $("preview-hint").textContent = L.previewAppHint;
      $("preview-refresh-label").textContent = L.refresh;
      if ($("preview-frame").hidden) loadAppPreview();
    } else {
      $("preview-title").textContent = L.previewForm;
      $("preview-hint").textContent = pv.noForm ? L.previewNoForm : L.previewFormHint;
      $("preview-refresh-label").textContent = L.previewRun;
      autoPreview();
    }
  }

  function autoPreview() {
    const p = st.project;
    if (!p || p.app || st.running || pv.asked || pv.done === pv.gen || !shown("preview")) return;
    pv.asked = true;
    pv.auto = true;
    pv.sent = pv.gen;
    send({ op: "preview", auto: true });
  }

  // A file changed: the form preview is out of date (and runs again when
  // it is on screen).
  function previewChanged() {
    pv.gen++;
    pv.noForm = false;
    if (shown("preview")) showPreviewHint();
  }

  function loadAppPreview() {
    const p = st.project;
    if (!p || !p.app || !p.html) return;
    const f = $("preview-frame");
    f.hidden = false;
    $("preview-error").hidden = true;
    f.src = "https://app.xljs/_project/" + p.html.split("/").map(encodeURIComponent).join("/") + "?t=" + Date.now();
  }

  let previewForm = null;
  function showFormPreview(form) {
    previewForm = form;
    pv.noForm = false;
    const f = $("preview-frame");
    $("preview-error").hidden = true;
    f.hidden = false;
    f.src = "form.html?preview=" + Date.now();
  }

  // Messages of the form in the preview frame (bridge.js): "\x01" + JSON.
  // The wizard's preview of the look (lookPreview) talks the same way.
  window.addEventListener("message", (e) => {
    const f = [$("preview-frame"), $("wz-look-frame")].find((x) => e.source === x.contentWindow);
    if (!f || typeof e.data !== "string" || e.data[0] !== "\x01") return;
    let m;
    try { m = JSON.parse(e.data.slice(1)); } catch { return; }
    const reply = (x) => f.contentWindow.postMessage("\x02" + JSON.stringify(x), "*");
    const form = f.id === "wz-look-frame" ? lookForm : previewForm;
    if (m.type === "ready" && form) reply(form);
    else if (f.id === "wz-look-frame" && (m.type === "cancel" || m.type === "close")) closeLookPreview();
    else if (m.type === "submit" || m.type === "button") reply({ type: "errors", errors: {}, error: L.previewNoRun });
  });

  function initPreview() {
    $("preview-refresh").onclick = () => {
      const p = st.project;
      if (!p) return;
      save();
      if (p.app) { loadAppPreview(); return; }
      pv.asked = true;
      pv.auto = false;
      pv.sent = pv.gen;
      send({ op: "preview" });
      status(L.previewRunning);
    };
    $("pick-toggle").onclick = () => (pick.on ? pickOff() : pickOn());
    $("pick-clear").onclick = () => { pickSet([]); };
    $("pick-copy").onclick = () => {
      if (!st.project) return;
      if (!pick.items.length) { status(L.pickNone, "error"); return; }
      save();
      send({ op: "pick", text: $("pick-text").value, full: $("pick-full").checked, elements: pick.items.map((x) => x.info) });
    };
    $("preview-frame").addEventListener("load", () => {
      pick.doc = null;
      if (!pick.on) return;
      pickAttach();
      // The same elements in the new page, where they are found again.
      const d = pick.doc;
      for (const x of pick.items) {
        let e = null;
        try { e = d && x.info.selector ? d.querySelector(x.info.selector) : null; } catch { /* not a selector */ }
        x.el = e;
      }
      pickMark();
    });
  }

  // ------------------------------------------------------------ pick

  // Picking elements of the preview for a request to the AI: a click
  // picks an element (Ctrl / Shift + click adds or removes one), the
  // request (what to change, the elements: a short name, a CSS path and
  // the HTML) is copied by Rust (op "pick", report::pick). The frame's page
  // is on the same origin (form.html, the app's folder under _project/), so
  // its document is reached directly; while picking, its clicks do nothing
  // else.
  const pick = { on: false, items: [], doc: null, hover: null };
  const PICK_STYLE = "[data-xljs-hover]{outline:2px dashed #e8590c!important;outline-offset:1px!important;cursor:crosshair!important}"
    + "[data-xljs-picked]{outline:2px solid #e8590c!important;outline-offset:1px!important}";

  function pickOn() {
    pick.on = true;
    $("pick-toggle").classList.add("on");
    $("pick-box").hidden = false;
    pickAttach();
    renderPick();
  }

  function pickOff() {
    pick.on = false;
    $("pick-toggle").classList.remove("on");
    $("pick-box").hidden = true;
    pickSet([]);
    pickDetach();
  }

  function pickDoc() {
    try { return $("preview-frame").contentDocument; } catch { return null; }
  }

  const PICK_EVENTS = [
    ["mouseover", (e) => {
      const t = e.target;
      if (!t || t.nodeType !== 1 || t === pick.hover) return;
      if (pick.hover) pick.hover.removeAttribute("data-xljs-hover");
      pick.hover = t;
      t.setAttribute("data-xljs-hover", "");
    }],
    ["mouseout", (e) => {
      if (e.target === pick.hover && !e.relatedTarget) { pick.hover.removeAttribute("data-xljs-hover"); pick.hover = null; }
    }],
    ["click", (e) => {
      e.preventDefault();
      e.stopImmediatePropagation();
      const t = e.target;
      if (!t || t.nodeType !== 1) return;
      const has = pick.items.find((x) => x.el === t);
      if (e.ctrlKey || e.shiftKey || e.metaKey) pickSet(has ? pick.items.filter((x) => x !== has) : [...pick.items, { el: t, info: pickInfo(t) }]);
      else pickSet(has && pick.items.length === 1 ? [] : [{ el: t, info: pickInfo(t) }]);
    }],
    ...["mousedown", "mouseup", "pointerdown", "pointerup", "dblclick", "auxclick", "dragstart", "submit"].map((t) => [t, (e) => { e.preventDefault(); e.stopImmediatePropagation(); }]),
  ];

  function pickAttach() {
    const d = pickDoc();
    if (!d || !d.documentElement || pick.doc === d) return;
    pickDetach();
    pick.doc = d;
    const s = d.createElement("style");
    s.id = "xljs-pick-style";
    s.textContent = PICK_STYLE;
    (d.head || d.documentElement).append(s);
    for (const [t, f] of PICK_EVENTS) d.addEventListener(t, f, true);
    pickMark();
  }

  function pickDetach() {
    const d = pick.doc;
    pick.doc = null;
    pick.hover = null;
    if (!d) return;
    try {
      for (const [t, f] of PICK_EVENTS) d.removeEventListener(t, f, true);
      const s = d.getElementById("xljs-pick-style");
      if (s) s.remove();
      for (const e of d.querySelectorAll("[data-xljs-hover], [data-xljs-picked]")) { e.removeAttribute("data-xljs-hover"); e.removeAttribute("data-xljs-picked"); }
    } catch { /* the page is gone */ }
  }

  function pickSet(items) {
    pick.items = items;
    pickMark();
    renderPick();
  }

  // Marks the picked elements in the frame.
  function pickMark() {
    const d = pick.doc;
    if (!d) return;
    try {
      for (const e of d.querySelectorAll("[data-xljs-picked]")) e.removeAttribute("data-xljs-picked");
      for (const x of pick.items) if (x.el && x.el.ownerDocument === d) x.el.setAttribute("data-xljs-picked", "");
    } catch { /* the page is gone */ }
  }

  function renderPick() {
    const list = $("pick-list");
    if (!pick.items.length) { list.replaceChildren(el("li", { className: "none" }, L.pickNone)); return; }
    list.replaceChildren(...pick.items.map((x) => el("li", { title: x.info.selector },
      el("span", {}, x.info.desc),
      el("button", { type: "button", onclick: () => pickSet(pick.items.filter((y) => y !== x)) }, icon("ic ic-x")))));
  }

  const cut = (t, n) => (t.length > n ? t.slice(0, n) + "…" : t);
  const squeeze = (t) => String(t || "").replace(/\s+/g, " ").trim();

  // What the AI is told about an element.
  function pickInfo(e) {
    const tag = e.tagName.toLowerCase();
    const d = e.ownerDocument;
    let label = "";
    try {
      const l = (e.id && d.querySelector(`label[for="${CSS.escape(e.id)}"]`)) || (tag !== "label" && e.closest("label"));
      if (l) label = squeeze(l.innerText);
    } catch { /* no label */ }
    const text = squeeze(e.innerText) || squeeze(e.value) || squeeze(e.getAttribute("placeholder")) || squeeze(e.getAttribute("aria-label")) || squeeze(e.getAttribute("title"));
    let desc = tag;
    if (e.getAttribute("type")) desc += `[type=${e.getAttribute("type")}]`;
    if (e.getAttribute("name")) desc += ` name="${e.getAttribute("name")}"`;
    const name = label && label !== text ? label : text;
    if (name) desc += `「${cut(name, 40)}」`;
    const c = e.cloneNode(true);
    for (const x of [c, ...c.querySelectorAll("[data-xljs-hover], [data-xljs-picked]")]) { x.removeAttribute("data-xljs-hover"); x.removeAttribute("data-xljs-picked"); }
    return { desc, selector: cssPath(e), html: cut(c.outerHTML.replace(/\n\s*\n/g, "\n"), 800) };
  }

  // A CSS path from the nearest id (or the body) to the element.
  function cssPath(e) {
    const parts = [];
    for (let x = e; x && x.nodeType === 1 && x.tagName !== "HTML"; x = x.parentElement) {
      const tag = x.tagName.toLowerCase();
      if (x.id && /^[A-Za-z][\w-]*$/.test(x.id)) { parts.unshift(tag + "#" + x.id); break; }
      if (tag === "body") { parts.unshift("body"); break; }
      let part = tag;
      const cls = [...x.classList].filter((c) => /^[A-Za-z_][\w-]*$/.test(c)).slice(0, 2);
      if (cls.length) part += "." + cls.join(".");
      const same = x.parentElement ? [...x.parentElement.children].filter((y) => y.tagName === x.tagName) : [];
      if (same.length > 1) part += `:nth-of-type(${same.indexOf(x) + 1})`;
      parts.unshift(part);
      if (parts.length >= 6) break;
    }
    return parts.join(" > ");
  }

  // ------------------------------------------------------------ runs

  function appendOutput(text, cls) {
    const out = $("output");
    const atEnd = out.scrollTop + out.clientHeight >= out.scrollHeight - 4;
    out.append(el("span", { className: cls || "" }, text));
    // Keep the output from growing without bound.
    while (out.childNodes.length > 5000) out.firstChild.remove();
    if (atEnd) out.scrollTop = out.scrollHeight;
  }

  function startRun(debug) {
    if (!st.project || st.running) return;
    save();
    $("output").replaceChildren();
    $("failure").hidden = true;
    $("output-badge").hidden = true;
    st.failure = null;
    send({ op: "run", debug });
  }

  function runStarted(m) {
    st.running = true;
    updateButtons();
    if (m.kind !== "preview") setTab("output");
    $("run-state").textContent = m.kind === "debug" ? L.debugging : m.kind === "preview" ? L.previewRunning : L.running;
    status($("run-state").textContent);
  }

  function runEnded(m) {
    st.running = false;
    updateButtons();
    if (m.kind === "preview") {
      status("");
      pv.asked = false;
      pv.auto = false;
      autoPreview();
      return;
    }
    autoPreview();
    const ok = m.code === 0;
    // Stopped by the user (the stop button, the stop key, the debugger
    // window closed): no failure to report.
    const byUser = m.code === null || m.stopped;
    const text = ok ? L.finished : m.stuck ? L.stoppedStuck : byUser ? L.killed : fmt(L.failedCode, m.code);
    $("run-state").textContent = text;
    status(text, ok ? "ok" : byUser ? "" : "error");
    if (ok || byUser) return;
    st.failure = { prompt: m.prompt || "", location: m.location || null };
    $("failure-title").textContent = m.stuck ? L.stuckTitle : L.errorTitle;
    $("failure-message").textContent = m.message || "";
    const at = $("failure-at");
    if (m.location) {
      at.hidden = false;
      at.textContent = fmt(L.at, m.location.file, m.location.line);
    } else at.hidden = true;
    $("failure").hidden = false;
    $("output-badge").hidden = false;
    setTab("output");
  }

  function failurePrompt() {
    const f = st.failure;
    if (!f) return "";
    let text = f.prompt;
    if ($("failure-full").checked && st.project) {
      const file = f.location ? f.location.file : st.project.entry;
      const src = file === st.file ? editor.value() : null;
      if (src !== null) text += "\n\n" + fmt(L.fullCode, file) + "\n```\n" + src.replace(/\s+$/, "") + "\n```";
    }
    return text;
  }

  async function copy(text, done) {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      // Without the clipboard API: through a selected textarea.
      const t = el("textarea", { value: text });
      document.body.append(t);
      t.select();
      document.execCommand("copy");
      t.remove();
    }
    status(done, "ok");
  }

  function initRun() {
    $("btn-run").onclick = () => startRun(false);
    $("btn-debug").onclick = () => startRun(true);
    $("btn-stop").onclick = () => send({ op: "stop" });
    $("output-clear").onclick = () => $("output").replaceChildren();
    $("failure-copy").onclick = () => copy(failurePrompt(), L.copiedPrompt);
    $("failure-at").onclick = () => {
      const loc = st.failure && st.failure.location;
      if (!loc) return;
      if (loc.file === st.file) { gotoLine(loc.line, loc.column); return; }
      save();
      st.wantEditor = loc.file;
      st.wantLine = loc;
      send({ op: "read", file: loc.file });
    };
  }

  // ------------------------------------------------------------ chat

  function openChat(url) {
    url = String(url || "").trim();
    if (!url) return;
    if (!/^https?:\/\//i.test(url)) url = "https://" + url;
    st.chat = url;
    $("chat-url").value = url;
    send({ op: "chat", url });
    syncBrowser();
  }

  // The logo of an AI chat: a bundled SVG (vendor/lobe-icons) or a Font
  // Awesome brand icon in the text color.
  const SERVICE_SVGS = { copilot: "copilot-color", m365: "copilot-color", gemini: "gemini-color", claude: "claude-color" };
  const SERVICE_FA = { openai: "fa-openai" };
  function serviceIcon(name) {
    if (SERVICE_SVGS[name]) return el("img", { className: "service-icon", src: `vendor/lobe-icons/${SERVICE_SVGS[name]}.svg`, alt: "" });
    return icon(SERVICE_FA[name] ? `fa-brands ${SERVICE_FA[name]} service-icon` : "ic ic-messages-square");
  }

  function initChat(services) {
    $("services").replaceChildren(...services.map((s) => el("button", { type: "button", className: "big", onclick: () => openChat(s.url) },
      serviceIcon(s.icon), el("span", {}, s.name))));
    $("chat-url-form").onsubmit = (e) => { e.preventDefault(); openChat($("chat-url-start").value); };
    $("chat-url").addEventListener("keydown", (e) => { if (e.key === "Enter") openChat($("chat-url").value); });
    $("chat-home").onclick = () => { st.chat = ""; send({ op: "chat", url: "" }); syncBrowser(); };
    $("chat-back").onclick = () => send({ op: "browser", path: "back" });
    $("chat-forward").onclick = () => send({ op: "browser", path: "forward" });
    $("chat-reload").onclick = () => send({ op: "browser", path: "reload" });
    $("chat-intro").onclick = $("chat-intro2").onclick = (e) => introMenu(e.currentTarget);
  }

  // ------------------------------------------------------------ project

  function openProject(m) {
    st.showWelcome = false;
    st.wizard = false;
    st.project = { path: m.path, dir: m.dir, entry: m.entry, app: m.app, html: m.html, title: m.title, files: m.files };
    showAttach("");
    st.pending = null;
    st.versionView = null;
    st.failure = null;
    previewForm = null;
    pickOff();
    pv.gen++;
    pv.asked = false;
    pv.noForm = false;
    $("preview-frame").hidden = true;
    $("preview-frame").removeAttribute("src");
    $("failure").hidden = true;
    $("output").replaceChildren();
    fillFiles(m.files);
    setEditor(m.entry, m.text);
    st.chat = m.chat || "";
    $("chat-url").value = st.chat;
    updateButtons();
    showDiff();
    setTab("code");
    syncBrowser();
    status(fmt(L.opened, m.path));
  }

  function renderRecent(list) {
    $("recent-title").hidden = !list.length;
    $("recent").replaceChildren(...list.map((r) => el("li", {},
      el("button", { type: "button", className: "link", title: r.path, onclick: () => send({ op: "open", path: r.path }) }, r.name),
      el("span", { className: "muted" }, " " + r.path))));
  }

  // ------------------------------------------------------------ wizard

  const wz = { books: [] };
  const radio = (name) => { const r = document.querySelector(`input[name="${name}"]:checked`); return r ? r.value : ""; };
  const setRadio = (name, v) => { const r = document.querySelector(`input[name="${name}"][value="${v}"]`); if (r) r.checked = true; };

  function openWizard(kind) {
    save();
    setRadio("wz-kind", kind);
    if (!$("wz-folder").value) $("wz-folder").value = st.home || "";
    $("wz-error").hidden = true;
    st.wizard = true;
    st.showWelcome = false;
    updateWizard();
    setTab("code");
    $("wizard").scrollTop = 0;
  }

  // The format of the program: a form app (ui.form), an app with its own
  // page (@mode app) or a script going step by step without a form.
  function format() {
    switch (radio("wz-format")) {
      case "app": return { mode: "app", form: "yes" };
      case "script": return { mode: "script", form: "no" };
      default: return { mode: "script", form: "yes" };
    }
  }

  function updateWizard() {
    const w = $("wizard"), port = radio("wz-kind") === "port";
    w.dataset.kind = port ? "port" : "new";
    const f = format();
    w.dataset.mode = f.mode;
    w.dataset.form = f.form;
    $("wz-accent").disabled = !$("wz-accent-on").checked;
    $("wz-background").disabled = !$("wz-background-on").checked;
    if (!$("wz-look-box").hidden) lookPreview();
    $("wz-overview-title").textContent = port ? L.wzOverviewPort : L.wzOverviewNew;
    $("wz-overview").placeholder = port ? L.wzOverviewPortHint : L.wzOverviewNewHint;
    $("wz-input-note").disabled = radio("wz-input") === "no";
    $("wz-output-note").disabled = radio("wz-output") === "no";
    $("wz-name").disabled = radio("wz-name-mode") !== "name";
    $("wz-rows").disabled = !$("wz-sheets").checked;
    $("wz-inline").disabled = !$("wz-vba").checked && !$("wz-sheets").checked;
  }

  function answers() {
    const port = radio("wz-kind") === "port";
    return {
      kind: port ? "port" : "new",
      books: port ? wz.books.filter((b) => !b.error).map((b) => b.path) : [],
      includeVba: $("wz-vba").checked,
      includeSheets: $("wz-sheets").checked,
      inlineWorkbook: $("wz-inline").checked,
      sampleRows: Number($("wz-rows").value) || 0,
      input: radio("wz-input"), inputNote: $("wz-input-note").value,
      output: radio("wz-output"), outputNote: $("wz-output-note").value,
      ...format(),
      nameMode: radio("wz-name-mode"), name: $("wz-name").value,
      overview: $("wz-overview").value,
      intro: $("wz-intro").checked,
      titleBar: radio("wz-titlebar"),
      accent: $("wz-accent-on").checked ? $("wz-accent").value : "",
      background: $("wz-background-on").checked ? $("wz-background").value : "",
      radius: $("wz-radius").value === "medium" ? "" : $("wz-radius").value,
    };
  }

  // The preview of the look: a sample form (one button) in a frame under a
  // title bar drawn here (the system one or a copy of titlebar.js's), with
  // the colors and corners applied by the form page itself (form.js
  // applyColors). An app's page is the AI's HTML, so for it this is only a
  // likeness.
  const RADIUS = { none: "0px", small: "4px", medium: "", large: "10px", full: "9999px" };
  let lookForm = null;
  function lookPreview() {
    const a = answers();
    const box = $("wz-look-box"), win = $("wz-window");
    box.hidden = false;
    win.dataset.bar = a.titleBar === "custom" ? "custom" : "system";
    const title = (a.nameMode === "name" && a.name.trim()) || L.wzPvTitle;
    win.querySelector(".wz-sysbar-title").textContent = title;
    win.querySelector(".wz-custombar-title").textContent = title;
    // The custom bar takes the background and the accent (as titlebar.js with form.js's variables).
    const bar = win.querySelector(".wz-custombar");
    bar.style.setProperty("--wz-bar-accent", a.accent || "");
    bar.style.setProperty("--wz-bar-bg", a.background || "");
    bar.style.setProperty("--wz-bar-fg", a.background ? (isDark(a.background) ? "#fafafa" : "#18181b") : "");
    const color = (c) => ({ light: c, dark: c });
    lookForm = {
      type: "init", texts: st.formTexts || {}, lang: st.lang || "ja",
      description: L.wzPvDescription, submit: L.wzPvSubmit, submitIcon: "ic ic-play",
      accent: color(a.accent), background: color(a.background), radius: RADIUS[$("wz-radius").value] || "",
      tabs: [], tabIcons: [], groups: [], buttons: [], buttonIcons: [], buttonTooltips: [],
      fields: [
        { name: "name", label: L.wzPvText, type: "text", value: L.wzPvTextValue, tab: -1, group: -1 },
        { name: "kind", label: L.wzPvSelect, type: "radio", options: [{ value: "a", label: L.wzPvOptionA }, { value: "b", label: L.wzPvOptionB }], value: "a", tab: -1, group: -1 },
        { name: "open", label: L.wzPvCheck, type: "checkbox", value: true, tab: -1, group: -1 },
      ],
    };
    $("wz-look-frame").src = "form.html?preview=" + Date.now();
  }

  // The preview closes with its button again, the form's Cancel / Close or
  // the drawn title bar's close button.
  function closeLookPreview() {
    $("wz-look-box").hidden = true;
    $("wz-look-frame").removeAttribute("src");
    lookForm = null;
  }

  // Whether a color takes white text: the same OKLab lightness rule as form.js's isDark.
  function isDark(c) {
    const ctx = Object.assign(document.createElement("canvas"), { width: 1, height: 1 }).getContext("2d");
    ctx.fillStyle = c;
    ctx.fillRect(0, 0, 1, 1);
    const [r, g, b] = [...ctx.getImageData(0, 0, 1, 1).data].map((v) => ((v /= 255) <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
    const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
    const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
    const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
    return 0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s <= 0.65;
  }

  function wizardError(text) {
    $("wz-error").textContent = text || "";
    $("wz-error").hidden = !text;
    return !text;
  }

  function checkAnswers(a) {
    if (a.kind === "port" && !a.books.length) return wizardError(L.wzNeedBooks);
    if (a.kind === "new" && !a.overview.trim()) return wizardError(L.wzNeedOverview);
    if (a.nameMode === "name" && !a.name.trim()) return wizardError(L.wzNeedName);
    return wizardError("");
  }

  function wizardBooks(m) {
    wz.books = m.books || [];
    $("wz-book-list").replaceChildren(...wz.books.map((b) => el("li", { className: b.error ? "err" : "" },
      icon("ic ic-file-spreadsheet"), " ", b.name, " — ",
      b.error ? b.error : b.modules ? fmt(L.wzModules, b.modules) : L.wzNoMacros)));
    const f = $("wz-folder");
    if (m.dir && (!f.value || f.value === st.home)) f.value = m.dir;
  }

  function wizardPrompt(m) {
    $("wz-result").hidden = false;
    $("wz-text").value = m.text;
    const n = m.text.length;
    $("wz-count").textContent = fmt(L.wzCount, n, m.file) + (m.attachment ? " " + fmt(L.wzAttachCount, m.attachment) : "") + (n > 15000 ? " " + L.wzLong : "");
    $("wz-result").scrollIntoView({ behavior: "smooth", block: "start" });
  }

  function initWizard() {
    $("wizard").addEventListener("change", updateWizard);
    $("wz-name").addEventListener("focus", () => setRadio("wz-name-mode", "name"));
    $("wz-name").parentElement.addEventListener("click", (e) => {
      if (e.target === $("wz-name") && $("wz-name").disabled) { setRadio("wz-name-mode", "name"); updateWizard(); $("wz-name").focus(); }
    });
    $("wz-books").onclick = () => send({ op: "wizardBooks" });
    $("wz-folder-pick").onclick = () => send({ op: "wizardFolder", path: $("wz-folder").value });
    $("wz-save-text").onclick = () => {
      const a = answers();
      if (!a.books.length) { wizardError(L.wzNeedBooks); return; }
      send({ op: "wizardSave", answers: a });
    };
    $("wz-make").onclick = () => {
      const a = answers();
      if (checkAnswers(a)) send({ op: "wizardPrompt", answers: a });
    };
    $("wz-create").onclick = () => {
      const a = answers();
      if (!checkAnswers(a)) return;
      if (!$("wz-folder").value.trim()) { wizardError(L.wzNeedFolder); return; }
      send({ op: "wizardCreate", answers: a, path: $("wz-folder").value.trim(), text: $("wz-text").value });
    };
    $("wz-look-preview").onclick = () => ($("wz-look-box").hidden ? lookPreview() : closeLookPreview());
    for (const x of document.querySelectorAll("#wz-window .wz-close")) x.onclick = closeLookPreview;
    $("wz-copy").onclick = () => copy($("wz-text").value, L.wzCopiedOnly);
    $("wz-cancel").onclick = () => { st.wizard = false; setTab("code"); };
    $("w-wizard").onclick = () => openWizard("new");
    $("w-port").onclick = () => openWizard("port");
  }

  // ------------------------------------------------------------ messages

  function handle(m) {
    switch (m.type) {
      case "init":
        L = m.texts || {};
        devTypes.load(m.dts || "");
        editor.phrases(L);
        for (const e of document.querySelectorAll("[data-t]")) e.textContent = L[e.dataset.t] || e.dataset.t;
        for (const e of document.querySelectorAll("[data-tip]")) e.title = L[e.dataset.tip] || "";
        document.body.dataset.dropHint = L.dropHint || "";
        $("chat-url").placeholder = L.urlPlaceholder;
        $("chat-url-start").placeholder = L.urlPlaceholder;
        $("pick-text").placeholder = L.pickPlaceholder || "";
        document.documentElement.lang = m.lang || "ja";
        initChat(m.services || []);
        renderRecent(m.recent || []);
        st.home = m.home || "";
        st.formTexts = m.formTexts || {};
        st.lang = m.lang || "ja";
        $("wz-rows").options[0].textContent = L.wzRowsNone;
        $("wz-rows").value = "3";
        $("wz-input-note").placeholder = L.wzInputHint;
        $("wz-output-note").placeholder = L.wzOutputHint;
        $("wz-name").placeholder = L.wzNameHint;
        updateButtons();
        showDiff();
        setTab("code");
        applyLayout();
        break;
      case "project": openProject(m); break;
      case "meta":
        if (st.project) Object.assign(st.project, { app: m.app, html: m.html, title: m.title });
        break;
      case "files":
        if (st.project) { st.project.files = m.files; fillFiles(m.files); }
        break;
      case "file":
        if (st.reloadCheck && m.file === st.file) {
          st.reloadCheck = false;
          if (!st.dirty && m.text !== st.saved) { setEditor(m.file, m.text); status(fmt(L.reloaded, m.file)); }
          break;
        }
        if (st.versionView && st.wantVersionCurrent && st.versionView.file === m.file) {
          st.wantVersionCurrent = false;
          st.versionView.current = m.text;
          showDiff();
        }
        if (st.pending && st.pending.target === m.file && st.pending.old === null) {
          st.pending.old = m.text;
          showDiff();
        }
        if (st.wantEditor === m.file) {
          st.wantEditor = null;
          setEditor(m.file, m.text);
          if (st.wantLine && st.wantLine.file === m.file) { gotoLine(st.wantLine.line, st.wantLine.column); st.wantLine = null; }
        }
        break;
      case "saved":
        if (m.file === st.file) $("save-state").textContent = st.dirty ? L.unsaved : L.saved;
        if (st.project && st.project.app) loadAppPreviewIfShown();
        else previewChanged();
        break;
      case "applied":
        if (m.file === st.file) {
          setEditor(m.file, m.text);
        }
        status(fmt(L.appliedMsg, m.file), "ok");
        if (st.project && st.project.app) loadAppPreviewIfShown();
        else previewChanged();
        setTab("code");
        break;
      case "versions": renderHistory(m.file, m.versions || []); break;
      case "version":
        st.versionView = { file: m.file, n: m.n, text: m.text, current: m.file === st.file ? editor.value() : "" };
        if (m.file !== st.file) { st.wantVersionCurrent = true; send({ op: "read", file: m.file }); }
        showDiff();
        setTab("diff");
        break;
      case "download":
        st.pending = { name: m.name, text: m.text, target: m.target, old: m.old === undefined ? null : m.old, files: m.files || [], rename: m.rename || "" };
        if (st.pending.target && st.pending.old === null) st.pending.old = "";
        st.versionView = null;
        showDiff();
        setTab("diff");
        status(fmt(L.downloaded, m.name));
        break;
      case "url":
        if (m.url && !m.url.startsWith("about:")) {
          $("chat-url").value = m.url;
          if (!st.chat) { st.chat = m.url; syncBrowser(); }
        }
        break;
      case "copy":
        copy(m.text, m.what === "intro" ? L.copiedIntro : m.what === "script" ? L.copiedScript : m.what === "wizard" ? L.wzCopied : m.what === "wizardAttach" ? L.wzCopiedAttach : m.what === "notify" ? L.copiedNotify : m.what === "pick" ? L.copiedPick : L.copied);
        break;
      case "runStarted": runStarted(m); break;
      case "output": appendOutput(m.text, m.stream === "stderr" ? "err" : ""); break;
      case "runEnded": runEnded(m); break;
      case "previewForm":
        pv.done = pv.sent;
        showFormPreview(m.form);
        if (!pv.auto) setTab("preview");
        status("");
        break;
      case "previewFailed":
        pv.done = pv.sent;
        // A preview that ran by itself keeps the form it showed (the error
        // above it): the script may be half edited.
        if (!pv.auto) $("preview-frame").hidden = true;
        $("preview-error").hidden = false;
        $("preview-error").textContent = m.message || "";
        if (!pv.auto) setTab("preview");
        break;
      case "previewSkipped":
        pv.asked = false;
        pv.auto = false;
        if (m.busy) break;
        pv.done = pv.sent;
        pv.noForm = true;
        if (shown("preview")) showPreviewHint();
        break;
      case "browserFailed":
        $("chat-failed").hidden = false;
        $("chat-failed").textContent = fmt(L.browserFailed, m.message);
        break;
      case "error":
        status(m.message, "error");
        break;
      case "info":
        status(m.message, "ok");
        break;
      case "packed":
        status([fmt(L.packed, m.path)].concat(m.warnings || []).join(" / "), m.warnings && m.warnings.length ? "error" : "ok");
        break;
      case "wizardBooks": wizardBooks(m); break;
      case "wizardFolder": $("wz-folder").value = m.path; break;
      case "wizardPrompt": wizardPrompt(m); break;
      case "wizardAttach": showAttach(m.file); setTab("code"); break;
    }
  }

  function loadAppPreviewIfShown() {
    if (shown("preview") && !$("preview-frame").hidden) loadAppPreview();
  }

  // ------------------------------------------------------------ start

  // The title bar only in a window without the OS one (web/titlebar.js).
  $("titlebar").hidden = !window.__xljsTitleBar;
  loadLayout();
  initEditor();
  initDiff();
  initPreview();
  initRun();
  initWizard();
  initTabs();
  applyLayout();
  $("btn-open").onclick = $("w-open").onclick = () => { save(); send({ op: "open" }); };
  $("btn-new").onclick = () => { save(); st.showWelcome = true; setTab("code"); };
  $("w-new").onclick = () => send({ op: "new", template: "script" });
  $("w-new-form").onclick = () => send({ op: "new", template: "form" });
  $("w-new-app").onclick = () => send({ op: "new", template: "app" });
  $("btn-folder").onclick = () => send({ op: "showFolder" });
  $("btn-pack").onclick = () => { save(); send({ op: "pack" }); };
  $("lay-reset").onclick = () => { st.layout = defaultLayout(); saveLayout(); applyLayout(); };
  new ResizeObserver(syncBrowser).observe($("chat-view"));
  window.addEventListener("resize", syncBrowser);
  document.addEventListener("visibilitychange", syncBrowser);
  document.addEventListener("keydown", (e) => {
    if (e.key === "F5") { e.preventDefault(); startRun(e.shiftKey); }
    else if (e.ctrlKey && (e.key === "s" || e.key === "S")) { e.preventDefault(); save(); }
    else if (e.ctrlKey && (e.key === "r" || e.key === "R" || e.key === "p" || e.key === "P")) e.preventDefault();
  });
  window.addEventListener("focus", () => {
    // Edited elsewhere (Notepad...) while nothing is unsaved here: reload.
    if (st.project && !st.dirty && st.file && !st.wantEditor) { st.reloadCheck = true; send({ op: "read", file: st.file }); }
  });
  initDrop();
  initFileDrag();
  if (wv) {
    wv.addEventListener("message", (e) => {
      if (typeof e.data !== "string" || e.data[0] !== "\x02") return;
      let m;
      try { m = JSON.parse(e.data.slice(1)); } catch { return; }
      handle(m);
    });
  }
  send({ op: "ready" });
})();
