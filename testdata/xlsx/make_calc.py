"""Writes calc.xlsx for the tests of the formula engine (src/api/xlsx_calc).

    python3 make_calc.py   (needs xlsxwriter)

The sheets:
  データ  40 rows of sales, with a formula column and names (one of this sheet only)
  数式    a formula in B and the value Excel gives for it in C, one per row; the
          formulas are saved without a result, as a library writes them
  配列    array formulas: one entered over a range, two that spill
  表      a table with a calculated column and formulas that name its columns
  共有    a shared formula as Excel saves a filled-down one (written into the
          sheet's XML here: xlsxwriter has none)
  保存値  formulas the engine leaves alone, with the results Excel saved

The values in C were checked against Excel (Office 2021), which recalculated
the workbook (tools/excel-verify).

The formulas are stored as Excel stores them: the functions newer than Excel
2007 with their _xlfn. prefix, LET's names with _xlpm., a spill reference
(G1#) as _xlfn.ANCHORARRAY(G1). Without them Excel shows #NAME? or does not
open the file at all; xlsxwriter adds the prefix to only some of them, so
to_file() puts them in after it.
"""
import datetime
import os
import re
import zipfile

import xlsxwriter

HERE = os.path.dirname(os.path.abspath(__file__))
PATH = os.path.join(HERE, "calc.xlsx")

