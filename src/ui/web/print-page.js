"use strict";
// Built-in page that turns an HTML document into a PDF (book.toPdf,
// xljs.htmlToPdf). The host sends "init" with the document, the charts
// that its <div class="xljs-print-chart" data-chart="i"> stand for (a
// ChartSpec, or { error } for a chart that cannot be drawn) and the chart
// texts; the page puts the document in place, draws the charts and answers
// "rendered" (the host then prints the page), or "error".

/** Waits for ms milliseconds (timers keep running in a hidden window). */
const wait = (ms) => new Promise((r) => setTimeout(r, ms));

/** Waits for the next frames, or at most ms if frames do not come. */
const frames = (n, ms) => Promise.race([
  new Promise((r) => { const step = () => (--n <= 0 ? r() : requestAnimationFrame(step)); requestAnimationFrame(step); }),
  wait(ms),
]);

/** Puts the document html in place of this page's body (its styles too; scripts do not run). */
function place(html) {
  const doc = new DOMParser().parseFromString(html, "text/html");
  for (const el of doc.head.querySelectorAll("style, link[rel~=stylesheet], base")) document.head.append(document.importNode(el, true));
  if (doc.title) document.title = doc.title;
  if (doc.documentElement.lang) document.documentElement.lang = doc.documentElement.lang;
  for (const s of Array.from(document.body.children)) if (s.tagName === "SCRIPT") s.remove();
  document.body.className = doc.body.className;
  document.body.replaceChildren(...Array.from(doc.body.childNodes).map((n) => document.importNode(n, true)));
  for (const s of document.body.querySelectorAll("script")) s.remove();
}

/** The number of chart boxes with something drawn (painted canvas pixels, or an SVG). */
function drawnCharts() {
  let n = 0;
  for (const box of document.querySelectorAll(".xljs-print-chart.xljs-chart")) {
    if (box.querySelector("svg")) { n++; continue; }
    const painted = Array.from(box.querySelectorAll("canvas")).some((c) => {
      try {
        if (!c.width || !c.height) return false;
        const d = c.getContext("2d").getImageData(0, 0, c.width, c.height).data;
        for (let i = 3; i < d.length; i += 4 * 97) if (d[i]) return true;
      } catch (e) { /* not a 2d canvas */ }
      return false;
    });
    if (painted) n++;
  }
  return n;
}

/**
 * Numbers that do not fit their cell are shown as ### filling it, as Excel
 * prints them (cut digits or dates would read as other values).
 */
function hashNumbers() {
  const range = document.createRange();
  for (const d of document.querySelectorAll(".xljs-sheet div[data-n]")) {
    const cs = getComputedStyle(d);
    const room = d.getBoundingClientRect().width - parseFloat(cs.paddingLeft) - parseFloat(cs.paddingRight);
    const fits = () => {
      range.selectNodeContents(d);
      return range.getBoundingClientRect().width <= room + 0.01;
    };
    if (fits()) continue;
    let s = "#";
    d.textContent = s + "#";
    while (s.length < 200 && fits()) {
      s += "#";
      d.textContent = s + "#";
    }
    d.textContent = s;
  }
}

/**
 * "Shrink to fit": the text of the cell is made smaller (zoom) until it fits
 * the width of its cell. The layout guessed a zoom from the characters; here
 * the text is measured.
 */
function shrinkToFit() {
  const range = document.createRange();
  for (const s of document.querySelectorAll(".xljs-sheet .xljs-shrink")) {
    const d = s.parentElement;
    const cs = getComputedStyle(d);
    const room = d.getBoundingClientRect().width - parseFloat(cs.paddingLeft) - parseFloat(cs.paddingRight);
    s.style.zoom = "1";
    range.selectNodeContents(s);
    const wide = range.getBoundingClientRect().width;
    s.style.zoom = wide > room && room > 0 ? String(Math.max(0.05, (room / wide) * 0.98)) : "1";
  }
}

/**
 * A path of a text warp (flat x,y in pt; k px per pt) measured along its
 * length: at(s) is the point and the unit tangent at the length s (px).
 */
function track(flat, k) {
  const pts = [];
  for (let i = 0; i + 1 < flat.length; i += 2) pts.push([flat[i] * k, flat[i + 1] * k]);
  const len = [0];
  for (let i = 1; i < pts.length; i++) len.push(len[i - 1] + Math.hypot(pts[i][0] - pts[i - 1][0], pts[i][1] - pts[i - 1][1]));
  const total = len[len.length - 1];
  const at = (s) => {
    s = Math.max(0, Math.min(total, s));
    let i = 1;
    while (i < pts.length - 1 && len[i] < s) i++;
    // The tangent of the segment (skipping empty ones).
    let j = i;
    while (j < pts.length - 1 && len[j] - len[j - 1] < 1e-9) j++;
    const [a, b] = [pts[j - 1], pts[j]];
    const d = Math.hypot(b[0] - a[0], b[1] - a[1]) || 1;
    const seg = len[i] - len[i - 1];
    const f = seg > 1e-9 ? (s - len[i - 1]) / seg : 0;
    const [p, q] = [pts[i - 1], pts[i]];
    return { x: p[0] + (q[0] - p[0]) * f, y: p[1] + (q[1] - p[1]) * f, tx: (b[0] - a[0]) / d, ty: (b[1] - a[1]) / d };
  };
  return { total, at, frac: (u) => at(u * total) };
}

