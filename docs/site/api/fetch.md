---
title: Web API（fetch）
api: fetch, RequestInit, Request, Response, ResponseInit, Body, BodyInit, Headers, HeadersInit, FormData, URL, URLSearchParams, AbortController, AbortSignal
---

```js
const res = await fetch("https://api.example.com/items?" + new URLSearchParams({ q: "部品" }));
if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);   // 404・500 では reject しない
const items = await res.json();
await fetch(url, { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(data), timeout: 30000 });
const form = new FormData();
form.append("file", await fs.file("報告.xlsx").getFile());        // multipart/form-data
await fetch(url, { method: "POST", body: form });
fs.writeBytes("master.xlsx", await (await fetch(fileUrl)).arrayBuffer());
```

- ブラウザの `fetch`・`Request`・`Response`・`Headers`・`FormData`・`URL`・`URLSearchParams`・`AbortController`（`AbortSignal.timeout`）と同じ使い方です。
- Windows では WinHTTP で通信します。プロキシは Windows の設定（自動構成スクリプト（PAC）も）、HTTPS の証明書は Windows の証明書ストアで確かめ、gzip は自動で展開します。社内サーバーの Windows 認証（Negotiate / NTLM）はサインイン中のユーザーで通ります（既定はイントラネットのサーバーだけ。`credentials: "include"` でどのサーバーにも、`"omit"` で使わない）。Cookie はその実行の間だけ保ちます。
- **認証付きのプロキシ**: Windows 認証（Negotiate / NTLM）のプロキシはサインイン中のユーザーで通ります（`credentials` に関係なく）。Basic 認証のプロキシは、初回にユーザー名とパスワードを Windows の資格情報ダイアログで尋ね、通ったものを Windows の資格情報マネージャー（「xljs:Basic:<realm>」の汎用資格情報）に保存して、次からは尋ねずに使います。違っていたら（407 がまた返ったら）もう一度尋ねます。保存したものは HTML の画面（WebView2。アプリモード・開発モードの AI チャットの枠）の Basic 認証にも使い、そちらで入力したものも `fetch` で使います。ただし Windows の設定にプロキシがないときは、HTML の画面の Basic 認証はサイトのものとして扱い、そのサイト（ホスト）だけに保存・使用します（「xljs:Basic:<ホスト>:<realm>」）。プロキシがあるときも、そのホスト用に保存したものがあればそちらを先に使います。プロキシがない環境では 407 にも答えません（サインイン中のユーザーも送りません）。消すときはコントロール パネルの「資格情報マネージャー」→「Windows 資格情報」から削除します。
- Windows のプロキシ設定の代わりに使うプロキシは環境変数 `XLJS_PROXY`（`host:port`）で指定できます。
- **User-Agent**: 既定ではこのパソコンに入っている Edge と同じ User-Agent（版はレジストリから読みます）を送るので、ブラウザ以外を断るサイトにもつながります。`xljs.exe` と同じフォルダの `xljs.json` の `userAgent`（または環境変数 `XLJS_USER_AGENT`）で選べます: `"edge"`（既定）・`"chrome"`（Chrome が入っていなければ Edge）・`"xljs"`（`xljs/<版>`）・それ以外の文字列はそのまま送ります。開発モードの AI チャットの枠にも同じものを使います。リクエストの `headers` に `User-Agent` を書けばそちらが優先です。

  ```json
  { "userAgent": "chrome" }
  ```
- xljs の拡張: `timeout`（ミリ秒。リダイレクトを含めて応答を待つ上限）、`res.text(encoding)`（省略時は Content-Type の charset。`Shift_JIS` なども読めます）。リダイレクトは 20 回まで自動でたどり、`redirect: "manual"` なら 3xx の応答をそのまま返します。
- 本文のストリーム（`res.body`）、`XMLHttpRequest`、`WebSocket` はありません。Windows 以外（開発用）では http だけ使えます。
