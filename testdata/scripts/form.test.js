import { ok, equal, deepEqual, rejects } from "./_assert.js";

// Answers come from form.test.answers.json (the mock UI merges each answer
// over the current values; null cancels).
fs.mkdir("data");
fs.writeText("data/a.xlsx", "");
fs.writeText("note.txt", "");

const fields = [
  { name: "src", label: "集計フォルダ", type: "folder", required: true },
  { name: "out", label: "出力先", type: "saveFile", accept: [".xlsx"], default: "集計.xlsx" },
  { name: "mode", label: "集計単位", type: "select", options: ["顧客別", "商品別"], default: "顧客別" },
  { name: "tax", label: "税込で集計", type: "checkbox" },
  { name: "n", label: "件数", type: "number", min: 1, max: 10 },
  { name: "files", type: "files", accept: ".xlsx" },
  { name: "code", pattern: "[A-Z]{3}" },
  { name: "day", type: "date", default: new Date(2026, 3, 1) },
];
let validated = 0;
const v = await ui.form({
  title: "売上集計",
  fields,
  validate: (vals) => {
    validated++;
    return vals.mode === "商品別" && !vals.tax ? { tax: "商品別は税込のみ" } : null;
  },
});
ok(v, "submitted");
ok(v.src instanceof FileSystemDirectoryHandle);
equal(v.src.name, "data");
ok(v.out instanceof FileSystemFileHandle);
equal(v.out.name, "集計.xlsx");
equal(v.mode, "商品別");
equal(v.tax, true);
equal(v.n, 5);
deepEqual(v.files.map((h) => h.name), ["a.xlsx"]);
equal(v.code, "ABC");
equal(v.day, "2026-04-01");
equal(validated, 2, "validate runs only after the built-in checks pass");

equal(await ui.form({ fields: ["x"] }), null, "cancel");

// onSubmit runs right away without a window.
const logs = [];
const r = await ui.form({
  fields: [{ name: "name", required: true }],
  onSubmit: async (vals, ctx) => {
    ctx.progress(1, 2, "途中");
    ctx.log("処理", vals.name);
    logs.push(ctx.cancelled);
  },
});
equal(r.name, "太郎");
deepEqual(logs, [false]);

// Remembered values come back as defaults.
await ui.form({ title: "記憶", remember: true, fields: ["a", { name: "pw", type: "password" }] });
const again = await ui.form({ title: "記憶", remember: true, fields: ["a", { name: "pw", type: "password" }] });
equal(again.a, "前回の値");
equal(again.pw, "", "passwords are not remembered");

await rejects(() => ui.form({ fields: [] }), /fields/);
await rejects(() => ui.form({ fields: [{ name: "a", type: "color" }] }), /color/);
await rejects(() => ui.form({ fields: [{ name: "a", type: "select" }] }), /options/);
await rejects(() => ui.form({ fields: ["a", "a"] }), /重複/);
console.log("form ok");
