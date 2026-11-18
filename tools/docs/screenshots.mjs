// The screenshots of the guide "はじめての xljs" (docs/site/guide/first-steps.md)
// and of the API pages (form, table, chart): the built-in pages (src/ui/web) in Chromium with the Rust side played here,
// as the site's dev mode mock does (demo/dev-demo.js). Run `node build.mjs`
// first (the pages are taken from _site/demo/web).
//
//   node build.mjs && node screenshots.mjs
//
// The pages draw with the fonts of the machine (Noto Sans CJK on Linux in
// place of Yu Gothic UI). The chat itself (a WebView2 of its own in the
// real window) is not drawn: a gray placeholder stands where it would be.

import fs from "node:fs";
import http from "node:http";
import path from "node:path";
import { createRequire } from "node:module";

// Playwright is not among the site's packages: the one installed on the machine.
const { chromium } = createRequire(import.meta.url)("playwright");

const ROOT = path.resolve(import.meta.dirname, "../..");
const WEB = path.join(ROOT, "_site/demo/web");
const OUT = path.join(ROOT, "docs/site/images");
if (!fs.existsSync(path.join(WEB, "dev.html"))) {
  console.error("_site/demo/web がありません。先に node build.mjs を実行してください");
  process.exit(1);
}

const messages = (lang) => JSON.parse(fs.readFileSync(path.join(ROOT, `src/i18n/locales/${lang}.json`), "utf8")).messages;
const all = { ...messages("en"), ...messages("ja") };
const raw = (k) => String(all[k] ?? k);
const t = (k, ...args) => String(all[k] ?? k).replace(/\{(\d+)\}/g, (_, i) => args[i]);
const under = (prefix) => Object.fromEntries(Object.entries(all).filter(([k]) => k.startsWith(prefix)).map(([k, v]) => [k.slice(prefix.length), v]));

// ------------------------------------------------------------ the Rust side

// src/cli/launch.rs: the start screen's choices.
const launchData = {
  message: t("cli.launchMessage"),
  choices: [
    ["cli.launchRun", "cli.launchRunNote", "play"],
    ["cli.launchDebug", "cli.launchDebugNote", "bug"],
    ["cli.launchDev", "cli.launchDevNote", "sparkles"],
    ["cli.launchPack", "cli.launchPackNote", "package"],
    ["cli.launchVBA", "cli.launchVBANote", "scroll-text"],
    ["cli.launchAssoc", "cli.launchAssocNote", "link"],
  ].map(([label, note, icon]) => ({ label: t(label), note: t(note), icon })),
};

// src/dev/mod.rs: SERVICES.
const services = [
  ["Microsoft Copilot", "https://copilot.microsoft.com/", "copilot"],
  ["Microsoft 365 Copilot", "https://m365.cloud.microsoft/chat", "m365"],
  ["Gemini", "https://gemini.google.com/app", "gemini"],
  ["ChatGPT", "https://chatgpt.com/", "openai"],
  ["Claude", "https://claude.ai/new", "claude"],
].map(([name, url, icon]) => ({ name, url, icon }));
const agents = [["m365", "Microsoft 365 Copilot"], ["gemini", "Gemini"], ["chatgpt", "ChatGPT"], ["claude", "Claude"]]
  .map(([id, name]) => ({ id, what: t(`dev.agentWhat.${id}`), name }));

// src/dev/agent.rs: kit(Service::Gemini).
const SKILLS = ["xljs-reference", "xljs-excel", "xljs-app-design", "xljs-pdf", "xljs-sqlite", "xljs-network", "xljs-chart", "vba-port"];
const aiDoc = (f) => fs.readFileSync(path.join(ROOT, "docs/ai", f), "utf8");
const skillDescription = (text) => (text.match(/^---[\s\S]*?^description:(.*)$/m)?.[1] ?? "").trim();
const item = (file, name, description) => ({ file, name, description, size: [...aiDoc(file)].length });
const geminiKit = {
  service: "gemini",
  what: t("dev.agentWhat.gemini"),
  name: t("dev.agentNameExample"),
  description: t("dev.agentDescriptionExample"),
  steps: t("dev.agentSteps.gemini"),
  instructions: item("gemini/instructions.md", "instructions.md", ""),
  filesKind: "knowledge",
  files: SKILLS.map((n) => {
    const file = `copilot/skills/${n}/SKILL.md`;
    return item(file, `${n}.txt`, skillDescription(aiDoc(file)));
  }),
  createUrl: "https://gemini.google.com/gems/create",
};

const DIR = "C:\\Users\\user\\Documents\\xljs";
const devData = {
  init: { type: "init", lang: "ja", texts: under("devPage."), formTexts: under("form."), services, agents, recent: [], home: DIR, homeChat: "", dts: "" },
  kit: geminiKit,
  folder: DIR,
};

