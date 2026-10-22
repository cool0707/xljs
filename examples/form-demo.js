// 簡易フォーム（ui.form）のサンプル。
// フォルダ内の Excel ファイルの指定したシートを集計して 1 つのブックに書き出す。
// onSubmit を指定すると、ウィンドウを開いたまま進捗とログを表示する。

const result = await ui.form({
  title: "売上集計",
  description: "フォルダ内の Excel ファイル（サブフォルダを含む）の明細シートを、顧客名か商品名ごとに件数と金額の合計にまとめます。",
  fields: [
    { name: "src", label: "集計フォルダ", type: "folder", required: true },
    { name: "out", label: "出力先", type: "saveFile", accept: [".xlsx"], default: "集計結果.xlsx", required: true },
    { name: "sheet", label: "シート名", default: "明細", required: true },
    { name: "key", label: "集計単位", type: "select", options: ["顧客名", "商品名"], default: "顧客名" },
    { name: "open", label: "完了後に出力したブックを開く", type: "checkbox" },
  ],
  submit: "集計する",
  remember: true, // 前回の入力を覚えておく
  onSubmit: async (v, ctx) => {
    const files = v.src.walk({ glob: "*.xlsx", exclude: ["~$*"] }).toArray();
    const totals = new Map();
    for (const [i, h] of files.entries()) {
      if (ctx.cancelled) return;
      ctx.progress(i, files.length, h.relativePath);
      const rows = workbooks.open(h).sheets[v.sheet]?.toObjects() ?? [];
      for (const r of rows) {
        const k = String(r[v.key] ?? "(なし)");
        const t = totals.get(k) ?? { [v.key]: k, 件数: 0, 合計: 0 };
        t.件数++;
        t.合計 += Number(r.金額) || 0;
        totals.set(k, t);
      }
      ctx.log(`${h.relativePath}: ${rows.length} 行`);
    }
    ctx.progress(files.length, files.length, "");
    const book = workbooks.create();
    book.addSheet("集計").write([...totals.values()], { table: true });
    book.save(v.out);
    ctx.log(`${totals.size} 件を ${v.out.name} に書き出しました`);
    if (v.open) shell.openPath(v.out);
    return v.out; // 完了後に「ファイルの場所を開く」ボタンが出る
  },
});
if (!result) console.log("キャンセルされました");
