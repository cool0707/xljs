# セル以外の機能が、xljs で開いてそのまま保存したあとに残るかを、機能ごとに Excel で確かめる（pwsh 7、Excel が必要）。
#   cargo build
#   pwsh -File tools/excel-verify/features.ps1 [-Features sparkline,pivot] [-Exe target/debug/xljs.exe] [-OutDir $env:TEMP\xljs-excel-features]
# 機能ごとに、Excel で開けるか（open）、Excel から見て同じか（excel）、ファイルの部品と要素が落ちていないか（parts）を出す。
# 差の詳細は <OutDir>/<機能>/*.txt。機能は excel-make-features.ps1 の $All。詳しくは docs/design/excel-verify.md
param(
    [string[]]$Features,
    [string]$Exe = (Join-Path $PSScriptRoot '../../target/debug/xljs.exe'),
    [string]$OutDir = (Join-Path $env:TEMP 'xljs-excel-features')
)
$ErrorActionPreference = 'Stop'
$Exe = (Resolve-Path $Exe).Path
$here = $PSScriptRoot
# pwsh -File では -Features a,b が 1 つの文字列で来る
$Features = @($Features | % { $_ -split ',' } | ? { $_ })
if (-not $Features) {
    $Features = (Get-Content "$here/excel-make-features.ps1" -Encoding utf8 | Select-String '^\$All = ' -Context 0, 1).ToString() |
        Select-String "'([a-z]+)'" -AllMatches | % { $_.Matches } | % { $_.Groups[1].Value }
}
Remove-Item $OutDir -Recurse -Force -ErrorAction SilentlyContinue
$failed = 0
foreach ($f in $Features) {
    $d = Join-Path $OutDir $f
    New-Item -ItemType Directory -Force $d | Out-Null
    pwsh -NoProfile -File "$here/excel-make-features.ps1" "$d/before.xlsx" -Features $f 2>&1 | Out-Null
    $p = Start-Process $Exe -ArgumentList '--ui=mock', "$here/resave.js", "$d/before.xlsx", "$d/after.xlsx" -PassThru -Wait -WindowStyle Hidden
    if ($p.ExitCode) { "FAIL  $f  xljs で保存できない ($($p.ExitCode))"; $failed++; continue }
    $open = (pwsh -NoProfile -File "$here/open-test.ps1" "$d/after.xlsx") -like 'OK*'
    $result = @()
    if ($open) {
        pwsh -NoProfile -File "$here/excel-features.ps1" "$d/before.xlsx" "$d/before.txt" 2>&1 | Out-Null
        pwsh -NoProfile -File "$here/excel-features.ps1" "$d/after.xlsx" "$d/after.txt" 2>&1 | Out-Null
        $diff = Compare-Object (Get-Content "$d/before.txt" -Encoding utf8) (Get-Content "$d/after.txt" -Encoding utf8)
        $diff | Format-Table -AutoSize | Out-String -Width 400 | Set-Content -Encoding utf8 "$d/excel.txt"
        if ($diff) { $result += "excel: $(@($diff).Count) 行の差" }
    } else { $result += 'Excel で開けない' }
    py -I "$here/partdiff.py" "$d/before.xlsx" "$d/after.xlsx" > "$d/parts.txt"
    if ($LASTEXITCODE) { $result += "parts: $(@(Get-Content "$d/parts.txt").Count) 件落ちる" }
    if ($result) { $failed++; "FAIL  $f  $($result -join ' / ') -> $d" } else { "PASS  $f" }
}
if ($failed) { "$failed 件の失敗"; exit 1 }
"すべて PASS"
