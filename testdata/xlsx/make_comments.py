"""Builds comments.xlsx: threaded comments and a note as Excel writes them,
for testdata/scripts/comments.test.js. Run: python3 testdata/xlsx/make_comments.py

The parts follow a workbook saved by Excel (Microsoft 365): H5 a thread of
one comment (emoji), I5 a thread with two replies, J5 a resolved thread with
a reply that mentions its author; each with the note Excel writes in its
place (author tc={id}, xr:uid) and its shape in the VML. K5 is a note
(Excel's old comment) with the author's name in bold as Excel writes it.
The author is a made-up person."""
import os, zipfile

PERSON = "{6A1C2E4F-1111-4A2B-9C3D-000000000001}"
H5, I5, I5R1, I5R2 = ("{F1B4CB8C-46DF-4C4F-A254-EF85EE259089}", "{561F93A2-A1B0-4E6D-87F0-08C41F4B7D0D}",
                      "{5D7A9E24-084B-42BE-A587-897A2978CFAD}", "{22266DC5-9C17-4DEF-AE79-D7997F21BF17}")
J5, J5R1 = "{20DBF0A1-1511-42CD-A07C-EB724B664232}", "{3BA5189F-F472-498C-8EBD-172C6D9FA4BA}"
DECL = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
MAIN = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
TC = "http://schemas.microsoft.com/office/spreadsheetml/2018/threadedcomments"

def fallback(text, replies=()):
    head = ("[スレッド化されたコメント]\n\n使用している Excel のバージョンでは、このスレッド化されたコメントを表示できますが、"
            "新しいバージョンの Excel でファイルを開いた場合はコメントに対する編集がすべて削除されます。"
            "詳細: https://go.microsoft.com/fwlink/?linkid=870924\n\nコメント:\n    ")
    return head + text + "".join(f"\n返信:\n{r}" for r in replies)

def note_shape(n, col, row):
    return f'''<v:shape id="_x0000_s{1024 + n}" type="#_x0000_t202" style='position:absolute;
  margin-left:{400 + 54 * n}pt;margin-top:67.5pt;width:96pt;height:59.25pt;z-index:{n};
  visibility:hidden' fillcolor="infoBackground [80]" strokecolor="none [81]"
  o:insetmode="auto">
  <v:fill color2="infoBackground [80]"/>
  <v:shadow color="none [81]" obscured="t"/>
  <v:path o:connecttype="none"/>
  <v:textbox style='mso-direction-alt:auto'>
   <div style='text-align:left'></div>
  </v:textbox>
  <x:ClientData ObjectType="Note">
   <x:MoveWithCells/>
   <x:SizeWithCells/>
   <x:Anchor>
    {col + 1}, 15, {row - 2}, 15, {col + 2}, 71, {row + 1}, 19</x:Anchor>
   <x:AutoFill>False</x:AutoFill>
   <x:Row>{row - 1}</x:Row>
   <x:Column>{col - 1}</x:Column>
  </x:ClientData>
 </v:shape>'''

def threaded(ref, dt, cid, text, parent=None, done=False, mentions=""):
    p = f' parentId="{parent}"' if parent else ""
    d = ' done="1"' if done else ""
    return (f'<threadedComment ref="{ref}" dT="{dt}" personId="{PERSON}" id="{cid}"{p}{d}>'
            f'<text>{text}</text>{mentions}</threadedComment>')

