// xljs page bridge: injected into every page shown in an xljs window.
// Defines window.xljs for talking to the script:
//   await xljs.invoke(name, ...args)   call a function the script handles
//   xljs.handle(name, fn)              let the script call fn (win.invoke)
//   xljs.emit(name, ...args)           send an event to the script (win.on)
//   xljs.on(name, fn) / xljs.off(name, fn)
//   xljs.files(fileList)               pass dropped/selected files (the
//                                      script receives their real paths)
//   xljs.close()                       close the window (window.close too)
// Messages from the host start with "\x02"; anything else is an echo of
// our own messages and is ignored.
(() => {
  "use strict";
  if (window.xljs || !window.chrome || !window.chrome.webview) return;
  const webview = window.chrome.webview;
  const HOST = "\x02";
  const FILES = Symbol("xljs.files");
  let seq = 0;
  const pending = new Map();
  const handlers = new Map();
  const listeners = new Map();

  function send(msg) {
    const files = [];
    const json = JSON.stringify(msg, function (key, value) {
      if (value && value[FILES]) {
        const list = value[FILES];
        const start = files.length;
        files.push(...list);
        return { $files: [start, list.length] };
      }
      return value;
    });
    if (files.length && typeof webview.postMessageWithAdditionalObjects === "function") {
      webview.postMessageWithAdditionalObjects(json, files);
    } else {
      webview.postMessage(json);
    }
  }

  function toError(v) {
    const e = new Error(v && v.message !== undefined ? v.message : String(v));
    if (v && v.name) e.name = v.name;
    return e;
  }

  webview.addEventListener("message", (ev) => {
    const data = ev.data;
    if (typeof data !== "string" || data[0] !== HOST) return;
    let m;
    try { m = JSON.parse(data.slice(1)); } catch (e) { return; }
    switch (m.type) {
      case "result":
      case "error": {
        const p = pending.get(m.id);
        if (!p) return;
        pending.delete(m.id);
        if (m.type === "result") p.resolve(m.value);
        else p.reject(toError(m.value));
        return;
      }
      case "invoke": {
        const fn = handlers.get(m.name) || (typeof window[m.name] === "function" ? window[m.name] : null);
        Promise.resolve()
          .then(() => {
            if (!fn) throw new Error("not a function: " + m.name);
            return fn(...(m.args || []));
          })
          .then(
            (value) => send({ type: "result", id: m.id, value: value === undefined ? null : value }),
            (e) => send({ type: "error", id: m.id, value: { name: e && e.name, message: e && e.message !== undefined ? e.message : String(e) } }),
          );
        return;
      }
      case "event": {
        for (const fn of [...(listeners.get(m.name) || [])]) {
          try { fn(...(m.args || [])); } catch (e) { console.error(e); }
        }
        return;
      }
    }
  });

  window.xljs = Object.freeze({
    invoke(name, ...args) {
      const id = ++seq;
      return new Promise((resolve, reject) => {
        pending.set(id, { resolve, reject });
        send({ type: "invoke", id, name: String(name), args });
      });
    },
    handle(name, fn) {
      if (fn) handlers.set(String(name), fn);
      else handlers.delete(String(name));
    },
    emit(name, ...args) { send({ type: "event", name: String(name), args }); },
    on(name, fn) {
      const k = String(name);
      if (!listeners.has(k)) listeners.set(k, new Set());
      listeners.get(k).add(fn);
    },
    off(name, fn) {
      const set = listeners.get(String(name));
      if (set) set.delete(fn);
    },
    files(list) { return { [FILES]: Array.from(list || []) }; },
    close() { send({ type: "event", name: "xljs:close", args: [] }); },
  });
  window.close = () => window.xljs.close();
})();
