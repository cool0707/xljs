# open-test.ps1 <file>...: Excel で開けるかを OK / NG で出す（開けないときは修復のダイアログを出さずに失敗する）
$Files = $args
$xl = New-Object -ComObject Excel.Application; $xl.DisplayAlerts = $false
foreach ($f in $Files) { $p = (Resolve-Path $f).Path
  try { $wb = $xl.Workbooks.Open($p, 0, $true); "OK  $f  [$($xl.ActiveWindow.Caption)]"; $wb.Close($false) } catch { "NG  $f  $($_.Exception.Message)" } }
$xl.Quit()

