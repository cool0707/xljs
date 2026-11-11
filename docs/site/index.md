---
title: 概要
description: JavaScript / TypeScript のスクリプトで Excel・CSV を処理する Windows 用のシングルバイナリ
---

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

## このサイトの読み方

- [インストールと実行](guide/run.md): ダウンロード、実行のしかた、コマンドとオプション
- [サンプル](guide/examples.md): 機能ごとのサンプルスクリプト
- [AI チャットでスクリプトを作る](guide/ai-chat.md): Claude・Gemini・Copilot などに書いてもらうための資料
- API: 左の目次から機能ごとのページへ。各ページの「リファレンス」は型定義（[`xljs.d.ts`](../../types/xljs.d.ts)）から作っているので、オプションの名前と型はここが正確です。上の検索欄で `ui.form` や `Worksheet.toPdf` のように引けます。

型定義は `xljs init` でスクリプトのフォルダに出力できます（[エディタの補完](guide/editor.md)）。
