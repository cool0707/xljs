import { ok, equal, throws, rejects } from "./_assert.js";

// book.toHtml / toPdf: the sheets as Excel would print them.
const wb = workbooks.create();
const ws = wb.addSheet("見積");
ws.write([["品名", "数量", "単価", "金額"], ["りんご", 3, 120, 360], ["みかん", 10, 30, 300]]);
ws.setValue("A5", "合計");
ws.setValue("D5", 660);
ws.style("A1:D1", { bold: true, background: "#DDEBF7", align: "center" });
ws.setValue("D6", -5);
ws.style("C2:D6", { format: "#,##0;[Red]-#,##0" });
ws.range("A1:D3").border({ outline: "medium", inside: "thin" });
ws.merge("A5:C5");
ws.setValue("F1", "長いテキストは右の空のセルにはみ出す");

const html = ws.toHtml();
ok(html.startsWith("<!doctype html>"), "a document");
equal(html.match(/class="xljs-page"/g).length, 1, "one page");
ok(html.includes("@page s0{size:595.276pt 841.89pt;"), "A4 portrait by default: " + html.match(/@page[^{]*\{[^}]*/));
ok(html.includes(">りんご<"), "values");
ok(html.includes(">360<"), "numbers");
ok(html.includes('colspan="3"'), "merged cells");
ok(/color:#FF0000/.test(html), "negative numbers in red");
// Borders are SVG lines over the table (stroked paths in the PDF, of the exact width), not CSS borders.
ok(/<svg class="xljs-lines" viewBox="0 0 [\d.]+ [\d.]+"/.test(html), "the lines of the borders");
ok(/<path d="[^"]*" style="stroke:#000000;stroke-width:1\.5;stroke-linecap:square"\/>/.test(html), "medium borders");
ok(/<path d="[^"]*" style="stroke:#000000;stroke-width:0\.75;stroke-linecap:square"\/>/.test(html), "thin borders");
ok(!/border-top:/.test(html), "no CSS borders");
ok(/background-color:#DDEBF7/i.test(html), "fill");
ok(html.includes("font-family:&quot;游ゴシック&quot;,&quot;Yu Gothic&quot;"), "the default font");

// Diagonal borders are SVG lines (vectors in the PDF), not CSS gradients.
const dg = wb.addSheet("斜め");
dg.setValue("A1", "x");
dg.range("B2").border({ diagonalDown: "thin" });
dg.range("C2").border({ diagonalUp: { style: "dashed", color: "#FF0000" }, diagonalDown: { style: "dashed", color: "#FF0000" } });
const dh = dg.toHtml();
ok(dh.includes('<svg class="xljs-diag"><line x1="0" y1="0" x2="100%" y2="100%" style="stroke:#000000;stroke-width:0.75pt"/></svg>'), "a diagonal border (a cell with only it is printed)");
ok(dh.includes('<line x1="0" y1="100%" x2="100%" y2="0" style="stroke:#FF0000;stroke-width:0.75pt;stroke-dasharray:3pt 1.5pt"/>'), "both diagonals, dashed");
ok(!/linear-gradient\(to top/.test(dh), "no gradients for the diagonals");
wb.removeSheet("斜め");

// Options: paper, orientation, scale, fit, margins, grid lines.
ok(ws.toHtml({ paper: "A3", landscape: true }).includes("size:1190.551pt 841.89pt"), "A3 landscape");
ok(ws.toHtml({ gridLines: true }).includes('style="stroke:#c8c8c8;stroke-width:0.5;stroke-linecap:square"') && !ws.toHtml().includes("#c8c8c8"), "grid lines");
ok(ws.toHtml({ margin: 10 }).includes("margin:28.346pt 28.346pt 28.346pt 28.346pt"), "margins in mm");
ok(ws.toHtml({ scale: 50 }).includes("zoom:0.5"), "scale");
ok(ws.toHtml({ scale: 50, zoom: 90 }).includes("zoom:0.45"), "zoom: on top of the scale");
throws(() => ws.toHtml({ zoom: 5 }), /zoom/);
ok(!html.includes("letter-spacing"), "condense: off by default");
ok(ws.toHtml({ condense: true }).includes("{letter-spacing:-0.04em}"), "condense: true is 4%");
const cd = ws.toHtml({ condense: 2.5 });
ok(cd.includes(".xljs-sheet td>div *,") && cd.includes("{letter-spacing:-0.025em}"), "condense: a percent");
ok(!ws.toHtml({ condense: 0 }).includes("letter-spacing"), "condense: 0 is off");
throws(() => ws.toHtml({ condense: 30 }), /condense/);
throws(() => ws.toHtml({ paper: "A9" }), /用紙のサイズ A9/);
throws(() => ws.toHtml({ scale: 1000 }), /scale/);
throws(() => wb.toHtml({ sheets: ["ない"] }), /シート ない/);
throws(() => wb.addSheet("空").toHtml(), /印刷する内容がありません/);

// Many rows and columns: pages down, then over; fitToWidth keeps one page wide.
const big = wb.addSheet("大きい表");
const rows = [];
for (let r = 0; r < 120; r++) rows.push(Array.from({ length: 30 }, (_, c) => r * 100 + c));
big.write(rows);
const pages = (h) => h.match(/class="xljs-page"/g).length;
const pp = ws.printPages();
equal(pp.sheet, ws.name, "printPages: sheet");
equal(pp.pages, pages(ws.toHtml()), "printPages: the same pages as toHtml");
equal(pp.across * pp.down, pp.pages, "printPages: across × down");
equal(ws.printPages({ scale: 50 }).scale, 50, "printPages: scale in percent");
equal(ws.printPages({ scale: 50, zoom: 90 }).scale, 45, "printPages: zoom");
equal(wb.addSheet("空2").printPages(), null, "printPages: nothing to print");
const bp = big.printPages();
ok(bp.across > 1 && bp.down > 1 && bp.pages === bp.across * bp.down, "printPages: pages across and down");
equal(big.printPages({ fitToWidth: 1 }).across, 1, "printPages: fitToWidth");
const all = big.toHtml();
ok(pages(all) > 4, "pages: " + pages(all));
const fit = big.toHtml({ fitToWidth: 1 });
ok(pages(fit) < pages(all), "fewer pages when fit to width");
equal(pages(big.toHtml({ fitToPage: true })), 1, "fit to one page");

// skipBlankPages: leave out pages with nothing (true) or with no values ("values").
const sparse = wb.addSheet("まばら");
sparse.setValue("A1", "先頭");
sparse.setValue("A200", "末尾");
sparse.range("A100").border({ outline: "thin" });
sparse.setValue("A150", "  ");
equal(pages(sparse.toHtml()), 6, "6 pages down");
const sk = sparse.toHtml({ skipBlankPages: true });
equal(pages(sk), 3, "skipBlankPages: true keeps the page with a border");
ok(sk.includes(">先頭<") && sk.includes(">末尾<"), "skipBlankPages: the values are kept");
equal(pages(sparse.toHtml({ skipBlankPages: "values" })), 2, "skipBlankPages: values leaves out the page with only a border");
const spp = sparse.printPages({ skipBlankPages: true });
equal([spp.pages, spp.across * spp.down, spp.areas[0].pages].join(), "3,6,3", "printPages: the pages printed");
equal(pages(sparse.toHtml({ skipBlankPages: false })), 6, "skipBlankPages: false");
throws(() => sparse.toHtml({ skipBlankPages: "x" }), /skipBlankPages/);

// Several sheets in one document, each with its own @page.
const both = wb.toHtml({ sheets: ["見積", "大きい表"] });
ok(both.includes("@page s0{") && both.includes("@page s1{"), "a page rule per sheet");
ok(both.includes("page:s1;"), "pages of the second sheet");

// Excel's charts: boxes where the charts are; toPdf draws them (the mock logs).
const book = workbooks.open(path.join(xljs.scriptDir, "..", "xlsx", "charts.xlsx"));
const sheet = book.sheets[0];
const ch = sheet.toHtml();
ok(ch.includes('class="xljs-print-chart" data-chart="0"'), "chart boxes");
ok(!sheet.toHtml({ charts: false }).includes("data-chart="), "charts: false");
const out = await sheet.toPdf(path.join(xljs.scriptDir, "charts.pdf"));
ok(out.endsWith("charts.pdf"), out);
await wb.toPdf("見積.pdf", { sheets: ["見積"] });

// The page setup of an Excel file (testdata/xlsx/make_print.py).
const pb = workbooks.open(path.join(xljs.scriptDir, "..", "xlsx", "print.xlsx"));
const ph = pb.toHtml();
equal(pages(ph), 4, "fit to 1 page wide (3 pages) and the print area (1 page); not the empty and hidden sheets");
ok(ph.includes("@page s0{size:841.89pt 595.276pt;"), "A4 landscape");
ok(ph.includes("@page s1{size:515.906pt 728.504pt;"), "B5 (JIS)");
ok(!ph.includes("@page s2") && !ph.includes("@page s3"), "no pages of the empty or the hidden sheet");
ok(ph.includes('@top-left{content:"売上一覧";') && ph.includes("font-weight:bold"), "header");
ok(ph.includes('@top-right{content:"一覧";'), "&A: the sheet name");
ok(ph.includes('@bottom-center{content:counter(page) " / " counter(pages) " ページ";'), "footer with page numbers");
ok(/align-items:center/.test(ph), "centered horizontally");
equal(ph.match(/>No</g).length, 3, "print titles on every page");
const area = pb.sheet("印刷範囲").toHtml();
ok(area.includes(">B2<") && area.includes(">D10<") && !area.includes(">A1<") && !area.includes(">E2<"), "the print area");
ok(ph.includes("<img") && ph.includes('data-chart="0"'), "picture and chart");
ok(!/>J\d/.test(ph), "hidden column");
equal(pages(pb.sheet("一覧").toHtml({ scale: 100 })) > 3, true, "options win over the sheet's fit to page");
ok(pages(pb.sheet("一覧").toHtml({ zoom: 50 })) < pages(pb.sheet("一覧").toHtml()), "zoom: on top of fit to page, fewer pages");
const pps = pb.printPages();
equal(pps.map((x) => x.sheet).join(","), "一覧,印刷範囲", "printPages: the printed sheets");
equal(pps.reduce((n, x) => n + x.pages, 0), pages(ph), "printPages: the same pages as toHtml");
equal(pps[0].across, 1, "printPages: fit to 1 page wide");
equal(pps[1].areas[0].range, "B2:D10", "printPages: the print area");
await pb.toPdf("print.pdf");

// Table styles, black and white, print areas by formulas (testdata/xlsx/make_tables.py).
const tb = workbooks.open(path.join(xljs.scriptDir, "..", "xlsx", "tables.xlsx"));
const th = tb.sheet("テーブル").toHtml();
ok(th.includes("background-color:#DDEBF7"), "header fill of the custom table style");
ok(th.includes("stroke:#808080;stroke-width:0.75;") && th.includes("stroke:#808080;stroke-width:0.25;"), "borders of the custom table style (thin, hair inside)");
ok(/font-weight:bold[^}]*color:#FFFFFF|color:#FFFFFF[^}]*font-weight:bold/.test(th) || th.includes("font-weight:bold;color:#FFFFFF"), "TableStyleMedium2: white bold header");
const mono = tb.sheet("白黒").toHtml();
ok(mono.includes(">塗り<") && !mono.includes("#FFC000"), "black and white: no fills");
ok(tb.sheet("白黒").toHtml({ blackAndWhite: false }).includes("#FFC000"), "blackAndWhite: false prints the colors");
ok(!tb.sheet("テーブル").toHtml({ blackAndWhite: true }).includes("#DDEBF7"), "blackAndWhite: true drops the colors");
const dyn = tb.sheet("可変範囲").toHtml();
ok(dyn.includes(">見出し<") && dyn.includes(">80<") && !dyn.includes(">外<"), "OFFSET print area: A1:C8");
equal(dyn.match(/<tr /g).length, 8, "OFFSET print area: 8 rows");
const cols = tb.sheet("列指定").toHtml();
equal(pages(cols), 1, "whole columns end at the used range");
equal(cols.match(/<tr /g).length, 10, "whole columns: 10 rows");

// Several fonts in one cell (testdata/xlsx/make_richtext.py).
const rb = workbooks.open(path.join(xljs.scriptDir, "..", "xlsx", "richtext.xlsx")).sheets[0];
equal(rb.getValue("B1"), "前インライン後", "inline rich text: the text of every run");
const rh = rb.toHtml();
const span = (t) => (rh.match(new RegExp(`<span style="([^"]*)">${t}</span>`)) ?? [])[1] ?? "";
ok(rh.includes("><span>通常<span"), "a run without properties has the cell's font");
ok(span("赤で大きく").includes("font-size:16pt") && span("赤で大きく").includes("color:#FF0000"), "red and larger: " + span("赤で大きく"));
ok(span("太字青").includes("font-family:&quot;ＭＳ Ｐゴシック&quot;") && span("太字青").includes("color:#0000FF"), "the font name is escaped: " + span("太字青"));
ok(span("細字").includes("font-weight:normal") && span("細字").includes("font-size:11pt"), "a run is not bold in a bold cell: " + span("細字"));
ok(span("下線").includes("text-decoration:underline") && span("取消").includes("text-decoration:line-through"), "underline and strike");
ok(span("2").includes("vertical-align:super") && span("斜体").includes("font-style:italic"), "superscript and italic");
ok(span("インライン").includes("font-size:18pt") && span("インライン").includes("color:#0070C0"), "inline rich text: " + span("インライン"));
ok(!rb.toHtml({ blackAndWhite: true }).includes("color:#FF0000"), "black and white: no colors of runs");

// Shrink to fit (the sheet 縮小 of richtext.xlsx).
const kh = workbooks.open(path.join(xljs.scriptDir, "..", "xlsx", "richtext.xlsx")).sheet("縮小").toHtml();
const zoom = (t) => Number((kh.match(new RegExp(`<span class="xljs-shrink" style="zoom:([0-9.]+)">(<span>)?${t}`)) ?? [])[1]);
ok(zoom("とても長い") > 0.15 && zoom("とても長い") < 0.35, "long text is shrunk: " + zoom("とても長い"));
equal(zoom("短い"), 1, "short text keeps its size");
ok(zoom("1,234,567,890,123") > 0.35 && zoom("1,234,567,890,123") < 0.6, "numbers are shrunk: " + zoom("1,234,567,890,123"));
ok(!/<div data-n[^>]*><span class="xljs-shrink"/.test(kh), "a shrunk number is not shown as ###");
ok(zoom("前") > 0.15 && zoom("前") < 0.35, "rich text is shrunk by its larger runs: " + zoom("前"));
ok(!/xljs-shrink[^>]*>折り返す/.test(kh), "wrapped text is not shrunk");
ok(/width:42pt;height:15pt"><span class="xljs-shrink" style="zoom:[0-9.]+">とても/.test(kh), "shrunk text does not run into the next cell");
// Set by a script, and kept when the book is saved and opened again.
const sb = workbooks.create();
const ss = sb.addSheet("s");
ss.setColumnWidth("A", 8);
ss.setValue("A1", "スクリプトで縮小する長い文字列");
ss.style("A1", { alignment: { shrink: true } });
ok(/xljs-shrink[^>]*>スクリプト/.test(ss.toHtml()), "shrink set by a script is printed");
const kb = workbooks.open(path.join(xljs.scriptDir, "..", "xlsx", "richtext.xlsx"));
kb.save("shrink-saved.xlsx");
const kh2 = workbooks.open("shrink-saved.xlsx").sheet("縮小").toHtml();
ok(/xljs-shrink[^>]*>とても長い/.test(kh2), "shrink to fit survives saving");
equal(workbooks.open("shrink-saved.xlsx").sheets[0].getValue("B1"), "前インライン後", "inline rich text survives saving");

// Shapes, text boxes, lines, groups and a freeform (testdata/xlsx/make_shapes.py).
const sh = workbooks.open(path.join(xljs.scriptDir, "..", "xlsx", "shapes.xlsx")).sheets[0].toHtml();
for (const t of ["テキストボックスの文字", "太字で赤", "角丸", "楕円", "ここに注意", "四角", "ひし形", "縦書きの文字", "雲の吹き出し"]) ok(sh.includes(`>${t}<`), t);
ok(!sh.includes("見えない") && !sh.includes(">印刷しない<"), "hidden shapes and shapes not printed");
equal(sh.match(/class="xljs-print-shape"/g).length, 20, "top-level objects");
ok(sh.includes('fill="#70AD47"'), "own fill");
ok(sh.includes('fill="#4F81BD"'), "theme fill by style (accent1 of the workbook's theme, Office 2007 in openpyxl)");
ok(sh.includes("marker-end=") && sh.includes("marker-start="), "arrowheads");
ok(sh.includes('stroke-dasharray="4 3"'), "dashed line");
ok(sh.includes("writing-mode:vertical-rl"), "vertical text");
ok(sh.includes("transform:rotate(20deg)"), "rotation");
ok(/<path d="M[^"]*C[^"]*" fill="none"/.test(sh), "freeform");
ok(/<path d="M[^"]*C[^"]*" fill="#FF0000"/.test(sh), "a heart (presets from their definitions)");
ok(sh.includes('fill="#CC9A00"'), "the eyes of a smiley face: a darker shade of its fill (darkenLess)");
// WordArt: text effects and text warps (laid out by the print page).
for (const t of ["影付きの文字", "アーチの文字", "WAVE", "二行目", "反射", "円に沿った文字の並び", "spaced caps"]) ok(sh.includes(`>${t}<`), t);
ok(sh.includes("filter:drop-shadow(2.121pt 2.121pt 1.5pt rgba(0,0,0,0.4))"), "a shadow of the text");
ok(/filter:drop-shadow\(0 0 4pt rgba\(192,80,77,0\.4\)\) drop-shadow\(0 0 4pt/.test(sh), "a glow (accent2 of the theme)");
ok(sh.includes("background-image:linear-gradient(180deg,#FFC000 0%,#C00000 100%);-webkit-background-clip:text"), "a gradient fill of the text");
ok(sh.includes("-webkit-text-stroke:1pt #002060;"), "an outline");
ok(sh.includes("color:transparent;-webkit-text-stroke:1.5pt #7030A0;"), "an outline only");
ok(sh.includes("-webkit-box-reflect:below 1pt linear-gradient(rgba(0,0,0,0.53) 0%,rgba(0,0,0,0.003) 35.5%,transparent 35.5%)"), "a reflection");
ok(sh.includes("text-transform:uppercase;letter-spacing:3pt;"), "capitals and spacing");
const warps = [...sh.matchAll(/class="xljs-warp" data-warp="([^"]*)"/g)].map((m) => JSON.parse(m[1].replace(/&quot;/g, '"')));
equal(warps.map((w) => w.paths.length).join(","), "1,2,2,1", "arch up, wave, inflate, circle");
ok(warps.every((w) => w.w > 0 && w.h > 0 && w.paths.every((p) => p.length >= 4 && p.every(Number.isFinite))), "the paths of the warps");
// Pictures placed in cells and cells that refer to them (testdata/xlsx/make_cellimages.py).
const ci = workbooks.open(path.join(xljs.scriptDir, "..", "xlsx", "cellimages.xlsx"));
const srcs = (h) => [...h.matchAll(/<img class="xljs-cell-image" alt="([^"]*)" src="(data:[^"]+)" style="([^"]*)"/g)].map((m) => ({ alt: m[1], src: m[2], pos: m[3] }));
const goods = srcs(ci.sheets[0].toHtml());
equal(goods.length, 5, "B2, D2 (=B2), B3, D3 (=B3), B4");
ok(goods[0].src.startsWith("data:image/png;base64,") && goods[2].src.startsWith("data:image/jpeg;base64,"), "PNG and JPEG");
equal(goods[1].src, goods[0].src, "=B2 shows the picture of B2");
equal(goods[3].src, goods[2].src, "=B3 shows the picture of B3");
equal(goods[4].alt, "ぶどうの写真", "alternative text");
equal(goods[0].pos, "object-position:center bottom", "centered at the bottom by default");
equal(goods[4].pos, "object-position:left top", "the cell's alignment");
ok(!ci.sheets[0].toHtml().includes("#VALUE!"), "no #VALUE! for pictures");
const refs = ci.sheets[1];
refs.cell("C2").formula = "=商品!B2";
refs.cell("D2").formula = '=IF(1>0,商品!B3,"")';
refs.cell("E2").formula = '=XLOOKUP("ぶどう",商品!A2:A4,商品!B2:B4)';
refs.cell("F2").formula = '=VLOOKUP("みかん",商品!A2:B4,2,FALSE)';
refs.cell("G2").formula = "=INDEX(商品!B2:B4,1)";
refs.cell("H2").formula = '=商品!B2&""';
const [img1, img2, img3] = [goods[0].src, goods[2].src, goods[4].src];
let got = srcs(refs.toHtml()).map((x) => x.src);
equal(got.length, 7, "references from another sheet");
ok(got[0] === img2 && got[1] === img1, "saved by Excel (vm) and saved without vm");
ok(got[2] === img1 && got[3] === img2 && got[4] === img3 && got[5] === img2 && got[6] === img1, "formulas written by the script: =, IF, XLOOKUP, VLOOKUP, INDEX");
ok(refs.toHtml().includes(">#VALUE!<"), "a picture used as text is #VALUE!");
equal(refs.cell("C2").calc(), "#VALUE!");
// A value written over a picture is no picture, also where it is referred to.
ci.sheets[0].setValue("B3", "売り切れ");
ok(ci.sheets[0].toHtml().includes(">売り切れ<") && srcs(ci.sheets[0].toHtml()).length === 4, "overwritten picture");
got = srcs(refs.toHtml()).map((x) => x.src);
ok(refs.toHtml().includes(">売り切れ<") && got.length === 5, "formulas that refer to it show the value");
await ci.toPdf("cellimages.pdf");

await xljs.htmlToPdf("<h1>こんにちは</h1>", "hello.pdf", { paper: "A5", landscape: true });
throws(() => xljs.htmlToPdf(1, "x.pdf"), /HTML は文字列/);
console.log("pdf ok");
