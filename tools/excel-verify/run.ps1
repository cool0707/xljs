# xljs の Excel ファイルの読み書きと数式の計算を、実機の Excel（COM）で確かめる（pwsh 7、Excel が必要）。
#   cargo build
#   pwsh -File tools/excel-verify/run.ps1 [-Exe target/debug/xljs.exe] [-OutDir $env:TEMP\xljs-excel-verify]
# 各項目の PASS/FAIL を出す。失敗があれば終了コード 1。差の詳細は <OutDir>/*.txt。
# 詳しくは docs/design/excel-verify.md。
param(
    [string]$Exe = (Join-Path $PSScriptRoot '../../target/debug/xljs.exe'),
    [string]$OutDir = (Join-Path $env:TEMP 'xljs-excel-verify')
)
$ErrorActionPreference = 'Stop'
$Exe = (Resolve-Path $Exe).Path
$root = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path
$here = $PSScriptRoot
Remove-Item $OutDir -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force "$OutDir/rt" | Out-Null
Push-Location $OutDir
$failed = 0

function Xljs([string[]]$ArgList) {
    $p = Start-Process $Exe -ArgumentList (@('--ui=mock', '--lang=ja') + $ArgList) -PassThru -Wait -WindowStyle Hidden
    if ($p.ExitCode) { throw "xljs $($ArgList -join ' ') -> $($p.ExitCode)" }
}
function ExcelDump([string]$Src, [string]$Out, [string[]]$Flags) {
    pwsh -NoProfile -File "$here/excel-dump.ps1" $Src $Out @Flags 2>&1 | Out-Null
    Test-Path $Out
}
# 差がなければ PASS。差の一覧は <name>.txt に残す。
function Check([string]$Name, [string]$A, [string]$B, [string[]]$What, [string]$Skip = '') {
    $env:SKIP = $Skip
    py -I "$here/compare.py" $A $B @What > "$Name.txt"
    $ok = $LASTEXITCODE -eq 0
    $env:SKIP = ''
    $last = (Get-Content "$Name.txt" -Tail 1)
    if ($ok) { "PASS  $Name  ($last)" } else { $script:failed++; "FAIL  $Name  ($last) -> $OutDir\$Name.txt" }
}
function Fail([string]$Name, [string]$Why) { $script:failed++; "FAIL  $Name  $Why" }

try {
    # 1. 数式の計算: calc.xlsx を xljs で計算した値と、Excel で再計算した値（保存値のシートは Excel にも計算できない数式）
    Copy-Item "$root/testdata/xlsx/calc.xlsx" calc.xlsx
    Xljs @("$here/dump.js", 'calc.xlsx', 'xljs-calc.json', 'calc', 'xljs-calc-saved.xlsx')
    if (ExcelDump calc.xlsx excel-calc.json @('-Recalc')) {
        Check 'calc' xljs-calc.json excel-calc.json @('v') '保存値'
    } else { Fail 'calc' 'calc.xlsx が Excel で開けない' }
    # xljs が計算して保存した値を、Excel が再計算せずに読む
    if (ExcelDump xljs-calc-saved.xlsx excel-calc-saved.json @('-Manual')) {
        Check 'calc-saved' xljs-calc.json excel-calc-saved.json @('v')
    } else { Fail 'calc-saved' 'xljs が保存したブックが Excel で開けない' }

    # 2. 読み込み: Excel が作ったブックを xljs で読む（値・数式・表示）
    pwsh -NoProfile -File "$here/excel-make.ps1" (Join-Path $OutDir 'excel-made.xlsx') | Out-Null
    Xljs @("$here/dump.js", 'excel-made.xlsx', 'xljs-made.json')
    if (ExcelDump excel-made.xlsx excel-made.json @('-Wide')) { Check 'read' xljs-made.json excel-made.json @('v', 'f', 't') }
    # 組み込みの表示形式（定義なしの numFmtId 0〜81）の表示
    py -I "$here/make_builtin.py" builtin.xlsx
    Xljs @("$here/dump.js", 'builtin.xlsx', 'xljs-builtin.json')
    if (ExcelDump builtin.xlsx excel-builtin.json @('-Wide')) { Check 'builtin-formats' xljs-builtin.json excel-builtin.json @('t') }

    # 3. 書き込み: xljs が書いたブックを Excel で開く
    Xljs @("$here/write.js", 'xljs-written.xlsx')
    Xljs @("$here/dump.js", 'xljs-written.xlsx', 'xljs-written.json')
    if (ExcelDump xljs-written.xlsx excel-written.json @('-Manual', '-Wide')) {
        Check 'write' xljs-written.json excel-written.json @('v', 'f')
        if (ExcelDump xljs-written.xlsx excel-written-recalc.json @('-Recalc', '-Wide')) { Check 'write-recalc' xljs-written.json excel-written-recalc.json @('v', 't') }
    } else { Fail 'write' 'xljs が書いたブックが Excel で開けない' }

    # 4. 往復: 開いてそのまま保存したブックを、元のブックと Excel で比べる
    $books = @((Join-Path $OutDir 'excel-made.xlsx'), (Join-Path $OutDir 'builtin.xlsx')) +
        (Get-ChildItem "$root/testdata/xlsx/*.xlsx", "$root/testdata/book/*.xlsx", "$root/examples/data/*.xlsx" | ForEach-Object FullName)
    foreach ($b in $books) {
        $n = [IO.Path]::GetFileNameWithoutExtension($b)
        Xljs @("$here/resave.js", $b, "rt/$n.xlsx")
        if (-not (ExcelDump $b "rt/$n.orig.json" @('-Manual', '-Wide'))) { "SKIP  roundtrip-$n  元のブックが Excel で開けない"; continue }
        if (-not (ExcelDump "rt/$n.xlsx" "rt/$n.saved.json" @('-Manual', '-Wide'))) { Fail "roundtrip-$n" '保存したブックが Excel で開けない'; continue }
        Check "roundtrip-$n" "rt/$n.saved.json" "rt/$n.orig.json" @('v', 'f', 't', 'n')
    }
} finally {
    Pop-Location
}
if ($failed) { "$failed 件の失敗"; exit 1 }
"すべて PASS"
