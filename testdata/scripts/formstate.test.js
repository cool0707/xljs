import { equal, rejects } from "./_assert.js";

// disabled / hidden (true or a function of the values) and ctx.setDisabled /
// setHidden: such fields are not checked and the console does not ask them.
const fields = () => [
  { name: "useMail", type: "checkbox", default: false },
  { name: "to", required: true, default: "a@example.jp", disabled: (v) => !v.useMail },
  { name: "kind", type: "select", options: ["a", "その他"], default: "a" },
  { name: "detail", required: true, hidden: (v) => v.kind !== "その他" },
  { name: "fixed", required: true, disabled: true },
  { name: "lock", type: "switch", onChange(on, v, ctx) { ctx.setDisabled("memo", on); ctx.setVisible("extra", on); } },
  { name: "memo", default: "初期" },
  { name: "extra", type: "number", required: true, hidden: true },
];

// 1st: the answers for disabled and hidden fields are not taken; required
// does not apply to them.
let v = await ui.form({ title: "状態", fields: fields() });
equal(v.to, "a@example.jp", "a disabled field keeps its value");
equal(v.detail, "", "a hidden field is not asked");
equal(v.fixed, "");
equal(v.memo, "初期", "disabled by an earlier onChange");
equal(v.extra, 7, "shown by an earlier onChange, then asked");

// 2nd: enabled and shown by the values answered, so checked (then fixed by the 3rd answer).
v = await ui.form({ title: "状態", fields: fields() });
equal(v.to, "b@example.jp");
equal(v.detail, "詳細");

// ctx.setDisabled names a field.
await rejects(() => ui.form({ title: "x", fields: [{ name: "a", onChange(_, v, ctx) { ctx.setDisabled("nope"); } }] }), /nope/);
console.log("formstate ok");
