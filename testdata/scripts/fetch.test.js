import { ok, equal, deepEqual, throws, rejects } from "./_assert.js";

// ---- URL
let u = new URL("HTTPS://user:pw@Example.COM:443/a/./b/../c d?q=あ #frag");
equal(u.href, "https://user:pw@example.com/a/c%20d?q=%E3%81%82%20#frag");
equal(u.origin, "https://example.com");
equal(u.host, "example.com");
equal(u.port, "");
equal(u.pathname, "/a/c%20d");
equal(u.search, "?q=%E3%81%82%20");
equal(u.searchParams.get("q"), "あ ");
equal(u.hash, "#frag");
equal(u.username, "user");
equal(new URL("../x?y#z", "http://h:8080/a/b/c").href, "http://h:8080/a/x?y#z");
equal(new URL("//other/p", "https://h/a").href, "https://other/p");
equal(new URL("?k=v", "http://h/a/b").href, "http://h/a/b?k=v");
equal(new URL("#f", "http://h/a?q").href, "http://h/a?q#f");
equal(new URL("/root", "http://h/a/b").href, "http://h/root");
equal(new URL("http://h").pathname, "/");
equal(new URL("http://[::1]:80/").host, "[::1]");
equal(new URL("file:///C:/data/a.xlsx").pathname, "/C:/data/a.xlsx");
equal(new URL("mailto:a@example.com").pathname, "a@example.com");
throws(() => new URL("/relative"), /URL が正しくありません/);
throws(() => new URL("http://"), /URL が正しくありません/);
equal(URL.canParse("http://h:99999/"), false);
equal(URL.parse("nope"), null);
u = new URL("http://h/p");
u.searchParams.set("名前", "山田 太郎");
u.searchParams.append("x", "1&2");
equal(u.href, "http://h/p?%E5%90%8D%E5%89%8D=%E5%B1%B1%E7%94%B0+%E5%A4%AA%E9%83%8E&x=1%262");
u.search = "?a=1";
equal(u.searchParams.get("a"), "1");
u.pathname = "日本/語";
u.hash = "top";
u.port = "8080";
equal(u.href, "http://h:8080/%E6%97%A5%E6%9C%AC/%E8%AA%9E?a=1#top");
equal(JSON.stringify({ u: new URL("http://h/") }), '{"u":"http://h/"}');

// ---- URLSearchParams
let p = new URLSearchParams("?a=1&b=x+y&a=2&c=%E3%81%82&d");
deepEqual(p.getAll("a"), ["1", "2"]);
equal(p.get("b"), "x y");
equal(p.get("c"), "あ");
equal(p.get("d"), "");
equal(p.get("none"), null);
equal(p.size, 5);
p.delete("a");
p.sort();
equal(p.toString(), "b=x+y&c=%E3%81%82&d=");
deepEqual([...new URLSearchParams({ k: 1, j: true })], [["k", "1"], ["j", "true"]]);
deepEqual([...new URLSearchParams([["x", "1"]]).keys()], ["x"]);

// ---- Headers
const h = new Headers({ "Content-Type": "text/plain", "X-A": " 1 " });
h.append("x-a", "2");
h.append("Set-Cookie", "a=1");
h.append("Set-Cookie", "b=2");
equal(h.get("content-type"), "text/plain");
equal(h.get("X-A"), "1, 2");
equal(h.has("x-b"), false);
deepEqual(h.getSetCookie(), ["a=1", "b=2"]);
deepEqual([...h], [["content-type", "text/plain"], ["set-cookie", "a=1"], ["set-cookie", "b=2"], ["x-a", "1, 2"]]);
h.set("x-a", "3");
equal(h.get("x-a"), "3");
h.delete("X-A");
equal(h.get("x-a"), null);
throws(() => h.set("bad name", "v"), /ヘッダーの名前/);
throws(() => h.set("x", "日本語"), /ヘッダーの値/);

