"""Writes rules.xlsx for the tests of src/book: formulas filled down and
across, input rules, defined names and conditional formats, some of them
split into pieces as Excel leaves them after copying cells, and charts (a
column chart with a line on a second axis, a pie); and columns.xlsx for
the columns of the tables with many rows: a range with a title above its
header and frozen panes, an Excel table, a range with a filter, ranges
with neither and small ones that are left out.

    python3 make_book.py   (needs openpyxl)
"""
import datetime
import os

from openpyxl import Workbook
from openpyxl.chart import BarChart, LineChart, PieChart, Reference
from openpyxl.formatting.rule import CellIsRule, ColorScaleRule, DataBarRule, FormulaRule, IconSetRule
from openpyxl.styles import Font, PatternFill
from openpyxl.workbook.defined_name import DefinedName
from openpyxl.worksheet.datavalidation import DataValidation
from openpyxl.worksheet.table import Table

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

# Columns of the amounts with the quantities as a line on a second axis.
bar = BarChart()
bar.type = "col"
bar.grouping = "clustered"
bar.title = "商品別の金額"
bar.x_axis.title = "商品"
bar.y_axis.title = "金額"
bar.y_axis.scaling.min = 0
bar.y_axis.majorGridlines = None
bar.add_data(Reference(ws, min_col=4, min_row=1, max_row=11), titles_from_data=True)
bar.set_categories(Reference(ws, min_col=1, min_row=2, max_row=11))
line = LineChart()
line.add_data(Reference(ws, min_col=3, min_row=1, max_row=11), titles_from_data=True)
line.y_axis.axId = 200
line.y_axis.title = "数量"
line.y_axis.crosses = "max"
bar.legend.position = "b"
bar += line
ws.add_chart(bar, "L2")

ws2 = wb.create_sheet("設定")
pie = PieChart()
pie.title = "単価の割合"
pie.add_data(Reference(ws, min_col=2, min_row=2, max_row=6))
pie.set_categories(Reference(ws, min_col=1, min_row=2, max_row=6))
ws2.add_chart(pie, "D2")
ws2["A1"] = "税率"
ws2["B1"] = 0.1
ws2["A2"] = "更新日"
ws2["B2"] = datetime.date(2025, 1, 1)
wb.defined_names["税率"] = DefinedName("税率", attr_text="設定!$B$1")
ws.defined_names["対象"] = DefinedName("対象", attr_text="売上!$A$2:$A$11")
ws.print_area = "A1:E12"

HERE = os.path.dirname(os.path.abspath(__file__))
wb.save(os.path.join(HERE, "rules.xlsx"))

wb = Workbook()
ws = wb.active
ws.title = "受注"
ws["A1"] = "受注一覧"
ws.append(["受注番号", "顧客コード", "受注日", "区分", "数量", "単価", "金額", "備考", "郵便番号"])
kinds = ["通常", "通常", "至急", "返品"]
for i in range(150):
    r = i + 3
    ws.append([
        f"J{i + 1:06d}",
        f"{(i * 7) % 120:06d}",
        None if i % 50 == 49 else datetime.date(2025, 1, 1) + datetime.timedelta(days=i),
        kinds[i % 4],
        "未定" if i in (10, 20) else i % 100 + 1,
        round(100 + i * 1.25, 2),
        f"=E{r}*F{r}",
        {3: "急ぎ ", 30: "ＡＢＣ商事", 60: "再送"}.get(i),
        f"{100 + i:03d}-{i * 37 % 10000:04d}",
    ])
ws.append(["合計", None, None, None, None, None, "=SUM(G3:G152)"])
ws.freeze_panes = "B3"
dv = DataValidation(type="list", formula1='"通常,至急,返品"')
dv.add("D3:D152")
ws.add_data_validation(dv)
# Notes next to the table but apart from it, and a small block.
ws["L1"] = "メモ"
ws["L2"] = "小さい表"
ws["M2"] = 1

ws = wb.create_sheet("社員")
ws.append(["社員番号", "氏名", "入社日時", "在籍"])
for i in range(120):
    ws.append([1001 + i, "山田　太郎" if i % 2 else "佐藤花子", datetime.datetime(2020, 4, 1, 9, 30) + datetime.timedelta(days=i), i % 3 != 0])
    ws.cell(row=i + 2, column=1).number_format = "00000"
ws.add_table(Table(displayName="社員一覧", ref="A1:D121"))
# A small table: left out.
ws["G1"] = "コード"
for i in range(5):
    ws.cell(row=i + 2, column=7, value=i)
ws.add_table(Table(displayName="小", ref="G1:G6"))

ws = wb.create_sheet("記録")
ws["A1"] = "記録"
ws["A2"] = "2025 年度"
ws.append([])
ws.append(["時刻", "値", "コメント"])
for i in range(100):
    ws.append([datetime.time(9 + i % 8, i % 60), i * 0.5 - 10, "ok"])
ws.auto_filter.ref = "A4:C104"

# No frozen panes or filter: the title right above the header is left
# out; a block of numbers alone has no header.
ws = wb.create_sheet("一覧")
ws["A1"] = "商品一覧"
ws.append(["品番", "品名", "価格"])
for i in range(100):
    ws.append([f"ab-{i:03d}", "ｶﾞﾑ" if i % 10 == 0 else "ガム", str(i * 10) if i % 4 == 0 else i * 10])
for i in range(120):
    ws.cell(row=i + 1, column=5, value=i % 7)

ws = wb.create_sheet("小さい")
ws.append(["名前", "値"])
for i in range(50):
    ws.append([f"n{i}", i])
wb.save(os.path.join(HERE, "columns.xlsx"))
