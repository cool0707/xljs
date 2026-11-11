// Builds the reference site (GitHub Pages) into _site/:
//   - the pages are the Markdown of docs/site (front matter: title, description,
//     api, source), listed in docs/site/site.json;
//   - the API entries (signatures and doc comments) come from types/xljs.d.ts:
//     a page lists the declarations it documents in `api`, and every declaration
//     of the d.ts must be on exactly one page;
//   - the js/ts code blocks of the site, the README, the examples and the
//     documents for AI chats are type-checked against the d.ts.
// Any problem fails the build (exit code 1).
//
//   node build.mjs [--out dir] [--serve [port]]

import fs from "node:fs";
import path from "node:path";
import http from "node:http";
import ts from "typescript";
import { Marked } from "marked";
import hljs from "highlight.js";

const ROOT = path.resolve(import.meta.dirname, "../..");
const SITE = path.join(ROOT, "docs/site");
const DTS = path.join(ROOT, "types/xljs.d.ts");
const REPO = "https://github.com/cool0707/xljs";
const args = process.argv.slice(2);
const opt = (name) => {
  const i = args.indexOf(name);
  return i < 0 ? undefined : args[i + 1] && !args[i + 1].startsWith("--") ? args[i + 1] : true;
};
const OUT = path.resolve(ROOT, typeof opt("--out") === "string" ? opt("--out") : "_site");

const problems = [];
const problem = (where, msg) => problems.push(`${where}: ${msg}`);
const rel = (p) => path.relative(ROOT, p).split(path.sep).join("/");
const esc = (s) => String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

// ------------------------------------------------------------------ d.ts

/** The doc comment just before node, as text (the tags are kept as lines). */
function docOf(node, sf) {
  const ranges = ts.getLeadingCommentRanges(sf.text, node.pos) ?? [];
  const r = ranges.filter((r) => sf.text.startsWith("/**", r.pos)).pop();
  if (!r) return "";
  return sf.text
    .slice(r.pos + 3, r.end - 2)
    .split("\n")
    .map((l) => l.replace(/^\s*\* ?/, ""))
    .join("\n")
    .trim();
}

/** Source text of node without doc comments, on one line. */
function sig(node, sf) {
  return sf.text
    .slice(node.getStart(sf), node.end)
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/\s+/g, " ")
    .replace(/\s*;\s*$/, "")
    .replace(/\{ /g, "{ ")
    .trim();
}

