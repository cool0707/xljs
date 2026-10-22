// ==xljs==
// @mode   app
// @html   ui/index.html
// @assets ui/
// @title  はじめてのアプリ
// @size   640x420
// ==/xljs==

// @mode app では、スクリプトの実行前にメインウィンドウ（ui/index.html）が開きます。
// ウィンドウを閉じるとスクリプトが終了します。
// 画面とスクリプトのやり取り（invoke / emit / state など）は今後追加予定です。

await app.main.closed;
console.log("ウィンドウが閉じられました");
