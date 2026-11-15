"""Writes the sample data of the examples into examples/data/:

- 売上/2026-04.xlsx .. 2026-06.xlsx: the sheet 明細 (伝票番号, 日付, 支店,
  顧客名, 商品名, 数量, 単価, 金額), one book per month. Used by the
  examples that aggregate a folder, convert a book (JSON, CSV) or import
  it into SQLite.
- 売上レポート.xlsx: monthly sales with a column + line chart and branch
  sales with a pie chart, and a page setup (A4 landscape, one page wide,
  header and footer). chart-sales.js and xlsx-to-pdf.js.
- 工程表.xlsx: the tasks of chart-gantt.js (作業, 開始, 終了, 進捗, 前提, 金額).
- 顧客一覧.csv (Shift_JIS) and 商品一覧.csv (UTF-8 with BOM): csv-to-xlsx.js
  and table-file-list.js.
- 資料/*.pdf: three short PDFs for pdf-merge-stamp.js (IPA Gothic, subset
  embedded).
- 積算.xlsx: a cost estimate for xlsx-calc-estimate.js. 工事一覧 (one row
  per job), 計算 (input cells and the breakdown as formulas: direct cost,
  temporary works, site and general overhead, tax) and 率表 (the rates as
  two tables the formulas look up with structured references). The rates
  are made up.

    python3 tools/examples/make_data.py [name...]   (needs openpyxl,
        reportlab and the IPA Gothic font; names: sales report gantt csvs
        pdfs estimate, all by default)

The data is fixed (seeded), so running it again gives the same values.
"""
import csv
import datetime
import os
import random
import sys

from openpyxl import Workbook
from openpyxl.chart import BarChart, LineChart, PieChart, Reference
from openpyxl.styles import Alignment, Font, PatternFill

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "examples", "data")
FONT = "游ゴシック"

BRANCHES = ["東京", "大阪", "名古屋", "福岡"]
CUSTOMERS = {
    "東京": ["丸の内商事", "青山物産", "品川電機"],
    "大阪": ["なにわ工業", "梅田産業"],
    "名古屋": ["栄テクノ", "中部精機"],
    "福岡": ["博多システム", "天神商会"],
}
PRODUCTS = [
    ("P001", "ノートPC", 128000),
    ("P002", "モニター", 32800),
    ("P003", "キーボード", 6980),
    ("P004", "マウス", 2480),
    ("P005", "ドッキングステーション", 24800),
    ("P006", "Webカメラ", 8980),
]

rnd = random.Random(20260401)


def style_header(ws, ncols):
    fill = PatternFill("solid", fgColor="305496")
    for c in range(1, ncols + 1):
        cell = ws.cell(row=1, column=c)
        cell.font = Font(name=FONT, bold=True, color="FFFFFF")
        cell.fill = fill
        cell.alignment = Alignment(horizontal="center")


def set_font(ws):
    for row in ws.iter_rows():
        for cell in row:
            if cell.font.name != FONT:
                f = cell.font
                cell.font = Font(name=FONT, bold=f.bold, color=f.color, size=f.size)


def sales_book(year, month, start_no):
    wb = Workbook()
    ws = wb.active
    ws.title = "明細"
    head = ["伝票番号", "日付", "支店", "顧客名", "商品名", "数量", "単価", "金額"]
    ws.append(head)
    no = start_no
    days = sorted(rnd.randint(1, 28) for _ in range(rnd.randint(18, 24)))
    for d in days:
        branch = rnd.choice(BRANCHES)
        customer = rnd.choice(CUSTOMERS[branch])
        _, product, price = rnd.choice(PRODUCTS)
        qty = rnd.choice([1, 1, 2, 2, 3, 5, 10, 20]) if price < 10000 else rnd.choice([1, 1, 2, 3, 5])
        ws.append([f"S{no:05d}", datetime.datetime(year, month, d), branch, customer, product, qty, price, qty * price])
        no += 1
    last = ws.max_row
    for r in range(2, last + 1):
        ws.cell(row=r, column=2).number_format = "yyyy/m/d"
        for c in (6, 7, 8):
            ws.cell(row=r, column=c).number_format = "#,##0"
    style_header(ws, len(head))
    for col, w in zip("ABCDEFGH", [11, 11, 8, 14, 24, 7, 10, 12]):
        ws.column_dimensions[col].width = w
    ws.freeze_panes = "A2"
    ws.auto_filter.ref = f"A1:H{last}"
    ws.page_setup.orientation = "landscape"
    ws.page_setup.paperSize = ws.PAPERSIZE_A4
    ws.page_setup.fitToWidth = 1
    ws.page_setup.fitToHeight = 0
    ws.sheet_properties.pageSetUpPr.fitToPage = True
    ws.print_title_rows = "1:1"
    ws.oddHeader.center.text = f"売上明細 {year}年{month}月"
    ws.oddFooter.center.text = "&P / &N"
    set_font(ws)
    return wb, no


