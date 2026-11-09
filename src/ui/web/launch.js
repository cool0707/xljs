"use strict";
// The start screen (launch.html; the other side is src/cli/launch.rs).
// Rust answers {op: "ready"} with {message, version, choices: [{label,
// note, icon}]} prefixed by \x02; the page sends {op: "choose", index}.
(() => {
  const wv = window.chrome && window.chrome.webview;
  const $ = (id) => document.getElementById(id);
  const send = (m) => { if (wv) wv.postMessage(JSON.stringify(m)); };
  const el = (tag, cls, text) => {
    const e = document.createElement(tag);
    if (cls) e.className = cls;
    if (text) e.textContent = text;
    return e;
  };

  function show(m) {
    $("message").textContent = m.message || "";
    $("version").textContent = m.version || "";
    const list = $("choices");
    list.textContent = "";
    m.choices.forEach((c, i) => {
      const b = el("button", "choice");
      b.type = "button";
      b.setAttribute("role", "listitem");
      // The name read by screen readers and UI Automation (tools/gui).
      b.setAttribute("aria-label", c.label);
      if (c.note) b.setAttribute("aria-description", c.note);
      const badge = el("span", "badge");
      badge.append(el("i", "ic ic-" + (c.icon || "chevron-right")));
      const text = el("span", "text");
      text.append(el("span", "label", c.label));
      if (c.note) text.append(el("span", "note", c.note));
      b.append(badge, text, el("i", "ic ic-chevron-right go"));
      b.addEventListener("click", () => send({ op: "choose", index: i }));
      list.append(b);
    });
    const first = list.querySelector("button");
    if (first) first.focus({ focusVisible: false });
  }

  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      e.preventDefault();
      send({ op: "cancel" });
      return;
    }
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    const items = [...document.querySelectorAll("button.choice")];
    if (!items.length) return;
    e.preventDefault();
    const i = items.indexOf(document.activeElement);
    const next = e.key === "ArrowDown" ? (i + 1) % items.length : (i <= 0 ? items.length : i) - 1;
    items[next].focus();
  });

  if (wv) {
    wv.addEventListener("message", (e) => {
      if (typeof e.data !== "string" || e.data[0] !== "\x02") return;
      try { show(JSON.parse(e.data.slice(1))); } catch { /* ignore */ }
    });
  }
  send({ op: "ready" });
})();
