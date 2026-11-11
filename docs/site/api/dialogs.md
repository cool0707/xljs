---
title: ダイアログ・ピッカー
api: alert, confirm, prompt, showOpenFilePicker, showSaveFilePicker, showDirectoryPicker, FilePickerOptions, OpenFilePickerOptions, SaveFilePickerOptions, DirectoryPickerOptions, FilePickerAcceptType, WellKnownDirectory, Notification, NotificationOptions, Ui, ui
---

| API | 戻り値 |
|---|---|
| `alert(msg)` / `confirm(msg)` / `prompt(msg, 既定値)` | 同期。`prompt` はキャンセルで `null` |
| `await showOpenFilePicker({ multiple, types, startIn, title })` | `FileSystemFileHandle[]`（キャンセルで `[]`） |
| `await showSaveFilePicker({ suggestedName, types, startIn })` | `FileSystemFileHandle \| null` |
| `await showDirectoryPicker({ startIn, title })` | `FileSystemDirectoryHandle \| null` |
| `ui.notify(msg, { title })` / `new Notification(title, { body })` | 画面右下に通知を出してすぐに戻る（コンソールでは 1 行表示） |

`types` は `[{ description: "Excel", accept: { "<MIME>": [".xlsx"] } }]` のほか、`accept: [".xlsx", ".xlsm"]` とも書けます。`showSaveFilePicker` で `types` を省略すると `suggestedName` の拡張子が種類になり、拡張子を消して入力しても付け足します（`集計` → `集計.xlsx`）。`startIn` には `"desktop"` `"documents"` `"downloads"`、パス、ハンドルを指定できます。

他のアプリで作業している間に `alert`・`confirm`・`prompt`・`ui.error`・ピッカー・フォームなどが開くと（Windows はダイアログを前面に出させないので、タスクバーが点滅するだけになる）、同じ内容の通知を画面右下に自動で出します。通知をクリックするとダイアログが前面に出ます。`ui.autoNotify = false` で止められます。alert・ピッカーなどの自動の通知は、そのダイアログを閉じると消えます。`ui.notify` の通知が表示中のままスクリプトが終わると、通知が閉じるまで（最大 10 秒）プロセスの終了を待ちます（自動の通知だけなら待ちません）。
