"""Writes rules.xlsx for the tests of src/book: formulas filled down and
across, input rules, defined names and conditional formats, some of them
split into pieces as Excel leaves them after copying cells, and charts (a
column chart with a line on a second axis, a pie); and columns.xlsx for
the columns of the tables with many rows: a range with a title above its
header and frozen panes, an Excel table, a range with a filter, ranges
with neither and small ones that are left out; and layout.xlsx for where
the tables of a sheet are: settings and totals above a table, a header
of several rows (merged cells, names of groups of columns, an empty row
for the buttons of the filter), formulas filled down under a row of
totals, names of groups above an Excel table, a sheet of settings.

    python3 make_book.py   (needs openpyxl)
"""
import datetime
import os

from openpyxl import Workbook
from openpyxl.chart import BarChart, LineChart, PieChart, Reference
from openpyxl.formatting.rule import CellIsRule, ColorScaleRule, DataBarRule, FormulaRule, IconSetRule
from openpyxl.styles import Alignment, Border, Font, PatternFill, Side
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

# Where the tables are.
wb = Workbook()
head = PatternFill(start_color="DDEBF7", end_color="DDEBF7", fill_type="solid")
thin = Side(style="thin")
box = Border(left=thin, right=thin, top=thin, bottom=thin)

# Settings and a total above the table; a header of three rows: names of
# groups (merged cells, centered across the selection), the names, and an
# empty row with the filter.
ws = wb.active
ws.title = "売上"
ws["A1"] = "売上一覧"
ws["A1"].font = Font(bold=True, size=14)
ws["A3"] = "期間"
ws["B3"] = datetime.date(2025, 4, 1)
ws["A4"] = "担当"
ws["B4"] = "山田"
ws["A5"] = "合計"
ws["D5"] = "=SUBTOTAL(9,D10:D129)"
ws["A7"] = "日付"
ws.merge_cells("A7:A8")
ws["B7"] = "得意先"
ws.merge_cells("B7:C7")
ws["D7"] = "金額"
ws["D7"].alignment = Alignment(horizontal="centerContinuous")
ws["E7"].alignment = Alignment(horizontal="centerContinuous")
ws["F7"] = "備考"
ws.merge_cells("F7:F8")
for c, v in zip("BCDE", ["コード", "名前", "税抜", "税込"]):
    ws[f"{c}8"] = v
for r in (7, 8, 9):
    for c in "ABCDEF":
        ws[f"{c}{r}"].fill = head
        ws[f"{c}{r}"].border = box
for i in range(120):
    r = i + 10
    ws[f"A{r}"] = datetime.date(2025, 4, 1) + datetime.timedelta(days=i % 30)
    ws[f"B{r}"] = f"C{i % 12:03d}"
    ws[f"C{r}"] = f"得意先{i % 12}"
    ws[f"D{r}"] = 1000 + i * 10
    ws[f"E{r}"] = f"=ROUND(D{r}*1.1,0)"
ws.auto_filter.ref = "A9:F129"

# No filter: a formula filled down tells the data, under a row of totals
# right under the header; a total in the settings right above the header.
ws = wb.create_sheet("在庫")
ws["A1"] = "在庫表"
ws["A2"] = "更新日"
ws["B2"] = datetime.date(2025, 4, 1)
ws["A3"] = "総数"
ws["B3"] = "=SUM(C6:C40)"
for c, v in zip("ABCDE", ["品番", "品名", "数量", "単価", "金額"]):
    ws[f"{c}4"] = v
    ws[f"{c}4"].fill = head
ws["A5"] = "合計"
ws["C5"] = "=SUM(C6:C40)"
ws["E5"] = "=SUM(E6:E40)"
for r in range(6, 41):
    ws[f"A{r}"] = f"P{r:03d}"
    ws[f"B{r}"] = f"部品{r}"
    ws[f"C{r}"] = r
    ws[f"D{r}"] = 50
    ws[f"E{r}"] = f"=C{r}*D{r}"

# The name of a group of columns above an Excel table.
ws = wb.create_sheet("名簿")
ws["B1"] = "連絡先"
ws.merge_cells("B1:C1")
ws.append(["氏名", "電話", "メール"])
for i in range(5):
    ws.append([f"社員{i}", f"03-0000-000{i}", f"u{i}@example.com"])
ws.add_table(Table(displayName="名簿", ref="A2:C7"))

# Settings: not a table.
ws = wb.create_sheet("設定")
for k, v in [("会社名", "サンプル商事"), ("部署", "営業部"), ("出力先", "C:\\out")]:
    ws.append([k, v])

# Nothing but the cells to tell by: settings above the header without an
# empty row between them, the header in bold.
ws = wb.create_sheet("予定")
ws["A1"] = "予定表"
ws["A2"] = "年度"
ws["B2"] = 2025
for c, v in zip("ABC", ["担当", "内容", "場所"]):
    ws[f"{c}3"] = v
    ws[f"{c}3"].font = Font(bold=True)
for i in range(6):
    ws.append([f"担当{i}", f"内容{i}", f"場所{i}"])
wb.save(os.path.join(HERE, "layout.xlsx"))