function parseDts() {
  const text = fs.readFileSync(DTS, "utf8");
  const sf = ts.createSourceFile("xljs.d.ts", text, ts.ScriptTarget.Latest, true);
  const global = sf.statements.find((s) => ts.isModuleDeclaration(s) && s.name.getText(sf) === "global");
  const symbols = new Map();
  const add = (name, kind, node) => {
    if (!symbols.has(name)) symbols.set(name, { name, decls: [] });
    symbols.get(name).decls.push({ kind, node, line: sf.getLineAndCharacterOfPosition(node.getStart(sf)).line + 1 });
  };
  for (const st of global.body.statements) {
    if (ts.isInterfaceDeclaration(st)) add(st.name.text, "interface", st);
    else if (ts.isClassDeclaration(st)) add(st.name.text, "class", st);
    else if (ts.isTypeAliasDeclaration(st)) add(st.name.text, "type", st);
    else if (ts.isFunctionDeclaration(st)) add(st.name.text, "function", st);
    else if (ts.isVariableStatement(st)) for (const d of st.declarationList.declarations) add(d.name.text, "var", st);
  }
  for (const s of symbols.values()) {
    const kinds = s.decls.map((d) => d.kind);
    s.kind = ["class", "interface", "function", "type", "var"].find((k) => kinds.includes(k));
    s.line = s.decls[0].line;
    // A global const of an interface type (ui: Ui): members resolve through it.
    const v = s.decls.find((d) => d.kind === "var");
    const t = v?.node.declarationList.declarations.find((d) => d.name.text === s.name)?.type;
    if (t && ts.isTypeReferenceNode(t)) s.typeName = t.typeName.getText(sf);
    s.members = new Map();
    for (const d of s.decls) {
      if (d.kind !== "interface" && d.kind !== "class") continue;
      for (const m of d.node.members) {
        const name = m.name ? m.name.getText(sf).replace(/^["']|["']$/g, "") : ts.isConstructorDeclaration(m) ? "constructor" : null;
        if (!name) continue;
        if (!s.members.has(name)) s.members.set(name, []);
        s.members.get(name).push(m);
      }
    }
  }
  return { sf, symbols };
}

const dts = parseDts();

/** Where each declaration is documented: name -> page path ("api/excel"). */
const symbolPage = new Map();

function symbolHref(name, from) {
  const p = symbolPage.get(name);
  return p ? pageHref(from, p) + "#" + name : null;
}

/**
 * Resolves `Name`, `Name.member`, `ui.form(...)` (through the const's type) and
 * `sheet.toPdf` (through the aliases of site.json) to an anchor.
 */
function resolveRef(ref) {
  const m = /^(?:new )?([A-Za-z_$][\w$]*)(?:\.([A-Za-z_$][\w$]*))?(?:\(.*\))?$/.exec(ref);
  if (!m) return null;
  const [, a, b] = m;
  const s = dts.symbols.get(a) ?? (b && dts.symbols.get(aliases[a]));
  if (!s) return null;
  if (!b) return { name: a, anchor: a };
  if (s.members.has(b)) return { name: a, anchor: `${a}.${b}` };
  const t = s.typeName && dts.symbols.get(s.typeName);
  if (t?.members.has(b)) return { name: t.name, anchor: `${t.name}.${b}` };
  return null;
}

/** Signature HTML with the type names linked (string literals left alone). */
function linkTypes(code, from, self) {
  return code.replace(/("(?:[^"\\]|\\.)*"|'(?:[^'\\]|\\.)*'|`[^`]*`)|([A-Za-z_$][\w$]*)|([^"'`A-Za-z_$]+)/g, (all, str, id) => {
    if (str) return `<span class="str">${esc(str)}</span>`;
    if (id) {
      const s = dts.symbols.get(id);
      if (s && id !== self && s.kind !== "function" && s.kind !== "var") {
        const h = symbolHref(id, from);
        if (h) return `<a href="${h}">${esc(id)}</a>`;
      }
      return esc(id);
    }
    return esc(all);
  });
}

function renderDoc(doc, from) {
  if (!doc) return "";
  const lines = doc.split("\n");
  const body = [];
  const examples = [];
  let cur = null;
  for (const l of lines) {
    const t = /^@(\w+)\s*(.*)$/.exec(l);
    if (t && ["example", "param", "returns", "deprecated", "see"].includes(t[1])) {
      cur = t[1] === "example" ? (examples.push([t[2]]), examples.at(-1)) : (body.push(`**@${t[1]}** ${t[2]}`), null);
    } else if (cur) cur.push(l);
    else body.push(l);
  }
  // The lines of a Japanese sentence join without a space.
  const text = body.join("\n").replace(/([^\x00-\x7F])\n(?=[^\x00-\x7F])/g, "$1");
  let html = md(text, { from });
  for (const ex of examples) html += `<pre class="example"><code>${highlight(ex.join("\n").trim(), "js")}</code></pre>`;
  return html;
}

