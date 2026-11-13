import { equal, rejects } from "./_assert.js";

// Images, rows of fields side by side, the toolbar and ui.forms with the mock:
// an image is not asked; the fields of a row and of the toolbar are asked
// like any other; a toolbar button is pressed by its label (or tooltip) and
// runs with the values as entered, then the form is asked again.
const png = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0]);
let gear = 0;
const v = await ui.form({
  title: "配置",
  toolbar: [
    { icon: "gear", tooltip: "設定", onClick(values, ctx) { gear++; equal(values.last, "", "not checked"); ctx.setValue("last", "山田"); } },
    { name: "dark", type: "switch", label: "ダーク" },
  ],
  fields: [
    { name: "logo", type: "image", default: png },
    { row: "氏名", fields: [{ name: "last", label: "姓", required: true }, { name: "first", label: "名" }] },
  ],
});
equal(gear, 1);
equal(v.last, "山田");
equal(v.first, "花子");
equal(v.dark, true);
equal("logo" in v, false, "an image has no value");

// ui.forms: each form in turn with the mock.
const r = await ui.forms({ forms: [{ label: "A", fields: [{ name: "a" }] }, { label: "B", fields: [{ name: "b" }] }] });
equal(r.map((x) => x && (x.a ?? x.b)).join(), "1,2");

// Errors.
await rejects(() => ui.form({ fields: [{ name: "i", type: "image", default: new Uint8Array([1, 2, 3]) }] }), /画像/);
await rejects(() => ui.form({ fields: [{ row: "r", fields: [{ row: "x", fields: [] }] }] }), /行「r」/);
await rejects(() => ui.form({ toolbar: [{ icon: "gear" }] }), /toolbar/);
await rejects(() => ui.forms({ forms: [] }), /ui.forms/);
await rejects(() => ui.forms({ side: "top", forms: [{}] }), /side/);
console.log("formlayout ok");
