# excel-make.ps1 <out.xlsx>: Excel 自身にブックを作らせる（xljs の読み込みの確認用）。数値の標準の表示・表示形式・Excel で入力した数式・テーブル・書式
param([string]$Out)
$ErrorActionPreference = 'Stop'
$xl = New-Object -ComObject Excel.Application
$xl.DisplayAlerts = $false
try {
  $wb = $xl.Workbooks.Add()
  while ($wb.Worksheets.Count -lt 4) { [void]$wb.Worksheets.Add([Type]::Missing, $wb.Worksheets.Item($wb.Worksheets.Count)) }
  $s1 = $wb.Worksheets.Item(1); $s1.Name = '標準'
  $s2 = $wb.Worksheets.Item(2); $s2.Name = '表示形式'
  $s3 = $wb.Worksheets.Item(3); $s3.Name = '数式'
  $s4 = $wb.Worksheets.Item(4); $s4.Name = '書式'

  # General numbers
  $nums = @('=1/3','=2/3','=-1/3','=1/7*1000','=1/7*1000000','=1/7/1000','=1/7/100000','=PI()*1E10','=PI()*1E11',
    '12345678901','123456789012','1234567890123','99999999999','100000000000','1E15','1E20','1.5E-5','0.0001234567891','1E-10',
    '=2^0.5','=10/3*1E5','0.1','=0.1+0.2','123.456','-0.000001','=1E100','=-1E-100','1234567.891','=12345.678901234','0.5','=1E-5','=1E-4/3',
    '=40000+1/3','999999999.99','=SQRT(2)*1E-7','=1E21+1','=-123456789012','=1/3*1E-9')
  for ($i = 0; $i -lt $nums.Count; $i++) { $s1.Cells.Item($i + 1, 1).Formula = $nums[$i] }

  # Number formats: value in A, format in B (as text), formatted cell in C
  $fmts = @(
    @(1234.5, '#,##0'), @(1234.5, '#,##0.00'), @(-1234.5, '#,##0;[赤]-#,##0'), @(0.256, '0.0%'), @(1234, '"¥"#,##0'), @(-1234, '¥#,##0;¥-#,##0'),
    @(1234.5678, '0.00E+00'), @(0.75, '# ?/?'), @(1.25, '# ??/??'), @(12, '000'), @(46113, 'yyyy/m/d'), @(46113, 'yyyy"年"m"月"d"日"'),
    @(46113, 'ggge"年"m"月"d"日"'), @(46113, '[$-ja-JP]ggge"年"m"月"d"日"'), @(46113, 'aaaa'), @(46113, 'aaa'), @(46113, 'ddd mmm d'), @(46113.5625, 'yyyy/mm/dd hh:mm:ss'),
    @(0.5625, 'h:mm AM/PM'), @(1.5, '[h]:mm'), @(0.000694444, 'mm:ss'), @(46113, 'm/d'), @(46113, 'mmmm'), @(12345, '[DBNum1]G/標準'), @(12345, '[DBNum2]G/標準'), @(12345, '[DBNum3]G/標準'),
    @(1234.5, '@'), @(0, '0.00;-0.00;"ゼロ"'), @(5, '0" 個"'), @(1234567, '#,##0,"千"'), @(0.5, '?/?'), @(46113, 'yyyy-mm-dd'), @(46113, 'e/m/d'),
    @(-0.5, '0.0;(0.0)'), @(3.14159, '0.000'), @(1234.5, 'G/標準'), @(46113, '[$-411]ge.m.d'), @(46113.75, 'h"時"mm"分"'))
  for ($i = 0; $i -lt $fmts.Count; $i++) {
    $s2.Cells.Item($i + 1, 1).Value2 = [double]$fmts[$i][0]
    $s2.Cells.Item($i + 1, 2).NumberFormat = '@'
    $s2.Cells.Item($i + 1, 2).Formula = [string]$fmts[$i][1]
    $c = $s2.Cells.Item($i + 1, 3); $c.Value2 = [double]$fmts[$i][0]; $c.NumberFormatLocal = $fmts[$i][1]
  }
  # Other values
  $s2.Range('E1').Value2 = '日本語の文字列'
  $s2.Range('E2').Value2 = $true
  $s2.Range('E3').Value2 = $false
  $s2.Range('E4').Formula = '=1/0'
  $s2.Range('E5').Formula = '=NA()'
  $s2.Range('E6').Formula = '=""'
  $s2.Range('E7').Value2 = "改行を`n含む"
  $s2.Range('E8').Formula = "'00123"
  $s2.Range('E9').Value2 = '𠮷野家 🍣'
  $s2.Range('E10').Formula = '=DATE(2026,4,1)+TIME(9,30,0)'

  # Formulas entered in Excel
  $s3.Range('A1:A10').Formula = '=ROW()*10'
  $s3.Range('B1').Formula = '=A1*2'
  $s3.Range('B1:B10').FillDown()
  $s3.Range('C1').Formula2 = '=SEQUENCE(4,2)'
  $s3.Range('E1').Formula2 = '=FILTER(A1:A10,A1:A10>60)'
  $s3.Range('F1').Formula2 = '=SUM(E1#)'
  $s3.Range('F2').Formula2 = '=XLOOKUP(50,A1:A10,B1:B10)'
  $s3.Range('F3').Formula2 = '=LET(a,A1,b,A2,a*b)'
  $s3.Range('F4').Formula2 = '=TEXTJOIN("-",TRUE,SORT(UNIQUE(A1:A3),,-1))'
  $s3.Range('F5').Formula2 = '=LAMBDA(x,x*3)(A2)'
  $s3.Range('F6').Formula2 = '=MAXIFS(A1:A10,B1:B10,">100")'
  $s3.Range('F7').Formula2 = '=@A1:A10'
  $s3.Range('F8').Formula2 = '=SUM(A1:A10*B1:B10)'
  $s3.Range('F9').FormulaArray = '=SUM(A1:A10*B1:B10)'
  $s3.Range('G1:G3').FormulaArray = '=A1:A3+1'
  $s3.Range('F10').Formula2 = "='書式'!A1&""/""&標準!A1"
  $wb.Names.Add('倍率', '=3') | Out-Null
  $s3.Range('F11').Formula2 = '=A1*倍率'
  $s3.Range('F12').Formula2 = '=TEXT(DATE(2026,4,1),"ggge年m月d日")'
  $s3.Range('F13').Formula2 = '=DATEDIF(DATE(2020,1,31),DATE(2026,3,1),"MD")'
  $s3.Range('F14').Formula2 = '=ROUND(1234.5678,-2)&"|"&FIXED(-1234.5)&"|"&DOLLAR(1234.5)'
  $s3.Range('F15').Formula2 = '=JIS("abc123")&ASC("ＡＢＣ１２３")'
  $s3.Range('F16').Formula2 = '=TEXT(-0.5,"0.0;(0.0)")&TEXT(1/3,"G/標準")'
  $s3.Range('F17').Formula2 = '=VALUE("令和8年4月1日")'
  $s3.Range('F18').Formula2 = '=""&1/3'
  $s3.Range('F19').Formula2 = '=TAKE(SORTBY(A1:B10,B1:B10,-1),2)'
  $s3.Range('F20').Formula2 = '=TEXTSPLIT("a,b,c",",")'
  # A table with structured references
  $s3.Range('A13:C13').Value2 = [object[]]@('品名','数量','単価')
  $data = @(@('りんご',3,120),@('みかん',5,80),@('ぶどう',1,600))
  for ($r = 0; $r -lt 3; $r++) { for ($c = 0; $c -lt 3; $c++) { $s3.Cells.Item(14 + $r, 1 + $c).Formula = [string]$data[$r][$c] } }
  $lo = $s3.ListObjects.Add(1, $s3.Range('A13:C16'), [Type]::Missing, 1)
  $lo.Name = '果物'
  $lo.ListColumns.Add() | Out-Null
  $lo.ListColumns.Item(4).Name = '金額'
  $lo.ListColumns.Item(4).DataBodyRange.Formula = '=[@数量]*[@単価]'
  $s3.Range('H13').Formula2 = '=SUM(果物[金額])'
  $s3.Range('H14').Formula2 = '=ROWS(果物[#Data])&"/"&COUNTA(果物[#Headers])'

  # Formatting
  $s4.Range('A1').Value2 = '結合'
  $s4.Range('A1:C2').Merge()
  $s4.Range('A4').Value2 = '太字赤'
  $s4.Range('A4').Font.Bold = $true; $s4.Range('A4').Font.Color = 255
  $s4.Range('B4').Value2 = '塗り'; $s4.Range('B4').Interior.Color = 0xF7EBDD
  $s4.Range('C4').Value2 = '罫線'; $s4.Range('C4').Borders.LineStyle = 1
  $s4.Range('D4').Value2 = 'リッチテキスト'
  $s4.Range('D4').Characters(1, 3).Font.Bold = $true
  $s4.Range('D4').Characters(4, 4).Font.Color = 0xFF0000
  $s4.Range('A5').Value2 = 12345.678; $s4.Range('A5').Style = 'Comma [0]'
  $s4.Range('A6').Value2 = 'ふりがな'; $s4.Range('A6').HorizontalAlignment = -4108
  $s4.Columns.Item(5).ColumnWidth = 30
  $s4.Rows.Item(8).RowHeight = 40
  $s4.Rows.Item(9).Hidden = $true

  $wb.SaveAs([IO.Path]::GetFullPath($Out), 51)
  $wb.Close($false)
} finally { $xl.Quit() }


