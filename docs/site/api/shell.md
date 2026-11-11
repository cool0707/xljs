---
title: エクスプローラー・プログラムの実行
api: Shell, shell, RunOptions, RunResult, ProcessIO, ChildProcess, ShortcutOptions, ShortcutDetails
---

## エクスプローラー・既定のアプリ

```js
shell.showItemInFolder(dest);                 // 保存したファイルを選択した状態でエクスプローラーを開く
shell.showItemInFolder([a, b, c]);            // 複数選択（フォルダが違えばフォルダごとに開く）
shell.openPath("集計.xlsx");                  // 既定のアプリ（Excel など）で開く
shell.openPath(dir);                          // フォルダを開く
shell.openExternal("https://example.com/");   // ブラウザで開く
```

パス・ハンドル・`File` のどれでも渡せます。mock UI では実際には開かず、ログに出すだけです。

## プログラムの実行

```js
// 終わるのを待つ（Promise）。終了コード・標準出力・標準エラーを受け取る
const r = await shell.run("robocopy", [src, dest, "/MIR", "/NP"]);
if (r.exitCode >= 8) throw new Error(r.stdout);
console.log(r.exitCode, r.ok, r.stdout, r.stderr);

// 引数の配列を渡さなければコマンドライン（cmd.exe /c。dir・パイプ・リダイレクトも使える）
const { stdout } = await shell.run("ipconfig /all");
const files = shell.runSync("dir /b *.csv").stdout.split(/\r?\n/).filter(Boolean);   // 同期版

// オプション
await shell.run("集計.exe", ["--month", "2026-04"], {
  cwd: "\\\\server\\共有\\集計",      // 作業フォルダー
  env: { TZ: "Asia/Tokyo" },                // 追加する環境変数（null で削除）
  window: "hidden",                         // "default"・"normal"・"hidden"・"minimized"・"maximized"
  input: "y\n",                             // 標準入力に書く内容
  timeout: 60_000,                          // ミリ秒。過ぎたら終わらせる（r.timedOut が true）
  encoding: "auto",                         // 出力の文字コード（既定 "auto": UTF-8 か CP932）。"buffer" なら Uint8Array
  onStdout: (line) => console.log(line),    // 出力の 1 行ごと（runSync でも呼ばれる）
});

// 待たずに起動する
const p = shell.spawn("notepad.exe", [file]);   // スクリプトが終わってもメモ帳はそのまま
console.log(p.pid, p.running);
p.kill();                                        // 終わらせる（それが起動したプログラムも）
const done = await p.wait();                     // 終わるのを待つ（結果は run と同じ）

// 標準入力に少しずつ書く
const sort = shell.spawn("sort", [], { stdin: "pipe" });
sort.write("b\na\n");
sort.end();
console.log((await sort.wait()).stdout);
```

- **引数の配列を渡すと** プログラムを直接起動します（空白や `"` を含む引数もそのまま渡ります。`.bat` / `.cmd` は cmd.exe 経由）。**渡さなければ** コマンドラインとして cmd.exe /c で実行します（既存のファイルのパスならそのまま起動）。`shell: true` / `false` で切り替えられます。
- 0 以外の終了コードでも例外にはなりません（`r.ok` か `r.exitCode` で判定）。プログラムが見つからないなど起動できないときは例外です。
- ウィンドウの既定（`"default"`）は、コンソールのプログラムはウィンドウを出さず（コンソール版 xljsc ではそのコンソールで動く）、メモ帳などはふつうに開きます。`"normal"` ではコンソールのプログラムも新しいウィンドウに表示します。
- 出力は既定で受け取ります（`stdout` / `stderr`: `"pipe"`）。`"inherit"` で xljs と同じ出力（コンソール版のコンソール）、`"ignore"` で捨てます。日本語の Windows のコマンドが書く CP932 も UTF-8 も既定の `"auto"` で読めます。
- `kill()` と `timeout` は、cmd.exe などが起動したプログラムもまとめて終わらせます（ジョブオブジェクト）。
- `runSync` は終わるまでスクリプトが止まります（その間は停止キーも効きません）。長くかかるものは `run` にして `ui.progress` などと組み合わせてください。

## ショートカット（.lnk・.url）

```js
// 作成（名前に .lnk がなければ付ける。URL なら .url のインターネットショートカット）
const desktop = fs.knownFolder("desktop");
shell.writeShortcutLink(path.join(desktop, "売上集計"), {
  target: "\\\\server\\共有\\売上.xlsx",   // リンク先（相対パスはカレントディレクトリから）
  cwd: "\\\\server\\共有",                // 作業フォルダー
  args: "",                                // 引数
  description: "毎月の売上集計",            // コメント
  icon: "%SystemRoot%\\System32\\shell32.dll", iconIndex: 21,
  windowState: "maximized",                // "normal"（既定）・"maximized"・"minimized"
  hotkey: "Ctrl+Alt+U",                    // ショートカットキー（"" でなし）
  runAsAdmin: false,                       // 管理者として実行
});
shell.writeShortcutLink(path.join(desktop, "社内ポータル"), { target: "https://portal.example.com/" });

// リンク先などの取得
const link = shell.readShortcutLink(path.join(desktop, "売上集計.lnk"));
console.log(link.target, link.args, link.cwd, link.description, link.hotkey);

// 編集: "update" は指定した項目だけ変える（"create" は作り直し、"replace" は既存のものを作り直す）
shell.writeShortcutLink(path.join(desktop, "売上集計.lnk"), "update", { target: "\\\\server2\\共有\\売上.xlsx" });

// フォルダ内のショートカットのリンク先を一覧にする
for (const h of fs.dir(desktop).walk({ glob: "*.lnk" })) console.log(h.name, shell.readShortcutLink(h).target);
```

Electron の `shell.readShortcutLink` / `shell.writeShortcutLink` と同じ形です（`windowState` `hotkey` `runAsAdmin` と .url は xljs の追加）。`writeShortcutLink` は書いたファイルのパスを返し、失敗すると例外を投げます。リンク先・アイコンの `%SystemRoot%` などの環境変数はそのまま保存され、読み取ると展開された値が返ります（Windows はアイコンの `C:\Windows\...` を `%SystemRoot%\...` として保存しますが、読み取ると元のパスになります）。
