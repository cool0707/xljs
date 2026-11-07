import { ok, equal } from "./_assert.js";

// Through the test server as a proxy that wants Basic authentication
// (XLJS_PROXY, set by the test; the logins are in memory).
// A name that does not resolve: only the proxy can answer (WinHTTP would
// go to 127.0.0.1 without the proxy).
const base = xljs.env("XLJS_TEST_HTTP").replace("127.0.0.1", "proxied.invalid");
ok(xljs.env("XLJS_PROXY"), "XLJS_PROXY is set by the test");

// The first login is refused, the second works and is saved.
let res = await fetch(`${base}/echo`);
equal(res.status, 200);
equal((await res.json()).path, "/echo");

// Saved: no prompt (the answers would be empty).
res = await fetch(`${base}/echo?again`);
equal(res.status, 200);
equal(new URL((await res.json()).path, base).search, "?again");

// Requests at the same time ask nothing either.
const all = await Promise.all([1, 2, 3].map((i) => fetch(`${base}/echo?${i}`)));
ok(all.every((r) => r.status === 200));

console.log("fetchproxy ok");
