"use strict";
// Pictures of the built-in pages for manuals (Ui::capture, testing::manual):
// with ?shot the page draws a window's title bar itself, the script's host
// sends "mark" after "init", and the page puts red boxes around what the
// user operated, then answers "marked" (optionally with the rectangle to
// keep). Uses host (bridge.js).
const shot = (() => {
  const on = new URLSearchParams(location.search).has("shot");
  const frames = (n) => new Promise((done) => {
    const next = () => (n-- <= 0 ? done() : requestAnimationFrame(next));
    next();
  });
  if (on) {
    const css = document.createElement("style");
    css.textContent = `
      .shot-title { flex: none; display: flex; align-items: center; gap: 8px; height: 32px; padding: 0 0 0 12px;
        background: var(--panel); border-bottom: 1px solid var(--border); font-size: 12px; color: var(--fg); user-select: none; }
      .shot-title .t { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
      .shot-title .x { width: 46px; text-align: center; color: var(--muted); font-size: 14px; }
      .shot-box { position: fixed; border: 3px solid #e11d1d; border-radius: 6px; pointer-events: none; z-index: 2147483647;
        box-shadow: 0 0 0 1px rgba(255, 255, 255, .8); }
      *, *::before, *::after { transition: none !important; animation: none !important; caret-color: transparent !important; }`;
    document.head.append(css);
  }

  /** Puts a window's title bar (the title and a close mark) on top of the page. */
  function titleBar(title, parent = document.body) {
    if (!title) return null;
    const bar = document.createElement("div");
    bar.className = "shot-title";
    const t = document.createElement("span");
    t.className = "t";
    t.textContent = title;
    const x = document.createElement("span");
    x.className = "x";
    x.textContent = "✕";
    bar.append(t, x);
    parent.prepend(bar);
    return bar;
  }

  /**
   * Draws a red box around each target (an element, or a list of elements
   * boxed together) and tells the host the page is ready. clip: an element
   * whose rectangle is the picture (none: the whole page).
   */
  async function mark(targets, clip) {
    if (document.activeElement && document.activeElement.blur) document.activeElement.blur();
    const lists = targets.map((t) => (Array.isArray(t) ? t : [t]).filter((e) => e && e.getClientRects().length));
    const first = lists.find((l) => l.length);
    if (first) first[0].scrollIntoView({ block: "nearest" });
    try { await document.fonts.ready; } catch { /* no fonts API */ }
    await frames(2);
    const vw = document.documentElement.clientWidth, vh = document.documentElement.clientHeight;
    for (const list of lists) {
      if (!list.length) continue;
      const rs = list.map((e) => e.getBoundingClientRect());
      const pad = 4;
      const left = Math.max(1, Math.min(...rs.map((r) => r.left)) - pad), top = Math.max(1, Math.min(...rs.map((r) => r.top)) - pad);
      const right = Math.min(vw - 1, Math.max(...rs.map((r) => r.right)) + pad), bottom = Math.min(vh - 1, Math.max(...rs.map((r) => r.bottom)) + pad);
      if (right <= left || bottom <= top) continue;
      const b = document.createElement("div");
      b.className = "shot-box";
      Object.assign(b.style, { left: left + "px", top: top + "px", width: right - left + "px", height: bottom - top + "px", boxSizing: "border-box" });
      document.body.append(b);
    }
    await frames(1);
    const msg = { type: "marked" };
    if (clip) {
      const r = clip.getBoundingClientRect();
      msg.clip = { x: Math.floor(r.left), y: Math.floor(r.top), width: Math.ceil(r.width), height: Math.ceil(r.height) };
    }
    host.send(msg);
  }

  return { on, titleBar, mark };
})();