def sales():
    os.makedirs(os.path.join(ROOT, "売上"), exist_ok=True)
    no = 1
    for month in (4, 5, 6):
        wb, no = sales_book(2026, month, no)
        wb.save(os.path.join(ROOT, "売上", f"2026-{month:02d}.xlsx"))


def report():
    wb = Workbook()
    ws = wb.active
    ws.title = "月別"
    ws.append(["月", "売上", "粗利"])
    for m in range(4, 16):
        y, mm = (2026, m) if m <= 12 else (2027, m - 12)
        sales = rnd.randint(380, 620) * 10000
        gross = int(sales * rnd.uniform(0.22, 0.34) / 1000) * 1000
        ws.append([f"{y}/{mm:02d}", sales, gross])
    last = ws.max_row
    for r in range(2, last + 1):
        ws.cell(row=r, column=2).number_format = "#,##0"
        ws.cell(row=r, column=3).number_format = "#,##0"
    style_header(ws, 3)
    for col, w in zip("ABC", [10, 14, 14]):
        ws.column_dimensions[col].width = w

    bar = BarChart()
    bar.title = "月別売上と粗利"
    bar.y_axis.title = "売上"
    bar.y_axis.numFmt = "#,##0"
    bar.add_data(Reference(ws, min_col=2, min_row=1, max_row=last), titles_from_data=True)
    bar.set_categories(Reference(ws, min_col=1, min_row=2, max_row=last))
    line = LineChart()
    line.add_data(Reference(ws, min_col=3, min_row=1, max_row=last), titles_from_data=True)
    line.y_axis.axId = 200
    line.y_axis.title = "粗利"
    line.y_axis.crosses = "max"
    bar += line
    bar.width, bar.height = 18, 9
    ws.add_chart(bar, "E2")

    ws.page_setup.orientation = "landscape"
    ws.page_setup.paperSize = ws.PAPERSIZE_A4
    ws.page_setup.fitToWidth = 1
    ws.page_setup.fitToHeight = 1
    ws.sheet_properties.pageSetUpPr.fitToPage = True
    ws.oddHeader.center.text = "売上レポート（月別）"
    ws.oddFooter.center.text = "&P / &N"
    set_font(ws)

    ws2 = wb.create_sheet("支店別")
    ws2.append(["支店", "売上"])
    for b in BRANCHES:
        ws2.append([b, rnd.randint(900, 2200) * 10000])
    for r in range(2, ws2.max_row + 1):
        ws2.cell(row=r, column=2).number_format = "#,##0"
    style_header(ws2, 2)
    ws2.column_dimensions["A"].width = 10
    ws2.column_dimensions["B"].width = 14
    pie = PieChart()
    pie.title = "支店別の売上"
    pie.add_data(Reference(ws2, min_col=2, min_row=1, max_row=ws2.max_row), titles_from_data=True)
    pie.set_categories(Reference(ws2, min_col=1, min_row=2, max_row=ws2.max_row))
    pie.width, pie.height = 12, 9
    ws2.add_chart(pie, "D2")
    ws2.page_setup.orientation = "landscape"
    ws2.page_setup.paperSize = ws2.PAPERSIZE_A4
    ws2.oddHeader.center.text = "売上レポート（支店別）"
    set_font(ws2)
    wb.save(os.path.join(ROOT, "売上レポート.xlsx"))


