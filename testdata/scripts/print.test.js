import { ok, equal, deepEqual, rejects } from "./_assert.js";

// book.print / sheet.print: the mock logs what would be printed (src/engine/tests.rs checks the log).
const printers = xljs.printers();
deepEqual(printers.map((p) => p.name), ["xljs test printer", "xljs simple printer"], "the mock's printers");
equal(printers[0].isDefault && printers[0].duplex && !printers[1].duplex, true, "default, duplex");

const pb = excel.open(path.join(xljs.scriptDir, "..", "xlsx", "print.xlsx"));
// A4 landscape (3 pages) and B5 (1 page): two jobs.
equal(await pb.print(), true, "printed");
equal(await pb.print({ printer: "xljs test printer", duplex: true, copies: 2, collate: false, color: false, mirrorMargins: true, title: "両面" }), true);
equal(await pb.sheet("印刷範囲").print({ duplex: "short" }), true, "a sheet");
await rejects(() => pb.print({ printer: "none" }), /none/, "unknown printer");
await rejects(() => pb.print({ duplex: "both" }), /duplex/, "bad duplex");
await rejects(() => pb.print({ copies: 0 }), /copies/, "bad copies");

// With the preview (the mock asks its settings as a form; print.test.answers.json).
equal(await pb.print({ preview: true }), true, "preview: duplex and copies");
equal(await pb.print({ preview: true }), false, "preview cancelled: nothing printed");
// The simple printer cannot print on both sides: duplex is left out.
equal(await pb.print({ preview: { settings: ["printer", "duplex", "color"] } }), true, "preview: another printer");
// printPreview with the printer's settings: options for print.
deepEqual(await pb.printPreview({ preview: { settings: ["copies", "color"] } }), { copies: 5, color: false }, "printPreview: the printer's settings");
// toPdf's preview never shows the printer's settings (nothing to ask here).
ok((await pb.toPdf("p.pdf", { preview: { settings: ["printer", "copies"] } })).endsWith("p.pdf"), "toPdf: no printer settings");

console.log("print ok");