// ---- AbortController
const ac = new AbortController();
let fired = 0;
ac.signal.addEventListener("abort", () => fired++);
ac.signal.onabort = () => fired++;
ac.abort();
ac.abort();
equal(fired, 2);
equal(ac.signal.aborted, true);
equal(ac.signal.reason.name, "AbortError");
throws(() => ac.signal.throwIfAborted(), /中止しました/);
equal(AbortSignal.abort("why").reason, "why");
equal(AbortSignal.any([new AbortController().signal, AbortSignal.abort(1)]).reason, 1);
// An unused timeout signal does not keep the script running (see the end).
const unused = AbortSignal.timeout(60000);
equal(unused.aborted, false);
throws(() => new AbortSignal(), /new で作れません/);

// ---- Request / Response without the network
const req = new Request("http://h/x", { method: "post", body: "本文", headers: { "X-Y": "1" } });
equal(req.method, "POST");
equal(req.headers.get("content-type"), "text/plain;charset=UTF-8");
equal(await req.clone().text(), "本文");
equal(await req.text(), "本文");
equal(req.bodyUsed, true);
await rejects(() => req.text(), /読み取られています/);
equal(new Request("http://h/", { method: "patch" }).method, "PATCH");
throws(() => new Request("http://h/", { body: "x" }), /GET には本文を付けられません/);
throws(() => new Request("relative"), /URL が正しくありません/);
throws(() => new Request("http://h/", { redirect: "nope" }), /redirect の値/);

let res = new Response('{"a":[1,2]}', { status: 201, headers: { "content-type": "application/json" } });
equal(res.ok, true);
equal(res.status, 201);
deepEqual(await res.json(), { a: [1, 2] });
res = Response.json({ x: 1 }, { status: 404 });
equal(res.ok, false);
equal(res.headers.get("content-type"), "application/json");
equal(await res.text(), '{"x":1}');
equal(Response.redirect("http://h/next", 301).headers.get("location"), "http://h/next");
equal(Response.error().type, "error");
throws(() => new Response("x", { status: 99 }), /ステータス/);
const blob = await new Response(new Uint8Array([1, 2, 3])).blob();
equal(blob.size, 3);
const fd = await new Response("a=1&b=%E3%81%82", { headers: { "content-type": "application/x-www-form-urlencoded" } }).formData();
equal(fd.get("b"), "あ");

// ---- fetch against the test server (src/net/testserver.rs)
const base = xljs.env("XLJS_TEST_HTTP");
ok(base, "XLJS_TEST_HTTP is set by the test");

res = await fetch(`${base}/echo?q=${encodeURIComponent("日本")}`, { headers: { "X-Test": "yes" } });
equal(res.status, 200);
equal(res.statusText, "OK");
equal(res.ok, true);
equal(res.url, `${base}/echo?q=%E6%97%A5%E6%9C%AC`);
equal(res.redirected, false);
let j = await res.json();
equal(j.method, "GET");
equal(j.headers["x-test"], "yes");
equal(j.headers.accept, "*/*");
equal(j.headers["user-agent"], "xljs/test");

// Bodies.
j = await (await fetch(`${base}/echo`, { method: "POST", body: JSON.stringify({ 名前: "山田" }), headers: { "Content-Type": "application/json" } })).json();
equal(j.method, "POST");
equal(j.headers["content-type"], "application/json");
deepEqual(JSON.parse(j.body), { 名前: "山田" });
j = await (await fetch(`${base}/echo`, { method: "PUT", body: new URLSearchParams({ a: "1", b: "あ" }) })).json();
equal(j.body, "a=1&b=%E3%81%82");
equal(j.headers["content-type"], "application/x-www-form-urlencoded;charset=UTF-8");
j = await (await fetch(`${base}/echo`, { method: "POST", body: new Uint8Array([0x41, 0x42]) })).json();
equal(j.body, "AB");
equal(j.length, 2);
const form = new FormData();
form.append("title", "報告");
form.append("file", new Blob(["中身"], { type: "text/plain" }), "メモ.txt");
j = await (await fetch(`${base}/echo`, { method: "POST", body: form })).json();
ok(j.headers["content-type"].startsWith("multipart/form-data; boundary="), j.headers["content-type"]);
ok(j.body.includes('Content-Disposition: form-data; name="title"\r\n\r\n報告\r\n'), j.body);
ok(j.body.includes('name="file"; filename="メモ.txt"\r\nContent-Type: text/plain\r\n\r\n中身\r\n'), j.body);
fs.writeText("upload.txt", "ファイルの中身", "utf8");
const fromDisk = new FormData();
fromDisk.append("f", await fs.file("upload.txt").getFile());
j = await (await fetch(`${base}/echo`, { method: "POST", body: fromDisk })).json();
ok(j.body.includes('filename="upload.txt"\r\nContent-Type: text/plain\r\n\r\nファイルの中身'), j.body);
j = await (await fetch(new Request(`${base}/echo`, { method: "DELETE" }))).json();
equal(j.method, "DELETE");
res = await fetch(`${base}/echo`, { method: "HEAD" });
equal(res.status, 200);
equal(await res.text(), "");

