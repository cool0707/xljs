---
title: 進捗表示
api: ProgressOptions, Progress, WithProgressOptions
---

```js
const p = ui.progress({ title: "処理中", total: files.length });
for (const [i, f] of files.entries()) {
  if (p.cancelled) break;
  p.update(i, f.name);
  // ...
}
p.close();

// まとめて書く場合。キャンセルされるとスクリプトが終了する（終了コード 1）ので、途中までの結果で先へ進まない
for (const f of ui.withProgress(files, { title: "処理中" })) { /* ... */ }
// キャンセル後も続けたいときは onCancel: "break"（ループが終わるだけ。ui.cancelled で判定）
for (const f of ui.withProgress(files, { title: "処理中", onCancel: "break" })) { /* ... */ }
for await (const h of ui.withProgress(dir.values())) { /* 総数不明ならマーキー表示 */ }
```

Windows では標準の進捗ダイアログ（IProgressDialog）、コンソールでは標準エラー出力にバーを表示します。