// Put in front of the pages: chrome.webview answered by window.__answer.
const mock = `
(() => {
  const listeners = [];
  window.__post = (m) => setTimeout(() => { for (const fn of listeners) fn({ data: "\\x02" + (typeof m === "string" ? m : JSON.stringify(m)) }); }, 0);
  // window.__shotTitle (set by an init script): the window's title and Font Awesome icon.
  window.__xljsTitleBar = Object.assign({ title: "xljs", icon: "", texts: ${JSON.stringify(under("window."))}, nonClient: true }, window.__shotTitle);
  window.chrome = window.chrome || {};
  window.chrome.webview = {
    addEventListener(type, fn) { if (type === "message") listeners.push(fn); },
    removeEventListener() {},
    postMessage(s) {
      if (typeof s !== "string" || s[0] === "\\x01") return;
      let m; try { m = JSON.parse(s); } catch { return; }
      window.__sent = (window.__sent || []).concat([m]);
      if (window.__answer) window.__answer(m);
    },
  };
  // The title bar's glyphs are those of Windows' icon font: drawn here.
  const glyphs = {
    minimize: '<svg viewBox="0 0 10 10"><path d="M0 5.5h10"/></svg>',
    maximize: '<svg viewBox="0 0 10 10"><rect x=".5" y=".5" width="9" height="9"/></svg>',
    close: '<svg viewBox="0 0 10 10"><path d="M0 0l10 10M10 0L0 10"/></svg>',
  };
  // On load: after titlebar.js has built the bar (on DOMContentLoaded, after this).
  window.addEventListener("load", () => {
    const css = document.createElement("style");
    css.textContent = ".xljs-tb-btn svg { width: 10px; height: 10px; fill: none; stroke: currentColor; stroke-width: 1; }";
    document.head.append(css);
    for (const b of document.querySelectorAll(".xljs-tb-btn")) b.innerHTML = glyphs[b.dataset.xljsWindow] || "";
    // A window with the OS title bar: the program's icon before the title.
    const title = document.querySelector(".xljs-tb-title");
    if (window.__shotTitle && window.__shotTitle.os && title && !title.querySelector("i")) {
      const img = document.createElement("img");
      img.src = "xljs-logo.png";
      img.style.cssText = "width:16px;height:16px;flex:none";
      title.prepend(img);
      // As Windows draws its own title bar: small text on gray.
      css.textContent += ".xljs-titlebar { --xljs-titlebar-height: 32px; background: #f3f3f3; } .xljs-tb-title { font-size: 12px; padding: 0 10px; gap: 8px; }";
    }
  });
})();
`;

// ------------------------------------------------------------ the server

const types = { ".html": "text/html; charset=utf-8", ".css": "text/css", ".js": "text/javascript", ".png": "image/png", ".svg": "image/svg+xml", ".woff2": "font/woff2", ".ttf": "font/ttf" };
const server = http.createServer((req, res) => {
  const name = decodeURIComponent(new URL(req.url, "http://x").pathname).replace(/^\/+/, "");
  const file = path.join(WEB, name);
  if (!file.startsWith(WEB) || !fs.existsSync(file) || fs.statSync(file).isDirectory()) {
    res.writeHead(404).end();
    return;
  }
  let body = fs.readFileSync(file);
  if (["launch.html", "dev.html", "form.html", "table.html", "chart.html"].includes(name)) {
    // The title bar script after the page's own (as WebView2 injects it).
    body = body.toString("utf8").replace("<head>", `<head>\n<script>${mock}</script>`).replace("</body>", '<script src="titlebar.js"></script>\n</body>');
  }
  res.writeHead(200, { "content-type": types[path.extname(name)] || "application/octet-stream" }).end(body);
});
await new Promise((r) => server.listen(0, "127.0.0.1", r));
const base = `http://127.0.0.1:${server.address().port}/`;

const browser = await chromium.launch();
const shot = async (page, name, opts = {}) => {
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(300);
  await page.screenshot({ path: path.join(OUT, name), ...opts });
  console.log("docs/site/images/" + name);
};

// ------------------------------------------------------------ the start screen

{
  const page = await browser.newPage({ viewport: { width: 600, height: 576 }, deviceScaleFactor: 2 });
  await page.addInitScript((data) => {
    window.__answer = (m) => { if (m.op === "ready") window.__post(data); };
  }, launchData);
  await page.goto(base + "launch.html");
  await page.waitForSelector("button.choice");
  // No focus ring on the first choice in the picture.
  await page.evaluate(() => document.activeElement && document.activeElement.blur());
  await shot(page, "first-launch.png");
  await page.close();
}

