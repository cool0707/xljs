---
title: 全体・その他
api: Xljs, xljs, exit, sleep, Console, console, setTimeout, setInterval, setImmediate, clearTimeout, clearInterval, clearImmediate, queueMicrotask, structuredClone, atob, btoa, TextEncoder, TextDecoder, DOMException, ObjectConstructor, ArrayConstructor
---

- `console.log/error/table/time` など、`setTimeout` / `setInterval`、`sleep(ms)`
- `fs.readJSON` などのほか、`csv.read` / `workbooks.open` もパス・ハンドル・`File`・`Blob` を受け付けます
- `xljs.computerName`（コンピューター名） `xljs.userName`（ログオン中のユーザー名）。ドメインに参加していなくても使えます（AD の詳しい情報は `ad.me()`）
- `xljs.args` `xljs.script` `xljs.scriptDir` `xljs.version` `xljs.title`（ダイアログのタイトル。`@title` で変更可） `xljs.env(name)` `exit(code)`
- 補っている組み込み: `Array.fromAsync`, `structuredClone`, `TextEncoder` / `TextDecoder`（Shift_JIS 対応）, `atob` / `btoa`, `Blob` / `File`, `fetch` / `URL` / `URLSearchParams` / `AbortController`, 数値・日付の `toLocaleString`（日本語形式）, 簡易 `Intl`
