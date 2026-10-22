// 表表示（ui.table）のサンプル。
// フォルダ内のファイル一覧を表で表示し、選んだファイルの一覧を表示し直す。

const dir = await showDirectoryPicker({ title: "一覧を表示するフォルダ" });
if (!dir) exit();

const files = dir.walk().toArray().map((h) => {
  const st = h.stat();
  return { 名前: h.name, フォルダ: path.dirname(h.relativePath), サイズ: st?.size ?? 0, 更新日時: new Date(st?.mtimeMs ?? 0), ハンドル: h };
});

// selectable を付けると、選んだ行（元のオブジェクト）が返る。キャンセルは []。
const picked = await ui.table(files, {
  title: "処理するファイルを選択",
  columns: ["名前", "フォルダ", "サイズ", "更新日時"],
  selectable: "multiple",
});
if (picked.length === 0) exit();

// 表示のみ（ウィンドウを閉じるまでプロセスは残る）。
ui.table(picked.map((f) => ({ 名前: f.名前, パス: f.ハンドル.path })), { title: `${picked.length} 件を選択しました` });
