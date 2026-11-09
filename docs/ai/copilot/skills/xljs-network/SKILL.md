---
name: xljs-network
description: xljs の Web API（fetch・Request・Response・FormData・URL・AbortController。プロキシ・Windows 認証）と Active Directory（ad。ユーザー・コンピューター・グループの検索）のリファレンス。xljs のスクリプトで Web API を呼ぶ・AD を調べるときに使う。
---

# xljs API リファレンス: Web API と Active Directory

xljs の `fetch` と `ad` です。基本の API（ファイル・Excel・CSV・JSON・フォーム・表・日付など）はスキル `xljs-reference` にあります。ここに書いていない関数・オプションは存在しないものとして扱ってください。

## Web API（fetch）

ブラウザと同じ `fetch` で社内・社外の Web API を呼べる（Windows の WinHTTP を使う。プロキシは Windows の設定（PAC も）のまま、HTTPS の証明書は Windows の証明書ストアで確かめる）。

```js
// GET して JSON を受け取る
const res = await fetch("https://api.example.com/items?" + new URLSearchParams({ 分類: "部品", 件数: 100 }));
if (!res.ok) throw new Error(`取得に失敗しました: ${res.status} ${res.statusText}`);  // 404・500 では reject しない
const items = await res.json();

// JSON を POST する
const r2 = await fetch("https://api.example.com/orders", {
  method: "POST",
  headers: { "Content-Type": "application/json", Authorization: "Bearer " + xljs.env("API_TOKEN") },
  body: JSON.stringify({ 番号: "A001", 金額: 12000 }),
  timeout: 30000,                          // xljs 拡張: 30 秒で応答がなければ TimeoutError
});

// フォームとファイルを送る（multipart/form-data）
const form = new FormData();
form.append("title", "月次報告");
form.append("file", await fs.file("報告.xlsx").getFile());  // ディスクのファイル
await fetch("https://intra.example.com/upload", { method: "POST", body: form });

// ダウンロードして保存する
const file = await fetch("https://intra.example.com/files/master.xlsx");
const data = await file.arrayBuffer();
fs.writeBytes("master.xlsx", data);
const book = workbooks.open(data);         // 保存せずに開くこともできる
```

- `res.text()` は Content-Type の charset（`Shift_JIS` なども）で読む。`res.text("cp932")` で文字コードを指定できる（xljs 拡張）。本文は 1 回しか読めない（2 回読むなら先に `res.clone()`）。
- オプション: `method`、`headers`（オブジェクト・`Headers`）、`body`（文字列は UTF-8、`URLSearchParams` はフォーム、`FormData`、`Blob`、`ArrayBuffer`）、`signal`（`AbortController` で中止、`AbortSignal.timeout(ms)`）、`redirect`（`"follow"` 既定・`"manual"`・`"error"`）、`timeout`（ミリ秒、xljs 拡張）。
- 社内サーバーの Windows 認証（Negotiate / NTLM）はサインイン中のユーザーで自動で通る（イントラネットのサーバーだけ。ほかのサーバーにも使うなら `credentials: "include"`）。Cookie はその実行の間だけ保たれる。
- 認証付きのプロキシも通る（Windows 認証はそのまま、Basic 認証は初回にユーザー名とパスワードを尋ねて Windows の資格情報マネージャーに保存する）。プロキシの ID・パスワードをスクリプトに書かない。
- つながらない・サーバーが見つからない・証明書の問題は `TypeError`、中止は `AbortError`、時間切れは `TimeoutError` で reject する。ヘッダーの値に日本語は使えない（必要なら `encodeURIComponent`）。
- `URL` / `URLSearchParams` は URL の組み立てに使える（`new URL("/api/a", base)`、`url.searchParams.set("q", "日本語")`）。

## Active Directory（ad。ドメイン参加の Windows のみ）

サインイン中のユーザーの権限で読むだけ。結果のキーは LDAP の属性名（`sAMAccountName` `displayName` `mail` `department` `title` `manager` `memberOf` `lastLogonTimestamp` `whenCreated` など）。日時は `Date`（無期限・なしは `null`）、`memberOf` は識別名の配列。`type`（"user" / "computer" / "group" ...）、`enabled` `lockedOut` `passwordExpired` も付く。

| API | 説明 |
|---|---|
| `ad.available()` | ドメインに参加しているか |
| `xljs.computerName` / `xljs.userName` | コンピューター名 / ログオン中のユーザー名（AD なしでも使える） |
| `ad.me()` / `ad.computer()` | 自分 / このコンピューター |
| `ad.get(id)` | 1 件（ログオン名・識別名・UPN・メール・SID・名前。なければ `null`） |
| `ad.search("山田*")` | 名前・ログオン名・表示名・メールで検索（既定はユーザー） |
| `ad.search({ type, filter, disabled, lockedOut, expired, expiringDays, inactiveDays, passwordExpired, passwordNeverExpires, memberOf, recursive, searchBase, properties, limit })` | 条件はすべて AND。`type`: "user"（既定）/ "computer" / "group" / "all"。`filter`: `{ department: "営業部", title: ["課長", "部長"], mail: true }` か LDAP のフィルタ文字列。`inactiveDays: 90` は 90 日ログオンなし。`memberOf` はグループ名か識別名 |

