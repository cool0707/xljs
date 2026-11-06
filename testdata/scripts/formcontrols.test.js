import { ok, equal, deepEqual, rejects } from "./_assert.js";

// The kinds of input added after the first ones (switch, checkboxes,
// segmented, combobox, slider, dateRange, tags, note), prefix/suffix,
// maxLength, and the buttons' menus, variants and confirm.
const fields = [
  { name: "notice", type: "note", variant: "warning", label: "注意", default: "元のファイルは変更しません" },
  { name: "on", type: "switch", label: "有効", default: true },
  { name: "depts", type: "checkboxes", options: ["営業", "総務", "経理"], default: ["経理"], required: true },
  { name: "unit", type: "segmented", options: ["日", "週", "月"], default: "週" },
  { name: "days", type: "segmented", multiple: true, options: [{ value: "1", label: "月" }, { value: "2", label: "火" }] },
  { name: "customer", type: "combobox", options: [{ value: "C01", label: "山田商店" }, { value: "C02", label: "佐藤工業" }] },
  { name: "free", type: "combobox", options: ["東京", "大阪"], allowCustom: true },
  { name: "rate", type: "slider", min: 0, max: 50, step: 5, suffix: "%" },
  { name: "period", type: "dateRange", default: { from: new Date(2026, 3, 1), to: "2026-04-30" } },
  { name: "words", type: "tags", default: ["見積"], pattern: "[^0-9]+", patternMessage: "数字は使えません" },
  { name: "price", type: "number", prefix: "¥", suffix: "円" },
  { name: "memo", maxLength: 5 },
];

// 1st answer: invalid values; 2nd: fixed.
const v = await ui.form({ title: "新しい項目", fields });
ok(!("notice" in v), "a note has no value");
equal(v.on, false);
deepEqual(v.depts, ["営業", "経理"], "checkboxes come in the order of the options");
equal(v.unit, "月");
deepEqual(v.days, ["1", "2"]);
equal(v.customer, "C02", "a combobox's label stands for its value");
equal(v.free, "名古屋", "allowCustom keeps what is typed");
equal(v.rate, 15);
ok(v.period.from instanceof Date && v.period.to instanceof Date);
equal(v.period.from.getTime(), new Date(2026, 3, 1).getTime());
equal(v.period.to.getTime(), new Date(2026, 4, 10).getTime());
deepEqual(v.words, ["見積", "請求"]);
equal(v.price, 1200);
equal(v.memo, "あいうえお", "maxLength counts characters");

// Defaults: a slider starts at its min, an empty dateRange is null, lists are [].
const d = await ui.form({
  title: "既定値",
  fields: [
    { name: "s", type: "slider" },
    { name: "r", type: "dateRange" },
    { name: "c", type: "checkboxes", options: ["a"] },
    { name: "t", type: "tags" },
    { name: "w", type: "switch" },
  ],
});
equal(d.s, 0);
equal(d.r, null);
deepEqual(d.c, []);
deepEqual(d.t, []);
equal(d.w, false);

// Groups that open and close keep their fields' values.
const g = await ui.form({
  title: "グループ",
  fields: [{ group: "詳細設定", collapsed: true, fields: [{ name: "x", default: "a" }] }],
});
equal(g.x, "a");

// Menus: the items come after their button ("label ▸ item", or the item's label);
// a menu button without onClick is not pressed itself. confirm asks once the values pass.
const clicks = [];
const r = await ui.form({
  title: "メニュー",
  fields: [{ name: "n", type: "number", required: true, default: 1 }],
  buttons: [
    {
      label: "保存", variant: "primary", onClick(values) { clicks.push("save:" + values.n); },
      menu: [
        { label: "名前を付けて保存", onClick(values) { clicks.push("saveAs:" + values.n); } },
        { label: "PDF", icon: "file-pdf", onClick() { clicks.push("pdf"); } },
      ],
    },
    { label: "その他", menu: [{ label: "削除", variant: "destructive", confirm: "削除しますか？", close: true, onClick() { clicks.push("delete"); } }] },
  ],
  onSubmit() { clicks.push("submit"); },
});
deepEqual(clicks, ["save:1", "saveAs:2", "pdf", "delete"]);
equal(r.n, 2);

// Bad definitions.
await rejects(() => ui.form({ fields: [{ name: "a", type: "checkboxes" }] }), /options/);
await rejects(() => ui.form({ fields: [{ name: "a", type: "note", variant: "loud" }] }), /loud/);
await rejects(() => ui.form({ fields: [], buttons: [{ label: "x", menu: [{ label: "y" }] }] }), /onClick/);
await rejects(() => ui.form({ fields: [], buttons: [{ label: "x", variant: "big", onClick() {} }] }), /big/);
console.log("formcontrols ok");
