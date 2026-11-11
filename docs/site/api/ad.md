---
title: Active Directory（ad）
api: AD, ad, ADObject, ADObjectType, ADFilter, ADConnectOptions, ADGetOptions, ADSearchOptions
---

ドメインに参加している Windows で、サインイン中のユーザーの権限でディレクトリを検索します（RSAT の ActiveDirectory モジュールは不要）。PowerShell の `Get-ADUser` / `Get-ADComputer` / `Get-ADGroup` / `Search-ADAccount` に当たるものを `ad.search` 1 つで行います。

```js
if (!ad.available()) { alert("このパソコンはドメインに参加していません"); exit(); }
ad.me();                                          // 自分（displayName, mail, department, memberOf ...）
ad.computer();                                    // このコンピューター
ad.domain();                                      // { name: "corp.example.com", netbiosName: "CORP", server, ... }
ad.get("yamada");                                 // 1 件（ログオン名・識別名・UPN・メール・SID・名前。なければ null）

ad.search("山田*");                               // 名前・ログオン名・表示名・メールで検索（既定はユーザー）
ad.search({ filter: { department: "営業部", title: ["課長", "部長"] } });   // Get-ADUser -Filter
ad.search({ filter: "(employeeID=00*)" });        // LDAP のフィルタも可
ad.search({ filter: { department: ad.me().department }, disabled: false }); // 自分の部署の有効なユーザー
ad.search({ disabled: true });                    // Search-ADAccount -AccountDisabled
ad.search({ lockedOut: true });                   // Search-ADAccount -LockedOut
ad.search({ expired: true });                     // 有効期限切れのアカウント
ad.search({ expiringDays: 30 });                  // 30 日以内に有効期限が切れる
ad.search({ passwordExpired: true });             // パスワードの有効期限切れ
ad.search({ type: "computer", inactiveDays: 90 }); // 90 日ログオンのないコンピューター（Get-ADComputer）
ad.search({ memberOf: "営業部", recursive: true }); // グループのメンバー（入れ子のグループも）
ad.search({ type: "group", searchBase: "OU=本社,DC=corp,DC=example,DC=com" });
ad.search({ properties: ["extensionAttribute1"] }); // 既定にない属性も読む（"*" ですべて）

sheet.write(ad.search({ disabled: true }).map((u) => ({ 名前: u.displayName, ログオン名: u.sAMAccountName, 最終ログオン: u.lastLogonTimestamp })));
```

- 結果のキーは LDAP の属性名です（`sAMAccountName`、`displayName`、`whenCreated` など）。日時は `Date`（無期限・なしは `null`）、SID・GUID は文字列、`memberOf` などは配列です。`enabled` `lockedOut` `passwordExpired` `passwordNeverExpires`（アカウント）、`groupScope` `groupCategory`（グループ）も付きます。
- 条件はすべて AND です。オプションの綴りを間違えるとエラーになります。
- 読み取りだけです（変更はできません）。ほかのドメインは `server` で指定します（`{ server: "dc01.example.com" }`）。
