"""Builds cellimages.xlsx: pictures placed in cells ("Place in Cell") as
Excel writes them (rich values: xl/metadata.xml and xl/richData/*), and
cells that refer to them, for the print layout (src/api/xlsx_cellimage.rs).
openpyxl cannot write them, so the parts are added to its output.
Run: python3 testdata/xlsx/make_cellimages.py"""
import os, io, zipfile
from openpyxl import Workbook
from openpyxl.styles import Alignment
from PIL import Image

here = os.path.dirname(os.path.abspath(__file__))

def png(w, h, color):
    b = io.BytesIO()
    Image.new("RGB", (w, h), color).save(b, "PNG")
    return b.getvalue()

def jpeg(w, h, color):
    b = io.BytesIO()
    Image.new("RGB", (w, h), color).save(b, "JPEG", quality=80)
    return b.getvalue()

wb = Workbook()
ws = wb.active
ws.title = "商品"
ws.append(["名前", "写真", None, "写真の参照"])
ws.append(["りんご"])
ws.append(["みかん"])
ws.append(["ぶどう"])
ws.column_dimensions["B"].width = 12
ws.column_dimensions["D"].width = 12
for r in (2, 3, 4):
    ws.row_dimensions[r].height = 40
ws["B4"].alignment = Alignment(horizontal="left", vertical="top")
ref = wb.create_sheet("参照")
ref.append(["Excel が保存した参照", "vm のない参照"])
ref.column_dimensions["A"].width = 20
ref.column_dimensions["B"].width = 20
ref.row_dimensions[2].height = 40
buf = io.BytesIO()
wb.save(buf)

# Rich values: 0 = image1 (りんご), 1 = image2 (みかん), 2 = image3 with alt text (ぶどう).
media = {"xl/media/image1.png": png(80, 40, (220, 30, 30)), "xl/media/image2.jpeg": jpeg(40, 40, (250, 150, 20)), "xl/media/image3.png": png(30, 60, (110, 40, 160))}
RD = "http://schemas.microsoft.com/office/spreadsheetml/2017/richdata"
rv = (f'<rvData xmlns="{RD}" count="3"><rv s="0"><v>0</v><v>5</v></rv><rv s="0"><v>1</v><v>5</v></rv>'
      '<rv s="1"><v>2</v><v>5</v><v>ぶどうの写真</v></rv></rvData>')
structures = (f'<rvStructures xmlns="{RD}" count="2"><s t="_localImage"><k n="_rvRel:LocalImageIdentifier" t="i"/><k n="CalcOrigin" t="i"/></s>'
              '<s t="_localImage"><k n="_rvRel:LocalImageIdentifier" t="i"/><k n="CalcOrigin" t="i"/><k n="Text" t="s"/></s></rvStructures>')
rels = ('<richValueRels xmlns="http://schemas.microsoft.com/office/spreadsheetml/2022/richvaluerel" '
        'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><rel r:id="rId1"/><rel r:id="rId2"/><rel r:id="rId3"/></richValueRels>')
IMG = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image"
rels_rels = ('<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
             f'<Relationship Id="rId1" Type="{IMG}" Target="../media/image1.png"/>'
             f'<Relationship Id="rId2" Type="{IMG}" Target="../media/image2.jpeg"/>'
             f'<Relationship Id="rId3" Type="{IMG}" Target="../media/image3.png"/></Relationships>')
types = (f'<rvTypesInfo xmlns="http://schemas.microsoft.com/office/spreadsheetml/2017/richdata2"><global><keyFlags>'
         '<key name="_Self"><flag name="ExcludeFromFile" value="1"/><flag name="ExcludeFromCalcComparison" value="1"/></key>'
         '</keyFlags></global></rvTypesInfo>')
# vm 1..3 -> rich values 0..2 (Excel gives the referring cell the vm of what it shows).
bks = "".join(f'<bk><extLst><ext uri="{{3e2802c4-a4d2-4d8b-9148-e3be6c30e623}}"><xlrd:rvb i="{i}"/></ext></extLst></bk>' for i in range(3))
metadata = ('<metadata xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:xlrd="http://schemas.microsoft.com/office/spreadsheetml/2017/richdata">'
            '<metadataTypes count="1"><metadataType name="XLRICHVALUE" minSupportedVersion="120000" copy="1" pasteAll="1" pasteValues="1" merge="1" '
            'splitFirst="1" rowColShift="1" clearFormats="1" clearComments="1" assign="1" coerce="1"/></metadataTypes>'
            f'<futureMetadata name="XLRICHVALUE" count="3">{bks}</futureMetadata>'
            '<valueMetadata count="3"><bk><rc t="1" v="0"/></bk><bk><rc t="1" v="1"/></bk><bk><rc t="1" v="2"/></bk></valueMetadata></metadata>')

