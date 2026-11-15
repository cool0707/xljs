# excel-shapes.ps1 <in.xlsx> <out.txt>: Excel から見た全シートの図形（名前・種類・文字・文字の変形・向き・線）を 1 行ずつ出す
param([string]$Src, [string]$Out)
$ErrorActionPreference = 'Stop'
$xl = New-Object -ComObject Excel.Application
$xl.DisplayAlerts = $false
try {
  $wb = $xl.Workbooks.Open((Resolve-Path $Src).Path, 0, $true)
  $lines = foreach ($ws in $wb.Worksheets) {
    foreach ($s in $ws.Shapes) {
      $text = try { $s.TextFrame2.TextRange.Text } catch { '' }
      $warp = try { $s.TextFrame2.WarpFormat } catch { '' }
      $orient = try { $s.TextFrame2.Orientation } catch { '' }
      $line = try { $s.Line.Visible } catch { '' }
      "$($ws.Name)|$($s.Name)|type=$($s.Type)|$text|warp=$warp|orient=$orient|line=$line|$($s.Width)x$($s.Height)"
    }
  }
  Set-Content -Encoding utf8 -Path $Out -Value @(@($lines) | Sort-Object)
  $wb.Close($false)
} finally { $xl.Quit() }
