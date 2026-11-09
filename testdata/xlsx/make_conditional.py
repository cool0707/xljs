"""Builds conditional.xlsx: conditional formatting of every kind (highlight
rules, top / bottom, average, duplicates, text, formulas, color scales, data
bars and icon sets, also the Excel 2010 extensions) for the tests of the
print layout (src/api/xlsx_cond.rs). Run with openpyxl:
python3 testdata/xlsx/make_conditional.py"""
import io, os, zipfile
from openpyxl import Workbook
from openpyxl.formatting.rule import CellIsRule, FormulaRule, ColorScaleRule, DataBarRule, IconSetRule, Rule
from openpyxl.styles import Font, PatternFill
from openpyxl.styles.differential import DifferentialStyle

wb = Workbook()
ws = wb.active
ws.title = "条件"
red = PatternFill("solid", bgColor="FFFFC7CE")
# A1:A10 = 1..10
for i in range(1, 11):
    ws.cell(i, 1).value = i          # A: cellIs, top 3
    ws.cell(i, 2).value = i * 10     # B: color scale
    ws.cell(i, 3).value = i          # C: data bar
    ws.cell(i, 4).value = i          # D: icon set
    ws.cell(i, 5).value = i - 5      # E: data bar with negatives (x14)
    ws.cell(i, 6).value = i          # F: 3Stars (x14)
ws.conditional_formatting.add("A1:A10", CellIsRule(operator="greaterThan", formula=["$H$1"], fill=red, font=Font(bold=True, color="FF9C0006"), stopIfTrue=True))
ws["H1"] = 7
ws.conditional_formatting.add("A1:A10", Rule(type="top10", rank=3, dxf=DifferentialStyle(fill=PatternFill("solid", bgColor="FFC6EFCE"))))
ws.conditional_formatting.add("B1:B10", ColorScaleRule(start_type="min", start_color="FFF8696B", mid_type="percentile", mid_value=50, mid_color="FFFFEB84", end_type="max", end_color="FF63BE7B"))
ws.conditional_formatting.add("C1:C10", DataBarRule(start_type="min", end_type="max", color="FF638EC6"))
ws.conditional_formatting.add("D1:D10", IconSetRule("3Arrows", "percent", [0, 33, 67]))
# G: text, duplicates and a formula relative to the row
for i, v in enumerate(["りんご", "みかん", "りんご", "ぶどう"], 1):
    ws.cell(i, 7).value = v
ws.conditional_formatting.add("G1:G4", Rule(type="duplicateValues", dxf=DifferentialStyle(font=Font(italic=True))))
ws.conditional_formatting.add("G1:G4", Rule(type="containsText", operator="containsText", text="ぶど", formula=['NOT(ISERROR(SEARCH("ぶど",G1)))'], dxf=DifferentialStyle(font=Font(strike=True))))
ws.conditional_formatting.add("A1:A10", FormulaRule(formula=["MOD($A1,2)=0"], font=Font(underline="single")))

path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "conditional.xlsx")
buf = io.BytesIO()
wb.save(buf)

# openpyxl writes no Excel 2010 rules: a data bar from an axis (E) and a
# 3Stars icon set (F), the way Excel writes them.
GUID = "{00000000-0000-0000-0000-00000000E0E0}"
MAIN = (
    '<conditionalFormatting sqref="E1:E10"><cfRule type="dataBar" priority="90"><dataBar>'
    '<cfvo type="min"/><cfvo type="max"/><color rgb="FF63C384"/></dataBar>'
    '<extLst><ext uri="{B025F937-C7B1-47D3-B67F-A62EFF666E3E}" xmlns:x14="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main">'
    f'<x14:id>{GUID}</x14:id></ext></extLst></cfRule></conditionalFormatting>'
)
EXT = (
    '<extLst><ext uri="{78C0D931-6437-407d-A8EE-F0AAD7539E65}" xmlns:x14="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main">'
    '<x14:conditionalFormattings>'
    '<x14:conditionalFormatting xmlns:xm="http://schemas.microsoft.com/office/excel/2006/main">'
    f'<x14:cfRule type="dataBar" id="{GUID}"><x14:dataBar minLength="0" maxLength="100" gradient="0" border="1">'
    '<x14:cfvo type="autoMin"/><x14:cfvo type="autoMax"/><x14:borderColor rgb="FF63C384"/>'
    '<x14:negativeFillColor rgb="FFFF0000"/><x14:axisColor rgb="FF000000"/></x14:dataBar></x14:cfRule>'
    '<xm:sqref>E1:E10</xm:sqref></x14:conditionalFormatting>'
    '<x14:conditionalFormatting xmlns:xm="http://schemas.microsoft.com/office/excel/2006/main">'
    '<x14:cfRule type="iconSet" priority="91" id="{00000000-0000-0000-0000-00000000F0F0}"><x14:iconSet iconSet="3Stars" showValue="0">'
    '<x14:cfvo type="percent"><xm:f>0</xm:f></x14:cfvo><x14:cfvo type="num"><xm:f>4</xm:f></x14:cfvo>'
    '<x14:cfvo type="num" gte="0"><xm:f>8</xm:f></x14:cfvo></x14:iconSet></x14:cfRule>'
    '<xm:sqref>F1:F10</xm:sqref></x14:conditionalFormatting>'
    '</x14:conditionalFormattings></ext></extLst>'
)
src = zipfile.ZipFile(buf)
with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as out:
    for item in src.infolist():
        data = src.read(item.filename)
        if item.filename == "xl/worksheets/sheet1.xml":
            s = data.decode()
            i = s.rindex("</conditionalFormatting>") + len("</conditionalFormatting>")
            s = s[:i] + MAIN + s[i:]
            s = s.replace("</worksheet>", EXT + "</worksheet>")
            data = s.encode()
        out.writestr(item, data)
