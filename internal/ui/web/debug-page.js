"use strict";
// The debugger window of xljs --debug: connects debug.js to Go (a
// synchronous request for each native call, WebView2 messages for events)
// and lets the user start the script once the breakpoints are set.
(() => {
  const wv = window.chrome && window.chrome.webview;
  const $ = (id) => document.getElementById(id);
  const transport = {
    call(text) {
      const x = new XMLHttpRequest();
      x.open("POST", "/_xljs/rpc", false);
      x.setRequestHeader("Content-Type", "application/json; charset=utf-8");
      x.send(text);
      if (x.status !== 200) throw new Error(`xljs: bridge error ${x.status} ${x.responseText}`);
      return x.responseText;
    },
    listen(fn) {
      // The WebView2 wrapper echoes the page's own messages; Go's start with \x02.
      wv.addEventListener("message", (e) => {
        if (typeof e.data === "string" && e.data[0] === "\x02") fn(e.data.slice(1));
      });
    },
  };

  // ?autorun (tests): run at once and close at the end.
  const autorun = new URLSearchParams(location.search).has("autorun");
  let session;
  // Labels, fetched while the bridge is up ({0} stays a placeholder).
  const L = {};
  const fmt = (s, ...args) => String(s).replace(/\{(\d+)\}/g, (m, i) => (args[i] === undefined ? m : args[i]));
  const setStatus = (text, cls) => {
    $("status").textContent = text;
    $("status").className = "status " + (cls || "");
  };
  const showError = (text) => {
    $("error").textContent = text;
    $("error").hidden = !text;
  };

  function status(state, detail) {
    switch (state) {
      case "running":
        setStatus(L.running);
        $("run").disabled = true;
        $("break-first").disabled = true;
        break;
      case "done":
        if (detail.error) {
          setStatus(fmt(L.failed, detail.code), "error");
          showError(detail.error);
        } else {
          setStatus(fmt(L.finished, detail.code), "ok");
        }
        $("run").textContent = L.close;
        $("run").disabled = false;
        $("run").onclick = () => wv.postMessage("close");
        $("run").focus();
        if (autorun) wv.postMessage("close");
        break;
    }
  }

  try {
    session = xljsDebug.start(transport, { status, console: window.console });
  } catch (e) {
    setStatus("xljs: " + e.message, "error");
    showError(String(e.stack || e));
    return;
  }
  const t = session.t;
  for (const k of ["running", "finished", "failed", "close"]) L[k] = t("debug." + k, "{0}");
  document.title = t("debug.windowTitle", xljs.title);
  $("name").textContent = xljs.script;
  $("hint").textContent = t("debug.hint");
  $("break-first-label").textContent = t("debug.breakFirst");
  $("run").textContent = t("debug.run");
  $("run").disabled = false;
  $("run").onclick = () => session.run($("break-first").checked);
  $("run").focus();
  setStatus(t("debug.ready"));
  if (autorun) setTimeout(() => $("run").click(), 0);

  window.addEventListener("error", (e) => session.fatal(e.error !== undefined ? e.error : e.message));
  window.addEventListener("unhandledrejection", (e) => session.rejected(e.promise, e.reason));
  window.addEventListener("rejectionhandled", (e) => session.handled(e.promise));
  document.addEventListener("keydown", (e) => {
    if (e.key === "F5" || (e.ctrlKey && (e.key === "r" || e.key === "R"))) {
      // No reload (see debug.reloaded); F5 runs.
      e.preventDefault();
      if (e.key === "F5" && !$("run").disabled) $("run").click();
    }
  });
})();