CASES = [
    ('=SUM(データ!F2:F41)', 64050),
    ('=SUM(金額列)*税率', 6405),
    ('=ROUND(AVERAGE(データ!F2:F41),2)', 1601.25),
    ('=MAX(データ!D2:D41)-MIN(データ!D2:D41)', 12),
    ('=COUNT(データ!A2:A41)', 40),
    ('=COUNTA(データ!B2:B41)', 40),
    ('=COUNTBLANK(データ!A1:H45)', 73),
    ('=SUMIF(データ!B2:B41,"佐藤",データ!F2:F41)', 15950),
    ('=SUMIFS(データ!F2:F41,データ!B2:B41,"鈴木",データ!D2:D41,">=5")', 11800),
    ('=COUNTIF(データ!C2:C41,"*ん*")', 13),
    ('=COUNTIFS(データ!B2:B41,"田中",データ!G2:G41,"大")', 3),
    ('=AVERAGEIF(データ!G2:G41,"中",データ!F2:F41)', 960.7142857142857),
    ('=AVERAGEIFS(データ!F2:F41,データ!B2:B41,"伊藤",データ!E2:E41,">150")', 1733.3333333333333),
    ('=MAXIFS(データ!F2:F41,データ!B2:B41,"高橋")', 3600),
    ('=MINIFS(データ!F2:F41,データ!C2:C41,"もも")', 250),
    ('=SUMPRODUCT(データ!D2:D41,データ!E2:E41)', 64050),
    ('=SUMPRODUCT((データ!B2:B41="佐藤")*(データ!D2:D41>3)*データ!F2:F41)', 15150),
    ('=VLOOKUP("ぶどう",データ!C2:F41,4,FALSE)', 400),
    ('=VLOOKUP(7,データ!D2:F41,3,FALSE)', 700),
    ('=HLOOKUP("単価",データ!A1:G41,5,FALSE)', 300),
    ('=INDEX(データ!F2:F41,MATCH("高橋",データ!B2:B41,0))', 400),
    ('=MATCH(250,データ!E2:E41,0)', 3),
    ('=INDEX(データ!A2:G41,10,3)', 'なし'),
    ('=LOOKUP(9,データ!D2:D14)', 9),
    ('=OFFSET(データ!A1,5,3)', 10),
    ('=SUM(OFFSET(データ!F1,1,0,10,1))', 16100),
    ('=INDIRECT("データ!F"&5)', 900),
    ('=ROW(データ!C7)+COLUMN(データ!C7)', 10),
    ('=ROWS(データ!A2:G41)*COLUMNS(データ!A2:G41)', 280),
    ('=CHOOSE(3,"a","b","c","d")', 'c'),
    ('=IF(AND(データ!D2>3,OR(データ!E2>100,データ!E3>100)),"yes","no")', 'yes'),
    ('=IFERROR(1/0,"err")', 'err'),
    ('=IFERROR(VLOOKUP("x",データ!C2:F41,4,FALSE),"なし")', 'なし'),
    ('=ISERROR(1/0)', True),
    ('=ISNUMBER(データ!D2)', True),
    ('=ISTEXT(データ!B2)', True),
    ('=ISBLANK(データ!H2)', True),
    ('=NOT(ISNA(MATCH("佐藤",データ!B2:B41,0)))', True),
    ('=IFS(データ!D2>10,"A",データ!D2>5,"B",TRUE,"C")', 'B'),
    ('=SWITCH(データ!C2,"りんご",1,"みかん",2,9)', 2),
    ('=ABS(-3.5)+INT(-3.5)+TRUNC(-3.5)+SIGN(-2)', -4.5),
    ('=ROUND(2.5,0)+ROUND(-2.5,0)+ROUND(1234.567,-2)', 1200),
    ('=ROUNDUP(1.21,1)+ROUNDDOWN(1.29,1)', 2.5),
    ('=CEILING(7,3)+FLOOR(7,3)+MROUND(7,3)', 21),
    ('=MOD(-7,3)+MOD(7,-3)', 0),
    ('=QUOTIENT(17,5)', 3),
    ('=POWER(2,10)+SQRT(144)+EXP(0)+LN(1)+LOG10(1000)+LOG(8,2)', 1043),
    ('=PRODUCT(1,2,3,4)', 24),
    ('=FACT(5)+COMBIN(5,2)+PERMUT(5,2)', 150),
    ('=GCD(12,18)+LCM(4,6)', 18),
    ('=ROUND(PI(),4)', 3.1416),
    ('=ROUND(SIN(PI()/6)+COS(0)+TAN(0),6)', 1.5),
    ('=SUMSQ(1,2,3)', 14),
    ('=LEN("あいうabc")', 6),
    ('=LEFT("東京都千代田区",3)&RIGHT("東京都千代田区",2)&MID("東京都千代田区",4,3)', '東京都田区千代田'),
    ('=UPPER("abc")&LOWER("ABC")&PROPER("hello world")', 'ABCabcHello World'),
    ('=TRIM("  a  b  ")', 'a b'),
    ('=SUBSTITUTE("a-b-c","-","+")', 'a+b+c'),
    ('=SUBSTITUTE("a-b-c","-","+",2)', 'a-b+c'),
    ('=REPLACE("abcdef",2,3,"XY")', 'aXYef'),
    ('=FIND("c","abcabc")+SEARCH("C","abcabc",4)', 9),
    ('=REPT("ab",3)', 'ababab'),
    ('=CONCATENATE("a","b",1)', 'ab1'),
    ('=TEXT(1234.5,"#,##0.00")', '1,234.50'),
    ('=TEXT(0.256,"0.0%")', '25.6%'),
    ('=TEXT(データ!A2,"yyyy/mm/dd")', '2026/01/04'),
    ('=TEXT(データ!A2,"yyyy年m月d日")', '2026年1月4日'),
    ('=TEXT(12,"000")', '012'),
    ('=VALUE("1,234")+VALUE("12%")', 1234.12),
    ('=EXACT("a","A")', False),
    ('=CODE("A")', 65),
    ('=CHAR(65)', 'A'),
    ('=T("x")&N(5)', 'x5'),
    ('=TEXTJOIN(",",TRUE,"a","","b")', 'a,b'),
    ('=CONCAT("a","b","c")', 'abc'),
    ('=NUMBERVALUE("1.5")', 1.5),
    ('=FIXED(1234.567,1)', '1,234.6'),
    ('=YEAR(データ!A2)*10000+MONTH(データ!A2)*100+DAY(データ!A2)', 20260104),
    ('=WEEKDAY(データ!A2)', 1),
    ('=WEEKDAY(データ!A2,2)', 7),
    ('=EOMONTH(DATE(2026,1,15),1)', 46081),
    ('=EDATE(DATE(2026,1,31),1)', 46081),
    ('=DATEDIF(DATE(2020,3,15),DATE(2026,1,10),"Y")', 5),
    ('=DATEDIF(DATE(2020,3,15),DATE(2026,1,10),"YM")', 9),
    ('=DATEDIF(DATE(2020,3,15),DATE(2026,1,10),"MD")', 26),
    ('=DAYS(DATE(2026,3,1),DATE(2026,1,1))', 59),
    ('=NETWORKDAYS(DATE(2026,1,1),DATE(2026,1,31))', 22),
    ('=WORKDAY(DATE(2026,1,1),10)', 46037),
    ('=WEEKNUM(DATE(2026,3,1))', 10),
    ('=ISOWEEKNUM(DATE(2026,1,1))', 1),
    ('=TIME(13,30,0)*24', 13.5),
    ('=HOUR(0.75)+MINUTE(0.76)+SECOND(0.7501)', 41),
    ('=DATEVALUE("2026/4/1")', 46113),
    ('=YEARFRAC(DATE(2026,1,1),DATE(2026,7,1))', 0.5),
    ('=MEDIAN(データ!F2:F41)', 1350),
    ('=MODE(データ!D2:D41)', 8),
    ('=LARGE(データ!F2:F41,3)+SMALL(データ!F2:F41,3)', 3850),
    ('=RANK(データ!F5,データ!F2:F41)', 28),
    ('=ROUND(STDEV(データ!F2:F41),4)', 1135.9785),
    ('=ROUND(STDEVP(データ!F2:F41),4)', 1121.6889),
    ('=ROUND(VAR(データ!D2:D41),4)', 14.025),
    ('=ROUND(PERCENTILE(データ!F2:F41,0.9),4)', 3185),
    ('=ROUND(QUARTILE(データ!F2:F41,1),4)', 675),
    ('=ROUND(CORREL(データ!D2:D41,データ!F2:F41),6)', 0.780512),
    ('=SUBTOTAL(9,データ!F2:F41)', 64050),
    ('=SUBTOTAL(103,データ!B2:B41)', 40),
    ('=AGGREGATE(9,6,データ!F2:F41)', 64050),
    ('=ROUND(PMT(0.05/12,36,-1000000),2)', 29970.9),
    ('=ROUND(FV(0.03,10,-10000),2)', 114638.79),
    ('=ROUND(PV(0.03,10,-10000),2)', 85302.03),
    ('=ROUND(NPV(0.1,-1000,300,400,500),4)', -19.1244),
    ('=ROUND(IRR({-1000,300,400,500}),6)', 0.088963),
    ('=ROUND(NPER(0.05,-100,1000),4)', 14.2067),
    ('=DEC2BIN(10)&"/"&DEC2HEX(255)&"/"&BIN2DEC("1010")&"/"&HEX2DEC("FF")', '1010/FF/10/255'),
    ('=SUM({1,2;3,4})', 10),
    ('=SUM(データ!D2:D5 データ!D4:D8)', 12),
    ('=XLOOKUP("みかん",データ!C2:C41,データ!F2:F41)', 1200),
    ('=XLOOKUP("zzz",データ!C2:C41,データ!F2:F41,"なし")', 'なし'),
    ('=XMATCH(250,データ!E2:E41)', 3),
    ('=SUM(FILTER(データ!F2:F41,データ!B2:B41="佐藤"))', 15950),
    ('=COUNTA(UNIQUE(データ!C2:C41))', 6),
    ('=INDEX(SORT(データ!F2:F41,1,-1),1)', 4550),
    ('=SUM(SEQUENCE(10))', 55),
    ('=LET(x,5,y,x*2,x+y)', 15),
    ('=IFNA(MATCH("zzz",データ!C2:C41,0),-1)', -1),
    ('=TEXTJOIN("/",TRUE,UNIQUE(データ!B2:B6))', '鈴木/高橋/田中/伊藤/佐藤'),
    ('=MAX(IF(データ!B2:B41="佐藤",データ!F2:F41))', 4200),
    ('=1/3', 0.3333333333333333),
    ('=0.1+0.2', 0.30000000000000004),
    ('=1E+20+1', 100000000000000000000),
    ('=2^0.5', 1.4142135623730951),
    ('=10%', 0.1),
    ('=-2^2', 4),
    ('="a"&1+2', 'a3'),
    ('=1=1', True),
    ('="a"<"b"', True),
    ('=1<>2', True),
    ('=TRUE+TRUE', 2),
    ('=""=0', False),
]