/**
 * WordArt's text warps (bodyPr's prstTxWarp): the lines of a .xljs-warp box
 * are laid out along the warp's paths (data-warp: { w, h, paths } in pt),
 * each character with its own affine transform. Paths in pairs are the top
 * and bottom of a band, and the band's lines are stretched to fill it; a
 * single path (an arch, a circle, the lines of a button) carries its lines
 * standing on it, stretched along it, on the side of the box's center.
 * Lines are shared out among the bands (or single paths) in order.
 */
function warpText() {
  for (const box of document.querySelectorAll(".xljs-warp")) {
    let spec;
    try { spec = JSON.parse(box.dataset.warp); } catch (e) { continue; }
    const k = box.offsetWidth / spec.w;
    if (!(k > 0) || !spec.paths || !spec.paths.length) continue;
    const paths = spec.paths.map((p) => track(p, k));
    // The lines: paragraphs, split at <br>.
    const lines = [];
    for (const p of box.querySelectorAll("p")) {
      let line = [];
      lines.push(line);
      for (const node of p.childNodes) {
        if (node.nodeName === "BR") lines.push((line = []));
        else if (node.nodeType === 1 && node.textContent) line.push(node);
      }
    }
    // Each character as a span of its run, measured on a row as it is set.
    const meas = document.createElement("div");
    meas.style.cssText = "position:absolute;left:0;top:0;visibility:hidden";
    box.append(meas);
    const laid = lines.map((runs) => {
      const row = document.createElement("div");
      row.style.cssText = "position:absolute;left:0;top:0;white-space:pre";
      meas.append(row);
      const chars = [];
      for (const run of runs) {
        // A run with an inner &nbsp; (empty paragraph) is a blank line.
        if (run.textContent === " ") continue;
        for (const ch of Array.from(run.textContent)) {
          const s = run.cloneNode(false);
          s.textContent = ch;
          row.append(s);
          chars.push(s);
        }
      }
      const xs = chars.map((s) => s.offsetLeft);
      for (const s of chars) Object.assign(s.style, { position: "absolute", left: "0", top: "0", margin: "0", transformOrigin: "0 0", whiteSpace: "pre" });
      const items = chars.map((s, i) => ({ s, x: xs[i], w: s.offsetWidth, h: s.offsetHeight }));
      const W = Math.max(0, ...items.map((c) => c.x + c.w));
      const H = Math.max(0, ...items.map((c) => c.h));
      return { items, W, H };
    });
    meas.remove();
    if (!laid.some((l) => l.items.length)) continue;
    const pairs = paths.length % 2 === 0;
    const slots = pairs ? paths.length / 2 : paths.length;
    const groups = Array.from({ length: slots }, () => []);
    laid.forEach((l, i) => groups[Math.min(slots - 1, Math.floor((i * slots) / laid.length))].push(l));
    const out = [];
    // Puts the character c (or its part from u0 to u1, clipped to it) with
    // the corners tl, tr (of that part) and bl.
    const place = (c, tl, tr, bl, u0 = 0, u1 = c.w) => {
      if (!(c.w > 0 && c.h > 0 && u1 > u0)) return;
      const a = (tr.x - tl.x) / (u1 - u0), b = (tr.y - tl.y) / (u1 - u0);
      const m = [a, b, (bl.x - tl.x) / c.h, (bl.y - tl.y) / c.h, tl.x - a * u0, tl.y - b * u0];
      if (!m.every(Number.isFinite)) return;
      const s = u0 > 0 || u1 < c.w ? c.s.cloneNode(true) : c.s;
      s.style.transform = `matrix(${m.join(",")})`;
      if (s !== c.s) {
        // Strips overlap a little so that no seam shows; the outer ones (and
        // all above and below) are open for outlines, shadows and glows.
        const l = u0 > 0 ? `${u0 - 0.4}px` : "-100%", r = u1 < c.w ? `${c.w - u1 - 0.4}px` : "-100%";
        s.style.clipPath = `inset(-100% ${r} -100% ${l})`;
      }
      // A gradient spans the line, not each character.
      if (s.style.backgroundImage) {
        s.style.backgroundSize = `${c.lineW}px ${c.lineH}px`;
        s.style.backgroundPosition = `${-c.x}px ${-(c.lineH - c.h)}px`;
      }
      out.push(s);
    };
    const cx = (spec.w * k) / 2, cy = (spec.h * k) / 2;
    groups.forEach((group, g) => {
      const m = group.length;
      group.forEach((line, li) => {
        const { W, H } = line;
        if (!(W > 0 && H > 0)) return;
        for (const c of line.items) Object.assign(c, { lineW: W, lineH: H });
        if (pairs) {
          // Between the top and bottom paths, this line's share of the band.
          const [top, bottom] = [paths[2 * g], paths[2 * g + 1]];
          const pt = (x, y) => {
            const a = top.frac(x / W), b = bottom.frac(x / W);
            const v = (li + y / H) / m;
            return { x: a.x + (b.x - a.x) * v, y: a.y + (b.y - a.y) * v };
          };
          for (const c of line.items) {
            // In vertical strips (about 5pt wide), each through its middle:
            // its center line and its width at half height.
            const y0 = H - c.h;
            const n = Math.max(1, Math.min(16, Math.round(c.w / (5 * k))));
            for (let i = 0; i < n; i++) {
              const u0 = (c.w * i) / n, u1 = (c.w * (i + 1)) / n, xm = c.x + (u0 + u1) / 2;
              const tm = pt(xm, y0), bm = pt(xm, H), l = pt(c.x + u0, y0 + c.h / 2), r = pt(c.x + u1, y0 + c.h / 2);
              const hx = (r.x - l.x) / 2, hy = (r.y - l.y) / 2;
              place(c, { x: tm.x - hx, y: tm.y - hy }, { x: tm.x + hx, y: tm.y + hy }, { x: bm.x - hx, y: bm.y - hy }, u0, u1);
            }
          }
          return;
        }
        // Standing on the path, stretched along it.
        const path = paths[g];
        const kx = path.total / W;
        const mid = path.at(path.total / 2);
        const side = mid.ty * (cx - mid.x) - mid.tx * (cy - mid.y);
        // The distance of the line's top from the path, along "up".
        const a = side > H / 2 ? (m - li) * H : side < -H / 2 ? -li * H : (m / 2 - li) * H;
        for (const c of line.items) {
          // Centered on the path's point under the character's middle, across
          // it as wide as the chord it takes, upright to it.
          const p0 = path.at(c.x * kx), p1 = path.at((c.x + c.w) * kx), pm = path.at((c.x + c.w / 2) * kx);
          const up = { x: pm.ty, y: -pm.tx };
          const half = Math.hypot(p1.x - p0.x, p1.y - p0.y) / 2;
          const hx = pm.tx * half, hy = pm.ty * half;
          const top = a - (H - c.h);
          const tx = pm.x + up.x * top, ty = pm.y + up.y * top;
          place(c, { x: tx - hx, y: ty - hy }, { x: tx + hx, y: ty + hy }, { x: tx - hx - up.x * c.h, y: ty - hy - up.y * c.h });
        }
      });
    });
    box.replaceChildren(...out);
  }
}

