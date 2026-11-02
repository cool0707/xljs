"""Builds tables.xlsx: tables with styles, a black-and-white sheet and print
areas given by formulas, for the tests of the print layout
(src/api/xlsx_print.rs, src/api/xlsx_table.rs). Run with openpyxl:
python3 testdata/xlsx/make_tables.py"""
import io, os, zipfile
from openpyxl import Workbook
from openpyxl.styles import PatternFill
from openpyxl.workbook.defined_name import DefinedName
from openpyxl.worksheet.table import Table, TableStyleInfo

wb = Workbook()
ws = wb.active
ws.title = "テーブル"
ws.append(["#", "研修名", "参加日"])
for i in range(1, 6):
    ws.append([i, f"研修{i}", None])
t = Table(displayName="参加報告", ref="A1:C6")
t.tableStyleInfo = TableStyleInfo(name="自作スタイル", showRowStripes=True)
ws.add_table(t)
ws["E1"], ws["F1"] = "品名", "数量"
for i in range(2, 5):
    ws.cell(i, 5).value = f"品{i}"
    ws.cell(i, 6).value = i
t2 = Table(displayName="既定", ref="E1:F4")
t2.tableStyleInfo = TableStyleInfo(name="TableStyleMedium2", showRowStripes=True)
ws.add_table(t2)

mono = wb.create_sheet("白黒")
mono["A1"] = "塗り"
mono["A1"].fill = PatternFill("solid", fgColor="FFC000")
mono.page_setup.blackAndWhite = True

dyn = wb.create_sheet("可変範囲")
dyn["A1"] = "見出し"
for r in range(3, 9):
    dyn.cell(r, 2).value = r * 10
for r in range(1, 31):
    dyn.cell(r, 4).value = "外"
# Rows 1 to (the last number in B3:B99) + 2 = 8, columns A:C.
dyn.defined_names["_xlnm.Print_Area"] = DefinedName(
    "_xlnm.Print_Area",
    attr_text="OFFSET(可変範囲!$A$1,0,0,MAX(_xlfn.IFNA(MATCH(10^10,可変範囲!$B$3:$B$99),0)+2,5),3)")

cols = wb.create_sheet("列指定")
for r in range(1, 11):
    cols.cell(r, 1).value = f"A{r}"
cols.defined_names["_xlnm.Print_Area"] = DefinedName("_xlnm.Print_Area", attr_text="列指定!$A:$B")

path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "tables.xlsx")
buf = io.BytesIO()
wb.save(buf)

# openpyxl does not write custom table styles: add one to styles.xml
# (header: light fill, not bold; whole table: thin borders, hair inside).
STYLE = (
    '<dxfs count="2">'
    '<dxf><font><b val="0"/><color theme="1"/></font><fill><patternFill><bgColor rgb="FFDDEBF7"/></patternFill></fill></dxf>'
    '<dxf><border><left style="thin"><color rgb="FF808080"/></left><right style="thin"><color rgb="FF808080"/></right>'
    '<top style="thin"><color rgb="FF808080"/></top><bottom style="thin"><color rgb="FF808080"/></bottom>'
    '<vertical style="hair"><color rgb="FF808080"/></vertical><horizontal style="thin"><color rgb="FF808080"/></horizontal></border></dxf>'
    '</dxfs><tableStyles count="1" defaultTableStyle="TableStyleMedium9" defaultPivotStyle="PivotStyleLight16">'
    '<tableStyle name="自作スタイル" pivot="0" count="2"><tableStyleElement type="wholeTable" dxfId="1"/>'
    '<tableStyleElement type="headerRow" dxfId="0"/></tableStyle></tableStyles>'
)
src = zipfile.ZipFile(buf)
with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as out:
    for item in src.infolist():
        data = src.read(item.filename)
        if item.filename == "xl/styles.xml":
            s = data.decode()
            import re
            s = re.sub(r'<dxfs count="0"\s*/>|<dxfs[^>]*>.*?</dxfs>', "", s, flags=re.S)
            s = re.sub(r'<tableStyles[^>]*/>|<tableStyles[^>]*>.*?</tableStyles>', "", s, flags=re.S)
            s = s.replace("</cellStyles>", "</cellStyles>" + STYLE, 1)
            data = s.encode()
        out.writestr(item, data)