// ------------------------------------------------------------ the dev mode

async function devPage() {
  const page = await browser.newPage({ viewport: { width: 1280, height: 820 }, deviceScaleFactor: 2 });
  await page.addInitScript((data) => {
    window.__answer = (m) => {
      switch (m.op) {
        case "ready": window.__post(data.init); break;
        case "chat": window.__post({ type: "url", url: m.url }); break;
        case "agentHint": {
          const gemini = /gemini\.google\.com/.test(m.url);
          window.__post({ type: "agentHint", url: m.url, service: gemini ? "gemini" : "", custom: false, what: gemini ? data.kit.what : "" });
          break;
        }
        case "agentKit": window.__post({ type: "agentKit", kit: data.kit }); break;
        case "wizardFolder": window.__post({ type: "wizardFolder", path: data.folder }); break;
      }
    };
  }, devData);
  await page.goto(base + "dev.html");
  await page.waitForFunction(() => document.querySelector("#welcome h2")?.textContent.length > 0);
  return page;
}

// Where the chat's own window would be.
async function chatPlaceholder(page, text) {
  await page.evaluate((text) => {
    const view = document.getElementById("chat-view");
    const p = document.createElement("div");
    p.style.cssText = "position:absolute;inset:0;display:flex;align-items:center;justify-content:center;background:#f4f5f7;color:#8a8f98;font-size:15px;z-index:0";
    p.textContent = text;
    view.prepend(p);
  }, text);
}

{
  // The start: the chat to open on the left, the script on the right.
  const page = await devPage();
  await page.evaluate(() => document.activeElement && document.activeElement.blur());
  await shot(page, "first-dev.png");

  // Gemini opened: the hint over the chat, and what goes into a Gem.
  await page.locator("#services button", { hasText: "Gemini" }).click();
  await page.waitForSelector("#agent-hint:not([hidden])");
  await chatPlaceholder(page, "（ここに Gemini の画面が表示されます）");
  await page.mouse.move(0, 0);
  await shot(page, "first-agent-hint.png");
  await page.click("#agent-hint-prompt");
  await page.waitForSelector("#agent:not([hidden])");
  await page.mouse.move(0, 0);
  await shot(page, "first-agent.png");
  await page.close();
}

{
  // The wizard, with the Gem's chat on the left.
  const page = await devPage();
  await page.evaluate(() => { document.getElementById("chat-default").checked = true; });
  await page.locator("#services button", { hasText: "Gemini" }).click();
  await chatPlaceholder(page, "（ここに xljs 用の Gem のチャットが表示されます）");
  await page.evaluate(() => { document.getElementById("agent-hint").remove(); });
  await page.click("#w-wizard");
  await page.waitForSelector("#wizard:not([hidden])");
  await page.mouse.move(0, 0);
  await shot(page, "first-wizard.png");
  await page.fill("#wz-overview", "フォルダ内の売上ファイル（Excel）を読み込んで、担当者ごと・月ごとの売上を集計し、集計表を新しいブックに保存したい。\n読み込むフォルダと集計する月の範囲は画面で選べるようにしたい。");
  await page.check('input[name="wz-name-mode"][value="name"]');
  await page.fill("#wz-name", "売上集計");
  await page.evaluate(() => {
    const s = document.getElementById("wz-make").closest("section");
    s.scrollIntoView({ block: "end" });
    document.activeElement.blur();
  });
  await page.mouse.move(0, 0);
  await shot(page, "first-wizard-overview.png");
  await page.close();
}

// ------------------------------------------------------------ the API pages
//
// The forms (and the Gantt chart in a form) are the first messages of real
// scripts, taken as the dev mode's form preview does (shots/README.md); the
// table is the message ui.table sends (prelude.js tableWindow), written here.

const shotJson = (name) => JSON.parse(fs.readFileSync(path.join(import.meta.dirname, "shots", name), "utf8"));
// The day the pictures show as today (the Gantt chart's base date).
const TODAY = new Date("2026-11-18T10:00:00Z");

async function windowPage(file, size, title, init, after) {
  const page = await browser.newPage({ viewport: size, deviceScaleFactor: 2, locale: "ja-JP" });
  await page.clock.setFixedTime(TODAY);
  await page.addInitScript(({ title, init, after }) => {
    window.__shotTitle = title;
    window.__answer = (m) => {
      if (m.type !== "ready") return;
      for (const x of [init, ...after]) window.__post(x);
    };
  }, { title, init, after });
  await page.goto(base + file);
  return page;
}