/** Draws the charts into their boxes; resolves when they are drawn. */
async function drawCharts(charts, m) {
  const boxes = Array.from(document.querySelectorAll(".xljs-print-chart[data-chart]"));
  if (!boxes.length) return;
  const ctx = { lang: m.lang || "ja", texts: m.texts || {}, dark: false, print: true };
  for (const box of boxes) {
    const c = charts[Number(box.dataset.chart)];
    if (!c || c.error) {
      box.append(Object.assign(document.createElement("div"), { className: "xljs-chart-error", textContent: c ? c.error : "" }));
      continue;
    }
    box.classList.add("xljs-chart");
    try {
      const spec = XljsChart.revive(c.spec);
      // Sharp on paper, drawn at once.
      spec.options = Object.assign({}, spec.options, { animation: { enabled: false }, overrideDevicePixelRatio: 3 });
      if (spec.type === "gantt") spec.viewSelect = false;
      XljsChart.create(box, spec, ctx);
    } catch (e) {
      box.append(Object.assign(document.createElement("div"), { className: "xljs-chart-error", textContent: e.message }));
    }
  }
  // Until every chart is drawn (or 5 seconds), then a moment for the last frame.
  const want = document.querySelectorAll(".xljs-print-chart.xljs-chart").length;
  const end = Date.now() + 5000;
  do await frames(2, 300); while (drawnCharts() < want && Date.now() < end);
  await wait(150);
}

host.on(async (m) => {
  if (m.type !== "init") return;
  try {
    place(m.html);
    if (document.fonts && document.fonts.ready) await Promise.race([document.fonts.ready, wait(3000)]);
    await Promise.all(Array.from(document.images).map((img) => (img.complete ? null : new Promise((r) => { img.onload = img.onerror = r; }))));
    warpText();
    shrinkToFit();
    hashNumbers();
    await drawCharts(m.charts || [], m);
    host.send({ type: "rendered", charts: document.querySelectorAll(".xljs-print-chart.xljs-chart").length, drawn: drawnCharts() });
  } catch (e) {
    host.send({ type: "error", message: String((e && e.message) || e) });
  }
});

host.send({ type: "ready" });
