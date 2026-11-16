"use strict";
// A small Markdown renderer for the development mode's preview of manuals
// (dev.js): headings, paragraphs (a line ending in two spaces or a
// backslash breaks), lists, block quotes, code, rules, tables, images,
// links, bold, italics and code spans. It builds elements (no HTML from the
// text is used). Relative image paths are resolved against base.
const markdown = (() => {
  const el = (tag, props = {}, ...children) => {
    const e = Object.assign(document.createElement(tag), props);
    for (const c of children) if (c !== null && c !== undefined) e.append(c);
    return e;
  };

  /** A path of the text as a URL (relative ones against base). */
  function url(src, base) {
    if (/^(https?:|data:)/i.test(src)) return src;
    return base + src.split("/").map((p) => encodeURIComponent(decodeURIComponent(p))).join("/");
  }

  /** The inline parts of a line: code, images, links, bold and italics. */
  function inline(text, base, opts) {
    const out = [];
    const re = /`([^`]+)`|!\[([^\]]*)\]\(([^)\s]+)\)|\[([^\]]+)\]\(([^)\s]+)\)|\*\*(.+?)\*\*|\*([^*\s][^*]*?)\*/g;
    let at = 0, m;
    while ((m = re.exec(text))) {
      if (m.index > at) out.push(text.slice(at, m.index));
      if (m[1] !== undefined) out.push(el("code", { textContent: m[1] }));
      else if (m[3] !== undefined) {
        const img = el("img", { alt: m[2], src: url(m[3], base) + (opts.stamp ? "?t=" + opts.stamp : "") });
        if (opts.onImage) img.onclick = () => opts.onImage(img.src);
        out.push(img);
      } else if (m[5] !== undefined) out.push(el("a", { href: m[5], title: m[5], onclick: (e) => e.preventDefault() }, ...inline(m[4], base, opts)));
      else if (m[6] !== undefined) out.push(el("strong", {}, ...inline(m[6], base, opts)));
      else out.push(el("em", {}, ...inline(m[7], base, opts)));
      at = re.lastIndex;
    }
    if (at < text.length) out.push(text.slice(at));
    return out;
  }

  /** The lines of a paragraph, with their breaks. */
  function para(lines, base, opts) {
    const out = [];
    lines.forEach((l, k) => {
      const hard = / {2,}$|\\$/.test(l);
      out.push(...inline(l.replace(/ {2,}$|\\$/, "").trim(), base, opts));
      if (k < lines.length - 1) out.push(hard ? el("br") : " ");
    });
    return out;
  }

  const cells = (l) => l.trim().replace(/^\||\|$/g, "").split("|").map((c) => c.trim());

  /**
   * Renders text into a fragment. opts: { stamp (added to image URLs so
   * that new pictures load), onImage(src) (a click on an image) }.
   */
  function render(text, base = "", opts = {}) {
    const frag = document.createDocumentFragment();
    const lines = String(text).replace(/\r\n?/g, "\n").split("\n");
    let i = 0;
    while (i < lines.length) {
      const l = lines[i];
      if (!l.trim()) { i++; continue; }
      let m;
      if ((m = /^(#{1,6})\s+(.*)$/.exec(l))) {
        frag.append(el("h" + m[1].length, {}, ...inline(m[2].replace(/\s+#+\s*$/, ""), base, opts)));
        i++;
      } else if (/^```/.test(l)) {
        const code = [];
        for (i++; i < lines.length && !/^```/.test(lines[i]); i++) code.push(lines[i]);
        i++;
        frag.append(el("pre", {}, el("code", { textContent: code.join("\n") })));
      } else if (/^\s*([-*_])(\s*\1){2,}\s*$/.test(l)) {
        frag.append(el("hr"));
        i++;
      } else if (/^>/.test(l)) {
        const q = [];
        for (; i < lines.length && /^>/.test(lines[i]); i++) q.push(lines[i].replace(/^>\s?/, ""));
        frag.append(el("blockquote", {}, render(q.join("\n"), base, opts)));
      } else if (/^\s*([-*+]|\d+[.)])\s+/.test(l)) {
        const ordered = /^\s*\d/.test(l);
        const list = el(ordered ? "ol" : "ul");
        for (; i < lines.length && /^\s*([-*+]|\d+[.)])\s+/.test(lines[i]); i++) {
          const item = [lines[i].replace(/^\s*([-*+]|\d+[.)])\s+/, "")];
          // Lines indented under an item belong to it.
          while (i + 1 < lines.length && /^\s{2,}\S/.test(lines[i + 1]) && !/^\s*([-*+]|\d+[.)])\s+/.test(lines[i + 1])) item.push(lines[++i].trim());
          list.append(el("li", {}, ...para(item, base, opts)));
        }
        frag.append(list);
      } else if (/^\s*\|.*\|\s*$/.test(l) && i + 1 < lines.length && /^\s*\|?\s*:?-{2,}/.test(lines[i + 1])) {
        const table = el("table");
        table.append(el("thead", {}, el("tr", {}, ...cells(l).map((c) => el("th", {}, ...inline(c, base, opts))))));
        const body = el("tbody");
        for (i += 2; i < lines.length && /^\s*\|/.test(lines[i]); i++) body.append(el("tr", {}, ...cells(lines[i]).map((c) => el("td", {}, ...inline(c, base, opts)))));
        table.append(body);
        frag.append(table);
      } else {
        const p = [];
        for (; i < lines.length && lines[i].trim() && !/^(#{1,6}\s|```|>|\s*([-*+]|\d+[.)])\s+)/.test(lines[i]); i++) p.push(lines[i]);
        if (!p.length) p.push(lines[i++]);
        frag.append(el("p", {}, ...para(p, base, opts)));
      }
    }
    return frag;
  }

  return { render };
})();
