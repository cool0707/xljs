import { ok, equal, deepEqual, rejects } from "./_assert.js";

// book.printPreview / sheet.printPreview and toPdf's preview option. The mock
// has no windows: the settings are asked as a form (printpreview.test.answers.json).
const pb = workbooks.open(path.join(xljs.scriptDir, "..", "xlsx", "print.xlsx"));

// 1: nothing changed: the options as given (the sheets' own settings are kept).
deepEqual(await pb.printPreview({ title: "x" }), { title: "x" }, "nothing changed");
// 2: the paper (the two sheets differ: A4 landscape and B5).
deepEqual(await pb.printPreview(), { paper: "A3" }, "paper");
// 3: cancelled.
equal(await pb.printPreview(), null, "cancelled");
// 4: fit to 2 pages wide, automatic tall.
deepEqual(await pb.printPreview({ scale: 50 }), { fitToWidth: 2, fitToHeight: 0 }, "scaling: fit replaces the scale");
// 5: Excel's narrow margins, portrait.
deepEqual(await pb.printPreview(), { margin: { top: 19.1, right: 6.4, bottom: 19.1, left: 6.4 }, landscape: false }, "margins and orientation");
// 6: the sheets printed.
deepEqual(await pb.printPreview(), { sheets: ["印刷範囲"] }, "sheets");
// 7: the settings chosen by the script; a sheet has no "sheets".
const one = pb.sheet("一覧");
deepEqual(await one.printPreview({}, { settings: ["blackAndWhite", "gridLines", "sheets"] }), { blackAndWhite: true }, "black and white");
// 8: custom margins and a scale.
deepEqual(await one.printPreview({ fitToPage: true }, { settings: true }),
  { margin: { top: 10, right: 5, bottom: 10, left: 5 }, scale: 80, skipBlankPages: "values", condense: 4, charts: false },
  "custom margins, scale, blank pages, condense, charts");
// Mirrored margins for printing on both sides.
deepEqual(await one.printPreview({}, { settings: ["mirrorMargins"] }), { mirrorMargins: true }, "mirror margins");
// No settings: nothing to ask.
deepEqual(await one.printPreview({ landscape: false }, { settings: false }), { landscape: false }, "no settings");
await rejects(() => pb.printPreview({}, { settings: ["paper", "color"] }), /color/, "unknown setting");
await rejects(() => pb.printPreview({}, { settings: "paper" }), /settings/, "settings not an array");

// 9: toPdf with a preview: the PDF with the settings chosen (the mock logs it).
const out = await pb.toPdf("preview.pdf", { preview: { settings: ["scaling"], ok: "出力" } });
ok(typeof out === "string" && out.endsWith("preview.pdf"), "toPdf resolves to the path");
// 10: cancelled: no PDF.
equal(await pb.toPdf("cancelled.pdf", { preview: true }), null, "toPdf cancelled");

console.log("printpreview ok");
