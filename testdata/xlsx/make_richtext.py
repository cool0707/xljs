"""Builds richtext.xlsx: cells with several fonts in one cell (rich text) for
the tests of the print layout (src/api/xlsx_print.rs) and of reading inline
strings (src/api/xlsx.rs). Run with openpyxl: python3 testdata/xlsx/make_richtext.py

openpyxl writes rich text as inline strings (t="inlineStr"); Excel writes
shared strings with every property of a run's font. A1:A3 are moved to the
shared strings the way Excel writes them; B1 stays an inline string."""
import io, os, re, zipfile
from openpyxl import Workbook
from openpyxl.cell.rich_text import CellRichText, TextBlock
from openpyxl.cell.text import InlineFont
from openpyxl.styles import Font

wb = Workbook()
ws = wb.active
ws.title = "書式"
ws["A1"] = "placeholder"
ws["A2"] = "placeholder"
ws["A2"].font = Font(name="ＭＳ Ｐゴシック", size=14, bold=True, color="FF00B050")
ws["A3"] = "placeholder"
ws["B1"] = CellRichText("前", TextBlock(InlineFont(rFont="ＭＳ 明朝", sz=18, color="FF0070C0", u="single"), "インライン"), "後")
ws.column_dimensions["A"].width = 30
ws.column_dimensions["B"].width = 30

def run(text, sz=11, font="游ゴシック", color='<color theme="1"/>', extra=""):
    return (f'<r><rPr>{extra}<sz val="{sz}"/>{color}<rFont val="{font}"/><family val="3"/>'
            f'<charset val="128"/><scheme val="minor"/></rPr><t xml:space="preserve">{text}</t></r>')

shared = [
    # A1: plain text, then red and larger, then plain again (the first run has no properties).
    '<t xml:space="preserve">通常</t>'.replace("<t", "<r><t", 1).replace("</t>", "</t></r>")
    + run("赤で大きく", sz=16, color='<color rgb="FFFF0000"/>') + run("戻る"),
    # A2: the cell is bold green ＭＳ Ｐゴシック 14pt; the runs say otherwise.
    run("細字", sz=11, font="ＭＳ Ｐゴシック") + run("太字青", sz=14, font="ＭＳ Ｐゴシック", color='<color rgb="FF0000FF"/>', extra="<b/>"),
    # A3: underline, strike, superscript, italic, a theme color.
    run("下線", extra="<u/>") + run("取消", extra="<strike/>") + run("m") + run("2", extra='<vertAlign val="superscript"/>')
    + run("斜体", extra="<i/>", color='<color theme="5"/>'),
]

buf = io.BytesIO()
wb.save(buf)
src = zipfile.ZipFile(buf)
files = {n: src.read(n) for n in src.namelist()}
sheet = files["xl/worksheets/sheet1.xml"].decode()
for i, ref in enumerate(["A1", "A2", "A3"]):
    sheet, k = re.subn(rf'<c r="{ref}"([^>]*?) t="inlineStr"><is>.*?</is></c>', rf'<c r="{ref}"\1 t="s"><v>{i}</v></c>', sheet)
    assert k == 1, ref
files["xl/worksheets/sheet1.xml"] = sheet.encode()
files["xl/sharedStrings.xml"] = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
    f'<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="{len(shared)}" uniqueCount="{len(shared)}">'
    + "".join(f"<si>{s}</si>" for s in shared) + "</sst>").encode()
files["[Content_Types].xml"] = files["[Content_Types].xml"].decode().replace(
    "</Types>", '<Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/></Types>').encode()
files["xl/_rels/workbook.xml.rels"] = files["xl/_rels/workbook.xml.rels"].decode().replace(
    "</Relationships>", '<Relationship Id="rIdSst" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/></Relationships>').encode()

out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "richtext.xlsx")
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
    for n, d in files.items():
        z.writestr(n, d)