/** One declaration of the d.ts as an entry of its page. */
function renderSymbol(s, from) {
  const { sf } = dts;
  const out = [];
  const docs = [];
  const heads = [];
  for (const d of s.decls) {
    const doc = docOf(d.node, sf);
    if (doc) docs.push(doc);
    if (d.kind === "interface" || d.kind === "class") {
      const n = d.node;
      const head = sf.text.slice(n.getStart(sf), n.members.pos).replace(/\{\s*$/, "").replace(/\s+/g, " ").trim();
      if (head !== `${d.kind} ${s.name}`) heads.push(head);
    } else if (d.kind === "var") {
      const v = d.node.declarationList.declarations.find((x) => x.name.text === s.name);
      const t = v.type;
      if (s.kind !== "var" && t && ts.isTypeLiteralNode(t)) {
        // var Blob: { prototype: Blob; new (...): Blob } — the constructor.
        for (const m of t.members) {
          if (ts.isConstructSignatureDeclaration(m)) heads.push("new " + s.name + sig(m, sf).replace(/^new\s*/, "").replace(/:\s*[\w<>]+$/, ""));
          else if (!/^prototype\b/.test(sig(m, sf))) heads.push(`${s.name}.${sig(m, sf)}`);
        }
      } else if (s.kind === "var") {
        heads.push(sig(d.node, sf).replace(/^declare\s+/, ""));
      }
    } else {
      heads.push(sig(d.node, sf));
    }
  }
  const kindLabel = { interface: "interface", class: "class", type: "type", function: "function", var: "global" }[s.kind];
  out.push(`<section class="api" id="${esc(s.name)}">`);
  out.push(`<h3 class="api-name"><span class="kind">${kindLabel}</span> <a class="self" href="#${esc(s.name)}">${esc(s.name)}</a></h3>`);
  if (heads.length) out.push(`<pre class="sig">${heads.map((h) => linkTypes(h, from, s.name)).join("\n")}</pre>`);
  for (const d of docs) out.push(`<div class="doc">${renderDoc(d, from)}</div>`);
  if (s.members.size) {
    out.push(`<dl class="members">`);
    for (const [name, nodes] of s.members) {
      const id = `${s.name}.${name}`;
      const sigs = nodes.map((m) => linkTypes(sig(m, sf), from, null)).join("\n");
      const mdocs = [...new Set(nodes.map((m) => docOf(m, sf)).filter(Boolean))];
      out.push(`<dt id="${esc(id)}"><a class="self" href="#${esc(id)}">#</a><pre class="sig">${sigs}</pre></dt>`);
      out.push(`<dd>${mdocs.map((d) => renderDoc(d, from)).join("")}</dd>`);
    }
    out.push(`</dl>`);
  }
  out.push(`<p class="src"><a href="${REPO}/blob/main/types/xljs.d.ts#L${s.line}">xljs.d.ts</a></p>`);
  out.push(`</section>`);
  return out.join("\n");
}

// ------------------------------------------------------------------ pages

function frontMatter(text) {
  const m = /^---\n([\s\S]*?)\n---\n?/.exec(text.replace(/\r\n/g, "\n"));
  if (!m) return [{}, text];
  const meta = {};
  for (const l of m[1].split("\n")) {
    const kv = /^(\w+):\s*(.*)$/.exec(l);
    if (kv) meta[kv[1]] = kv[2].trim();
  }
  return [meta, text.replace(/\r\n/g, "\n").slice(m[0].length)];
}

const { nav, aliases } = JSON.parse(fs.readFileSync(path.join(SITE, "site.json"), "utf8"));
const pages = new Map(); // "api/excel" -> page
for (const sec of nav) {
  for (const p of sec.pages) {
    const file = path.join(SITE, p + ".md");
    if (!fs.existsSync(file)) {
      problem("docs/site/site.json", `${p}.md がない`);
      continue;
    }
    let [meta, body] = frontMatter(fs.readFileSync(file, "utf8"));
    let srcFile = file;
    if (meta.source) {
      // The body is another Markdown file of the repository (docs/ai/README.md).
      srcFile = path.resolve(path.dirname(file), meta.source);
      body = frontMatter(fs.readFileSync(srcFile, "utf8"))[1].replace(/^# .*\n+/, "");
    }
    const api = meta.api ? meta.api.split(",").map((s) => s.trim()).filter(Boolean) : [];
    pages.set(p, { path: p, file, srcFile, title: meta.title ?? p, description: meta.description ?? "", api, body, section: sec.title });
  }
}
for (const f of walk(SITE)) {
  if (!f.endsWith(".md")) continue;
  const p = rel(f).replace(/^docs\/site\//, "").replace(/\.md$/, "");
  if (!pages.has(p)) problem(rel(f), "site.json の nav にない");
}
for (const pg of pages.values()) {
  for (const name of pg.api) {
    if (!dts.symbols.has(name)) problem(rel(pg.file), `api: ${name} は xljs.d.ts にない`);
    else if (symbolPage.has(name)) problem(rel(pg.file), `api: ${name} は ${symbolPage.get(name)}.md にもある`);
    else symbolPage.set(name, pg.path);
  }
}
for (const s of dts.symbols.values()) {
  if (!symbolPage.has(s.name)) problem(`types/xljs.d.ts:${s.line}`, `${s.name} がどのページの api にもない（docs/site/api/*.md の api: に足す）`);
}

function walk(dir) {
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? walk(path.join(dir, e.name)) : [path.join(dir, e.name)]));
}

function pageHref(from, to) {
  let r = path.posix.relative(path.posix.dirname(from), to) + ".html";
  return r;
}

// ------------------------------------------------------------------ markdown

function highlight(code, lang) {
  const l = { js: "javascript", ts: "typescript", json: "json", html: "xml", css: "css", sh: "bash", bash: "bash", ps1: "powershell" }[lang];
  if (l) return hljs.highlight(code, { language: l }).value;
  return esc(code);
}

function slug(text) {
  return text
    .replace(/<[^>]+>/g, "")
    .replace(/&[a-z]+;/g, "")
    .trim()
    .toLowerCase()
    .replace(/[\s　]+/g, "-")
    .replace(/[!"#$%&'()*+,./:;<=>?@[\\\]^`{|}~（）「」『』、。・：？！]/g, "");
}

const copied = new Set();
const linkChecks = []; // [where, fromPage, targetPage, anchor]

/** Markdown to HTML. from: the page path the HTML is for; src: the file the Markdown is in (for relative links). */
function md(text, { from, src = SITE + "/" + from + ".md", toc = null } = {}) {
  const ids = new Set();
  const marked = new Marked({
    gfm: true,
    renderer: {
      heading({ tokens, depth }) {
        const html = this.parser.parseInline(tokens);
        let id = slug(html) || "h";
        for (let i = 2; ids.has(id); i++) id = slug(html) + "-" + i;
        ids.add(id);
        if (toc && depth <= 3 && depth >= 2) toc.push({ depth, id, html: html.replace(/<a [^>]*>|<\/a>/g, "") });
        return `<h${depth} id="${id}"><a class="self" href="#${id}">${html}</a></h${depth}>\n`;
      },
      code({ text, lang }) {
        const l = (lang ?? "").split(/\s+/)[0];
        return `<pre class="code"><code class="lang-${esc(l)}">${highlight(text, l)}</code></pre>\n`;
      },
      link({ href, title, tokens }) {
        const inner = this.parser.parseInline(tokens);
        return `<a href="${esc(fixHref(href, from, src))}"${title ? ` title="${esc(title)}"` : ""}>${inner}</a>`;
      },
      image({ href, title, text }) {
        return `<img src="${esc(fixHref(href, from, src, true))}" alt="${esc(text)}"${title ? ` title="${esc(title)}"` : ""}>`;
      },
      html({ text }) {
        return text.replace(/(src|href)="([^"]+)"/g, (a, k, v) => `${k}="${esc(fixHref(v, from, src, k === "src"))}"`);
      },
    },
  });
  return autolink(marked.parse(text), from);
}

/** Links of the Markdown: other pages of the site, files of the repository (to GitHub), images (copied). */
function fixHref(href, from, src, image = false) {
  if (/^([a-z]+:|#|\/\/)/i.test(href)) return href;
  const [p, hash] = href.split("#");
  const abs = path.resolve(path.dirname(src), decodeURI(p));
  const r = rel(abs);
  if (r.startsWith("..")) {
    problem(rel(src), `リンク先がリポジトリの外: ${href}`);
    return href;
  }
  // A page of the site, by its file or by the file it shows (source:).
  for (const pg of pages.values()) {
    if (abs === pg.file || abs === pg.srcFile) {
      linkChecks.push([rel(src), from, pg.path, hash]);
      return pageHref(from, pg.path) + (hash ? "#" + hash : "");
    }
  }
  if (!fs.existsSync(abs)) problem(rel(src), `リンク先がない: ${href}`);
  if (image) {
    const dest = "files/" + r;
    if (!copied.has(r)) {
      copied.add(r);
      fs.mkdirSync(path.dirname(path.join(OUT, dest)), { recursive: true });
      fs.copyFileSync(abs, path.join(OUT, dest));
    }
    return path.posix.relative(path.posix.dirname(from), dest);
  }
  const kind = fs.existsSync(abs) && fs.statSync(abs).isDirectory() ? "tree" : "blob";
  return `${REPO}/${kind}/main/${r}${hash ? "#" + hash : ""}`;
}

/** `Workbook` / `sheet.toPdf`-like inline code that names a declaration links to its entry. */
function autolink(html, from) {
  const parts = html.split(/(<a\b[\s\S]*?<\/a>|<pre\b[\s\S]*?<\/pre>|<h\d\b[\s\S]*?<\/h\d>)/);
  return parts
    .map((part, i) => {
      if (i % 2) return part;
      return part.replace(/<code>([^<]+)<\/code>/g, (all, c) => {
        const t = c.replace(/&amp;/g, "&").replace(/^await /, "").trim();
        const r = resolveRef(t);
        if (!r) return all;
        const p = symbolPage.get(r.name);
        if (!p) return all;
        return `<a class="api-link" href="${pageHref(from, p)}#${r.anchor}">${all}</a>`;
      });
    })
    .join("");
}

// ------------------------------------------------------------------ type-check the code blocks

function codeBlocks(file) {
  const text = fs.readFileSync(file, "utf8").replace(/\r\n/g, "\n");
  const blocks = [];
  const re = /^```(js|ts|javascript|typescript)(?![\w-])([^\n]*)\n([\s\S]*?)^```/gm;
  for (let m; (m = re.exec(text)); ) {
    if (/\bnocheck\b/.test(m[2])) continue;
    const line = text.slice(0, m.index).split("\n").length + 1;
    blocks.push({ file, line, lang: m[1].startsWith("t") ? "ts" : "js", code: m[3] });
  }
  return blocks;
}

function checkCode() {
  const files = [
    ...[...pages.values()].map((p) => p.srcFile),
    path.join(ROOT, "README.md"),
    path.join(ROOT, "examples/README.md"),
    ...walk(path.join(ROOT, "docs/ai")).filter((f) => f.endsWith(".md")),
  ];
  const blocks = [...new Set(files)].flatMap(codeBlocks);
  const vfs = new Map([["/x/xljs.d.ts", fs.readFileSync(DTS, "utf8")]]);
  blocks.forEach((b, i) => {
    b.name = `/x/b${i}.${b.lang}`;
    // A module (top-level await) and the line numbers kept.
    vfs.set(b.name, b.code + "\nexport {};\n");
  });
  const options = {
    target: ts.ScriptTarget.ES2022,
    module: ts.ModuleKind.ESNext,
    moduleResolution: ts.ModuleResolutionKind.Bundler,
    moduleDetection: ts.ModuleDetectionKind.Force,
    lib: ["lib.es2023.d.ts"],
    allowJs: true,
    checkJs: true,
    noEmit: true,
    strict: false,
    skipLibCheck: true,
    types: [],
  };
  const host = ts.createCompilerHost(options);
  const orig = { getSourceFile: host.getSourceFile, fileExists: host.fileExists, readFile: host.readFile };
  host.getSourceFile = (f, lang, ...rest) => (vfs.has(f) ? ts.createSourceFile(f, vfs.get(f), lang, true) : orig.getSourceFile.call(host, f, lang, ...rest));
  host.fileExists = (f) => vfs.has(f) || orig.fileExists.call(host, f);
  host.readFile = (f) => vfs.get(f) ?? orig.readFile.call(host, f);
  const program = ts.createProgram([...vfs.keys()], options, host);
  // Snippets use variables of the text around them (book, sheet, rows...):
  // unknown names are fine, the API they call on known names is not.
  const ignore = new Set([2307, 2304, 2552, 2582, 2580, 7006, 2448, 2454, 1375, 1378, 2451, 2393, 2300]);
  const byName = new Map(blocks.map((b) => [b.name, b]));
  for (const d of ts.getPreEmitDiagnostics(program)) {
    if (ignore.has(d.code)) continue;
    const b = d.file && byName.get(d.file.fileName);
    const msg = ts.flattenDiagnosticMessageText(d.messageText, "\n");
    if (!b) {
      problem(d.file ? d.file.fileName : "tsc", msg);
      continue;
    }
    const { line } = d.file.getLineAndCharacterOfPosition(d.start);
    problem(`${rel(b.file)}:${b.line + line}`, `TS${d.code}: ${msg}`);
  }
  return blocks.length;
}

// ------------------------------------------------------------------ layout

const CSS = fs.readFileSync(path.join(import.meta.dirname, "site.css"), "utf8");
const JS = fs.readFileSync(path.join(import.meta.dirname, "site.js"), "utf8");

function layout(pg, content, toc) {
  const to = (p) => pageHref(pg.path, p);
  const navHtml = nav
    .map(
      (sec) =>
        `<h2>${esc(sec.title)}</h2><ul>${sec.pages
          .filter((p) => pages.has(p))
          .map((p) => `<li${p === pg.path ? ' class="current"' : ""}><a href="${to(p)}">${esc(pages.get(p).title)}</a></li>`)
          .join("")}</ul>`,
    )
    .join("");
  const tocHtml = toc.length
    ? `<nav class="toc" aria-label="このページの内容"><h2>このページの内容</h2><ul>${toc
        .map((t) => `<li class="d${t.depth}"><a href="#${t.id}">${t.html}</a></li>`)
        .join("")}</ul></nav>`
    : "";
  const base = path.posix.relative(path.posix.dirname(pg.path), ".") || ".";
  const title = pg.path === "index" ? "xljs リファレンス" : `${pg.title} - xljs`;
  return `<!doctype html>
<html lang="ja">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${esc(title)}</title>
${pg.description ? `<meta name="description" content="${esc(pg.description)}">` : ""}
<link rel="icon" href="${base}/files/assets/xljs.png">
<link rel="stylesheet" href="${base}/site.css">
</head>
<body>
<header class="top">
  <button class="menu" aria-label="メニュー" aria-expanded="false">☰</button>
  <a class="brand" href="${to("index")}"><img src="${base}/files/assets/xljs.png" alt="">xljs</a>
  <div class="search"><input type="search" placeholder="API・ページを検索（/）" aria-label="検索" autocomplete="off"><ul class="results" hidden></ul></div>
  <a class="gh" href="${REPO}">GitHub</a>
</header>
<div class="wrap">
<nav class="side" aria-label="目次">${navHtml}</nav>
<main>
<p class="crumb">${esc(pg.section)}</p>
<h1>${esc(pg.title)}</h1>
${content}
<footer><a href="${REPO}/edit/main/${rel(pg.srcFile)}">このページを編集</a></footer>
</main>
${tocHtml}
</div>
<script>window.XLJS_BASE = ${JSON.stringify(base)};</script>
<script src="${base}/search.js"></script>
<script src="${base}/site.js"></script>
</body>
</html>
`;
}

// ------------------------------------------------------------------ build

fs.rmSync(OUT, { recursive: true, force: true });
fs.mkdirSync(OUT, { recursive: true });
fs.mkdirSync(path.join(OUT, "files/assets"), { recursive: true });
fs.copyFileSync(path.join(ROOT, "assets/xljs.png"), path.join(OUT, "files/assets/xljs.png"));
copied.add("assets/xljs.png");
fs.copyFileSync(DTS, path.join(OUT, "xljs.d.ts"));
fs.writeFileSync(path.join(OUT, "site.css"), CSS);
fs.writeFileSync(path.join(OUT, "site.js"), JS);
fs.writeFileSync(path.join(OUT, ".nojekyll"), "");

const search = [];
const pageIds = new Map();
for (const pg of pages.values()) {
  const toc = [];
  const parts = pg.body.split(/^<!-- api -->$/m);
  let content = md(parts[0], { from: pg.path, src: pg.srcFile, toc });
  let apiHtml = "";
  if (pg.api.length) {
    const syms = pg.api.filter((n) => dts.symbols.has(n)).map((n) => dts.symbols.get(n));
    toc.push({ depth: 2, id: "api", html: "リファレンス" });
    for (const s of syms) toc.push({ depth: 3, id: s.name, html: esc(s.name) });
    apiHtml = `<h2 id="api"><a class="self" href="#api">リファレンス</a></h2>\n` + syms.map((s) => renderSymbol(s, pg.path)).join("\n");
    for (const s of syms) {
      search.push({ t: s.name, k: s.kind, u: `${pg.path}.html#${s.name}` });
      for (const m of s.members.keys()) search.push({ t: `${s.name}.${m}`, k: "member", u: `${pg.path}.html#${s.name}.${m}` });
      // ui.form, fs.readJSON...: the globals that hold the interface.
      for (const g of dts.symbols.values()) {
        if (g.typeName === s.name) for (const m of s.members.keys()) search.push({ t: `${g.name}.${m}`, k: "member", u: `${pg.path}.html#${s.name}.${m}` });
      }
    }
  }
  if (parts.length > 1) {
    // Headings after the marker come after the entries in the outline.
    const rest = [];
    content += apiHtml + md(parts[1], { from: pg.path, src: pg.srcFile, toc: rest });
    toc.push(...rest);
  } else content += apiHtml;
  search.push({ t: pg.title, k: "page", u: `${pg.path}.html` });
  for (const h of toc) if (h.id !== "api" && !dts.symbols.has(h.id)) search.push({ t: h.html.replace(/<[^>]+>/g, ""), k: pg.title, u: `${pg.path}.html#${h.id}` });
  const html = layout(pg, content, toc);
  pageIds.set(pg.path, new Set([...html.matchAll(/\bid="([^"]+)"/g)].map((m) => m[1].replace(/&quot;/g, '"').replace(/&amp;/g, "&"))));
  const dest = path.join(OUT, pg.path + ".html");
  fs.mkdirSync(path.dirname(dest), { recursive: true });
  fs.writeFileSync(dest, html);
}
fs.writeFileSync(path.join(OUT, "search.js"), `window.XLJS_SEARCH = ${JSON.stringify(search)};\n`);

for (const [where, , to, hash] of linkChecks) {
  if (hash && !pageIds.get(to)?.has(decodeURIComponent(hash))) problem(where, `${to} に #${hash} がない`);
}

const nblocks = checkCode();

if (problems.length) {
  console.error(problems.map((p) => "error: " + p).join("\n"));
  console.error(`\n${problems.length} 件の問題があります`);
  process.exit(1);
}
console.log(`${pages.size} ページ、API ${dts.symbols.size} 件、コード例 ${nblocks} 個を検査 → ${rel(OUT)}/`);

if (opt("--serve")) {
  const port = typeof opt("--serve") === "string" ? Number(opt("--serve")) : 8080;
  const types = { ".html": "text/html; charset=utf-8", ".css": "text/css", ".js": "text/javascript", ".png": "image/png", ".ts": "text/plain; charset=utf-8" };
  http
    .createServer((req, res) => {
      let p = decodeURIComponent(new URL(req.url, "http://x").pathname);
      if (p.endsWith("/")) p += "index.html";
      const f = path.join(OUT, path.normalize(p));
      if (!f.startsWith(OUT) || !fs.existsSync(f)) return res.writeHead(404).end("not found");
      res.writeHead(200, { "content-type": types[path.extname(f)] ?? "application/octet-stream" }).end(fs.readFileSync(f));
    })
    .listen(port, () => console.log(`http://localhost:${port}/`));
}
