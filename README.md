# <img src="assets/xljs.png" width="40" align="top"> xljs

JavaScript / TypeScript で書いたスクリプトで Excel・CSV の集計やデータ変換を行う、Windows 用のシングルバイナリです。Excel のインストールは不要です。

```js
const dir = await showDirectoryPicker();
if (!dir) exit();

const rows = [];
for (const h of ui.withProgress(dir.walk("*.xlsx"), { title: "集計中" })) {
  const book = workbooks.open(h);
  for (const r of book.sheets["明細"]?.toObjects() ?? []) rows.push({ ...r, ファイル: h.relativePath });
}

const dest = await showSaveFilePicker({ suggestedName: "集計結果.xlsx" });
if (dest) {
  const out = workbooks.create();
  out.addSheet("明細").write(rows);
  out.save(dest);
  alert(`${rows.length} 件を集計しました`);
}
```

- **言語**: ES2023 相当（オプショナルチェーン、`??`、async/await、トップレベル await、クラスフィールドなど）と TypeScript。`import` でファイル分割可能
- **ファイル操作**: ブラウザの File System Access API 風（`showOpenFilePicker` / `FileSystemFileHandle` など）＋ フォルダ走査などの独自拡張
- **Excel**: xlsx の読み書き（[umya-spreadsheet](https://github.com/MathNya/umya-spreadsheet)）。値は数値・文字列・真偽値・`Date` で受け渡し
- **PDF**: Excel で印刷したときと同じページでの PDF 出力（WebView2）、PDF の結合・ページの削除・挿入・回転・文字の追加（Excel・Acrobat 不要）
- **CSV / JSON**: Shift_JIS(CP932) / UTF-8 の自動判定、BOM 付き UTF-8 での書き出し、JSON Lines
- **エクスプローラー連携**: 出力したファイルを選択した状態でエクスプローラーを開く、既定のアプリで開く
- **UI**: Windows ネイティブのダイアログ（メッセージボックス、ファイル選択、進捗ダイアログ）、入力フォーム・表・グラフ・HTML の画面（WebView2）。コンソールでも動作
- **その他**: SQLite（ファイルサーバーでの共有も）、Web API（`fetch`）、Active Directory、プログラムの実行、exe へのパック、VBA マクロ・数式の取り出し

## ドキュメント

**リファレンス: https://cool0707.github.io/xljs/**

- [インストールと実行](https://cool0707.github.io/xljs/guide/run.html) — ダウンロード、コマンドとオプション
- [API](https://cool0707.github.io/xljs/api/globals.html) — 機能ごとの説明と、型定義から作ったリファレンス
- [サンプル](examples/README.md)
- [AI チャットでスクリプトを作る](docs/ai/README.md) — Claude・Gemini・Copilot などに渡すプロンプトとリファレンス
- [開発者向けメモ](docs/development.md) — ビルド、テスト、ドキュメントの構成

## ダウンロード

[Releases](https://github.com/cool0707/xljs/releases) から `xljs-windows-x64.zip`（ARM は `xljs-windows-arm64.zip`）を取得し、`xljs.exe` にスクリプトをドラッグ＆ドロップして実行します。

## ライセンス

Apache License 2.0（[LICENSE](LICENSE)）。移植元の Go 版 xljs は MIT ライセンスです。

同梱のライブラリ: Day.js（MIT、`src/api/dayjs/LICENSE`）、Font Awesome Free（アイコン CC BY 4.0・フォント SIL OFL 1.1・CSS MIT、`src/ui/web/fontawesome/LICENSE.txt`）。