def image_cell(r, vm, formula=None, style=""):
    f = f"<f>{formula}</f>" if formula else ""
    v = f' vm="{vm}"' if vm else ""
    return f'<c r="{r}"{style} t="e"{v}>{f}<v>#VALUE!</v></c>'

def add_cells(xml, cells):
    """Puts the cells at the end of their rows."""
    for row, cs in cells.items():
        start = xml.index(f'<row r="{row}"')
        end = xml.index("</row>", start)
        xml = xml[:end] + "".join(cs) + xml[end:]
    return xml

src = zipfile.ZipFile(buf)
out = zipfile.ZipFile(os.path.join(here, "cellimages.xlsx"), "w", zipfile.ZIP_DEFLATED)
for item in src.infolist():
    data = src.read(item.filename)
    if item.filename == "[Content_Types].xml":
        data = data.replace(b"</Types>", (
            '<Default Extension="png" ContentType="image/png"/><Default Extension="jpeg" ContentType="image/jpeg"/>'
            '<Override PartName="/xl/metadata.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheetMetadata+xml"/>'
            '<Override PartName="/xl/richData/rdrichvalue.xml" ContentType="application/vnd.ms-excel.rdrichvalue+xml"/>'
            '<Override PartName="/xl/richData/rdrichvaluestructure.xml" ContentType="application/vnd.ms-excel.rdrichvaluestructure+xml"/>'
            '<Override PartName="/xl/richData/richValueRel.xml" ContentType="application/vnd.ms-excel.richvaluerel+xml"/>'
            '<Override PartName="/xl/richData/rdRichValueTypes.xml" ContentType="application/vnd.ms-excel.rdrichvaluetypes+xml"/></Types>').encode())
    if item.filename == "xl/_rels/workbook.xml.rels":
        data = data.replace(b"</Relationships>", (
            '<Relationship Id="rId90" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sheetMetadata" Target="metadata.xml"/>'
            '<Relationship Id="rId91" Type="http://schemas.microsoft.com/office/2022/10/relationships/richValueRel" Target="richData/richValueRel.xml"/>'
            '<Relationship Id="rId92" Type="http://schemas.microsoft.com/office/2017/06/relationships/rdRichValue" Target="richData/rdrichvalue.xml"/>'
            '<Relationship Id="rId93" Type="http://schemas.microsoft.com/office/2017/06/relationships/rdRichValueStructure" Target="richData/rdrichvaluestructure.xml"/>'
            '<Relationship Id="rId94" Type="http://schemas.microsoft.com/office/2017/06/relationships/rdRichValueTypes" Target="richData/rdRichValueTypes.xml"/>'
            '</Relationships>').encode())
    if item.filename == "xl/worksheets/sheet1.xml":
        s = data.decode()
        # B4 keeps the style openpyxl gave it (left, top).
        b4_style = ' s="1"'
        s = s.replace('<c r="B4" s="1" t="n"></c>', "")
        s = add_cells(s, {
            2: [image_cell("B2", 1), image_cell("D2", 1, "B2")],
            3: [image_cell("B3", 2), image_cell("D3", 2, "B3")],
            4: [image_cell("B4", 3, style=b4_style)],
        })
        data = s.encode()
    if item.filename == "xl/worksheets/sheet2.xml":
        s = data.decode()
        # A2: a reference to the other sheet as Excel saves it; B2: one saved without vm.
        s = add_cells(s, {2: [image_cell("A2", 2, "商品!B3"), image_cell("B2", None, "商品!B2")]})
        data = s.encode()
    out.writestr(item, data)
out.writestr("xl/metadata.xml", metadata)
out.writestr("xl/richData/rdrichvalue.xml", rv)
out.writestr("xl/richData/rdrichvaluestructure.xml", structures)
out.writestr("xl/richData/richValueRel.xml", rels)
out.writestr("xl/richData/_rels/richValueRel.xml.rels", rels_rels)
out.writestr("xl/richData/rdRichValueTypes.xml", types)
for name, b in media.items():
    out.writestr(name, b)
out.close()
