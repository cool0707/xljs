// xljs prelude: builds the public JavaScript API on top of the Go natives.
// Evaluated once per runtime; the completion value is a function that
// receives the natives object. Must only use syntax goja supports natively
// (no async generators).
(function (native) {
  "use strict";
  const G = globalThis;
  const asyncIteratorSymbol = Symbol.asyncIterator || Symbol.for("Symbol.asyncIterator");
  const inspectSymbol = Symbol.for("xljs.inspect");

  /** Localized message for key (see internal/i18n/locales/*.json). */
  const T = (key, ...args) => native.t(key, ...args);

  const define = (obj, name, value) =>
    Object.defineProperty(obj, name, { value, writable: true, configurable: true, enumerable: false });
  const defineGlobal = (name, value) => define(G, name, value);

  // ---------------------------------------------------------------- util

  const pad2 = (n) => String(n).padStart(2, "0");
  function formatDateLocal(d, withMs) {
    if (isNaN(d.getTime())) return "Invalid Date";
    let s = `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())} ${pad2(d.getHours())}:${pad2(d.getMinutes())}:${pad2(d.getSeconds())}`;
    if (withMs && d.getMilliseconds()) s += "." + String(d.getMilliseconds()).padStart(3, "0");
    return s;
  }

  // Display width (East Asian wide characters count as 2).
  function strWidth(s) {
    let w = 0;
    for (const ch of String(s)) {
      const c = ch.codePointAt(0);
      if (c < 0x20) continue;
      w += (c >= 0x1100 && (c <= 0x115f || (c >= 0x2e80 && c <= 0xa4cf) || (c >= 0xac00 && c <= 0xd7a3) ||
        (c >= 0xf900 && c <= 0xfaff) || (c >= 0xfe30 && c <= 0xfe4f) || (c >= 0xff00 && c <= 0xff60) ||
        (c >= 0xffe0 && c <= 0xffe6) || (c >= 0x1f300 && c <= 0x1faff) || (c >= 0x20000 && c <= 0x3fffd))) ? 2 : 1;
    }
    return w;
  }

  // ---------------------------------------------------------------- inspect

  function inspect(value, opts) {
    const depthLimit = opts && opts.depth !== undefined ? opts.depth : 3;
    const seen = [];
    function fmt(v, depth, nested) {
      switch (typeof v) {
        case "string": return nested ? JSON.stringify(v) : v;
        case "number": return Object.is(v, -0) ? "-0" : String(v);
        case "bigint": return v + "n";
        case "boolean": case "undefined": return String(v);
        case "symbol": return v.toString();
        case "function": {
          if (/^class\s/.test(Function.prototype.toString.call(v))) return `[class ${v.name || "(anonymous)"}]`;
          return `[Function: ${v.name || "(anonymous)"}]`;
        }
      }
      if (v === null) return "null";
      if (typeof v[inspectSymbol] === "function") return v[inspectSymbol]();
      if (v instanceof Date) return formatDateLocal(v, true);
      if (v instanceof Error) return v.stack || `${v.name}: ${v.message}`;
      if (v instanceof RegExp) return String(v);
      if (seen.includes(v)) return "[Circular]";
      if (depth > depthLimit) return Array.isArray(v) ? "[Array]" : "[Object]";
      seen.push(v);
      try {
        let parts, open, close, prefix = "";
        if (Array.isArray(v) || ArrayBuffer.isView(v)) {
          const max = 100;
          parts = [];
          const len = v.length;
          for (let i = 0; i < Math.min(len, max); i++) parts.push(fmt(v[i], depth + 1, true));
          if (len > max) parts.push(`... ${len - max} more items`);
          open = "["; close = "]";
          if (ArrayBuffer.isView(v)) prefix = `${v.constructor.name}(${len}) `;
        } else if (v instanceof Map) {
          parts = [...v].map(([k, val]) => `${fmt(k, depth + 1, true)} => ${fmt(val, depth + 1, true)}`);
          open = "{"; close = "}"; prefix = `Map(${v.size}) `;
        } else if (v instanceof Set) {
          parts = [...v].map((x) => fmt(x, depth + 1, true));
          open = "{"; close = "}"; prefix = `Set(${v.size}) `;
        } else if (v instanceof ArrayBuffer) {
          return `ArrayBuffer { byteLength: ${v.byteLength} }`;
        } else if (v instanceof Promise) {
          return "Promise { ... }";
        } else {
          const keys = Object.keys(v);
          parts = keys.map((k) => {
            const key = /^[A-Za-z_$][\w$]*$/.test(k) || /^[^\x00-\x7f]+$/.test(k) ? k : JSON.stringify(k);
            return `${key}: ${fmt(v[k], depth + 1, true)}`;
          });
          const ctor = Object.getPrototypeOf(v) && Object.getPrototypeOf(v).constructor;
          if (ctor && ctor !== Object && ctor.name) prefix = ctor.name + " ";
          open = "{"; close = "}";
        }
        if (parts.length === 0) return prefix + open + close;
        const single = `${prefix}${open} ${parts.join(", ")} ${close}`;
        if (single.length <= 76 && !single.includes("\n")) return single;
        const ind = "  ".repeat(depth + 1);
        return `${prefix}${open}\n${ind}${parts.join(`,\n${ind}`)}\n${"  ".repeat(depth)}${close}`;
      } finally {
        seen.pop();
      }
    }
    return fmt(value, 0, false);
  }

  function formatArgs(args) {
    if (args.length === 0) return "";
    let first = args[0];
    let rest = Array.prototype.slice.call(args, 1);
    if (typeof first === "string" && /%[sdifjoO%]/.test(first)) {
      first = first.replace(/%([sdifjoO%])/g, (m, c) => {
        if (c === "%") return "%";
        if (rest.length === 0) return m;
        const a = rest.shift();
        switch (c) {
          case "s": return typeof a === "string" ? a : inspect(a);
          case "d": case "i": return String(c === "i" ? Math.trunc(Number(a)) : Number(a));
          case "f": return String(Number(a));
          case "j": try { return JSON.stringify(a); } catch (e) { return "[Circular]"; }
          default: return inspect(a);
        }
      });
    } else {
      first = inspect(first);
    }
    return [first].concat(rest.map((a) => inspect(a))).join(" ");
  }

  // ---------------------------------------------------------------- console

  const timers = new Map();
  const counts = new Map();
  let groupIndent = "";
  const out = (s) => native.stdout(groupIndent ? s.replace(/^/gm, groupIndent) + "\n" : s + "\n");
  const err = (s) => native.stderr(groupIndent ? s.replace(/^/gm, groupIndent) + "\n" : s + "\n");

  function table(data, columns) { out(tableText(data, columns)); }
  /** console.table's layout as a string. */
  function tableText(data, columns) {
    if (data === null || typeof data !== "object") return inspect(data);
    const rows = Array.isArray(data) ? data.map((r, i) => [i, r]) : Object.entries(data);
    const cols = [];
    let hasValues = false;
    for (const [, r] of rows) {
      if (r !== null && typeof r === "object" && !(r instanceof Date)) {
        for (const k of Object.keys(r)) if (!cols.includes(k)) cols.push(k);
      } else hasValues = true;
    }
    const useCols = columns || cols;
    const header = ["(index)", ...useCols, ...(hasValues ? ["Values"] : [])];
    const cell = (v) => (v === undefined ? "" : typeof v === "string" ? v : inspect(v, { depth: 0 }));
    const body = rows.map(([k, r]) => {
      const isObj = r !== null && typeof r === "object" && !(r instanceof Date);
      return [String(k), ...useCols.map((c) => (isObj ? cell(r[c]) : "")), ...(hasValues ? [isObj ? "" : cell(r)] : [])];
    });
    const widths = header.map((h, i) => Math.max(strWidth(h), ...body.map((r) => strWidth(r[i]))));
    const line = (l, m, r) => l + widths.map((w) => "─".repeat(w + 2)).join(m) + r;
    const fmtRow = (r) => "│" + r.map((c, i) => " " + c + " ".repeat(widths[i] - strWidth(c)) + " ").join("│") + "│";
    return [line("┌", "┬", "┐"), fmtRow(header), line("├", "┼", "┤"), ...body.map(fmtRow), line("└", "┴", "┘")].join("\n");
  }

  const consoleObj = {
    log: (...a) => out(formatArgs(a)),
    info: (...a) => out(formatArgs(a)),
    debug: (...a) => out(formatArgs(a)),
    warn: (...a) => err(formatArgs(a)),
    error: (...a) => err(formatArgs(a)),
    trace: (...a) => err("Trace: " + formatArgs(a) + "\n" + (new Error().stack || "").split("\n").slice(2).join("\n")),
    dir: (v, o) => out(inspect(v, o)),
    table,
    assert: (cond, ...a) => { if (!cond) err("Assertion failed" + (a.length ? ": " + formatArgs(a) : "")); },
    time: (label = "default") => { timers.set(label, Date.now()); },
    timeLog: (label = "default", ...a) => out(`${label}: ${Date.now() - (timers.get(label) || Date.now())}ms ${formatArgs(a)}`.trimEnd()),
    timeEnd: (label = "default") => { out(`${label}: ${Date.now() - (timers.get(label) || Date.now())}ms`); timers.delete(label); },
    count: (label = "default") => { const c = (counts.get(label) || 0) + 1; counts.set(label, c); out(`${label}: ${c}`); },
    countReset: (label = "default") => { counts.delete(label); },
    group: (...a) => { if (a.length) out(formatArgs(a)); groupIndent += "  "; },
    groupEnd: () => { groupIndent = groupIndent.slice(2); },
  };
  consoleObj.groupCollapsed = consoleObj.group;
  defineGlobal("console", consoleObj);

  // ---------------------------------------------------------------- polyfills

  if (!Array.fromAsync) {
    define(Array, "fromAsync", async function fromAsync(items, mapFn, thisArg) {
      const result = [];
      let i = 0;
      if (items != null && typeof items[asyncIteratorSymbol] === "function") {
        for await (const v of items) result.push(mapFn ? await mapFn.call(thisArg, v, i++) : v);
      } else {
        for (const v of (items != null && typeof items[Symbol.iterator] === "function" ? items : Array.from(items))) {
          const x = await v;
          result.push(mapFn ? await mapFn.call(thisArg, x, i++) : x);
        }
      }
      return result;
    });
  }

  if (typeof G.structuredClone !== "function") {
    defineGlobal("structuredClone", function structuredClone(value) {
      const memo = new Map();
      function clone(v) {
        if (v === null || typeof v !== "object") {
          if (typeof v === "function" || typeof v === "symbol") throw new TypeError(T("data.structuredClone"));
          return v;
        }
        if (memo.has(v)) return memo.get(v);
        let r;
        if (v instanceof Date) r = new Date(v.getTime());
        else if (v instanceof RegExp) r = new RegExp(v.source, v.flags);
        else if (v instanceof Map) { r = new Map(); memo.set(v, r); for (const [k, x] of v) r.set(clone(k), clone(x)); return r; }
        else if (v instanceof Set) { r = new Set(); memo.set(v, r); for (const x of v) r.add(clone(x)); return r; }
        else if (v instanceof ArrayBuffer) r = v.slice(0);
        else if (ArrayBuffer.isView(v)) r = new v.constructor(v);
        else if (Array.isArray(v)) { r = []; memo.set(v, r); v.forEach((x, i) => { r[i] = clone(x); }); return r; }
        else { r = {}; memo.set(v, r); for (const k of Object.keys(v)) r[k] = clone(v[k]); return r; }
        memo.set(v, r);
        return r;
      }
      return clone(value);
    });
  }

  if (typeof G.queueMicrotask !== "function") {
    defineGlobal("queueMicrotask", (fn) => { Promise.resolve().then(fn); });
  }

  const B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  function bytesToBase64(bytes) {
    let s = "";
    for (let i = 0; i < bytes.length; i += 3) {
      const a = bytes[i], b = bytes[i + 1], c = bytes[i + 2];
      s += B64[a >> 2] + B64[((a & 3) << 4) | ((b || 0) >> 4)] +
        (i + 1 < bytes.length ? B64[((b & 15) << 2) | ((c || 0) >> 6)] : "=") +
        (i + 2 < bytes.length ? B64[c & 63] : "=");
    }
    return s;
  }
  function base64ToBytes(s) {
    s = String(s).replace(/[\s=]/g, "");
    const out = new Uint8Array(Math.floor((s.length * 3) / 4));
    let j = 0;
    for (let i = 0; i < s.length; i += 4) {
      const n = (B64.indexOf(s[i]) << 18) | (B64.indexOf(s[i + 1]) << 12) |
        ((B64.indexOf(s[i + 2]) & 63) << 6) | (B64.indexOf(s[i + 3]) & 63);
      out[j++] = (n >> 16) & 255;
      if (i + 2 < s.length) out[j++] = (n >> 8) & 255;
      if (i + 3 < s.length) out[j++] = n & 255;
    }
    return out.slice(0, j);
  }
  if (typeof G.btoa !== "function") {
    defineGlobal("btoa", (s) => {
      s = String(s);
      const bytes = new Uint8Array(s.length);
      for (let i = 0; i < s.length; i++) {
        const c = s.charCodeAt(i);
        if (c > 255) throw new Error(T("data.btoa"));
        bytes[i] = c;
      }
      return bytesToBase64(bytes);
    });
    defineGlobal("atob", (s) => String.fromCharCode(...base64ToBytes(s)));
  }

  class TextEncoder {
    get encoding() { return "utf-8"; }
    encode(s = "") { return new Uint8Array(native.encode(String(s), "utf8")); }
  }
  class TextDecoder {
    constructor(label = "utf-8", options = {}) {
      this._enc = native.normEncoding(label);
      this._label = String(label).toLowerCase();
      this.fatal = !!options.fatal;
      this.ignoreBOM = !!options.ignoreBOM;
    }
    get encoding() { return this._label; }
    decode(input) {
      if (input === undefined) return "";
      return native.decode(input, this._enc === "utf8bom" ? "utf8" : this._enc);
    }
  }
  if (typeof G.TextEncoder !== "function") defineGlobal("TextEncoder", TextEncoder);
  if (typeof G.TextDecoder !== "function") defineGlobal("TextDecoder", TextDecoder);

  // Locale formatting (goja has no Intl). Japanese conventions.
  function groupDigits(intStr) { return intStr.replace(/\B(?=(\d{3})+(?!\d))/g, ","); }
  function formatNumber(n, o = {}) {
    if (!isFinite(n)) return String(n);
    const minF = o.minimumFractionDigits !== undefined ? o.minimumFractionDigits : 0;
    const maxF = Math.max(minF, o.maximumFractionDigits !== undefined ? o.maximumFractionDigits : (o.style === "percent" ? 0 : 3));
    let v = o.style === "percent" ? n * 100 : n;
    let s = Math.abs(v).toFixed(maxF);
    if (maxF > minF) s = s.replace(/0+$/, (z) => z.slice(0, Math.max(0, minF - (s.split(".")[1] || "").replace(/0+$/, "").length))).replace(/\.$/, "");
    let [i, f] = s.split(".");
    if (f !== undefined && f.length < minF) f = f.padEnd(minF, "0");
    if (f === undefined && minF > 0) f = "0".repeat(minF);
    let r = (o.useGrouping === false ? i : groupDigits(i)) + (f ? "." + f : "");
    if (v < 0 && Number(r.replace(/,/g, "")) !== 0) r = "-" + r;
    if (o.style === "percent") r += "%";
    if (o.style === "currency") r = (o.currency === "JPY" || !o.currency ? "￥" : o.currency + " ") + r;
    return r;
  }
  define(Number.prototype, "toLocaleString", function (locales, options) { return formatNumber(Number(this), options || {}); });
  define(Date.prototype, "toLocaleDateString", function () {
    return isNaN(this) ? "Invalid Date" : `${this.getFullYear()}/${this.getMonth() + 1}/${this.getDate()}`;
  });
  define(Date.prototype, "toLocaleTimeString", function () {
    return isNaN(this) ? "Invalid Date" : `${this.getHours()}:${pad2(this.getMinutes())}:${pad2(this.getSeconds())}`;
  });
  define(Date.prototype, "toLocaleString", function () {
    return isNaN(this) ? "Invalid Date" : `${this.toLocaleDateString()} ${this.toLocaleTimeString()}`;
  });
  if (typeof G.Intl !== "object") {
    class NumberFormat {
      constructor(locales, options) {
        this._o = options || {};
        this.format = (n) => formatNumber(Number(n), this._o);
      }
      resolvedOptions() { return Object.assign({ locale: "ja-JP" }, this._o); }
    }
    class DateTimeFormat {
      constructor(locales, options) {
        this._o = options || {};
        this.format = (d) => this._format(d);
      }
      _format(d) {
        d = d === undefined ? new Date() : new Date(d);
        const o = this._o;
        if (o.timeStyle && !o.dateStyle) return d.toLocaleTimeString();
        if (o.hour !== undefined || o.timeStyle) return d.toLocaleString();
        return d.toLocaleDateString();
      }
      resolvedOptions() { return Object.assign({ locale: "ja-JP", timeZone: "local" }, this._o); }
    }
    class Collator {
      constructor(locales, options) {
        this._o = options || {};
        this.compare = (a, b) => this._compare(a, b);
      }
      _compare(a, b) {
        if (this._o.numeric) {
          const re = /(\d+)|(\D+)/g;
          const pa = String(a).match(re) || [], pb = String(b).match(re) || [];
          for (let i = 0; i < Math.min(pa.length, pb.length); i++) {
            const x = pa[i], y = pb[i];
            if (/^\d/.test(x) && /^\d/.test(y)) { const d = Number(x) - Number(y); if (d) return d < 0 ? -1 : 1; }
            else if (x !== y) return x < y ? -1 : 1;
          }
          return pa.length - pb.length;
        }
        return String(a).localeCompare(String(b));
      }
    }
    defineGlobal("Intl", { NumberFormat, DateTimeFormat, Collator });
  }

  defineGlobal("sleep", (ms) => new Promise((resolve) => setTimeout(resolve, ms)));

  // ---------------------------------------------------------------- errors

  class DOMException extends Error {
    constructor(message = "", name = "Error") {
      super(message);
      Object.defineProperty(this, "name", { value: name, configurable: true, writable: true });
    }
  }
  if (typeof G.DOMException !== "function") defineGlobal("DOMException", DOMException);
  const notFound = (p) => new DOMException(T("fs.notFound", p), "NotFoundError");
  const typeMismatch = (p) => new DOMException(T("fs.typeMismatch", p), "TypeMismatchError");

  // ---------------------------------------------------------------- Blob / File

  function toUint8(part) {
    if (part instanceof ArrayBuffer) return new Uint8Array(part);
    if (ArrayBuffer.isView(part)) return new Uint8Array(part.buffer, part.byteOffset, part.byteLength);
    if (part instanceof Blob) return new Uint8Array(part._bytes());
    return new Uint8Array(native.encode(String(part), "utf8"));
  }
  function concatBytes(parts) {
    const arrays = parts.map(toUint8);
    const total = arrays.reduce((s, a) => s + a.length, 0);
    const outBuf = new Uint8Array(total);
    let off = 0;
    for (const a of arrays) { outBuf.set(a, off); off += a.length; }
    return outBuf.buffer;
  }

  class Blob {
    constructor(parts = [], options = {}) {
      this._buf = concatBytes(Array.from(parts));
      this._type = options.type ? String(options.type).toLowerCase() : "";
    }
    _bytes() { return this._buf; }
    get size() { return this._bytes().byteLength; }
    get type() { return this._type; }
    async arrayBuffer() { return this._bytes().slice(0); }
    async bytes() { return new Uint8Array(this._bytes().slice(0)); }
    /** text(encoding = "utf8"): xljs extension, accepts any supported encoding or "auto". */
    async text(encoding) { return native.decode(this._bytes(), encoding || "utf8"); }
    slice(start = 0, end = undefined, type = "") {
      const b = new Blob([], { type });
      b._buf = this._bytes().slice(start, end);
      return b;
    }
    [inspectSymbol]() { return `Blob { size: ${this.size}, type: ${JSON.stringify(this.type)} }`; }
  }

  class File extends Blob {
    constructor(parts, name, options = {}) {
      super(parts || [], options);
      this._name = String(name);
      this._lastModified = options.lastModified !== undefined ? Number(options.lastModified) : Date.now();
      this._path = null;
    }
    static _fromPath(path, st) {
      const f = new File([], native.pathBasename(path), { lastModified: st.mtimeMs, type: mimeOf(path) });
      f._path = path;
      f._size = st.size;
      f._buf = null;
      return f;
    }
    _bytes() {
      if (this._buf === null) this._buf = native.fsReadBytes(this._path);
      return this._buf;
    }
    get size() { return this._buf === null ? this._size : this._buf.byteLength; }
    get name() { return this._name; }
    get lastModified() { return this._lastModified; }
    get lastModifiedDate() { return new Date(this._lastModified); }
    /** Full path of the file on disk (xljs extension; null for in-memory files). */
    get path() { return this._path; }
    async text(encoding) {
      if (this._buf === null) return native.fsReadText(this._path, encoding || "utf8");
      return super.text(encoding);
    }
    [inspectSymbol]() { return `File { name: ${JSON.stringify(this.name)}, size: ${this.size}${this._path ? ", path: " + JSON.stringify(this._path) : ""} }`; }
  }
  if (typeof G.Blob !== "function") defineGlobal("Blob", Blob);
  if (typeof G.File !== "function") defineGlobal("File", File);

  const MIME = {
    ".xlsx": "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    ".xlsm": "application/vnd.ms-excel.sheet.macroEnabled.12",
    ".xls": "application/vnd.ms-excel",
    ".csv": "text/csv", ".tsv": "text/tab-separated-values", ".txt": "text/plain",
    ".json": "application/json", ".html": "text/html", ".xml": "application/xml",
    ".pdf": "application/pdf", ".png": "image/png", ".jpg": "image/jpeg", ".jpeg": "image/jpeg",
  };
  const mimeOf = (p) => MIME[native.pathExtname(p).toLowerCase()] || "";

  // ---------------------------------------------------------------- handles

  /** Converts a path-like value (string, handle, File with path) to a path string. */
  function toPath(x, what = "path") {
    if (typeof x === "string") return x;
    if (x instanceof FileSystemHandle) return x.path;
    if (x instanceof File && x.path) return x.path;
    if (x && typeof x.path === "string") return x.path;
    throw new TypeError(T("fs.expectedPath", what));
  }

  const sameName = native.platform === "windows"
    ? (a, b) => a.toLowerCase() === b.toLowerCase()
    : (a, b) => a === b;

  class FileSystemHandle {
    constructor(kind, path, relativePath) {
      define(this, "_kind", kind);
      define(this, "_path", native.pathResolve(path));
      define(this, "_rel", relativePath === undefined ? null : relativePath);
    }
    get kind() { return this._kind; }
    get name() { return native.pathBasename(this._path); }
    /** Absolute path (xljs extension). */
    get path() { return this._path; }
    /** Path relative to the directory a walk() started from (xljs extension). */
    get relativePath() { return this._rel === null ? this.name : this._rel; }
    async isSameEntry(other) { return other instanceof FileSystemHandle && other.kind === this.kind && sameName(other.path, this.path); }
    async queryPermission() { return "granted"; }
    async requestPermission() { return "granted"; }
    /** Whether the entry currently exists (xljs extension). */
    exists() {
      const st = native.fsStat(this._path);
      return !!st && (this._kind === "file" ? st.isFile : st.isDirectory);
    }
    toString() { return this._path; }
    toJSON() { return this._path; }
    [inspectSymbol]() { return `${this.constructor.name} { ${JSON.stringify(this._path)} }`; }
  }

  class FileSystemWritableFileStream {
    constructor(path, keep) {
      this._path = path;
      this._chunks = [];
      this._pos = 0;
      this._data = keep && native.fsStat(path) ? new Uint8Array(native.fsReadBytes(path)) : new Uint8Array(0);
      this._closed = false;
    }
    _ensure(size) {
      if (this._data.length >= size) return;
      const n = new Uint8Array(Math.max(size, this._data.length * 2));
      n.set(this._data);
      this._data = n;
      this._len = this._len || 0;
    }
    async write(data) {
      if (this._closed) throw new TypeError(T("fs.streamClosed"));
      if (data && typeof data === "object" && !(data instanceof Blob) && !(data instanceof ArrayBuffer) && !ArrayBuffer.isView(data) && data.type) {
        if (data.type === "seek") { this._pos = data.position; return; }
        if (data.type === "truncate") { this._truncate(data.size); return; }
        if (data.position !== undefined) this._pos = data.position;
        data = data.data;
      }
      const bytes = toUint8(data);
      const len = this._length();
      const end = this._pos + bytes.length;
      this._ensure(end);
      this._data.set(bytes, this._pos);
      this._pos = end;
      this._len = Math.max(len, end);
    }
    _length() { return this._len === undefined ? this._data.length : this._len; }
    _truncate(size) { this._ensure(size); this._len = size; if (this._pos > size) this._pos = size; }
    async seek(position) { this._pos = position; }
    async truncate(size) { this._truncate(size); }
    async close() {
      if (this._closed) return;
      this._closed = true;
      native.fsWriteBytes(this._path, this._data.subarray(0, this._length()), false);
    }
    async abort() { this._closed = true; }
  }

  class FileSystemFileHandle extends FileSystemHandle {
    constructor(path, relativePath) { super("file", path, relativePath); }
    async getFile() {
      const st = native.fsStat(this._path);
      if (!st) throw notFound(this._path);
      if (!st.isFile) throw typeMismatch(this._path);
      return File._fromPath(this._path, st);
    }
    async createWritable(options = {}) {
      return new FileSystemWritableFileStream(this._path, !!options.keepExistingData);
    }
    // xljs extensions (synchronous)
    stat() { return native.fsStat(this._path); }
    readText(encoding = "auto") { return native.fsReadText(this._path, encoding); }
    readBytes() { return native.fsReadBytes(this._path); }
    writeText(text, encoding = "utf8") { native.fsWriteText(this._path, String(text), encoding, false); return this; }
    appendText(text, encoding = "utf8") { native.fsWriteText(this._path, String(text), encoding, true); return this; }
    writeBytes(data) { native.fsWriteBytes(this._path, toUint8(data), false); return this; }
  }

  function makeAsyncIterable(items, map) {
    let i = 0;
    const it = {
      next() {
        if (i >= items.length) return Promise.resolve({ done: true, value: undefined });
        return Promise.resolve({ done: false, value: map(items[i++]) });
      },
      return(value) { i = items.length; return Promise.resolve({ done: true, value }); },
    };
    it[asyncIteratorSymbol] = () => it;
    return it;
  }

  /** Result of walk(): iterable with for..of and for await..of. */
  class WalkResult {
    constructor(entries) { define(this, "_entries", entries); }
    [Symbol.iterator]() { return this._entries[Symbol.iterator](); }
    [asyncIteratorSymbol]() { return makeAsyncIterable(this._entries, (x) => x); }
    get length() { return this._entries.length; }
    toArray() { return this._entries.slice(); }
    map(fn) { return this._entries.map(fn); }
    filter(fn) { return this._entries.filter(fn); }
    forEach(fn) { this._entries.forEach(fn); }
    [inspectSymbol]() { return `WalkResult(${this._entries.length}) ${inspect(this._entries)}`; }
  }

  function walkEntries(root, opts = {}) {
    if (typeof opts === "string" || Array.isArray(opts)) opts = { glob: opts };
    const include = opts.glob !== undefined ? opts.glob : opts.include;
    const list = native.fsWalk(root, include === undefined ? [] : [].concat(include),
      opts.exclude === undefined ? [] : [].concat(opts.exclude),
      !!opts.directories, opts.files === false);
    return new WalkResult(list.map((e) => e.isDirectory
      ? new FileSystemDirectoryHandle(e.path, e.rel)
      : new FileSystemFileHandle(e.path, e.rel)));
  }

  class FileSystemDirectoryHandle extends FileSystemHandle {
    constructor(path, relativePath) { super("directory", path, relativePath); }
    _child(name) {
      if (/[\\/]/.test(name) || name === "." || name === "..") throw new TypeError(T("fs.invalidName", name));
      return native.pathJoin(this._path, name);
    }
    async getFileHandle(name, options = {}) {
      const p = this._child(name);
      const st = native.fsStat(p);
      if (!st) {
        if (!options.create) throw notFound(p);
        native.fsWriteBytes(p, new Uint8Array(0), false);
      } else if (!st.isFile) throw typeMismatch(p);
      return new FileSystemFileHandle(p);
    }
    async getDirectoryHandle(name, options = {}) {
      const p = this._child(name);
      const st = native.fsStat(p);
      if (!st) {
        if (!options.create) throw notFound(p);
        native.fsMkdir(p);
      } else if (!st.isDirectory) throw typeMismatch(p);
      return new FileSystemDirectoryHandle(p);
    }
    async removeEntry(name, options = {}) {
      const p = this._child(name);
      if (!native.fsStat(p)) throw notFound(p);
      native.fsRemove(p, !!options.recursive);
    }
    async resolve(handle) {
      const rel = native.pathRelative(this._path, handle.path);
      if (rel === ".." || rel.startsWith(".." + native.pathSep) || native.pathIsAbsolute(rel)) return null;
      return rel === "." ? [] : rel.split(native.pathSep);
    }
    _list() {
      return native.fsReadDir(this._path).map((e) => e.isDirectory
        ? new FileSystemDirectoryHandle(native.pathJoin(this._path, e.name))
        : new FileSystemFileHandle(native.pathJoin(this._path, e.name)));
    }
    entries() { return makeAsyncIterable(this._list(), (h) => [h.name, h]); }
    keys() { return makeAsyncIterable(this._list(), (h) => h.name); }
    values() { return makeAsyncIterable(this._list(), (h) => h); }
    [asyncIteratorSymbol]() { return this.entries(); }
    // xljs extensions
    /** Recursively lists entries. opts: { glob, exclude, directories, files } */
    walk(opts) { return walkEntries(this._path, opts); }
    /** Lists the direct children synchronously. */
    list() { return this._list(); }
    /** Handle for a file in this directory (no existence check). */
    file(name) { return new FileSystemFileHandle(native.pathJoin(this._path, name)); }
    /** Handle for a sub directory (no existence check). */
    dir(name) { return new FileSystemDirectoryHandle(native.pathJoin(this._path, name)); }
  }

  defineGlobal("FileSystemHandle", FileSystemHandle);
  defineGlobal("FileSystemFileHandle", FileSystemFileHandle);
  defineGlobal("FileSystemDirectoryHandle", FileSystemDirectoryHandle);
  defineGlobal("FileSystemWritableFileStream", FileSystemWritableFileStream);

  // ---------------------------------------------------------------- fs / path

  // ---------------------------------------------------------------- json helpers

  /** Reads text from a path, handle, File or Blob. */
  function readSourceText(src, encoding) {
    if (src instanceof Blob && !(src instanceof File && src.path)) return native.decode(src._bytes(), encoding || "auto");
    return native.fsReadText(toPath(src), encoding || "auto");
  }
  function localISO(d) {
    if (isNaN(d.getTime())) return null;
    const off = -d.getTimezoneOffset();
    const sign = off >= 0 ? "+" : "-";
    const ms = d.getMilliseconds() ? "." + String(d.getMilliseconds()).padStart(3, "0") : "";
    return `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())}T${pad2(d.getHours())}:${pad2(d.getMinutes())}:${pad2(d.getSeconds())}${ms}` +
      `${sign}${pad2(Math.floor(Math.abs(off) / 60))}:${pad2(Math.abs(off) % 60)}`;
  }
  const ISO_DATETIME = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}(:\d{2}(\.\d+)?)?(Z|[+-]\d{2}:?\d{2})?$/;
  function jsonParse(text, opts = {}) {
    const reviver = opts.dates
      ? (k, v) => (typeof v === "string" && ISO_DATETIME.test(v) ? new Date(v) : v)
      : undefined;
    return JSON.parse(text.replace(/^\uFEFF/, ""), reviver);
  }
  function jsonParseFile(src, text, opts, line) {
    try {
      return jsonParse(text, opts);
    } catch (e) {
      const where = (src instanceof Blob && !(src instanceof File && src.path) ? "Blob" : toPath(src)) + (line ? `:${line}` : "");
      throw new SyntaxError(T("json.parseError", where, e.message));
    }
  }
  /** JSON.stringify where Dates use local time with offset by default (dates: "iso" for UTC). */
  function jsonStringify(value, opts = {}, space) {
    const local = opts.dates !== "iso";
    return JSON.stringify(value, local ? function (k, v) {
      const raw = this[k];
      return raw instanceof Date ? localISO(raw) : v;
    } : null, space);
  }

  const fsObj = {
    file: (p) => new FileSystemFileHandle(toPath(p)),
    dir: (p) => new FileSystemDirectoryHandle(toPath(p)),
    /** Async variants mirroring the picker style. */
    getFileHandle: async (p) => {
      const h = new FileSystemFileHandle(toPath(p));
      if (!h.exists()) throw notFound(h.path);
      return h;
    },
    getDirectoryHandle: async (p) => {
      const h = new FileSystemDirectoryHandle(toPath(p));
      if (!h.exists()) throw notFound(h.path);
      return h;
    },
    exists: (p) => !!native.fsStat(toPath(p)),
    stat: (p) => native.fsStat(toPath(p)),
    isFile: (p) => { const s = native.fsStat(toPath(p)); return !!s && s.isFile; },
    isDirectory: (p) => { const s = native.fsStat(toPath(p)); return !!s && s.isDirectory; },
    readText: (p, encoding = "auto") => native.fsReadText(toPath(p), encoding),
    writeText: (p, text, encoding = "utf8") => native.fsWriteText(toPath(p), String(text), encoding, false),
    appendText: (p, text, encoding = "utf8") => native.fsWriteText(toPath(p), String(text), encoding, true),
    readBytes: (p) => native.fsReadBytes(toPath(p)),
    writeBytes: (p, data) => native.fsWriteBytes(toPath(p), toUint8(data), false),
    /** Reads a JSON file (path, handle, File or Blob). opts: { encoding = "auto", dates = false } */
    readJSON: (src, opts = {}) => jsonParseFile(src, readSourceText(src, opts.encoding), opts),
    /** Writes value as JSON. opts: { space = 2, encoding = "utf8", dates = "local" | "iso", append } (a number means space) */
    writeJSON: (dest, value, opts = {}) => {
      if (typeof opts === "number" || typeof opts === "string") opts = { space: opts };
      native.fsWriteText(toPath(dest), jsonStringify(value, opts, opts.space === undefined ? 2 : opts.space) + "\n", opts.encoding || "utf8", false);
    },
    /** Reads JSON Lines (one JSON value per line). */
    readJSONL: (src, opts = {}) => {
      const out = [];
      readSourceText(src, opts.encoding).split(/\r?\n/).forEach((l, i) => {
        if (l.trim() !== "") out.push(jsonParseFile(src, l, opts, i + 1));
      });
      return out;
    },
    /** Writes an array as JSON Lines. opts: { encoding = "utf8", dates, append } */
    writeJSONL: (dest, rows, opts = {}) => {
      const text = Array.from(rows || []).map((r) => jsonStringify(r, opts)).join("\n") + "\n";
      native.fsWriteText(toPath(dest), text, opts.encoding || "utf8", !!opts.append);
    },
    list: (p = ".") => native.fsReadDir(toPath(p)).map((e) => e.name),
    walk: (p, opts) => walkEntries(toPath(p), opts),
    glob: (pattern, opts = {}) => walkEntries(opts.cwd ? toPath(opts.cwd) : native.fsCwd(), { glob: pattern, exclude: opts.exclude }).toArray(),
    mkdir: (p) => native.fsMkdir(toPath(p)),
    remove: (p, opts = {}) => native.fsRemove(toPath(p), !!opts.recursive),
    rename: (from, to) => native.fsRename(toPath(from), toPath(to)),
    move: (from, to) => native.fsRename(toPath(from), toPath(to)),
    copy: (from, to) => native.fsCopy(toPath(from), toPath(to)),
    cwd: () => native.fsCwd(),
    chdir: (p) => native.fsChdir(toPath(p)),
  };
  defineGlobal("fs", fsObj);

  const pathObj = {
    sep: native.pathSep,
    join: (...a) => native.pathJoin(...a.map((x) => toPath(x))),
    resolve: (...a) => native.pathResolve(...a.map((x) => toPath(x))),
    dirname: (p) => native.pathDirname(toPath(p)),
    basename: (p, ext) => native.pathBasename(toPath(p), ext || ""),
    extname: (p) => native.pathExtname(toPath(p)),
    normalize: (p) => native.pathNormalize(toPath(p)),
    isAbsolute: (p) => native.pathIsAbsolute(toPath(p)),
    relative: (from, to) => native.pathRelative(toPath(from), toPath(to)),
  };
  defineGlobal("path", pathObj);

  // ---------------------------------------------------------------- xljs

  const xljsObj = {
    version: native.version,
    args: native.args,
    script: native.script,
    scriptDir: native.pathDirname(native.script),
    platform: native.platform,
    ui: native.uiName,
    title: native.title,
    exit: (code = 0) => native.exit(code),
    env: (name) => native.getenv(String(name)),
    inspect,
  };
  defineGlobal("xljs", xljsObj);
  defineGlobal("exit", xljsObj.exit);

  // ---------------------------------------------------------------- dialogs & pickers

  const title = () => String(xljsObj.title || "xljs");
  const msg = (m) => (m === undefined ? "" : typeof m === "string" ? m : inspect(m));
  defineGlobal("alert", (message) => native.uiAlert(title(), msg(message)));
  defineGlobal("confirm", (message) => native.uiConfirm(title(), msg(message)));
  defineGlobal("prompt", (message, defaultValue) =>
    native.uiPrompt(title(), msg(message), defaultValue === undefined || defaultValue === null ? "" : String(defaultValue)));

  function toFilters(types, excludeAll) {
    const filters = [];
    for (const t of types || []) {
      let exts = [];
      const acc = t.accept;
      if (typeof acc === "string") exts = [acc];
      else if (Array.isArray(acc)) exts = acc;
      else if (acc && typeof acc === "object") for (const k of Object.keys(acc)) exts = exts.concat(acc[k]);
      const patterns = exts.map((e) => (e.startsWith("*") ? e : e.startsWith(".") ? "*" + e : "*." + e));
      if (patterns.length) filters.push({ description: t.description || "", patterns });
    }
    if (filters.length && !excludeAll) filters.push({ description: T("ui.allFiles"), patterns: ["*.*"] });
    return filters;
  }
  function startInPath(startIn) {
    if (!startIn) return "";
    if (startIn instanceof FileSystemFileHandle) return native.pathDirname(startIn.path);
    if (startIn instanceof FileSystemDirectoryHandle) return startIn.path;
    const s = String(startIn);
    return native.knownFolder(s) || s;
  }
  function pickerOptions(o = {}) {
    return {
      title: o.title || "",
      multiple: !!o.multiple,
      startIn: startInPath(o.startIn),
      suggestedName: o.suggestedName || "",
      filters: toFilters(o.types, o.excludeAcceptAllOption),
    };
  }
  /** Returns [] when cancelled. */
  async function showOpenFilePicker(options = {}) {
    return native.uiOpen(pickerOptions(options)).map((p) => new FileSystemFileHandle(p));
  }
  /** Returns null when cancelled. */
  async function showSaveFilePicker(options = {}) {
    const p = native.uiSave(pickerOptions(options));
    return p === null ? null : new FileSystemFileHandle(p);
  }
  /** Returns null when cancelled. */
  async function showDirectoryPicker(options = {}) {
    const p = native.uiDirectory(pickerOptions(options));
    return p === null ? null : new FileSystemDirectoryHandle(p);
  }
  defineGlobal("showOpenFilePicker", showOpenFilePicker);
  defineGlobal("showSaveFilePicker", showSaveFilePicker);
  defineGlobal("showDirectoryPicker", showDirectoryPicker);

  // ---------------------------------------------------------------- shell

  const isURL = (s) => typeof s === "string" && /^[a-z][a-z0-9+.-]*:\/\//i.test(s);
  const shellObj = {
    /**
     * Opens Explorer with the files selected. Accepts paths, handles or Files,
     * as separate arguments or arrays. Files in different folders open one
     * window per folder.
     */
    showItemInFolder(...items) {
      const paths = items.flat(Infinity).filter((x) => x !== null && x !== undefined).map((x) => toPath(x));
      native.shellShowInFolder(paths);
    },
    /** Opens a file or folder with its default application. */
    openPath(target) { native.shellOpen(toPath(target)); },
    /** Opens a URL in the default browser. */
    openExternal(url) {
      if (!isURL(url)) throw new TypeError(T("shell.expectedURL", url));
      native.shellOpen(String(url));
    },
  };
  shellObj.showInFolder = shellObj.showItemInFolder;
  shellObj.open = (target) => (isURL(target) ? shellObj.openExternal(target) : shellObj.openPath(target));
  defineGlobal("shell", shellObj);

  // ---------------------------------------------------------------- progress

  let lastCancelled = false;
  class Progress {
    constructor(options = {}) {
      if (typeof options === "string") options = { title: options };
      define(this, "_total", Math.max(0, Math.floor(Number(options.total) || 0)));
      define(this, "_done", 0);
      define(this, "_message", options.message ? String(options.message) : "");
      define(this, "_closed", false);
      define(this, "_n", native.uiProgress(options.title !== undefined ? String(options.title) : title(), this._total,
        this._message, options.cancellable !== false));
      lastCancelled = false;
    }
    get total() { return this._total; }
    set total(n) { this._total = Math.max(0, Math.floor(Number(n) || 0)); this._push(); }
    get done() { return this._done; }
    get message() { return this._message; }
    set message(m) { this._message = String(m); this._push(); }
    get cancelled() {
      if (this._closed) return lastCancelled;
      const c = this._n.cancelled();
      if (c) lastCancelled = true;
      return c;
    }
    _push() { if (!this._closed) this._n.update(this._done, this._total, this._message); }
    /** Sets the number of completed items and optionally the message. */
    update(done, message) {
      this._done = Math.max(0, Math.floor(Number(done) || 0));
      if (message !== undefined) this._message = String(message);
      this._push();
      return this;
    }
    increment(message) { return this.update(this._done + 1, message); }
    close() {
      if (this._closed) return;
      this.cancelled; // latch cancellation state
      this._closed = true;
      this._n.close();
    }
  }

  function* withProgressSync(iterable, opts, label, total) {
    const p = new Progress(Object.assign({}, opts, { total }));
    let i = 0;
    try {
      for (const item of iterable) {
        if (p.cancelled) return;
        p.update(i, label(item, i));
        yield item;
        i++;
      }
      p.update(i, "");
    } finally {
      p.close();
    }
  }

  function withProgressAsync(iterable, opts, label, total) {
    const iter = iterable[asyncIteratorSymbol]();
    const p = new Progress(Object.assign({}, opts, { total }));
    let i = 0, finished = false;
    const finish = () => { if (!finished) { finished = true; p.close(); } };
    const it = {
      async next() {
        if (finished) return { done: true, value: undefined };
        if (p.cancelled) {
          finish();
          if (iter.return) await iter.return();
          return { done: true, value: undefined };
        }
        let r;
        try { r = await iter.next(); } catch (e) { finish(); throw e; }
        if (r.done) { p.update(i, ""); finish(); return r; }
        p.update(i, label(r.value, i));
        i++;
        return r;
      },
      async return(value) {
        finish();
        if (iter.return) await iter.return();
        return { done: true, value };
      },
      async throw(e) { finish(); throw e; },
    };
    it[asyncIteratorSymbol] = () => it;
    return it;
  }

  function defaultLabel(item) {
    if (item instanceof FileSystemHandle) return item.relativePath;
    if (typeof item === "string" || typeof item === "number") return String(item);
    return "";
  }

  /**
   * Wraps an iterable and shows progress while it is consumed. Stops early
   * (without error) when the user cancels; check ui.cancelled afterwards.
   */
  function withProgress(iterable, options = {}) {
    if (typeof options === "string") options = { title: options };
    const label = typeof options.label === "function" ? options.label : defaultLabel;
    let total = options.total;
    if (total === undefined) {
      if (typeof iterable.length === "number") total = iterable.length;
      else if (typeof iterable.size === "number") total = iterable.size;
      else total = 0;
    }
    if (iterable && typeof iterable[Symbol.iterator] === "function") return withProgressSync(iterable, options, label, total);
    if (iterable && typeof iterable[asyncIteratorSymbol] === "function") return withProgressAsync(iterable, options, label, total);
    throw new TypeError(T("ui.expectedIterable"));
  }

  const uiObj = {
    alert: G.alert,
    confirm: G.confirm,
    prompt: G.prompt,
    showOpenFilePicker,
    showSaveFilePicker,
    showDirectoryPicker,
    progress: (options) => new Progress(options),
    withProgress,
    /** Whether the most recent progress was cancelled by the user. */
    get cancelled() { return lastCancelled; },
    error: (message) => native.uiError(title(), msg(message)),
    Progress,
  };
  defineGlobal("ui", uiObj);

  // ---------------------------------------------------------------- app (HTML windows)

  // Messages between the script and a page (see internal/ui/bridge.js):
  //   { type: "invoke", id, name, args } -> { type: "result"|"error", id, value }
  //   { type: "event", name, args }
  // Files the page passes with xljs.files() arrive as {$files: [start, count]}
  // plus their paths; they become file/directory handles.

  const errorValue = (e) => ({
    name: (e && e.name) || "Error",
    message: e && e.message !== undefined ? String(e.message) : String(e),
  });
  function handleFor(p) {
    const st = native.fsStat(p);
    return st && st.isDirectory ? new FileSystemDirectoryHandle(p) : new FileSystemFileHandle(p);
  }
  function decodeFiles(v, files) {
    if (Array.isArray(v)) return v.map((x) => decodeFiles(x, files));
    if (v && typeof v === "object") {
      const keys = Object.keys(v);
      if (keys.length === 1 && Array.isArray(v.$files)) {
        const [start, count] = v.$files;
        return files.slice(start, start + count).map(handleFor);
      }
      const o = {};
      for (const k of keys) o[k] = decodeFiles(v[k], files);
      return o;
    }
    return v;
  }
  function messageReplacer(k, v) {
    const raw = this[k];
    return raw instanceof FileSystemHandle ? raw.path : v;
  }

  /** An HTML window opened by app.open (or the main window of @mode app). */
  class AppWindow {
    constructor(opts) {
      define(this, "_handlers", new Map());
      define(this, "_listeners", new Map());
      define(this, "_pending", new Map());
      define(this, "_seq", 0);
      define(this, "_closed", false);
      const n = native.appOpen(Object.assign({}, opts, { onMessage: (m, files) => this._receive(m, files) }));
      define(this, "_n", n);
      /** Resolves when the window has been closed. */
      define(this, "closed", n.closed.then(() => {
        this._closed = true;
        const pending = [...this._pending.values()];
        this._pending.clear();
        for (const p of pending) p.reject(new Error(T("app.windowClosed")));
      }));
    }
    _post(m) { if (!this._closed) this._n.post(JSON.stringify(m, messageReplacer)); }
    _receive(text, files) {
      let m;
      try { m = JSON.parse(text); } catch (e) { return; }
      if (!m || typeof m !== "object") return;
      const args = files && files.length ? decodeFiles(m.args || [], files) : m.args || [];
      switch (m.type) {
        case "invoke": {
          const fn = this._handlers.get(m.name);
          Promise.resolve()
            .then(() => {
              if (!fn) throw new Error(T("app.noHandler", String(m.name)));
              return fn.apply(this, args);
            })
            .then(
              (value) => this._post({ type: "result", id: m.id, value: value === undefined ? null : value }),
              (e) => this._post({ type: "error", id: m.id, value: errorValue(e) }));
          return;
        }
        case "result":
        case "error": {
          const p = this._pending.get(m.id);
          if (!p) return;
          this._pending.delete(m.id);
          if (m.type === "result") p.resolve(m.value);
          else p.reject(Object.assign(new Error(m.value && m.value.message), { name: (m.value && m.value.name) || "Error" }));
          return;
        }
        case "event":
          if (m.name === "xljs:close") return this.close();
          for (const fn of [...(this._listeners.get(m.name) || [])]) fn.apply(this, args);
          return;
      }
    }
    /** Calls a function of the page (xljs.handle, or a global function). */
    invoke(name, ...args) {
      if (this._closed) return Promise.reject(new Error(T("app.windowClosed")));
      const id = ++this._seq;
      return new Promise((resolve, reject) => {
        this._pending.set(id, { resolve, reject });
        this._post({ type: "invoke", id, name: String(name), args });
      });
    }
    /** Lets the page call fn with xljs.invoke(name, ...args). */
    handle(name, fn) {
      if (fn) this._handlers.set(String(name), fn);
      else this._handlers.delete(String(name));
      return this;
    }
    /** Sends an event to the page (xljs.on). */
    emit(name, ...args) { this._post({ type: "event", name: String(name), args }); return this; }
    /** Listens to events from the page (xljs.emit). */
    on(name, fn) {
      const k = String(name);
      if (!this._listeners.has(k)) this._listeners.set(k, new Set());
      this._listeners.get(k).add(fn);
      return this;
    }
    off(name, fn) {
      const set = this._listeners.get(String(name));
      if (set) set.delete(fn);
      return this;
    }
    close() { this._n.close(); }
  }

  const appObj = {
    /** The main window of an `@mode app` script (null otherwise). */
    main: null,
    /**
     * Opens a window. html is relative to the script folder and must be
     * covered by @html/@assets. The process keeps running while it is open.
     */
    open(options = {}) {
      if (typeof options === "string") options = { html: options };
      return new AppWindow({
        html: options.html === undefined ? "" : String(options.html),
        title: options.title === undefined ? title() : String(options.title),
        width: Number(options.width) || 0,
        height: Number(options.height) || 0,
      });
    },
  };
  defineGlobal("app", appObj);
  native.openMain = () => {
    appObj.main = new AppWindow({ html: native.appHTML || "", title: title(), width: native.appWidth || 0, height: native.appHeight || 0 });
  };

  /** A stand-in for a window when the UI has none (console, mock). */
  const noWindow = () => ({ closed: Promise.resolve(), close() {} });

  // ---------------------------------------------------------------- ui.form

  const FIELD_TYPES = ["text", "password", "number", "date", "month", "time", "select", "radio", "checkbox",
    "textarea", "file", "files", "folder", "saveFile"];

  function acceptList(a) {
    if (a === undefined || a === null) return [];
    const list = Array.isArray(a) ? a : [a];
    return list.map((e) => String(e).trim()).filter(Boolean).map((e) => (e.startsWith(".") || e.startsWith("*") ? e : "." + e));
  }

  function normalizeFields(fields) {
    if (!Array.isArray(fields) || fields.length === 0) throw new TypeError(T("form.noFields"));
    const names = new Set();
    return fields.map((f, i) => {
      if (typeof f === "string") f = { name: f };
      if (!f || f.name === undefined || f.name === "") throw new TypeError(T("form.noName", i + 1));
      const name = String(f.name);
      const type = f.type || "text";
      if (!FIELD_TYPES.includes(type)) throw new TypeError(T("form.badType", name, String(type)));
      if (names.has(name)) throw new TypeError(T("form.duplicateName", name));
      names.add(name);
      const options = Array.isArray(f.options) ? f.options.map(String) : [];
      if ((type === "select" || type === "radio") && options.length === 0) throw new TypeError(T("form.noOptions", name));
      return {
        spec: f, name, type, options,
        label: f.label === undefined ? name : String(f.label),
        required: !!f.required,
        accept: acceptList(f.accept),
        remember: f.remember !== false && type !== "password",
      };
    });
  }

  const pad4 = (n) => String(n).padStart(4, "0");
  /** A value as the page holds it: string, boolean (checkbox) or paths (files). */
  function toRaw(f, v) {
    const pathOf = (x) => (x instanceof FileSystemHandle ? x.path : x instanceof File && x.path ? x.path : x === null || x === undefined ? "" : String(x));
    switch (f.type) {
      case "checkbox": return v === true || v === "true" || v === 1;
      case "files": return (Array.isArray(v) ? v : v === null || v === undefined || v === "" ? [] : [v]).map(pathOf).filter(Boolean);
      case "file": case "folder": case "saveFile": return pathOf(Array.isArray(v) ? v[0] : v);
    }
    if (v === null || v === undefined) return "";
    if (v instanceof Date && !isNaN(v)) {
      const d = `${pad4(v.getFullYear())}-${pad2(v.getMonth() + 1)}-${pad2(v.getDate())}`;
      if (f.type === "month") return d.slice(0, 7);
      if (f.type === "time") return `${pad2(v.getHours())}:${pad2(v.getMinutes())}`;
      return d;
    }
    return String(v);
  }
  /** The value handed to the script. */
  function fromRaw(f, raw) {
    switch (f.type) {
      case "checkbox": return !!raw;
      case "number": return raw === "" ? null : Number(raw);
      case "files": return raw.map((p) => new FileSystemFileHandle(p));
      case "file": case "saveFile": return raw ? new FileSystemFileHandle(raw) : null;
      case "folder": return raw ? new FileSystemDirectoryHandle(raw) : null;
    }
    return raw;
  }
  function valuesOf(fields, raw) {
    const values = {};
    for (const f of fields) values[f.name] = fromRaw(f, raw[f.name]);
    return values;
  }
  const extOk = (f, p) => !f.accept.length || f.accept.some((e) => e === "*" || e === "*.*" || p.toLowerCase().endsWith(e.replace(/^\*/, "").toLowerCase()));

  /** Built-in checks, then options.validate. Returns {field: message}; "" is a message for the whole form. */
  async function checkForm(fields, raw, validate) {
    const errors = {};
    for (const f of fields) {
      const v = raw[f.name];
      if (v === "" || v === false || (Array.isArray(v) && v.length === 0)) {
        if (f.required) errors[f.name] = T("form.required");
        continue;
      }
      const s = f.spec;
      let e = "";
      switch (f.type) {
        case "number": {
          const n = Number(v);
          if (!isFinite(n)) e = T("form.notNumber");
          else if (s.min !== undefined && n < s.min) e = T("form.min", s.min);
          else if (s.max !== undefined && n > s.max) e = T("form.max", s.max);
          break;
        }
        case "select": case "radio":
          if (!f.options.includes(v)) e = T("form.badOption");
          break;
        case "folder": {
          const st = native.fsStat(v);
          if (!st || !st.isDirectory) e = T("form.notFolder");
          break;
        }
        case "file": case "files":
          for (const p of Array.isArray(v) ? v : [v]) {
            const st = native.fsStat(p);
            if (!st || st.isDirectory) e = T("form.notFile", p);
            else if (!extOk(f, p)) e = T("form.badExtension", f.accept.join(", "));
            if (e) break;
          }
          break;
        case "saveFile":
          if (!extOk(f, v)) e = T("form.badExtension", f.accept.join(", "));
          break;
      }
      if (!e && s.pattern !== undefined && typeof v === "string") {
        const re = s.pattern instanceof RegExp ? s.pattern : new RegExp("^(?:" + s.pattern + ")$");
        if (!re.test(v)) e = s.patternMessage ? String(s.patternMessage) : T("form.pattern");
      }
      if (e) errors[f.name] = e;
    }
    if (Object.keys(errors).length === 0 && typeof validate === "function") {
      const r = await validate(valuesOf(fields, raw));
      if (typeof r === "string" && r) errors[""] = r;
      else if (r && typeof r === "object") {
        for (const k of Object.keys(r)) if (r[k]) errors[fields.some((f) => f.name === k) ? k : ""] = String(r[k]);
      }
    }
    return errors;
  }

  // Remembered values: <config dir>/<script name>.json, { "<form key>": { field: raw } }.
  function rememberFile() {
    const dir = native.configDir();
    return dir ? native.pathJoin(dir, native.pathBasename(native.script, native.pathExtname(native.script)) + ".json") : null;
  }
  function rememberKey(options) {
    if (!options.remember) return null;
    return typeof options.remember === "string" ? options.remember : String(options.title || "form");
  }
  function loadRemembered(key) {
    const file = rememberFile();
    try {
      const all = file && native.fsStat(file) ? JSON.parse(native.fsReadText(file, "utf8")) : {};
      return all && typeof all[key] === "object" && all[key] ? all[key] : {};
    } catch (e) {
      return {};
    }
  }
  function saveRemembered(key, fields, raw) {
    const file = rememberFile();
    if (!file) return;
    try {
      let all = {};
      if (native.fsStat(file)) all = JSON.parse(native.fsReadText(file, "utf8")) || {};
      const vals = {};
      for (const f of fields) if (f.remember) vals[f.name] = raw[f.name];
      all[key] = vals;
      native.fsMkdir(native.pathDirname(file));
      native.fsWriteText(file, JSON.stringify(all, null, 2), "utf8", false);
    } catch (e) {
      err(T("form.rememberFailed", e && e.message));
    }
  }

  /** What onSubmit returned, as a path to offer with "open folder". */
  function outputPath(r) {
    if (r instanceof FileSystemHandle) return { path: r.path, dir: r.kind === "directory" };
    if (typeof r === "string" && r) {
      const st = native.fsStat(r);
      if (st) return { path: native.pathResolve(r), dir: st.isDirectory };
    }
    return null;
  }

  /**
   * Shows a form built from field definitions. Resolves with the values
   * (null when cancelled). With onSubmit the window stays open while it
   * runs and shows its progress and log.
   */
  async function form(options = {}) {
    const fields = normalizeFields(options.fields);
    const key = rememberKey(options);
    const saved = key ? loadRemembered(key) : {};
    const raw = {};
    for (const f of fields) raw[f.name] = toRaw(f, f.remember && f.name in saved ? saved[f.name] : f.spec.default);
    if (native.uiName === "native") return formWindow(options, fields, raw, key);
    return formConsole(options, fields, raw, key);
  }

  async function formConsole(options, fields, raw, key) {
    let errors = {};
    for (;;) {
      const spec = {
        title: options.title ? String(options.title) : title(),
        description: options.description ? String(options.description) : "",
        fields: fields.map((f) => ({ name: f.name, label: f.label, type: f.type, options: f.options, required: f.required, value: raw[f.name], error: errors[f.name] || "" })),
      };
      const res = native.uiForm(JSON.stringify(spec));
      if (res === null) return null;
      const got = JSON.parse(res);
      for (const f of fields) if (f.name in got) raw[f.name] = toRaw(f, got[f.name]);
      errors = await checkForm(fields, raw, options.validate);
      if (Object.keys(errors).length === 0) break;
      if (errors[""]) err(errors[""]);
    }
    const values = valuesOf(fields, raw);
    if (key) saveRemembered(key, fields, raw);
    if (typeof options.onSubmit === "function") {
      let p = null;
      const ctx = {
        get cancelled() { return p ? p.cancelled : native.interrupted(); },
        progress(done, total, message) {
          if (!p) p = new Progress({ title: options.title ? String(options.title) : title(), total });
          if (total !== undefined && total !== p.total) p.total = total;
          p.update(done, message);
        },
        log: (...a) => out(formatArgs(a)),
        window: null,
      };
      try {
        await options.onSubmit(values, ctx);
      } finally {
        if (p) p.close();
      }
    }
    return values;
  }

  function formWindow(options, fields, raw, key) {
    const run = typeof options.onSubmit === "function";
    const formTitle = options.title ? String(options.title) : title();
    let height = 150 + (options.description ? 40 : 0);
    for (const f of fields) height += f.type === "textarea" ? 120 : f.type === "radio" ? 34 + 26 * f.options.length : f.type === "checkbox" ? 40 : 66;
    const win = new AppWindow({
      page: "form",
      title: formTitle,
      width: Number(options.width) || 600,
      height: Number(options.height) || Math.max(run ? 420 : 260, Math.min(760, height)),
    });
    const byName = (name) => {
      const f = fields.find((x) => x.name === name);
      if (!f) throw new Error(T("form.unknownField", String(name)));
      return f;
    };
    return new Promise((resolve) => {
      let result = null, running = false, cancelled = false, closed = false;
      win.handle("init", () => ({
        title: formTitle,
        description: options.description ? String(options.description) : "",
        submit: options.submit ? String(options.submit) : T("form.submit"),
        run,
        fields: fields.map((f) => ({
          name: f.name, label: f.label, type: f.type, options: f.options, required: f.required,
          placeholder: f.spec.placeholder === undefined ? "" : String(f.spec.placeholder),
          help: f.spec.help === undefined ? "" : String(f.spec.help),
          min: f.spec.min, max: f.spec.max, step: f.spec.step, rows: f.spec.rows, accept: f.accept,
        })),
        values: raw,
        text: {
          cancel: T("ui.cancel"), browse: T("form.browse"), drop: T("form.drop"), clear: T("form.clear"),
          close: T("form.close"), back: T("form.back"), openFolder: T("form.openFolder"), openFile: T("form.openFile"),
          running: T("form.running"), cancelling: T("form.cancelling"), done: T("form.done"),
          cancelled: T("form.cancelled"), failed: T("form.failed"), required: T("form.requiredMark"),
          noDrop: T("form.noDrop"),
        },
      }));
      win.handle("pick", async (name) => {
        const f = byName(name);
        const cur = raw[f.name];
        const types = f.accept.length ? [{ description: "", accept: f.accept }] : undefined;
        const at = (p) => (p ? new FileSystemFileHandle(Array.isArray(p) ? p[0] || "" : p) : undefined);
        switch (f.type) {
          case "file": case "files": {
            const hs = await showOpenFilePicker({ title: f.label, types, multiple: f.type === "files", startIn: at(cur) });
            if (!hs.length) return null;
            return f.type === "files" ? hs.map((h) => h.path) : hs[0].path;
          }
          case "folder": {
            const h = await showDirectoryPicker({ title: f.label, startIn: cur ? new FileSystemDirectoryHandle(cur) : undefined });
            return h ? h.path : null;
          }
          case "saveFile": {
            const h = await showSaveFilePicker({
              title: f.label, types, startIn: cur ? at(native.pathResolve(cur)) : undefined,
              suggestedName: cur ? native.pathBasename(cur, "") : f.spec.suggestedName ? String(f.spec.suggestedName) : "",
            });
            return h ? h.path : null;
          }
        }
        return null;
      });
      // Files dropped on a field arrive as handles (xljs.files on the page).
      win.handle("drop", (name, handles) => {
        const f = byName(name);
        const paths = (handles || []).map((h) => h.path);
        if (!paths.length) return null;
        return f.type === "files" ? paths : paths[0];
      });
      win.handle("submit", async (pageRaw) => {
        if (running) return { errors: {} };
        for (const f of fields) if (pageRaw && f.name in pageRaw) raw[f.name] = toRaw(f, pageRaw[f.name]);
        const errors = await checkForm(fields, raw, options.validate);
        if (Object.keys(errors).length) return { errors };
        result = valuesOf(fields, raw);
        if (key) saveRemembered(key, fields, raw);
        if (!run) {
          win.close();
          return { ok: true };
        }
        running = true;
        cancelled = false;
        Promise.resolve().then(() => runSubmit(result));
        return { ok: true };
      });
      win.on("cancel", () => { cancelled = true; });
      win.handle("open", (p, dir) => {
        if (dir) native.shellOpen(String(p));
        else native.shellShowInFolder([String(p)]);
        return null;
      });
      win.closed.then(() => {
        closed = true;
        cancelled = true;
        if (!running) resolve(result);
      });

      async function runSubmit(values) {
        let last = 0, timer = null, latest = null;
        const flush = () => {
          timer = null;
          if (latest) win.emit("progress", latest);
          latest = null;
          last = Date.now();
        };
        const ctx = {
          get cancelled() { return cancelled; },
          progress(done, total, message) {
            latest = { done: Number(done) || 0, total: Number(total) || 0, message: message === undefined ? "" : String(message) };
            if (Date.now() - last >= 50 || (latest.total && latest.done >= latest.total)) {
              if (timer !== null) clearTimeout(timer);
              flush();
            } else if (timer === null) {
              timer = setTimeout(flush, 50);
            }
          },
          log: (...a) => { win.emit("log", formatArgs(a)); },
          window: win,
        };
        let state = "done", message = "", output = null;
        try {
          output = outputPath(await options.onSubmit(values, ctx));
          if (cancelled) state = "cancelled";
        } catch (e) {
          state = "failed";
          message = e instanceof Error ? `${e.name}: ${e.message}` : String(e);
          err(inspect(e));
        }
        if (timer !== null) { clearTimeout(timer); flush(); }
        running = false;
        win.emit("finished", { state, message, output });
        if (closed) resolve(result);
      }
    });
  }

  // ---------------------------------------------------------------- ui.table

  function tableCell(v) {
    if (v === null || v === undefined) return null;
    switch (typeof v) {
      case "number": return isFinite(v) ? v : String(v);
      case "string": case "boolean": return v;
      case "bigint": return String(v);
    }
    if (v instanceof Date) {
      if (isNaN(v)) return "";
      const s = formatDateLocal(v, false);
      return s.endsWith(" 00:00:00") ? s.slice(0, 10) : s;
    }
    if (v instanceof FileSystemHandle) return v.path;
    return inspect(v, { depth: 0 });
  }

  /** Columns and cells of rows (objects, arrays or plain values). */
  function tableModel(rows, columns) {
    const isRecord = (r) => r !== null && typeof r === "object" && !Array.isArray(r) && !(r instanceof Date) && !(r instanceof FileSystemHandle);
    let cols;
    if (Array.isArray(columns) && columns.length) {
      cols = columns.map((c) => (c && typeof c === "object" ? { key: c.key, label: String(c.label === undefined ? c.key : c.label) } : { key: c, label: String(c) }));
    } else if (rows.some(isRecord)) {
      cols = keysOf(rows.filter(isRecord)).map((k) => ({ key: k, label: k }));
      if (rows.some((r) => !isRecord(r))) cols.push({ key: undefined, label: T("table.value") });
    } else if (rows.some(Array.isArray)) {
      const n = Math.max(...rows.map((r) => (Array.isArray(r) ? r.length : 1)));
      cols = Array.from({ length: n }, (_, i) => ({ key: i, label: String(i + 1) }));
    } else {
      cols = [{ key: undefined, label: T("table.value") }];
    }
    const valueAt = (r, c) => {
      if (c.key === undefined) return isRecord(r) || Array.isArray(r) ? undefined : r;
      if (Array.isArray(r)) return typeof c.key === "number" ? r[c.key] : undefined;
      if (isRecord(r)) return r[c.key];
      return c.key === 0 ? r : undefined;
    };
    return { cols, valueAt };
  }

  function sheetName(s) {
    const n = String(s).replace(/[\[\]:*?\/\\]/g, "_").slice(0, 31).trim();
    return n || "Sheet1";
  }

  /**
   * Shows rows in a table (sort, filter, copy, save). Without selectable
   * it returns the window right away; with selectable ("single" |
   * "multiple") it resolves with the chosen rows ([] when cancelled).
   */
  function showTable(rows, options = {}) {
    if (typeof options === "string") options = { title: options };
    rows = rows === null || rows === undefined ? [] : Array.from(rows);
    const selectable = options.selectable === true ? "multiple" : options.selectable ? String(options.selectable) : "";
    if (selectable && selectable !== "single" && selectable !== "multiple") throw new TypeError(T("table.badSelectable", selectable));
    const tableTitle = options.title ? String(options.title) : title();
    const { cols, valueAt } = tableModel(rows, options.columns);

    if (native.uiName !== "native") {
      const recs = rows.map((r) => {
        const o = {};
        for (const c of cols) o[c.label] = valueAt(r, c);
        return o;
      });
      const sel = native.uiTable(tableTitle, tableText(recs), rows.length, selectable);
      if (!selectable) return noWindow();
      return Promise.resolve(sel ? sel.map((i) => rows[i]) : []);
    }

    const win = new AppWindow({ page: "table", title: tableTitle, width: Number(options.width) || 960, height: Number(options.height) || 620 });
    win.handle("init", () => ({
      title: tableTitle,
      selectable,
      columns: cols.map((c) => c.label),
      rows: rows.map((r) => cols.map((c) => tableCell(valueAt(r, c)))),
      text: {
        filter: T("table.filter"), copy: T("table.copy"), copied: T("table.copied"), saveCSV: T("table.saveCSV"),
        saveExcel: T("table.saveExcel"), saved: T("table.saved"), ok: "OK", cancel: T("ui.cancel"), close: T("form.close"),
        count: T("table.count"), filtered: T("table.filtered"), selected: T("table.selected"),
      },
    }));
    win.handle("save", async (format, indices) => {
      const xlsx = format === "xlsx";
      const h = await showSaveFilePicker({
        title: xlsx ? T("table.saveExcel") : T("table.saveCSV"),
        suggestedName: tableTitle.replace(/[\\\/:*?"<>|]/g, "_") + (xlsx ? ".xlsx" : ".csv"),
        types: [xlsx ? { description: "Excel", accept: [".xlsx"] } : { description: "CSV", accept: [".csv"] }],
      });
      if (!h) return null;
      const data = indices.map((i) => cols.map((c) => {
        const v = valueAt(rows[i], c);
        return v !== null && typeof v === "object" && !(v instanceof Date) ? tableCell(v) : v;
      }));
      const columns = cols.map((c) => c.label);
      if (xlsx) {
        const book = workbooks.create();
        book.addSheet(sheetName(tableTitle)).write(data, { columns });
        book.save(h);
      } else {
        csvObj.write(h, data, { columns });
      }
      return h.name;
    });
    if (!selectable) return win;
    return new Promise((resolve) => {
      let done = false;
      win.handle("ok", (indices) => {
        done = true;
        resolve((indices || []).filter((i) => i >= 0 && i < rows.length).map((i) => rows[i]));
        win.close();
        return null;
      });
      win.closed.then(() => { if (!done) resolve([]); });
    });
  }

  uiObj.form = form;
  uiObj.table = showTable;

  // ---------------------------------------------------------------- csv

  function rowsFromRecords(records, opts) {
    const header = opts.header === undefined ? true : opts.header;
    let recs = records;
    if (opts.skipEmptyLines !== false) recs = recs.filter((r) => !(r.length === 0 || (r.length === 1 && r[0] === "")));
    if (opts.trim) recs = recs.map((r) => r.map((s) => s.trim()));
    const cast = opts.cast ? castValue : (x) => x;
    if (!header) {
      if (opts.columns) return recs.map((r) => objectFrom(opts.columns, r, cast));
      return opts.cast ? recs.map((r) => r.map(cast)) : recs;
    }
    if (recs.length === 0) return [];
    const keys = uniqueKeys(recs[0].map((h) => (opts.trim ? h.trim() : h)));
    return recs.slice(1).map((r) => objectFrom(keys, r, cast));
  }
  function uniqueKeys(names) {
    const used = new Map();
    return names.map((raw, i) => {
      let k = raw === null || raw === undefined || String(raw) === "" ? `列${i + 1}` : String(raw);
      if (used.has(k)) {
        let n = used.get(k) + 1;
        while (used.has(`${k}_${n}`)) n++;
        used.set(k, n);
        k = `${k}_${n}`;
      }
      used.set(k, 1);
      return k;
    });
  }
  function objectFrom(keys, values, cast) {
    const o = {};
    keys.forEach((k, i) => { o[k] = i < values.length ? cast(values[i]) : (cast === castValue ? null : ""); });
    return o;
  }
  function castValue(s) {
    if (typeof s !== "string") return s;
    if (s === "") return null;
    if (/^-?(0|[1-9]\d*)(\.\d+)?$/.test(s) || /^-?(0|[1-9]\d{0,2}(,\d{3})+)(\.\d+)?$/.test(s)) return Number(s.replace(/,/g, ""));
    if (s === "true" || s === "TRUE") return true;
    if (s === "false" || s === "FALSE") return false;
    return s;
  }
  function cellToString(v) {
    if (v === null || v === undefined) return "";
    if (v instanceof Date) {
      if (isNaN(v)) return "";
      const d = `${v.getFullYear()}/${pad2(v.getMonth() + 1)}/${pad2(v.getDate())}`;
      if (v.getHours() || v.getMinutes() || v.getSeconds()) return `${d} ${pad2(v.getHours())}:${pad2(v.getMinutes())}:${pad2(v.getSeconds())}`;
      return d;
    }
    if (typeof v === "object") return JSON.stringify(v);
    return String(v);
  }
  function recordsFromRows(rows, opts) {
    rows = Array.from(rows || []);
    if (rows.length === 0) return opts.columns && opts.header !== false ? [opts.columns.slice()] : [];
    if (Array.isArray(rows[0])) {
      const recs = rows.map((r) => r.map(cellToString));
      if (opts.columns && opts.header !== false) recs.unshift(opts.columns.map(String));
      return recs;
    }
    const cols = opts.columns || keysOf(rows);
    const recs = rows.map((r) => cols.map((c) => cellToString(r[c])));
    if (opts.header !== false) recs.unshift(cols.map(String));
    return recs;
  }
  function keysOf(rows) {
    const cols = [], seen = new Set();
    for (const r of rows) for (const k of Object.keys(r || {})) if (!seen.has(k)) { seen.add(k); cols.push(k); }
    return cols;
  }
  function delimiterFor(target, opts) {
    if (opts.delimiter) return opts.delimiter;
    if (typeof target === "string" || target instanceof FileSystemHandle || (target instanceof File && target.path)) {
      const p = typeof target === "string" ? target : target.path || target.name;
      if (/\.(tsv|tab)$/i.test(p)) return "\t";
    }
    return ",";
  }
  const csvObj = {
    /** Parses CSV text. opts: { header=true, delimiter=",", columns, cast, trim, skipEmptyLines=true, comment } */
    parse(text, opts = {}) {
      return rowsFromRecords(native.csvParse(String(text), opts.delimiter || ",", opts.comment || ""), opts);
    },
    /** Converts rows (objects or arrays) to CSV text. opts: { header=true, columns, delimiter, eol="\r\n", quoteAll } */
    stringify(rows, opts = {}) {
      const crlf = opts.eol === undefined ? true : opts.eol === "\r\n";
      return native.csvStringify(recordsFromRows(rows, opts), opts.delimiter || ",", crlf, !!opts.quoteAll);
    },
    /** Reads a CSV file (path, handle or File). opts: parse options + encoding="auto" */
    read(src, opts = {}) {
      let text;
      if (src instanceof Blob && !(src instanceof File && src.path)) text = native.decode(src._bytes(), opts.encoding || "auto");
      else text = native.fsReadText(toPath(src), opts.encoding || "auto");
      return rowsFromRecords(native.csvParse(text, delimiterFor(src, opts), opts.comment || ""), opts);
    },
    /** Writes rows to a CSV file. opts: stringify options + encoding="utf8bom" */
    write(dest, rows, opts = {}) {
      const crlf = opts.eol === undefined ? true : opts.eol === "\r\n";
      const text = native.csvStringify(recordsFromRows(rows, opts), delimiterFor(dest, opts), crlf, !!opts.quoteAll);
      native.fsWriteText(toPath(dest), text, opts.encoding || "utf8bom", !!opts.append);
    },
  };
  defineGlobal("csv", csvObj);

  // ---------------------------------------------------------------- workbooks

  const XLSX_MIME = MIME[".xlsx"];

  /**
   * Normalizes a style spec. Besides the full form
   * { font, fill, border, alignment, numberFormat } it accepts shorthands:
   * bold, italic, underline, strike, color, fontSize, fontName,
   * background (= fill), align, valign, wrap, format (= numberFormat).
   */
  function normStyle(s) {
    if (!s || typeof s !== "object") return {};
    const out = {};
    const font = Object.assign({}, s.font);
    for (const k of ["bold", "italic", "underline", "strike"]) if (s[k] !== undefined) font[k] = s[k];
    if (s.color !== undefined) font.color = s.color;
    if (s.fontSize !== undefined) font.size = s.fontSize;
    if (s.fontName !== undefined) font.name = s.fontName;
    if (Object.keys(font).length) out.font = font;
    const fill = s.fill !== undefined ? s.fill : s.background;
    if (fill !== undefined) out.fill = fill;
    const al = Object.assign({}, s.alignment);
    if (s.align !== undefined) al.horizontal = s.align;
    if (s.valign !== undefined) al.vertical = s.valign;
    if (s.wrap !== undefined) al.wrap = s.wrap;
    if (Object.keys(al).length) out.alignment = al;
    if (s.border !== undefined) out.border = s.border;
    const nf = s.numberFormat !== undefined ? s.numberFormat : s.format;
    if (nf !== undefined) out.numberFormat = nf;
    if (s.protection !== undefined) out.protection = s.protection;
    return out;
  }

  /** Style used by write({ table: true }). */
  const TABLE_STYLE = {
    header: { bold: true, background: "#DDEBF7", align: "center", valign: "center", border: { all: { style: "thin", color: "#8EA9DB" } } },
    body: { border: { all: { style: "thin", color: "#BFBFBF" } } },
  };

  class Range {
    constructor(sheet, address) {
      define(this, "_sheet", sheet);
      define(this, "_address", String(address).toUpperCase());
    }
    get address() { return this._address; }
    get sheet() { return this._sheet; }
    /** Applies (merges) a style; opts.replace discards the existing style first. */
    style(spec, opts = {}) {
      native.xlsxStyle(this._sheet._book._h, this._sheet._name, this._address, normStyle(spec), !!opts.replace);
      return this;
    }
    font(spec) { return this.style({ font: spec }); }
    fill(color) { return this.style({ fill: color }); }
    border(spec) { return this.style({ border: spec }); }
    align(horizontal, vertical) { return this.style({ alignment: { horizontal, vertical } }); }
    numberFormat(format) { return this.style({ numberFormat: format }); }
    merge() { native.xlsxMerge(this._sheet._book._h, this._sheet._name, this._address, false); return this; }
    unmerge() { native.xlsxMerge(this._sheet._book._h, this._sheet._name, this._address, true); return this; }
    /** Values of the range as a 2-D array; assigning writes from the top-left cell. */
    get values() {
      const [a, b] = this._address.split(":");
      const [c1, r1] = native.xlsxParseRef(a);
      const [c2, r2] = native.xlsxParseRef(b || a);
      const rows = this._sheet.toArrays();
      const out = [];
      for (let r = Math.min(r1, r2); r <= Math.max(r1, r2); r++) {
        const row = rows[r - 1] || [];
        const line = [];
        for (let c = Math.min(c1, c2); c <= Math.max(c1, c2); c++) line.push(c - 1 < row.length ? row[c - 1] : null);
        out.push(line);
      }
      return out;
    }
    set values(rows) {
      native.xlsxWriteRows(this._sheet._book._h, this._sheet._name, this._address.split(":")[0], rows);
    }
    [inspectSymbol]() { return `Range ${this._sheet._name}!${this._address}`; }
  }

  class Cell {
    constructor(sheet, address) {
      define(this, "_sheet", sheet);
      define(this, "_address", address.toUpperCase());
    }
    get address() { return this._address; }
    get row() { return native.xlsxParseRef(this._address)[1]; }
    get column() { return native.xlsxParseRef(this._address)[0]; }
    /** Typed value: number, string, boolean, Date or null. */
    get value() { return native.xlsxGet(this._sheet._book._h, this._sheet._name, this._address); }
    set value(v) { native.xlsxSet(this._sheet._book._h, this._sheet._name, this._address, v); }
    /** Displayed text (number format applied). */
    get text() { return native.xlsxGetText(this._sheet._book._h, this._sheet._name, this._address); }
    /** Formula with leading "=", or null. */
    get formula() { return native.xlsxGetFormula(this._sheet._book._h, this._sheet._name, this._address); }
    set formula(f) { native.xlsxSetFormula(this._sheet._book._h, this._sheet._name, this._address, f === null || f === undefined ? "" : String(f)); }
    /** Evaluates the formula with excelize's calculation engine (subset of Excel functions). */
    calc() { return native.xlsxCalc(this._sheet._book._h, this._sheet._name, this._address); }
    /** Applies (merges) a style to the cell. */
    style(spec, opts) { new Range(this._sheet, this._address).style(spec, opts); return this; }
    /** The cell's current style ({ font, fill, border, alignment, numberFormat }). */
    getStyle() { return native.xlsxGetStyle(this._sheet._book._h, this._sheet._name, this._address); }
    [inspectSymbol]() { return `Cell ${this._sheet._name}!${this._address} ${inspect(this.value)}`; }
  }

  function refOf(a, b) {
    if (typeof a === "number") return native.xlsxCellName(b, a); // (row, column)
    return String(a);
  }

  class Worksheet {
    constructor(book, name) {
      define(this, "_book", book);
      define(this, "_name", name);
    }
    get book() { return this._book; }
    get name() { return this._name; }
    set name(v) {
      native.xlsxRenameSheet(this._book._h, this._name, String(v));
      this._name = String(v);
    }
    /** cell("B2") or cell(row, column) (1-based). */
    cell(a, b) { return new Cell(this, refOf(a, b)); }
    getValue(a, b) { return native.xlsxGet(this._book._h, this._name, refOf(a, b)); }
    setValue(a, b, c) {
      if (typeof a === "number") native.xlsxSet(this._book._h, this._name, refOf(a, b), c);
      else native.xlsxSet(this._book._h, this._name, String(a), b);
      return this;
    }
    /** All used rows as arrays of typed values. opts: { text: true } returns displayed strings. */
    toArrays(opts = {}) {
      const rows = native.xlsxReadAll(this._book._h, this._name, !!opts.text);
      return opts.skipEmpty ? rows.filter((r) => r.some((v) => v !== null && v !== "")) : rows;
    }
    /**
     * Rows as objects keyed by the header row.
     * opts: { header = 1 (row number), columns, skipEmpty = true, text = false }
     */
    toObjects(opts = {}) {
      const rows = native.xlsxReadAll(this._book._h, this._name, !!opts.text);
      const headerRow = opts.header === undefined ? 1 : opts.header;
      let keys, body;
      if (opts.columns) {
        keys = opts.columns;
        body = headerRow ? rows.slice(headerRow) : rows;
      } else {
        const h = rows[headerRow - 1] || [];
        keys = uniqueKeys(h.map((v) => (v === null ? "" : v instanceof Date ? formatDateLocal(v) : String(v).trim())));
        body = rows.slice(headerRow);
      }
      if (opts.skipEmpty !== false) body = body.filter((r) => r.some((v) => v !== null && v !== ""));
      return body.map((r) => {
        const o = {};
        keys.forEach((k, i) => { o[k] = i < r.length ? r[i] : null; });
        return o;
      });
    }
    /** Alias of toObjects(). */
    rows(opts) { return this.toObjects(opts); }
    /**
     * Writes rows (array of objects or array of arrays) starting at opts.start ("A1").
     * Objects get a header row unless opts.header === false.
     */
    write(rows, opts = {}) {
      rows = Array.from(rows || []);
      const start = String(opts.start || "A1").toUpperCase();
      let data, cols = null, hasHeader = false;
      if (rows.length && !Array.isArray(rows[0])) {
        cols = opts.columns || keysOf(rows);
        data = rows.map((r) => cols.map((c) => r[c]));
        if (opts.header !== false) { data.unshift(cols); hasHeader = true; }
      } else {
        data = rows;
        if (opts.columns && opts.header !== false) { data = [opts.columns].concat(data); hasHeader = true; cols = opts.columns; }
      }
      native.xlsxWriteRows(this._book._h, this._name, start, data);
      this._formatWritten(start, data, cols, hasHeader, opts);
      return this;
    }
    /** Applies the formatting options of write() to the written block. */
    _formatWritten(start, data, cols, hasHeader, opts) {
      const preset = opts.table ? (typeof opts.table === "object" ? opts.table : TABLE_STYLE) : null;
      const headerStyle = opts.headerStyle || (preset && preset.header);
      const bodyStyle = opts.style || (preset && preset.body);
      const nCols = data.reduce((m, r) => Math.max(m, r.length), 0);
      if (data.length === 0 || nCols === 0) return;
      const [c1, r1] = native.xlsxParseRef(start);
      const c2 = c1 + nCols - 1, r2 = r1 + data.length - 1;
      const ref = (c, r) => native.xlsxCellName(c, r);
      const bodyTop = hasHeader ? r1 + 1 : r1;
      if (bodyStyle) this.style(`${ref(c1, r1)}:${ref(c2, r2)}`, bodyStyle);
      if (opts.columnStyles && bodyTop <= r2) {
        for (const [key, spec] of Object.entries(opts.columnStyles)) {
          let idx = cols ? cols.indexOf(key) : -1;
          if (idx < 0 && /^\d+$/.test(key)) idx = Number(key);
          if (idx < 0 && /^[A-Z]+$/i.test(key)) idx = native.xlsxParseRef(key + "1")[0] - c1;
          if (idx >= 0 && idx < nCols) this.style(`${ref(c1 + idx, bodyTop)}:${ref(c1 + idx, r2)}`, spec);
        }
      }
      if (hasHeader && headerStyle) this.style(`${ref(c1, r1)}:${ref(c2, r1)}`, headerStyle);
      const want = (k) => opts[k] !== undefined ? opts[k] : !!preset;
      if (want("autoFit")) this.autoFit(`${native.xlsxColumnName(c1)}:${native.xlsxColumnName(c2)}`);
      if (want("freeze") && hasHeader) this.freeze(ref(1, r1 + 1));
      if (want("filter") && hasHeader) this.autoFilter(`${ref(c1, r1)}:${ref(c2, r2)}`);
    }
    /** Appends rows below the last used row (objects are written without a header). */
    append(rows, opts = {}) {
      const next = native.xlsxRowCount(this._book._h, this._name) + 1;
      const header = opts.header !== undefined ? opts.header : next === 1;
      return this.write(rows, Object.assign({}, opts, { start: "A" + next, header }));
    }
    /** Used range, e.g. "A1:D20". */
    get dimension() { return native.xlsxDimension(this._book._h, this._name); }
    get rowCount() { return native.xlsxRowCount(this._book._h, this._name); }
    /** setColumnWidth("A", 20) or setColumnWidth("A:C", 20) */
    setColumnWidth(cols, width) {
      const [a, b] = String(cols).split(":");
      native.xlsxSetColWidth(this._book._h, this._name, a, b || a, width);
      return this;
    }
    getColumnWidth(col) { return native.xlsxGetColWidth(this._book._h, this._name, String(col)); }
    setRowHeight(row, height) {
      native.xlsxSetRowHeight(this._book._h, this._name, row, height);
      return this;
    }
    /** Sets column widths from the cell contents. cols: "A:D" (default: all used columns). */
    autoFit(cols, opts = {}) {
      if (typeof cols === "object" && cols !== null) { opts = cols; cols = undefined; }
      let c1 = 1, c2 = 16384;
      if (cols) {
        const [a, b] = String(cols).toUpperCase().split(":");
        c1 = native.xlsxParseRef(a + "1")[0];
        c2 = native.xlsxParseRef((b || a) + "1")[0];
      }
      native.xlsxAutoFit(this._book._h, this._name, c1, c2, opts.min !== undefined ? opts.min : 6, opts.max !== undefined ? opts.max : 60);
      return this;
    }
    /** range("A1:D10") */
    range(address) { return new Range(this, address); }
    /** style("A1:D1", { bold: true, background: "#DDEBF7", border: "thin" }) */
    style(address, spec, opts) { new Range(this, address).style(spec, opts); return this; }
    merge(address) { new Range(this, address).merge(); return this; }
    unmerge(address) { new Range(this, address).unmerge(); return this; }
    /** Freezes the rows above and columns left of the cell: freeze("A2") freezes row 1. */
    freeze(cell) {
      if (cell && typeof cell === "object") cell = native.xlsxCellName((cell.columns || 0) + 1, (cell.rows || 0) + 1);
      native.xlsxFreeze(this._book._h, this._name, cell ? String(cell) : "");
      return this;
    }
    unfreeze() { native.xlsxFreeze(this._book._h, this._name, ""); return this; }
    /** Adds filter buttons to the header row of the range (default: the used range). */
    autoFilter(address) {
      native.xlsxAutoFilter(this._book._h, this._name, address || this.dimension);
      return this;
    }
    [inspectSymbol]() { return `Worksheet ${JSON.stringify(this._name)}`; }
  }

  class Workbook {
    constructor(h) { define(this, "_h", h); }
    /** Path the workbook was opened from or last saved to (null for new workbooks). */
    get path() { return native.xlsxPath(this._h); }
    get sheetNames() { return native.xlsxSheetNames(this._h); }
    /** Array of worksheets; sheets also accessible by name: book.sheets["売上"]. */
    get sheets() {
      const names = native.xlsxSheetNames(this._h);
      const arr = names.map((n) => new Worksheet(this, n));
      names.forEach((n, i) => { if (!(n in arr)) define(arr, n, arr[i]); });
      return arr;
    }
    /** sheet("名前") or sheet(0); null when missing. */
    sheet(nameOrIndex) {
      if (typeof nameOrIndex === "number") {
        const n = native.xlsxSheetNames(this._h)[nameOrIndex];
        return n === undefined ? null : new Worksheet(this, n);
      }
      return native.xlsxHasSheet(this._h, String(nameOrIndex)) ? new Worksheet(this, String(nameOrIndex)) : null;
    }
    addSheet(name) {
      native.xlsxAddSheet(this._h, String(name));
      return new Worksheet(this, String(name));
    }
    removeSheet(sheet) {
      native.xlsxRemoveSheet(this._h, sheet instanceof Worksheet ? sheet.name : String(sheet));
    }
    /** Saves to dest (path or handle), or to the original path. */
    save(dest) {
      native.xlsxSave(this._h, dest === undefined || dest === null ? "" : toPath(dest, "save()"));
      return this;
    }
    toArrayBuffer() { return native.xlsxToBytes(this._h); }
    toBlob() { return new Blob([native.xlsxToBytes(this._h)], { type: XLSX_MIME }); }
    /** Default font of the workbook: { name, size } */
    get defaultFont() { return native.xlsxDefaultFont(this._h); }
    close() { native.xlsxClose(this._h); }
    [inspectSymbol]() { return `Workbook { path: ${JSON.stringify(this.path)}, sheets: ${inspect(this.sheetNames)} }`; }
  }

  const workbooksObj = {
    /** Opens an .xlsx from a path, handle, File or Blob. */
    open(src) {
      if (src instanceof Blob && !(src instanceof File && src.path)) return new Workbook(native.xlsxOpenBytes(src._bytes(), src.name || ""));
      if (src instanceof ArrayBuffer || ArrayBuffer.isView(src)) return new Workbook(native.xlsxOpenBytes(src, ""));
      return new Workbook(native.xlsxOpen(toPath(src, "workbooks.open()")));
    },
    /** Creates an empty workbook. The first addSheet() replaces the default sheet. */
    create(options) { return new Workbook(native.xlsxCreate(options)); },
    Workbook,
    Worksheet,
    Cell,
    Range,
    /** Converts a column number to letters (1 -> "A"). */
    columnName: (n) => native.xlsxColumnName(n),
    /** Builds a cell reference from (row, column), 1-based. */
    cellName: (row, column) => native.xlsxCellName(column, row),
  };
  defineGlobal("workbooks", workbooksObj);
});