parts = {
    "[Content_Types].xml": DECL + '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="vml" ContentType="application/vnd.openxmlformats-officedocument.vmlDrawing"/>'
        '<Default Extension="xml" ContentType="application/xml"/>'
        '<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>'
        '<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>'
        '<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>'
        '<Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/>'
        '<Override PartName="/xl/comments1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml"/>'
        '<Override PartName="/xl/threadedComments/threadedComment1.xml" ContentType="application/vnd.ms-excel.threadedcomments+xml"/>'
        '<Override PartName="/xl/persons/person.xml" ContentType="application/vnd.ms-excel.person+xml"/></Types>',
    "_rels/.rels": DECL + '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>',
    "xl/workbook.xml": DECL + f'<workbook xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">'
        '<sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets><calcPr calcId="191029"/></workbook>',
    "xl/_rels/workbook.xml.rels": DECL + '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>'
        '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>'
        '<Relationship Id="rId5" Type="http://schemas.microsoft.com/office/2017/10/relationships/person" Target="persons/person.xml"/>'
        '<Relationship Id="rId4" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/></Relationships>',
    "xl/styles.xml": DECL + f'<styleSheet xmlns="{MAIN}"><fonts count="1"><font><sz val="11"/><name val="游ゴシック"/><family val="3"/><charset val="128"/></font></fonts>'
        '<fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills>'
        '<borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders>'
        '<cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs>'
        '<cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/></cellXfs>'
        '<cellStyles count="1"><cellStyle name="標準" xfId="0" builtinId="0"/></cellStyles></styleSheet>',
    "xl/sharedStrings.xml": DECL + f'<sst xmlns="{MAIN}" count="2" uniqueCount="2"><si><t>a</t></si><si><t>s</t></si></sst>',
    "xl/worksheets/sheet1.xml": DECL + f'<worksheet xmlns="{MAIN}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">'
        '<dimension ref="H5:K5"/><sheetData><row r="5" spans="8:11"><c r="H5" t="s"><v>0</v></c><c r="I5" t="s"><v>1</v></c></row></sheetData>'
        '<pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/><legacyDrawing r:id="rId1"/></worksheet>',
    "xl/worksheets/_rels/sheet1.xml.rels": DECL + '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        '<Relationship Id="rId3" Type="http://schemas.microsoft.com/office/2017/10/relationships/threadedComment" Target="../threadedComments/threadedComment1.xml"/>'
        '<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments" Target="../comments1.xml"/>'
        '<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/vmlDrawing" Target="../drawings/vmlDrawing1.vml"/></Relationships>',
    "xl/comments1.xml": DECL + f'<comments xmlns="{MAIN}" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" mc:Ignorable="xr" '
        'xmlns:xr="http://schemas.microsoft.com/office/spreadsheetml/2014/revision">'
        f'<authors><author>tc={H5}</author><author>tc={I5}</author><author>tc={J5}</author><author>佐藤 花子</author></authors><commentList>'
        f'<comment ref="H5" authorId="0" shapeId="0" xr:uid="{H5}"><text><t>{fallback("👤😀絵文字")}</t></text></comment>'
        f'<comment ref="I5" authorId="1" shapeId="0" xr:uid="{I5}"><text><t>{fallback("かきくけこ", ["返信１", "返信２"])}</t></text></comment>'
        f'<comment ref="J5" authorId="2" shapeId="0" xr:uid="{J5}"><text><t>{fallback("あいうえお", ["@山田 太郎 返信ですね"])}</t></text></comment>'
        '<comment ref="K5" authorId="3" shapeId="0" xr:uid="{9E0B7C11-2222-4F3A-8D4E-000000000002}"><text>'
        '<r><rPr><b/><sz val="9"/><color indexed="81"/><rFont val="MS P ゴシック"/><family val="3"/><charset val="128"/></rPr><t>佐藤 花子:</t></r>'
        '<r><rPr><sz val="9"/><color indexed="81"/><rFont val="MS P ゴシック"/><family val="3"/><charset val="128"/></rPr><t xml:space="preserve">\n古いメモ</t></r>'
        '</text></comment></commentList></comments>',
    "xl/drawings/vmlDrawing1.vml": '<xml xmlns:v="urn:schemas-microsoft-com:vml"\n xmlns:o="urn:schemas-microsoft-com:office:office"\n'
        ' xmlns:x="urn:schemas-microsoft-com:office:excel">\n <o:shapelayout v:ext="edit">\n  <o:idmap v:ext="edit" data="1"/>\n'
        ' </o:shapelayout><v:shapetype id="_x0000_t202" coordsize="21600,21600" o:spt="202"\n  path="m,l,21600r21600,l21600,xe">\n'
        '  <v:stroke joinstyle="miter"/>\n  <v:path gradientshapeok="t" o:connecttype="rect"/>\n </v:shapetype>'
        + note_shape(1, 8, 5) + note_shape(2, 9, 5) + note_shape(3, 10, 5) + note_shape(4, 11, 5) + "</xml>",
    "xl/threadedComments/threadedComment1.xml": DECL + f'<ThreadedComments xmlns="{TC}" xmlns:x="{MAIN}">'
        + threaded("H5", "2026-11-15T23:10:33.81", H5, "👤😀絵文字")
        + threaded("I5", "2026-11-15T23:09:53.90", I5, "かきくけこ")
        + threaded("I5", "2026-11-15T23:10:45.37", I5R1, "返信１", parent=I5)
        + threaded("I5", "2026-11-15T23:10:52.44", I5R2, "返信２", parent=I5)
        + threaded("J5", "2026-11-15T23:09:34.87", J5, "あいうえお", done=True)
        + threaded("J5", "2026-11-15T23:11:10.22", J5R1, "@山田 太郎 返信ですね", parent=J5,
                   mentions=f'<mentions><mention mentionpersonId="{PERSON}" mentionId="{{0B5E7A31-3333-4C2D-9E8F-000000000003}}" startIndex="0" length="6"/></mentions>')
        + "</ThreadedComments>",
    "xl/persons/person.xml": DECL + f'<personList xmlns="{TC}" xmlns:x="{MAIN}">'
        f'<person displayName="山田 太郎" id="{PERSON}" userId="S::taro@example.com::00000000-0000-0000-0000-000000000001" providerId="AD"/></personList>',
}

out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "comments.xlsx")
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
    for name, data in parts.items():
        info = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
        info.compress_type = zipfile.ZIP_DEFLATED
        z.writestr(info, data)
print(out)