const settle = async (page) => {
  await page.evaluate(() => document.activeElement && document.activeElement.blur());
  await page.mouse.move(0, 0);
};

{
  // A tool: the form after a run (examples/form-aggregate.xljs).
  const init = shotJson("form-aggregate.json");
  const files = ["2026-04/東京.xlsx", "2026-04/大阪.xlsx", "2026-05/東京.xlsx", "2026-05/大阪.xlsx", "2026-06/東京.xlsx", "2026-06/大阪.xlsx"];
  const run = [
    { type: "picked", field: "src", value: "C:\\Users\\user\\Documents\\売上", quiet: true },
    { type: "running" },
    { type: "progress", done: files.length, total: files.length, message: files[files.length - 1] },
    { type: "log", lines: files },
    { type: "log", lines: ["顧客 12 件を集計結果.xlsx に保存しました"] },
    { type: "done", ok: true, output: true },
  ];
  const page = await windowPage("form.html", { width: 640, height: 760 }, { title: "売上集計", icon: "fa-solid fa-chart-column" }, init, run);
  await page.waitForSelector("#status.ok");
  await settle(page);
  await shot(page, "form-tool.png");
  await page.close();
}

{
  // Tabs and a table to choose rows from (examples/form-tabs-table.xljs).
  const page = await windowPage("form.html", { width: 640, height: 500 }, { title: "支店別集計", os: true }, shotJson("form-tabs-table.json"), []);
  await page.waitForSelector("#tabs:not([hidden])");
  await settle(page);
  await shot(page, "form-tabs.png");
  await page.close();
}

{
  // A Gantt chart in a form (examples/chart-gantt.xljs).
  const page = await windowPage("form.html", { width: 960, height: 600 }, { title: "工程表", os: true }, shotJson("chart-gantt.json"), []);
  await page.waitForSelector(".gantt .bar-wrapper");
  await page.waitForTimeout(500);
  await settle(page);
  await shot(page, "chart-gantt.png");
  await page.close();
}

{
  // ui.table with conditional formats, and a column's filter.
  const rows = [
    ["田中", "東京", 42, 5230000, 112, "達成"],
    ["山田", "東京", 35, 3980000, 96, "未達"],
    ["佐藤", "大阪", 51, 6120000, 131, "達成"],
    ["鈴木", "大阪", 28, 2410000, 74, "未達"],
    ["高橋", "名古屋", 39, 4470000, 103, "達成"],
    ["伊藤", "名古屋", 22, 1890000, 63, "未達"],
    ["渡辺", "福岡", 33, 3650000, 99, "未達"],
    ["中村", "福岡", 46, 5580000, 120, "達成"],
  ];
  const tableTexts = {
    filter: raw("table.filter"), count: raw("table.count"), countFiltered: raw("table.countFiltered"), loading: raw("table.loading"),
    selected: raw("table.selected"), copy: raw("table.copy"), copied: raw("table.copied"), saveCsv: raw("table.saveCsv"),
    saveXlsx: raw("table.saveXlsx"), saved: raw("table.saved"), ok: raw("form.ok"), cancel: raw("form.cancel"), close: raw("form.close"),
    empty: raw("table.empty"), grid: { ...under("grid."), ops: under("grid.op.") },
  };
  const style = (o) => ({ cols: null, conds: [], fn: false, row: false, style: {}, bar: null, scale: null, ...o });
  const formats = [
    style({ cols: [5], conds: [["equals", "未達"]], row: true, style: { color: "#9c0006", background: "#ffc7ce" } }),
    style({ cols: [3], bar: { color: "#638EC6", height: 0.75 } }),
    style({ cols: [4], scale: ["#f8696b", "#ffeb84", "#63be7b"] }),
  ];
  const init = {
    type: "init", select: "", total: rows.length, texts: tableTexts, dblclick: false, accent: null, background: null, radius: "",
    columns: ["担当", "支店", "件数", "金額", "達成率", "状態"].map((label) => ({ label, width: 110 })), formats,
  };
  const page = await windowPage("table.html", { width: 820, height: 420 }, { title: "売上一覧", os: true }, init, [{ type: "rows", rows }]);
  await page.waitForFunction(() => document.body.innerText.includes("中村"));
  await settle(page);
  await shot(page, "table-formats.png");
  // The filter of the 支店 column.
  const head = page.locator(".gk-head", { hasText: "支店" });
  await head.hover();
  await head.locator(".gk-filter").click();
  await page.waitForSelector(".gk-filter-pop");
  await page.locator(".gk-filter-pop input").first().fill("大阪");
  await page.waitForFunction(() => !document.body.innerText.includes("中村"));
  await shot(page, "table-filter.png");
  await page.close();
}

await browser.close();
server.close();
