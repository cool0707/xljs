"use strict";
// Shared parts of the built-in tables (table.js: ui.table, form.js: table
// fields): the column headings (sort mark, filter and menu buttons, resize
// grip), the column menu (pin, freeze, hide, the list of columns), the
// column filters, the layout of pinned (sticky) columns and the conditional
// formats of cells (the formats option, compiled by prelude.js).
//
// Cells are wire values: numbers, strings (dates as "yyyy-mm-dd" or
// "yyyy-mm-dd hh:mm:ss"), booleans or null.
const GridKit = (() => {
  const mk = (tag, props = {}, ...children) => {
    const e = Object.assign(document.createElement(tag), props);
    for (const c of children) if (c !== null && c !== undefined) e.append(c);
    return e;
  };
  const NS = "http://www.w3.org/2000/svg";
  /** An icon (16x16 SVG) from path data; stroke: drawn with lines, not filled. */
  function icon(d, stroke, cls) {
    const s = document.createElementNS(NS, "svg");
    s.setAttribute("viewBox", "0 0 16 16");
    s.setAttribute("aria-hidden", "true");
    if (cls) s.setAttribute("class", cls);
    const p = document.createElementNS(NS, "path");
    p.setAttribute("d", d);
    if (stroke) {
      p.setAttribute("fill", "none");
      p.setAttribute("stroke", "currentColor");
      p.setAttribute("stroke-width", stroke);
      p.setAttribute("stroke-linecap", "round");
      p.setAttribute("stroke-linejoin", "round");
    } else p.setAttribute("fill", "currentColor");
    s.append(p);
    return s;
  }
  const ICONS = {
    up: ["M4 10l4-4 4 4", "1.7"],
    down: ["M4 6l4 4 4-4", "1.7"],
    menu: ["M8 2.1a1.4 1.4 0 110 2.8 1.4 1.4 0 010-2.8zM8 6.6a1.4 1.4 0 110 2.8 1.4 1.4 0 010-2.8zM8 11.1a1.4 1.4 0 110 2.8 1.4 1.4 0 010-2.8z"],
    filter: ["M2.5 4h11M4.5 8h7M6.5 12h3", "1.5"],
    pin: ["M9.5 1.5l5 5-1.4 1.4-1-1-2.6 2.6.4 3.1-1.4 1.4-2.8-2.8L2.3 14.6l-.9-.9 3.4-3.4L2 7.5l1.4-1.4 3.1.4 2.6-2.6-1-1z"],
    freeze: ["M2.5 2.5h11v11h-11zM6.5 2.5v11M2.5 6.5h4", "1.3"],
    hide: ["M2 2l12 12M6.6 6.7A2 2 0 009.3 9.4M4.2 4.4C2.8 5.3 1.8 6.6 1.3 8c1.1 3 3.8 5 6.7 5 1.3 0 2.5-.4 3.6-1M7 3.1c.3 0 .7-.1 1-.1 2.9 0 5.6 2 6.7 5-.3.9-.8 1.7-1.4 2.4", "1.4"],
    search: ["M7 2.5a4.5 4.5 0 110 9 4.5 4.5 0 010-9zM10.3 10.3l3.2 3.2", "1.4"],
  };
  const svg = (name, cls) => icon(ICONS[name][0], ICONS[name][1], cls);

  const fold = (s) => String(s).normalize("NFKC").toLowerCase();
  const text = (v) => (v === null || v === undefined ? "" : String(v));
  const empty = (v) => v === null || v === undefined || v === "";
  /** A number typed in a filter ("1,000", full-width digits), or NaN. */
  const parseNum = (s) => {
    const t = fold(s).replace(/[,\s]/g, "");
    return t === "" ? NaN : Number(t);
  };
  const DATE = /^\d{4}-\d{2}-\d{2}( \d{2}:\d{2}(:\d{2})?)?$/;

  /** The kind of a column from its values (rows: arrays of cells): "number", "date" or "text". */
  function kindOf(rows, c) {
    let kind = "";
    for (const r of rows) {
      const v = r[c];
      if (empty(v)) continue;
      const k = typeof v === "number" ? "number" : typeof v === "string" && DATE.test(v) ? "date" : "text";
      if (kind && kind !== k) return "text";
      kind = k;
    }
    return kind || "text";
  }

  // ---- filters: { kind, join: "and" | "or", conds: [{ op, v, v2 }] }

  const OPS = {
    text: ["contains", "notContains", "equals", "notEqual", "startsWith", "endsWith", "blank", "notBlank"],
    number: ["equals", "notEqual", "lessThan", "lessThanOrEqual", "greaterThan", "greaterThanOrEqual", "inRange", "blank", "notBlank"],
    date: ["equals", "notEqual", "lessThan", "greaterThan", "inRange", "blank", "notBlank"],
  };
  const noValue = (op) => op === "blank" || op === "notBlank";
  /** Whether a condition is complete (it filters). */
  function condOn(c) {
    if (!c || !c.op) return false;
    if (noValue(c.op)) return true;
    if (c.op === "inRange") return c.v !== "" || c.v2 !== "";
    return c.v !== "";
  }
  const filterOn = (f) => !!f && f.conds.some(condOn);

  function condMatch(kind, c, v) {
    if (c.op === "blank") return empty(v);
    if (c.op === "notBlank") return !empty(v);
    if (kind === "number") {
      if (typeof v !== "number") return c.op === "notEqual";
      const a = parseNum(c.v), b = parseNum(c.v2);
      switch (c.op) {
        case "equals": return v === a;
        case "notEqual": return v !== a;
        case "lessThan": return v < a;
        case "lessThanOrEqual": return v <= a;
        case "greaterThan": return v > a;
        case "greaterThanOrEqual": return v >= a;
        case "inRange": return (isNaN(a) || v >= a) && (isNaN(b) || v <= b);
      }
      return true;
    }
    if (kind === "date") {
      const d = text(v).slice(0, 10);
      if (!d) return c.op === "notEqual";
      switch (c.op) {
        case "equals": return d === c.v;
        case "notEqual": return d !== c.v;
        case "lessThan": return d < c.v;
        case "greaterThan": return d > c.v;
        case "inRange": return (!c.v || d >= c.v) && (!c.v2 || d <= c.v2);
      }
      return true;
    }
    const t = fold(text(v)), q = fold(c.v);
    switch (c.op) {
      case "contains": return t.includes(q);
      case "notContains": return !t.includes(q);
      case "equals": return t === q;
      case "notEqual": return t !== q;
      case "startsWith": return t.startsWith(q);
      case "endsWith": return t.endsWith(q);
    }
    return true;
  }

  /** Whether a cell passes a filter. */
  function match(f, v) {
    const on = f.conds.filter(condOn);
    if (!on.length) return true;
    return f.join === "or" ? on.some((c) => condMatch(f.kind, c, v)) : on.every((c) => condMatch(f.kind, c, v));
  }

  // ---- popups (the column menu and the filter): one at a time

  let popup = null;
  function closePopup(focusBack) {
    if (!popup) return;
    const p = popup;
    popup = null;
    p.node.remove();
    p.anchor.setAttribute("aria-expanded", "false");
    p.anchor.classList.remove("open");
    if (p.onClose) p.onClose();
    if (focusBack) p.anchor.focus();
  }
  /** Opens node below anchor (closing the popup open there instead). Returns false when it was open. */
  function openPopup(anchor, node, onClose) {
    if (popup && popup.anchor === anchor) { closePopup(); return false; }
    closePopup();
    node.classList.add("gk-popup");
    document.body.append(node);
    const r = anchor.getBoundingClientRect(), w = node.offsetWidth;
    node.style.left = Math.max(8, Math.min(r.right - w, window.innerWidth - w - 8)) + "px";
    let top = r.bottom + 4;
    const over = top + node.offsetHeight - (window.innerHeight - 8);
    if (over > 0) top = Math.max(8, r.top - node.offsetHeight - 4 > 8 ? r.top - node.offsetHeight - 4 : top - over);
    node.style.top = top + "px";
    anchor.setAttribute("aria-expanded", "true");
    anchor.classList.add("open");
    popup = { anchor, node, onClose };
    return true;
  }
  document.addEventListener("mousedown", (e) => {
    if (popup && !popup.node.contains(e.target) && !popup.anchor.contains(e.target)) closePopup();
  }, true);
  document.addEventListener("keydown", (e) => {
    if (popup && e.key === "Escape") { e.preventDefault(); e.stopImmediatePropagation(); closePopup(true); }
  }, true);
  window.addEventListener("resize", () => closePopup());
  window.addEventListener("blur", () => closePopup());

  // ---- column state: hidden columns, pinned (sticky) columns and the frozen columns

  class Columns {
    constructor(n) { this.n = n; this.reset(); }
    reset() {
      this.hidden = new Set();
      this.pins = new Map(); // column -> "left" | "right", in the order pinned
      this.freeze = -1; // the columns not pinned up to this one stay on the left
    }
    pinOf(c) { return this.pins.get(c) || ""; }
    pin(c, side) {
      this.pins.delete(c);
      if (side) this.pins.set(c, side);
      if (side && this.freeze === c) this.freeze = -1;
    }
    get visible() { let k = 0; for (let c = 0; c < this.n; c++) if (!this.hidden.has(c)) k++; return k; }
    setHidden(c, hide) {
      if (hide && this.visible <= 1 && !this.hidden.has(c)) return;
      if (hide) this.hidden.add(c); else this.hidden.delete(c);
    }
    /** The visible columns as shown: [{ c, side: "left" | "right" | "" }] (left: pinned or frozen). */
    order() {
      const left = [], mid = [], right = [];
      for (const [c, side] of this.pins) if (!this.hidden.has(c) && c < this.n) (side === "left" ? left : right).push(c);
      for (let c = 0; c < this.n; c++) if (!this.hidden.has(c) && !this.pins.has(c)) mid.push(c);
      const k = this.freeze >= 0 ? mid.filter((c) => c <= this.freeze).length : 0;
      return [
        ...left.map((c) => ({ c, side: "left" })),
        ...mid.map((c, i) => ({ c, side: i < k ? "left" : "" })),
        ...right.map((c) => ({ c, side: "right" })),
      ];
    }
    get any() { return this.hidden.size > 0 || this.pins.size > 0 || this.freeze >= 0; }
  }

  /**
   * Offsets of the sticky columns: for each of order (see Columns.order),
   * { left | right (px), edge } where edge marks the last one on the left /
   * the first on the right; start: the width before the first column.
   */
  function stickyLayout(order, widthOf, start) {
    const out = order.map(() => null);
    let x = start;
    let lastLeft = -1, firstRight = -1;
    order.forEach((o, i) => {
      if (o.side !== "left") return;
      out[i] = { left: x };
      x += widthOf(o.c);
      lastLeft = i;
    });
    x = 0;
    for (let i = order.length - 1; i >= 0; i--) {
      if (order[i].side !== "right") continue;
      out[i] = { right: x };
      x += widthOf(order[i].c);
      firstRight = i;
    }
    if (lastLeft >= 0) out[lastLeft].edge = "left";
    if (firstRight >= 0) out[firstRight].edge = "right";
    return out;
  }

  /** Makes a cell sticky as laid out by stickyLayout (s: its entry, or null). */
  function stick(cell, s) {
    cell.classList.toggle("gk-stk", !!s);
    cell.classList.toggle("gk-edge-l", !!s && s.edge === "left");
    cell.classList.toggle("gk-edge-r", !!s && s.edge === "right");
    cell.style.left = s && s.left !== undefined ? s.left + "px" : "";
    cell.style.right = s && s.right !== undefined ? s.right + "px" : "";
  }

  // ---- headings

  /**
   * Fills a heading cell: the label, the sort mark, the filter and menu
   * buttons and the resize grip. o: { label, sort (1 | -1 | 0), filtered,
   * numeric, texts, onSort(), onFilter(button), onMenu(button), onResize(event), onFit() }.
   */
  function heading(cell, o) {
    cell.classList.add("gk-th");
    cell.classList.toggle("gk-filtered", !!o.filtered);
    cell.classList.toggle("gk-num", !!o.numeric);
    const t = o.texts || {};
    const label = mk("span", { className: "gk-label", textContent: o.label });
    label.addEventListener("mouseenter", () => { label.title = label.scrollWidth > label.clientWidth ? o.label : ""; });
    const sort = mk("span", { className: "gk-sort" }, o.sort ? svg(o.sort > 0 ? "up" : "down") : null);
    const filter = mk("button", { type: "button", className: "gk-btn gk-filter", tabIndex: -1, title: fmt(t.filterOf || "{0}", o.label) }, svg("filter"));
    const menu = mk("button", { type: "button", className: "gk-btn gk-menu", tabIndex: -1, title: fmt(t.menuOf || "{0}", o.label) }, svg("menu"));
    filter.setAttribute("aria-haspopup", "dialog");
    menu.setAttribute("aria-haspopup", "menu");
    for (const b of [filter, menu]) {
      b.addEventListener("mousedown", (e) => e.stopPropagation());
      b.addEventListener("click", (e) => { e.stopPropagation(); e.preventDefault(); });
    }
    filter.addEventListener("click", () => o.onFilter(filter));
    menu.addEventListener("click", () => o.onMenu(menu));
    const grip = mk("span", { className: "gk-grip" });
    grip.addEventListener("mousedown", (e) => { e.preventDefault(); e.stopPropagation(); o.onResize(e); });
    grip.addEventListener("click", (e) => e.stopPropagation());
    grip.addEventListener("dblclick", (e) => { e.stopPropagation(); o.onFit(); });
    cell.replaceChildren(mk("span", { className: "gk-head" }, label, sort, filter, menu), grip);
    // Not the click that ends dragging the grip (when the mouse is let go over the same heading).
    cell.addEventListener("click", () => { if (Date.now() - resizedAt > 300) o.onSort(); });
    cell.setAttribute("aria-sort", o.sort > 0 ? "ascending" : o.sort < 0 ? "descending" : "none");
    return { filter, menu };
  }

  let resizedAt = 0;
  /** Drags a column's right edge (e: the mousedown); set(width) applies it. */
  function resize(e, w0, set) {
    const x0 = e.clientX;
    document.body.classList.add("gk-resizing");
    const move = (ev) => set(Math.max(40, Math.round(w0 + ev.clientX - x0)));
    const up = () => {
      resizedAt = Date.now();
      document.body.classList.remove("gk-resizing");
      document.removeEventListener("mousemove", move);
      document.removeEventListener("mouseup", up);
    };
    document.addEventListener("mousemove", move);
    document.addEventListener("mouseup", up);
  }

  // ---- the column menu

  /**
   * o: { anchor, c, labels, cols (Columns), texts, onChange(), onReset() }.
   * The menu changes cols and calls onChange.
   */
  function columnMenu(o) {
    const t = o.texts, cols = o.cols, c = o.c;
    const node = mk("div", { className: "gk-menu-pop" });
    node.setAttribute("role", "menu");
    const item = (ic, label, fn, disabled) => {
      const b = mk("button", { type: "button", className: "gk-item", disabled: !!disabled }, svg(ic), mk("span", { textContent: label }));
      b.setAttribute("role", "menuitem");
      b.onclick = () => { fn(); closePopup(); o.onChange(); };
      return b;
    };
    const side = cols.pinOf(c);
    node.append(mk("div", { className: "gk-title", textContent: o.labels[c] }));
    node.append(item("pin", side === "left" ? t.unpin : t.pinLeft, () => cols.pin(c, side === "left" ? "" : "left")));
    node.append(item("pin", side === "right" ? t.unpin : t.pinRight, () => cols.pin(c, side === "right" ? "" : "right")));
    const frozen = cols.freeze >= 0 && !side && c <= cols.freeze;
    node.append(item("freeze", cols.freeze === c ? t.unfreeze : t.freeze, () => { cols.freeze = cols.freeze === c ? -1 : c; }, !!side));
    if (cols.freeze >= 0 && cols.freeze !== c && frozen) node.lastChild.title = t.frozen;
    node.append(item("hide", t.hide, () => cols.setHidden(c, true), cols.visible <= 1));
    // The list of columns: shown or hidden (pinned ones are marked).
    const q = mk("input", { type: "search", placeholder: t.findColumn, spellcheck: false });
    q.setAttribute("aria-label", t.findColumn);
    const list = mk("ul", { className: "gk-cols" });
    const fill = () => {
      const k = fold(q.value.trim());
      list.replaceChildren(...o.labels.map((label, i) => {
        if (k && !fold(label).includes(k)) return null;
        const box = mk("input", { type: "checkbox", checked: !cols.hidden.has(i), disabled: !cols.hidden.has(i) && cols.visible <= 1 });
        box.onchange = () => { cols.setHidden(i, !box.checked); fill(); o.onChange(); };
        const pin = cols.pinOf(i) ? mk("span", { className: "gk-pin", textContent: t.pinned }) : null;
        return mk("li", {}, mk("label", {}, box, mk("span", { textContent: label }), pin));
      }).filter(Boolean));
    };
    q.addEventListener("input", fill);
    fill();
    node.append(mk("div", { className: "gk-sec" }, mk("div", { className: "gk-sec-title", textContent: t.columns }), q, list));
    const all = mk("button", { type: "button", textContent: t.showAll });
    all.onclick = () => { cols.hidden.clear(); fill(); o.onChange(); };
    const reset = mk("button", { type: "button", textContent: t.reset, title: t.resetTip || "" });
    reset.onclick = () => { closePopup(); o.onReset(); };
    node.append(mk("div", { className: "gk-foot" }, all, reset));
    if (openPopup(o.anchor, node)) node.querySelector(".gk-item").focus();
  }

  // ---- the filter popup

  /**
   * o: { anchor, kind, filter (or null), texts, onChange(filter or null) }.
   * Typing filters at once (after a pause).
   */
  function filterPopup(o) {
    const t = o.texts, kind = o.kind;
    const f = o.filter ? JSON.parse(JSON.stringify(o.filter)) : { kind, join: "and", conds: [] };
    f.kind = kind;
    const ops = OPS[kind];
    while (f.conds.length < 2) f.conds.push({ op: ops[0], v: "", v2: "" });
    const node = mk("div", { className: "gk-filter-pop" });
    node.setAttribute("role", "dialog");
    let timer = 0;
    const apply = (now) => {
      clearTimeout(timer);
      const send = () => o.onChange(filterOn(f) ? JSON.parse(JSON.stringify(f)) : null);
      if (now) send(); else timer = setTimeout(send, 200);
    };
    const inputType = kind === "date" ? "date" : "text";
    const condBox = (c) => {
      const sel = mk("select", {}, ...ops.map((op) => mk("option", { value: op, textContent: (t.ops || {})[op] || op })));
      sel.value = c.op;
      const v = mk("input", { type: inputType, value: c.v, placeholder: t.value, spellcheck: false });
      const v2 = mk("input", { type: inputType, value: c.v2, placeholder: t.to, spellcheck: false });
      if (kind === "number") v.inputMode = v2.inputMode = "decimal";
      const first = mk("div", { className: "gk-input" }, inputType === "text" ? svg("search") : null, v);
      const second = mk("div", { className: "gk-input" }, inputType === "text" ? svg("search") : null, v2);
      const show = () => {
        first.hidden = noValue(c.op);
        second.hidden = c.op !== "inRange";
        v.placeholder = c.op === "inRange" ? t.from : t.value;
      };
      sel.onchange = () => { c.op = sel.value; show(); sync(); apply(true); };
      v.oninput = () => { c.v = v.value; sync(); apply(); };
      v2.oninput = () => { c.v2 = v2.value; sync(); apply(); };
      for (const i of [v, v2]) i.addEventListener("keydown", (e) => { if (e.key === "Enter") { e.preventDefault(); apply(true); closePopup(true); } });
      show();
      return mk("div", { className: "gk-cond" }, sel, first, second);
    };
    const one = condBox(f.conds[0]);
    const and = mk("input", { type: "radio", name: "gk-join", checked: f.join !== "or" });
    const or = mk("input", { type: "radio", name: "gk-join", checked: f.join === "or" });
    and.onchange = or.onchange = () => { f.join = or.checked ? "or" : "and"; apply(true); };
    const join = mk("div", { className: "gk-join" }, mk("label", {}, and, t.and), mk("label", {}, or, t.or));
    const two = condBox(f.conds[1]);
    // The second condition once the first is set (as in AG Grid).
    const sync = () => { join.hidden = two.hidden = !condOn(f.conds[0]) && !condOn(f.conds[1]); };
    sync();
    const reset = mk("button", { type: "button", textContent: t.resetFilter });
    reset.onclick = () => { closePopup(true); o.onChange(null); };
    node.append(one, join, two, mk("div", { className: "gk-foot" }, reset));
    if (openPopup(o.anchor, node, () => { if (timer) apply(true); })) {
      const first = node.querySelector(".gk-input:not([hidden]) input") || node.querySelector("select");
      first.focus();
    }
  }

  // ---- conditional formats
  //
  // A rule from prelude.js: { cols: [column] | null, conds: [[op, value]],
  // fn: whether it has a function (its hits come with the rows), row,
  // style: { color, background, bold, italic, underline, strike },
  // bar: color, scale: [colors] }. The first rule that matches wins for each
  // property.

  function test(op, v, x) {
    switch (op) {
      case "empty": return empty(v) === !!x;
      case "equals": return v === x || (typeof v === "string" && typeof x === "string" && fold(v) === fold(x));
      case "notEquals": return !(v === x || (typeof v === "string" && typeof x === "string" && fold(v) === fold(x)));
      case "contains": return !empty(v) && fold(text(v)).includes(fold(x));
      case "notContains": return !fold(text(v)).includes(fold(x));
      case "startsWith": return !empty(v) && fold(text(v)).startsWith(fold(x));
      case "endsWith": return !empty(v) && fold(text(v)).endsWith(fold(x));
    }
    if (empty(v) || typeof v !== typeof x || typeof v === "boolean") return false;
    switch (op) {
      case "gt": return v > x;
      case "gte": return v >= x;
      case "lt": return v < x;
      case "lte": return v <= x;
      case "between": return v >= x[0] && v <= x[1];
    }
    return false;
  }
  function between(op, v, x) {
    if (op !== "between") return test(op, v, x);
    if (empty(v) || typeof v !== typeof x[0] || typeof v === "boolean") return false;
    return v >= x[0] && v <= x[1];
  }

  /** Min and max of the numbers of each column used by bars and scales: Map(column -> [min, max]). */
  function stats(rules, rows, ncols) {
    const out = new Map();
    for (const r of rules || []) {
      if (!r.bar && !r.scale) continue;
      for (const c of r.cols || Array.from({ length: ncols }, (_, i) => i)) {
        if (out.has(c)) continue;
        let lo = Infinity, hi = -Infinity;
        for (const row of rows) {
          const v = row[c];
          if (typeof v === "number") { if (v < lo) lo = v; if (v > hi) hi = v; }
        }
        out.set(c, [lo, hi]);
      }
    }
    return out;
  }

  const mix = (a, b, t) => `color-mix(in srgb, ${a} ${Math.round((1 - t) * 100)}%, ${b})`;

  /**
   * The styles of a row's cells: [rowStyle, Map(column -> style)] where a
   * style is { color, background, bold, italic, underline, strike, bar: [color, from, to, height] }.
   * hits: the "rule:column" keys of the rules with functions that matched.
   */
  function rowFormats(rules, row, hits, st) {
    if (!rules || !rules.length) return null;
    const rowStyle = {};
    const cells = new Map();
    const put = (s, src) => { for (const k in src) if (s[k] === undefined && src[k] !== undefined) s[k] = src[k]; };
    rules.forEach((r, ri) => {
      const cols = r.cols || row.map((_, i) => i);
      const ok = (c) => {
        if (r.fn) return !!hits && hits.includes(ri + ":" + c);
        return r.conds.every(([op, x]) => between(op, row[c], x));
      };
      // A function without columns is called with the row.
      const matched = r.fn && !r.cols ? (hits && hits.includes(ri + ":-1") ? [-1] : []) : cols.filter(ok);
      if (!matched.length) return;
      if (r.row || matched[0] === -1) { put(rowStyle, r.style); return; }
      for (const c of matched) {
        let s = cells.get(c);
        if (!s) cells.set(c, (s = {}));
        put(s, r.style);
        const v = row[c];
        const mm = st && st.get(c);
        if (typeof v !== "number" || !mm || !(mm[1] >= mm[0])) continue;
        if (r.bar && s.bar === undefined) {
          const lo = Math.min(0, mm[0]), hi = Math.max(0, mm[1]), span = hi - lo || 1;
          const a = (Math.min(0, v) - lo) / span, b = (Math.max(0, v) - lo) / span;
          s.bar = [r.bar.color, a, b, r.bar.height];
        }
        if (r.scale && s.background === undefined) {
          const span = mm[1] - mm[0];
          const p = span ? (v - mm[0]) / span : 0.5;
          const sc = r.scale;
          s.background = sc.length === 2 ? mix(sc[0], sc[1], p) : p <= 0.5 ? mix(sc[0], sc[1], p * 2) : mix(sc[1], sc[2], p * 2 - 1);
        }
      }
    });
    return [rowStyle, cells];
  }

  /** Applies a style from rowFormats to a cell element (selected: the row is selected, its color wins). */
  function paint(cell, rowStyle, s, selected) {
    const get = (k) => (s && s[k] !== undefined ? s[k] : rowStyle ? rowStyle[k] : undefined);
    const st = cell.style;
    const fg = get("color"), bg = get("background");
    st.color = fg && !selected ? fg : "";
    st.fontWeight = get("bold") ? "700" : "";
    st.fontStyle = get("italic") ? "italic" : "";
    const deco = [get("underline") ? "underline" : "", get("strike") ? "line-through" : ""].filter(Boolean).join(" ");
    st.textDecoration = deco;
    const bar = s && s.bar;
    const layers = [], sizes = [];
    if (bar) {
      // A band of the row's height (height: 0 to 1), centered.
      const [color, a, b, h] = bar;
      const c = `color-mix(in srgb, ${color} 55%, transparent)`;
      layers.push(`linear-gradient(to right, transparent ${a * 100}%, ${c} ${a * 100}%, ${c} ${b * 100}%, transparent ${b * 100}%)`);
      sizes.push(`100% ${Math.round(h * 100)}%`);
    }
    if (bg && !selected) { layers.push(`linear-gradient(${bg}, ${bg})`); sizes.push("100% 100%"); }
    st.backgroundImage = layers.join(", ");
    st.backgroundSize = sizes.join(", ");
    st.backgroundPosition = layers.length ? "left center" : "";
    st.backgroundRepeat = layers.length ? "no-repeat" : "";
    cell.classList.toggle("gk-fmt", !!(bg && !selected) || !!bar);
  }

  return {
    svg, kindOf, OPS, filterOn, match, Columns, stickyLayout, stick, heading, resize,
    columnMenu, filterPopup, closePopup, stats, rowFormats, paint,
  };
})();