wb = xlsxwriter.Workbook(PATH)
ws = wb.add_worksheet("データ")
date = wb.add_format({"num_format": "yyyy/m/d"})
ws.write_row(0, 0, ["日付", "担当", "商品", "数量", "単価", "金額", "区分"])
people = ["佐藤", "鈴木", "高橋", "田中", "伊藤"]
goods = ["りんご", "みかん", "ぶどう", "もも", "なし", "ﾊﾞﾅﾅ"]
for i in range(1, 41):
    ws.write_datetime(i, 0, datetime.date(2026, 1, 1) + datetime.timedelta(days=i * 3), date)
    ws.write(i, 1, people[i % 5])
    ws.write(i, 2, goods[i % 6])
    ws.write(i, 3, (i * 7) % 13 + 1)
    ws.write(i, 4, 100 + (i % 6) * 50)
    ws.write_formula(i, 5, f"=D{i+1}*E{i+1}", None, "")
    ws.write_formula(i, 6, f'=IF(F{i+1}>=1500,"大",IF(F{i+1}>=600,"中","小"))', None, "")
wb.define_name("税率", "=0.1")
wb.define_name("金額列", "=データ!$F$2:$F$41")
wb.define_name("データ!数量列", "=データ!$D$2:$D$41")
# A name of this sheet is not seen from the others.
ws.write_formula("I1", "=SUM(数量列)", None, "")

