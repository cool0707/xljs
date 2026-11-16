# シートの名前の変更を確かめるブックを Excel に作らせる（pwsh 7、Excel が必要。rename.ps1 が使う）
# シート「元」と、それを参照する数式・名前・入力規則・条件付き書式・スパークライン・グラフ・ピボット・フォームコントロール・ハイパーリンクを置いたシート「参照」
param([string]$Out)
$ErrorActionPreference = 'Stop'
$Out = [IO.Path]::GetFullPath($Out)
$xl = New-Object -ComObject Excel.Application
$xl.DisplayAlerts = $false
try {
    $wb = $xl.Workbooks.Add()
    while ($wb.Worksheets.Count -lt 2) { [void]$wb.Worksheets.Add([Type]::Missing, $wb.Worksheets.Item($wb.Worksheets.Count)) }
    $src = $wb.Worksheets.Item(1); $src.Name = '元'
    $ref = $wb.Worksheets.Item(2); $ref.Name = '参照'
    $src.Range('A1:B1').Value2 = [object[]]@('名前', '数')
    $names = 'あ', 'い', 'う', 'え'
    for ($i = 0; $i -lt 4; $i++) { $src.Cells.Item($i + 2, 1).Formula = [string]$names[$i]; $src.Cells.Item($i + 2, 2).Formula = [string](($i + 1) * 10) }
    $src.Range("D2:G5").Formula = "=ROW()*COLUMN()"
    # 数式・名前
    $ref.Range('A1').Formula = '=元!B2+1'
    $ref.Range('A2').Formula = '=SUM(元!$B$2:$B$5)'
    [void]$wb.Names.Add('範囲', '=元!$B$2:$B$5')
    $ref.Range('A3').Formula = '=SUM(範囲)'
    $ref.Range('A4').Formula = '=INDIRECT("元!B2")'
    # 入力規則（他のシート）
    $ref.Range('B1').Validation.Add(3, 1, 1, '=元!$A$2:$A$5')
    # 条件付き書式（他のシート）
    $fc = $ref.Range('C1:C4').FormatConditions.Add(2, [Type]::Missing, '=C1>元!$B$2')
    $fc.Interior.Color = 255
    # スパークライン
    [void]$ref.Range('D1:D4').SparklineGroups.Add(1, '元!D2:G5')
    # グラフ
    $co = $ref.ChartObjects().Add(300, 10, 300, 200)
    $co.Chart.SetSourceData($src.Range('A1:B5'))
    # ピボット（範囲から）
    $pc = $wb.PivotCaches().Create(1, '元!R1C1:R5C2')
    $pt = $pc.CreatePivotTable($ref.Range('F10'), 'ピボット1')
    $pt.PivotFields('名前').Orientation = 1
    [void]$pt.AddDataField($pt.PivotFields('数'))
    # フォームコントロール
    $dd = $ref.Shapes.AddFormControl(2, 300, 230, 120, 20)
    $dd.ControlFormat.ListFillRange = '元!$A$2:$A$5'
    $dd.ControlFormat.LinkedCell = '元!$C$2'
    # ハイパーリンク
    [void]$ref.Hyperlinks.Add($ref.Range('A6'), '', '元!A1', [Type]::Missing, '元へ')
    # 印刷タイトル・印刷範囲
    $src.PageSetup.PrintTitleRows = '$1:$1'
    $src.PageSetup.PrintArea = '$A$1:$B$5'
    $ref.Activate()
    $wb.SaveAs($Out, 51)
    $wb.Close($false)
} finally { $xl.Quit(); [void][Runtime.InteropServices.Marshal]::ReleaseComObject($xl) }
