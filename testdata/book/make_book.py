"""Writes rules.xlsx for the tests of src/book: formulas filled down and
across, input rules, defined names and conditional formats, some of them
split into pieces as Excel leaves them after copying cells.

    python3 make_book.py   (needs openpyxl)
"""
import datetime
import os

from openpyxl import Workbook
from openpyxl.formatting.rule import CellIsRule, ColorScaleRule, DataBarRule, FormulaRule, IconSetRule
from openpyxl.styles import Font, PatternFill
from openpyxl.workbook.defined_name import DefinedName
from openpyxl.worksheet.datavalidation import DataValidation

wb = Workbook()
ws = wb.active
ws.title = "売上"
ws.append(["商品", "単価", "数量", "金額", "税込"])
for i in range(2, 12):
    ws.append([f"商品{i}", 100 * i, i])
    ws[f"D{i}"] = f"=B{i}*C{i}"
    ws[f"E{i}"] = f"=D{i}*(1+税率)"
ws["D12"] = "=SUM(D2:D11)"
ws["E12"] = "=SUM(E2:E11)"
# A block filled down and across: running totals.
for r in range(2, 6):
    for c in "GHI":
        ws[f"{c}{r}"] = f"=SUM($B$2:{chr(ord(c) - 5)}{r})&\"件\""
ws["D13"] = "=_xlfn.XLOOKUP(A2,A:A,D:D)"

dv = DataValidation(type="list", formula1='"東京,大阪,名古屋"', allow_blank=True)
dv.add("F2:F11")
ws.add_data_validation(dv)
dv = DataValidation(type="whole", operator="between", formula1="1", formula2="100", allow_blank=True,
                    showErrorMessage=True, errorTitle="数量", error="1〜100 の整数", promptTitle="数量", prompt="1〜100", showInputMessage=True)
dv.add("C2:C11")
ws.add_data_validation(dv)
# The same custom rule split into two pieces.
for ref in ("A2:A6", "A7:A11"):
    dv = DataValidation(type="custom", formula1=f"COUNTIF($A:$A,{ref.split(':')[0]})=1", showErrorMessage=True, errorStyle="warning")
    dv.add(ref)
    ws.add_data_validation(dv)
dv = DataValidation(type="date", operator="greaterThanOrEqual", formula1="45658", showErrorMessage=False)
dv.add("J2:J11")
ws.add_data_validation(dv)

red = PatternFill(start_color="FFC7CE", end_color="FFC7CE", fill_type="solid")
for ref in ("D2:D6", "D7:D11"):
    ws.conditional_formatting.add(ref, FormulaRule(formula=[f"{ref.split(':')[0]}>$K$1"], fill=red, font=Font(bold=True, color="9C0006")))
ws.conditional_formatting.add("C2:C11", CellIsRule(operator="between", formula=["1", "5"], font=Font(italic=True)))
ws.conditional_formatting.add("B2:B11", ColorScaleRule(start_type="min", start_color="F8696B", end_type="max", end_color="63BE7B"))
ws.conditional_formatting.add("E2:E11", DataBarRule(start_type="min", end_type="max", color="638EC6"))
ws.conditional_formatting.add("H2:H5", IconSetRule("3TrafficLights1", "percent", [0, 33, 67]))

ws2 = wb.create_sheet("設定")
ws2["A1"] = "税率"
ws2["B1"] = 0.1
ws2["A2"] = "更新日"
ws2["B2"] = datetime.date(2025, 1, 1)
wb.defined_names["税率"] = DefinedName("税率", attr_text="設定!$B$1")
ws.defined_names["対象"] = DefinedName("対象", attr_text="売上!$A$2:$A$11")
ws.print_area = "A1:E12"

wb.save(os.path.join(os.path.dirname(os.path.abspath(__file__)), "rules.xlsx"))