// Status codes are not errors.
res = await fetch(`${base}/status/404`);
equal(res.ok, false);
equal(res.status, 404);
equal(res.statusText, "Not Found");
equal(await res.text(), "status 404");
res = await fetch(`${base}/status/204`);
equal(res.status, 204);
equal((await res.arrayBuffer()).byteLength, 0);

// Text in the charset of Content-Type, or the one given.
equal(await (await fetch(`${base}/sjis`)).text(), "日本語のテキスト");
equal(await (await fetch(`${base}/chunked`)).text(), "こんにちは、世界");
deepEqual(await (await fetch(`${base}/json`)).json(), { a: 1, b: "あ" });
equal(new Uint8Array(await (await fetch(`${base}/sjis`)).arrayBuffer())[0], 0x93);
equal((await (await fetch(`${base}/sjis`)).bytes()).length, 16);
deepEqual((await fetch(`${base}/cookies`)).headers.getSetCookie(), ["a=1; Path=/", "b=2; Path=/"]);

// Redirects.
res = await fetch(`${base}/redirect/3`);
equal(res.redirected, true);
equal(res.url, `${base}/echo`);
equal((await res.json()).path, "/echo");
res = await fetch(`${base}/redirect/1`, { redirect: "manual" });
equal(res.status, 302);
equal(res.headers.get("location"), "/echo");
await rejects(() => fetch(`${base}/redirect/1`, { redirect: "error" }), /TypeError .*リダイレクトしました/);
await rejects(() => fetch(`${base}/redirect/25`), /リダイレクトが多すぎます/);
const to = (status) => `${base}/redirect-to?status=${status}&url=${encodeURIComponent("/echo")}`;
j = await (await fetch(to(303), { method: "POST", body: "x" })).json();
equal(j.method, "GET");
equal(j.length, 0);
j = await (await fetch(to(302), { method: "POST", body: "x" })).json();
equal(j.method, "GET");
j = await (await fetch(to(307), { method: "POST", body: "keep" })).json();
equal(j.method, "POST");
equal(j.body, "keep");

// Credentials in the URL become Basic authentication.
const authed = new URL(`${base}/auth`);
authed.username = "yamada";
authed.password = "p@ss";
equal(await (await fetch(authed)).text(), "Basic " + btoa("yamada:p@ss"));
equal((await fetch(`${base}/auth`)).status, 401);

// Aborting and time limits.
const slow = new AbortController();
setTimeout(() => slow.abort(), 100);
let t0 = Date.now();
let e = await rejects(() => fetch(`${base}/slow`, { signal: slow.signal }), /AbortError/);
ok(Date.now() - t0 < 2000, "aborted at once");
ok(e instanceof DOMException);
await rejects(() => fetch(`${base}/echo`, { signal: AbortSignal.abort() }), /AbortError/);
t0 = Date.now();
await rejects(() => fetch(`${base}/slow`, { signal: AbortSignal.timeout(100) }), /TimeoutError/);
e = await rejects(() => fetch(`${base}/slow`, { timeout: 100 }), /TimeoutError/);
ok(e.message.includes("100 ミリ秒以内に応答がありませんでした"), e.message);
ok(Date.now() - t0 < 2000, "timed out at once");
// A signal that never aborts and a time limit that is not reached.
equal((await fetch(`${base}/echo`, { signal: AbortSignal.timeout(60000), timeout: 60000 })).status, 200);

// Network errors are TypeErrors.
await rejects(() => fetch("ftp://example.com/"), /TypeError .*http と https/);
await rejects(() => fetch("http://127.0.0.1:1/"), /TypeError .*接続できません/);
await rejects(() => fetch("http://nonexistent.invalid/"), /TypeError .*見つかりません/);

console.log("fetch ok");