def gantt():
    wb = Workbook()
    ws = wb.active
    ws.title = "工程"
    ws.append(["作業", "開始", "終了", "進捗", "前提", "金額"])
    d = datetime.datetime
    tasks = [
        ("要件定義", d(2026, 10, 19), d(2026, 10, 30), 1.0, "", 800000),
        ("基本設計", d(2026, 11, 2), d(2026, 11, 13), 0.9, "要件定義", 1200000),
        ("詳細設計", d(2026, 11, 9), d(2026, 11, 20), 0.5, "基本設計", 1500000),
        ("実装", d(2026, 11, 16), d(2026, 12, 11), 0.15, "詳細設計", 4000000),
        ("結合テスト", d(2026, 12, 7), d(2026, 12, 18), 0, "実装", 1800000),
        ("受入テスト", d(2026, 12, 21), d(2026, 12, 25), 0, "結合テスト", 600000),
        ("リリース", d(2026, 12, 28), d(2026, 12, 28), 0, "受入テスト", 200000),
    ]
    for t in tasks:
        ws.append(list(t))
    last = ws.max_row
    for r in range(2, last + 1):
        ws.cell(row=r, column=2).number_format = "yyyy/m/d"
        ws.cell(row=r, column=3).number_format = "yyyy/m/d"
        ws.cell(row=r, column=4).number_format = "0%"
        ws.cell(row=r, column=6).number_format = "#,##0"
    style_header(ws, 6)
    for col, w in zip("ABCDEF", [14, 11, 11, 8, 14, 12]):
        ws.column_dimensions[col].width = w
    ws.freeze_panes = "A2"
    set_font(ws)
    wb.save(os.path.join(ROOT, "工程表.xlsx"))


def csvs():
    rows = [["顧客コード", "顧客名", "支店", "担当", "電話番号", "取引開始日"]]
    staff = ["山田", "佐藤", "鈴木", "高橋", "田中"]
    n = 1
    for b in BRANCHES:
        for c in CUSTOMERS[b]:
            start = datetime.date(2015 + rnd.randint(0, 10), rnd.randint(1, 12), rnd.randint(1, 28))
            tel = f"0{rnd.randint(3, 99)}-{rnd.randint(100, 9999)}-{rnd.randint(1000, 9999)}"
            rows.append([f"{n:05d}", c, b, rnd.choice(staff), tel, start.strftime("%Y/%m/%d")])
            n += 1
    with open(os.path.join(ROOT, "顧客一覧.csv"), "w", encoding="cp932", newline="") as f:
        csv.writer(f, lineterminator="\r\n").writerows(rows)

    rows = [["商品コード", "商品名", "単価", "備考"]]
    notes = {"P001": "保守契約あり", "P005": "USB Type-C 対応"}
    for code, name, price in PRODUCTS:
        rows.append([code, name, price, notes.get(code, "")])
    with open(os.path.join(ROOT, "商品一覧.csv"), "w", encoding="utf-8-sig", newline="") as f:
        csv.writer(f, lineterminator="\r\n").writerows(rows)


def pdfs():
    from reportlab.lib.pagesizes import A4, landscape
    from reportlab.pdfbase import pdfmetrics
    from reportlab.pdfbase.ttfonts import TTFont
    from reportlab.pdfgen import canvas

    pdfmetrics.registerFont(TTFont("IPAGothic", "/usr/share/fonts/opentype/ipafont-gothic/ipag.ttf"))
    os.makedirs(os.path.join(ROOT, "資料"), exist_ok=True)
    docs = [
        ("1_概要.pdf", A4, ["プロジェクト概要", "", "目的: 受注業務の集計を自動化する", "期間: 2026年10月〜12月", "体制: 開発 3 名・業務担当 2 名"]),
        ("2_スケジュール.pdf", landscape(A4), ["スケジュール", "", "10月: 要件定義", "11月: 設計・実装", "12月: テスト・リリース"]),
        ("3_予算.pdf", A4, ["予算", "", "人件費: 9,000,000 円", "機器: 1,100,000 円", "合計: 10,100,000 円"]),
    ]
    for name, size, lines in docs:
        c = canvas.Canvas(os.path.join(ROOT, "資料", name), pagesize=size, invariant=1)
        for page in (1, 2):
            c.setFont("IPAGothic", 22 if page == 1 else 16)
            y = size[1] - 100
            for i, line in enumerate(lines if page == 1 else [lines[0] + "（補足）", "", "このページは結合の確認用です。"]):
                c.drawString(72, y, line)
                y -= 34 if i == 0 else 24
                c.setFont("IPAGothic", 13)
            c.showPage()
        c.save()


