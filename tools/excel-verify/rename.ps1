# シートの名前を変えたとき、そのシートを参照するものが Excel と同じく付いてくるかを確かめる（pwsh 7、Excel が必要）。
#   cargo build
#   pwsh -File tools/excel-verify/rename.ps1 [-Exe target/debug/xljs.exe] [-OutDir $env:TEMP\xljs-excel-rename]
# Excel に作らせたブック（excel-make-rename.ps1）のシート「元」を、Excel と xljs でそれぞれ「元 データ」に変えて保存し、
# 両方を Excel で開いて、数式・名前・入力規則・条件付き書式・スパークライン・グラフ・ピボット・フォームコントロール・ハイパーリンクを比べる
param(
    [string]$Exe = (Join-Path $PSScriptRoot '../../target/debug/xljs.exe'),
    [string]$OutDir = (Join-Path $env:TEMP 'xljs-excel-rename')
)
$ErrorActionPreference = 'Stop'
$Exe = (Resolve-Path $Exe).Path
New-Item -ItemType Directory -Force $OutDir | Out-Null
$src = Join-Path $OutDir 'rename.xlsx'
$byExcel = Join-Path $OutDir 'excel.xlsx'
$byXljs = Join-Path $OutDir 'xljs.xlsx'
$from = '元'; $to = '元 データ'

pwsh -NoProfile -File (Join-Path $PSScriptRoot 'excel-make-rename.ps1') -Out $src
if ($LASTEXITCODE) { throw 'ブックを作れない' }

$js = Join-Path $OutDir 'rename.js'
Set-Content -Encoding utf8 $js "const b = workbooks.open(xljs.args[0]); b.sheet(xljs.args[2]).name = xljs.args[3]; b.save(xljs.args[1]);"
& $Exe --ui=console $js $src $byXljs $from $to
if ($LASTEXITCODE) { throw 'xljs で名前を変えられない' }

$xl = New-Object -ComObject Excel.Application
$xl.DisplayAlerts = $false
function Dump([string]$path) {
    $wb = $xl.Workbooks.Open($path, 0, $true)
    try {
        $r = $wb.Worksheets.Item('参照')
        'sheets: ' + (($wb.Worksheets | % { $_.Name }) -join ',')
        foreach ($a in 'A1', 'A2', 'A3', 'A4') { "$a " + $r.Range($a).Formula + ' = ' + $r.Range($a).Text }
        foreach ($n in $wb.Names) { 'name ' + $n.Name + ' ' + $n.RefersTo }
        'validation ' + $r.Range('B1').Validation.Formula1
        'conditional ' + $r.Range('C1').FormatConditions.Item(1).Formula1
        'sparkline ' + $r.Range('D1').SparklineGroups.Item(1).SourceData
        'chart ' + $r.ChartObjects(1).Chart.SeriesCollection(1).Formula
        'pivot ' + $r.PivotTables(1).SourceData
        $dd = $r.Shapes | ? { $_.Type -eq 8 } | select -First 1
        'control ' + $dd.ControlFormat.ListFillRange + ' ' + $dd.ControlFormat.LinkedCell
        'hyperlink ' + $r.Hyperlinks.Item(1).SubAddress
    } finally { $wb.Close($false) }
}
try {
    $wb = $xl.Workbooks.Open($src)
    $wb.Worksheets.Item($from).Name = $to
    $wb.SaveAs($byExcel, 51); $wb.Close($false)
    try { $x = Dump $byXljs } catch { "FAIL  Excel で開けない: $($_.Exception.Message)"; exit 1 }
    $e = Dump $byExcel
} finally { $xl.Quit() }
$x | Set-Content -Encoding utf8 (Join-Path $OutDir 'xljs.txt')
$e | Set-Content -Encoding utf8 (Join-Path $OutDir 'excel.txt')
$diff = Compare-Object $e $x
if ($diff) {
    'FAIL  Excel と違う（<= Excel、=> xljs）'
    $diff | % { "  $($_.SideIndicator) $($_.InputObject)" }
    exit 1
}
$x | % { "  $_" }
'PASS'