f = wb.add_worksheet("数式")
for i, (formula, expected) in enumerate(CASES):
    f.write_string(i, 0, formula)
    if formula.startswith("=MAX(IF("):
        f.write_array_formula(i, 1, i, 1, "{" + formula + "}", None, "")
    else:
        f.write_formula(i, 1, formula, None, "")
    f.write(i, 2, expected)

g = wb.add_worksheet("配列")
g.write_column(0, 0, [1, 2, 3, 4, 5])
g.write_column(0, 1, [10, 20, 30, 40, 50])
g.write_array_formula("C1:C5", "{=A1:A5*B1:B5}", None, "")
g.write_dynamic_array_formula("E1", "=FILTER(A1:A5,B1:B5>20)", None, "")
g.write_dynamic_array_formula("G1:G5", "=SORT(B1:B5,1,-1)", None, "")
g.write_formula("I1", "=SUM(C1:C5)", None, "")
g.write_formula("I2", "=SUM(E1:E3)", None, "")
# Without dynamic-array metadata Excel shows the one value in the formula's row.
g.write_formula("I3", "=A1:A5", None, "")
g.write_formula("I4", "=SUM(G1#)", None, "")

t = wb.add_worksheet("表")
t.add_table("A1:D6", {
    "name": "売上表",
    "columns": [{"header": "品名"}, {"header": "数量"}, {"header": "単価"}, {"header": "金額", "formula": "=[@数量]*[@単価]"}],
    "data": [["a", 1, 100], ["b", 2, 200], ["c", 3, 300], ["d", 4, 400], ["e", 5, 500]],
})
t.write_formula("F1", "=SUM(売上表[金額])", None, "")
t.write_formula("F2", '=SUMIF(売上表[品名],"c",売上表[数量])', None, "")
t.write_formula("F3", "=COUNTA(売上表[#Headers])", None, "")
t.write_formula("F4", "=ROWS(売上表[#Data])", None, "")

