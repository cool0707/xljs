# excel-dump.ps1 <in.xlsx> <out.json> [-Recalc] [-Manual] [-Wide]: ブックの全セル（値 Value2・数式 Formula2・表示 Text・表示形式）を JSON に出す。
#   -Recalc: 開いたあと CalculateFull で再計算する
#   -Manual: 計算方法を手動にして開き、ファイルに保存されている値をそのまま読む
#   -Wide:   列幅を広げてから表示を読む（狭い列の #### や桁の切り詰めを避ける）
param([string]$Src, [string]$Out, [switch]$Recalc, [switch]$Manual, [switch]$Wide)
$ErrorActionPreference = 'Stop'
$errs = @{ -2146826281='#DIV/0!'; -2146826246='#N/A'; -2146826259='#NAME?'; -2146826288='#NULL!'; -2146826252='#NUM!'; -2146826265='#REF!'; -2146826273='#VALUE!' }
$xl = New-Object -ComObject Excel.Application
$xl.DisplayAlerts = $false
$xl.Visible = $false
try {
  if ($Manual) { $tmp = $xl.Workbooks.Add(); $xl.Calculation = -4135; $xl.CalculateBeforeSave = $false }
  $wb = $xl.Workbooks.Open((Resolve-Path $Src).Path, 0, $true)
  if ($Recalc) { $xl.CalculateFull() }
  $sheets = [ordered]@{}
  foreach ($ws in $wb.Worksheets) {
    $ur = $ws.UsedRange; if ($Wide) { $ws.Cells.ColumnWidth = 80; $ur = $ws.UsedRange }
    $cells = [ordered]@{}
    foreach ($c in $ur.Cells) {
      $v = $c.Value2
      $hf = $c.HasFormula
      if ($null -eq $v -and -not $hf) { continue }
      if ($v -is [int]) { if ($errs.ContainsKey($v)) { $v = $errs[$v] } else { $v = "#ERR$v" } }
      $f = $null; if ($hf) { $f = $c.Formula2 }
      $cells[$c.Address($false, $false)] = [ordered]@{ v = $v; f = $f; t = $c.Text; nf = $c.NumberFormat }
    }
    $sheets[$ws.Name] = [ordered]@{ dim = $ur.Address($false, $false); cells = $cells }
  }
  [ordered]@{ caption = $xl.ActiveWindow.Caption; name = $wb.Name; sheets = $sheets } | ConvertTo-Json -Depth 6 | Set-Content -Encoding utf8 $Out
  $wb.Close($false)
  if ($Manual) { $tmp.Close($false) }
} finally {
  $xl.Quit()
  [void][Runtime.InteropServices.Marshal]::ReleaseComObject($xl)
}

