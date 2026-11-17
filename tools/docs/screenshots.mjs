// The screenshots of the guide "はじめての xljs" (docs/site/guide/first-steps.md):
// the built-in pages (src/ui/web) in Chromium with the Rust side played here,
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
const SKILLS = ["xljs-reference", "xljs-app-design", "xljs-pdf", "xljs-sqlite", "xljs-network", "xljs-chart", "vba-port"];
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
  window.__xljsTitleBar = { title: "xljs", icon: "", texts: ${JSON.stringify(under("window."))}, nonClient: true };
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
  document.addEventListener("DOMContentLoaded", () => {
    const css = document.createElement("style");
    css.textContent = ".xljs-tb-btn svg { width: 10px; height: 10px; fill: none; stroke: currentColor; stroke-width: 1; }";
    document.head.append(css);
    for (const b of document.querySelectorAll(".xljs-tb-btn")) b.innerHTML = glyphs[b.dataset.xljsWindow] || "";
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
  if (name === "launch.html" || name === "dev.html") {
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

await browser.close();
server.close();