def estimate():
    from openpyxl.styles import Border, Side
    from openpyxl.workbook.defined_name import DefinedName
    from openpyxl.worksheet.datavalidation import DataValidation
    from openpyxl.worksheet.table import Table, TableStyleInfo

    wb = Workbook()
    thin = Side(style="thin", color="808080")
    box = Border(left=thin, right=thin, top=thin, bottom=thin)
    head_fill = PatternFill("solid", fgColor="305496")
    input_fill = PatternFill("solid", fgColor="FFF2CC")
    total_fill = PatternFill("solid", fgColor="D9E1F2")

    # ---- 工事一覧: 1 行が 1 件の工事。見出しは「計算」シートの入力欄の項目名と同じ ----
    ws = wb.active
    ws.title = "工事一覧"
    head = ["工事番号", "工事名", "工種", "直接工事費", "工期"]
    ws.append(head)
    jobs = [
        ("K-2601", "市民会館 外壁改修工事", "建築", 48_600_000, 5),
        ("K-2602", "第二小学校 体育館新築工事", "建築", 312_000_000, 14),
        ("K-2603", "市道 3 号線 舗装補修工事", "土木", 26_400_000, 3),
        ("K-2604", "中央公園 護岸整備工事", "土木", 158_000_000, 9),
        ("K-2605", "浄水場 受変電設備更新工事", "設備", 87_500_000, 7),
        ("K-2606", "庁舎 空調設備改修工事", "設備", 9_800_000, 2),
    ]
    for j in jobs:
        ws.append(list(j))
    for r in range(2, ws.max_row + 1):
        ws.cell(row=r, column=4).number_format = "#,##0"
        ws.cell(row=r, column=5).number_format = '0"か月"'
    style_header(ws, len(head))
    for col, w in zip("ABCDE", [11, 30, 8, 15, 8]):
        ws.column_dimensions[col].width = w
    ws.freeze_panes = "A2"

    # ---- 率表: 工種ごとの比率・率の式の係数と、一般管理費等率 ----
    rt = wb.create_sheet("率表")
    rt["A1"] = "工種別の比率と係数（サンプルの値。実際の積算基準ではない）"
    rt["A1"].font = Font(name=FONT, bold=True)
    rate_head = ["工種", "材料費比率", "労務費比率", "共通仮設係数", "共通仮設指数",
                 "現場管理係数", "現場管理指数", "工期指数", "法定福利費率"]
    rates = [
        ("建築", 0.55, 0.32, 0.080, -0.10, 0.200, -0.12, 0.10, 0.155),
        ("土木", 0.40, 0.35, 0.120, -0.12, 0.260, -0.13, 0.12, 0.160),
        ("設備", 0.62, 0.28, 0.060, -0.09, 0.180, -0.11, 0.08, 0.150),
    ]
    for c, h in enumerate(rate_head, 1):
        rt.cell(row=3, column=c, value=h)
    for i, row in enumerate(rates):
        for c, v in enumerate(row, 1):
            cell = rt.cell(row=4 + i, column=c, value=v)
            if c in (2, 3, 9):
                cell.number_format = "0.0%"
            elif c > 1:
                cell.number_format = "0.000"
    t = Table(displayName="率表", ref=f"A3:I{3 + len(rates)}")
    t.tableStyleInfo = TableStyleInfo(name="TableStyleMedium2", showRowStripes=True)
    rt.add_table(t)

    rt["A9"] = "一般管理費等率（工事原価が「工事原価の下限」以上のときの率）"
    rt["A9"].font = Font(name=FONT, bold=True)
    rt["A10"], rt["B10"] = "工事原価の下限", "一般管理費等率"
    bands = [(0, 0.140), (50_000_000, 0.120), (100_000_000, 0.110), (300_000_000, 0.100), (1_000_000_000, 0.090)]
    for i, (lo, rate) in enumerate(bands):
        rt.cell(row=11 + i, column=1, value=lo).number_format = "#,##0"
        rt.cell(row=11 + i, column=2, value=rate).number_format = "0.0%"
    t = Table(displayName="一般管理費率", ref=f"A10:B{10 + len(bands)}")
    t.tableStyleInfo = TableStyleInfo(name="TableStyleMedium2", showRowStripes=True)
    rt.add_table(t)
    for col, w in zip("ABCDEFGHI", [16, 14, 12, 13, 13, 13, 13, 10, 13]):
        rt.column_dimensions[col].width = w

    # ---- 計算: 入力欄（A 列の項目名 → B 列）と内訳（数式） ----
    cs = wb.create_sheet("計算", 1)
    cs["A1"] = "工事費内訳書"
    cs["A1"].font = Font(name=FONT, bold=True, size=16)
    cs["A3"] = "入力"
    cs["A3"].font = Font(name=FONT, bold=True)
    inputs = [("工事番号", None), ("工事名", None), ("工種", None), ("直接工事費", "#,##0"), ("工期", '0"か月"')]
    sample = jobs[0]
    for i, (label, fmt) in enumerate(inputs):
        r = 4 + i
        cs.cell(row=r, column=1, value=label).border = box
        v = cs.cell(row=r, column=2, value=sample[i])
        v.fill, v.border = input_fill, box
        if fmt:
            v.number_format = fmt
    dv = DataValidation(type="list", formula1='"建築,土木,設備"', allow_blank=False)
    dv.add("B6")
    cs.add_data_validation(dv)
    wb.defined_names["工種"] = DefinedName("工種", attr_text="計算!$B$6")
    wb.defined_names["工期"] = DefinedName("工期", attr_text="計算!$B$8")

    def rate(col):
        return f"INDEX(率表[{col}],MATCH(工種,率表[工種],0))"

    head_row = 10
    for c, h in enumerate(["項目", "率", "金額", "備考"], 1):
        cell = cs.cell(row=head_row, column=c, value=h)
        cell.font = Font(name=FONT, bold=True, color="FFFFFF")
        cell.fill, cell.border = head_fill, box
        cell.alignment = Alignment(horizontal="center")
    # (項目, 率, 金額, 備考, 合計の行か)
    items = [
        ("直接工事費", None, "=B7", "入力の直接工事費", True),
        ("　材料費", f"={rate('材料費比率')}", "=ROUNDDOWN(C11*B12,0)", "直接工事費 × 材料費比率", False),
        ("　労務費", f"={rate('労務費比率')}", "=ROUNDDOWN(C11*B13,0)", "直接工事費 × 労務費比率", False),
        ("　機械経費等", None, "=C11-C12-C13", "直接工事費の残り", False),
        ("共通仮設費", f"=ROUND({rate('共通仮設係数')}*(C11/1000000)^{rate('共通仮設指数')},4)",
         "=ROUNDDOWN(C11*B15,-3)", "率 = 係数 × (直接工事費/百万)^指数、千円未満切り捨て", False),
        ("純工事費", None, "=C11+C15", "直接工事費 ＋ 共通仮設費", True),
        ("現場管理費", f"=ROUND({rate('現場管理係数')}*(C16/1000000)^{rate('現場管理指数')}*(工期/12)^{rate('工期指数')},4)",
         "=ROUNDDOWN(C16*B17,-3)", "率 = 係数 × (純工事費/百万)^指数 × (工期/12)^工期指数", False),
        ("工事原価", None, "=C16+C17", "純工事費 ＋ 現場管理費", True),
        ("一般管理費等", "=LOOKUP(C18,一般管理費率[工事原価の下限],一般管理費率[一般管理費等率])",
         "=C20-C18", "工事価格との差（率で計算した額を万円未満切り捨てで調整）", False),
        ("工事価格", None, "=ROUNDDOWN(C18*(1+B19),-4)", "工事原価 × (1 ＋ 一般管理費等率)、万円未満切り捨て", True),
        ("消費税等相当額", 0.1, "=ROUNDDOWN(C20*B21,0)", "工事価格 × 消費税率", False),
        ("工事費計", None, "=C20+C21", "", True),
        (None, None, None, None, False),
        ("（参考）法定福利費", f"={rate('法定福利費率')}", "=ROUNDDOWN(C13*B24,0)", "労務費 × 法定福利費率（事業主負担分）", False),
    ]
    for i, (label, rt_, amount, note, total) in enumerate(items):
        r = head_row + 1 + i
        if label is None:
            continue
        cells = [cs.cell(row=r, column=c) for c in range(1, 5)]
        for cell, v in zip(cells, (label, rt_, amount, note)):
            cell.value = v
            cell.border = box
            if total:
                cell.fill = total_fill
                cell.font = Font(name=FONT, bold=True)
        cells[1].number_format = "0.00%"
        cells[2].number_format = "#,##0"
    for col, w in zip("ABCD", [20, 32, 16, 52]):
        cs.column_dimensions[col].width = w
    cs.column_dimensions["B"].width = 32  # 入力の工事名が入る幅
    cs.page_setup.orientation = "landscape"
    cs.page_setup.paperSize = cs.PAPERSIZE_A4
    cs.page_setup.fitToWidth = 1
    cs.page_setup.fitToHeight = 1
    cs.sheet_properties.pageSetUpPr.fitToPage = True
    for s in (ws, cs, rt):
        set_font(s)
    wb.save(os.path.join(ROOT, "積算.xlsx"))


STEPS = {"sales": sales, "report": report, "gantt": gantt, "csvs": csvs, "pdfs": pdfs, "estimate": estimate}
os.makedirs(ROOT, exist_ok=True)
for name in sys.argv[1:] or STEPS:
    STEPS[name]()