s = wb.add_worksheet("共有")
s.write_row(0, 0, ["数量", "単価", "金額", "累計"])
for i in range(1, 6):
    s.write(i, 0, i)
    s.write(i, 1, 100 * i)
    s.write_formula(i, 2, f"=A{i+1}*B{i+1}*(1+税率)", None, "")
    s.write_formula(i, 3, f"=SUM(C$2:C{i+1})", None, "")

k = wb.add_worksheet("保存値")
k.write("A1", 5)
# Results as Excel saved them: the engine has nothing to work these out with.
k.write_formula("B1", "=MYFUNC(A1)", None, 123)
k.write_formula("B2", "='[other.xlsx]Sheet1'!$A$1+A1", None, 42)
k.write_formula("B3", "=未定義の名前*2", None, 7)
k.write_formula("B4", "=PHONETIC(A1)", None, "ゴ")
# These depend on them and on A1.
k.write_formula("C1", "=B1+A1", None, 128)
k.write_formula("C2", "=B2*2", None, 84)
# A formula with a result that does not change stays as Excel saved it
# until something it depends on changes.
k.write_formula("D1", "=A1*10", None, 999)
wb.close()

# 共有!C2:C6 and D2:D6 as shared formulas: the first cell has the text, the others only the index.
def share(xml, col, si):
    for row in range(2, 7):
        cell = f'<c r="{col}{row}"'
        a = xml.index(cell)
        b = xml.index("</c>", a)
        body = xml[a:b]
        fa, fb = body.index("<f>"), body.index("</f>")
        text = body[fa + 3:fb]
        new = f'<f t="shared" ref="{col}2:{col}6" si="{si}">{text}</f>' if row == 2 else f'<f t="shared" si="{si}"/>'
        xml = xml[:a] + body[:fa] + new + body[fb + 4:] + xml[b:]
    return xml

# The functions of CASES that need _xlfn. in the file and that xlsxwriter
# leaves without it.
FUTURE = ["MAXIFS", "MINIFS", "IFS", "TEXTJOIN", "CONCAT", "NUMBERVALUE", "DAYS", "ISOWEEKNUM",
          "AGGREGATE", "XMATCH", "IFNA", "LET"]

def to_file(xml):
    def formula(m):
        s = re.sub(r'(?<![\w.])(' + "|".join(FUTURE) + r')\(', r'_xlfn.\1(', m.group(2))
        s = s.replace("_xlfn.LET(x,5,y,x*2,x+y)", "_xlfn.LET(_xlpm.x,5,_xlpm.y,_xlpm.x*2,_xlpm.x+_xlpm.y)")
        s = re.sub(r'\b([A-Z]+[0-9]+)#', r'_xlfn.ANCHORARRAY(\1)', s)
        return m.group(1) + s + "</f>"
    return re.sub(r'(<f(?: [^>]*)?>)([^<]*)</f>', formula, xml)

tmp = PATH + ".tmp"
with zipfile.ZipFile(PATH) as zin, zipfile.ZipFile(tmp, "w", zipfile.ZIP_DEFLATED) as zout:
    names = zin.read("xl/workbook.xml").decode()
    # The sheets are written in order: 共有 is the fifth.
    assert names.index('name="共有"') > names.index('name="表"')
    for item in zin.infolist():
        data = zin.read(item.filename)
        if item.filename == "xl/worksheets/sheet5.xml":
            xml = share(share(data.decode(), "C", 0), "D", 1)
            data = xml.encode()
        if item.filename.startswith("xl/worksheets/sheet"):
            data = to_file(data.decode()).encode()
        zout.writestr(item, data)
os.replace(tmp, PATH)
print("wrote", PATH)
