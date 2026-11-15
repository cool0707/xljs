# excel-make-features.ps1 <out.xlsx> [-Features a,b,...]: Excel 自身に、セル以外の機能を含むブックを作らせる（保存で落ちるものの確認用）。
# -Features を省くと全部。機能の名前は $All を参照。データのシート（テーブル「売上」）はいつも作る
param([string]$Out, [string[]]$Features)
$All = 'tablestyle', 'sparkline', 'pivot', 'slicer', 'timeline', 'tableslicer', 'formcontrol', 'validation', 'note', 'protect',
    'hyperlink', 'extlink', 'conditional', 'page', 'outline', 'image', 'phonetic', 'names', 'customxml', 'view', 'bookprotect'
if (-not $Features) { $Features = $All }
$bad = $Features | ? { $_ -notin $All }
if ($bad) { throw "知らない機能: $bad（$($All -join ', ')）" }
function Want([string]$f) { $f -in $Features }

$ErrorActionPreference = 'Stop'
$Out = [IO.Path]::GetFullPath($Out)
$dir = Split-Path $Out
$xl = New-Object -ComObject Excel.Application
$xl.DisplayAlerts = $false
try {
    $wb = $xl.Workbooks.Add()
    while ($wb.Worksheets.Count -lt 4) { [void]$wb.Worksheets.Add([Type]::Missing, $wb.Worksheets.Item($wb.Worksheets.Count)) }
    $data = $wb.Worksheets.Item(1); $data.Name = 'データ'
    $rep = $wb.Worksheets.Item(2); $rep.Name = '集計'
    $ui = $wb.Worksheets.Item(3); $ui.Name = '入力'
    $misc = $wb.Worksheets.Item(4); $misc.Name = 'その他'

    # データ（テーブル「売上」）
    $people = '佐藤', '鈴木', '高橋', '田中'
    $goods = 'りんご', 'みかん', 'ぶどう'
    $data.Range('A1:E1').Formula = [object[]]@('日付', '担当', '商品', '数量', '金額')
    for ($i = 0; $i -lt 24; $i++) {
        $r = $i + 2
        $data.Cells.Item($r, 1).Formula = "=DATE(2026,$(1 + $i % 12),$(1 + $i))"
        $data.Cells.Item($r, 2).Formula = [string]$people[$i % 4]
        $data.Cells.Item($r, 3).Formula = [string]$goods[$i % 3]
        $data.Cells.Item($r, 4).Formula = [string](($i * 7) % 11 + 1)
        $data.Cells.Item($r, 5).Formula = "=D$r*$(100 + ($i % 3) * 50)"
    }
    $data.Range('A2:A25').Value2 = $data.Range('A2:A25').Value2   # 数式をやめて日付の値に
    $data.Range('A2:A25').NumberFormatLocal = 'yyyy/m/d'
    $lo = $data.ListObjects.Add(1, $data.Range('A1:E25'), [Type]::Missing, 1)
    $lo.Name = '売上'

    if (Want 'tablestyle') {
        $ts = $wb.TableStyles.Add('独自の表')
        $ts.TableStyleElements.Item(5).Font.Bold = $true            # xlHeaderRow = 5
        $ts.TableStyleElements.Item(5).Interior.Color = 0x7F3F00
        $ts.TableStyleElements.Item(5).Font.Color = 0xFFFFFF
        $lo.TableStyle = '独自の表'
    }
    if (Want 'sparkline') {
        $data.Range('G1').Formula = '推移'
        for ($k = 0; $k -lt 4; $k++) {
            $data.Cells.Item(2 + $k, 7).Formula = [string]$people[$k]
            for ($m = 0; $m -lt 12; $m++) { $data.Cells.Item(2 + $k, 8 + $m).Formula = [string](($k * 3 + $m * 5) % 9 + 1) }
        }
        $g1 = $data.Range('T2:T5').SparklineGroups.Add(1, 'H2:S5')          # xlSparkLine
        $g1.SeriesColor.Color = 0xC07000
        $g1.Points.Markers.Visible = $true
        [void]$data.Range('U2:U5').SparklineGroups.Add(2, 'H2:S5')          # xlSparkColumn
        [void]$data.Range('V2:V5').SparklineGroups.Add(3, 'H2:S5')          # 勝敗
    }
    if ((Want 'pivot') -or (Want 'slicer') -or (Want 'timeline')) {
        $pc = $wb.PivotCaches().Create(1, '売上')
        $pt = $pc.CreatePivotTable($rep.Range('A3'), '担当別')
        $pt.PivotFields('担当').Orientation = 1                          # xlRowField
        $pt.PivotFields('商品').Orientation = 2                          # xlColumnField
        [void]$pt.AddDataField($pt.PivotFields('金額'), '合計 / 金額', -4157)   # xlSum
        if (Want 'slicer') {
            $sc = $wb.SlicerCaches.Add2($pt, '商品')
            [void]$sc.Slicers.Add($rep, [Type]::Missing, '商品', '商品', 10, 400, 150, 180)
        }
        if (Want 'timeline') {
            $tc = $wb.SlicerCaches.Add2($pt, '日付', [Type]::Missing, 2)    # xlTimeline
            [void]$tc.Slicers.Add($rep, [Type]::Missing, '日付', '日付', 200, 400, 300, 110)
        }
    }
    if (Want 'tableslicer') {
        $sc2 = $wb.SlicerCaches.Add2($lo, '担当')
        [void]$sc2.Slicers.Add($data, [Type]::Missing, '担当表', '担当', 10, 700, 140, 160)
    }
    if (Want 'formcontrol') {
        $ui.Range('B2').Formula = 'TRUE'
        $btn = $ui.Shapes.AddFormControl(0, 200, 10, 100, 24)          # xlButtonControl
        $btn.Name = '実行ボタン'
        $btn.TextFrame.Characters().Text = '実行'
        $cb = $ui.Shapes.AddFormControl(1, 200, 50, 120, 20)           # xlCheckBox
        $cb.Name = '確認'
        $cb.ControlFormat.LinkedCell = '$B$2'
        $cb.TextFrame.Characters().Text = '確認済み'
        $ui.Range('D2').Formula = '東京'; $ui.Range('D3').Formula = '大阪'; $ui.Range('D4').Formula = '名古屋'
        $dd = $ui.Shapes.AddFormControl(2, 200, 80, 120, 20)           # xlDropDown
        $dd.Name = '支店'
        $dd.ControlFormat.ListFillRange = '$D$2:$D$4'
        $dd.ControlFormat.LinkedCell = '$B$3'
        $dd.ControlFormat.ListIndex = 2
        $sp = $ui.Shapes.AddFormControl(9, 200, 110, 20, 40)           # xlSpinner
        $sp.Name = '回数'
        $sp.ControlFormat.LinkedCell = '$B$4'
        $sp.ControlFormat.Min = 1; $sp.ControlFormat.Max = 10; $sp.ControlFormat.Value = 3
        $ob = $ui.Shapes.AddFormControl(7, 200, 160, 120, 20)          # xlOptionButton
        $ob.Name = '選択1'
    }
    if (Want 'validation') {
        $v = $ui.Range('B6').Validation
        $v.Add(1, 1, 1, '1', '100')                                      # 整数 1〜100
        $v.InputTitle = '数量'; $v.InputMessage = '1〜100 の整数を入れてください'
        $v.ErrorTitle = '入力エラー'; $v.ErrorMessage = '範囲外です'
        $ui.Range('F2').Formula = '東京'; $ui.Range('F3').Formula = '大阪'
        $ui.Range('B7').Validation.Add(3, 1, 1, '=$F$2:$F$3')            # リスト
    }
    if (Want 'note') {
        [void]$ui.Range('C2').AddComment('確認したら TRUE')
        # スレッド形式のコメント（Excel 2021 は COM から作れない）
        try { [void]$ui.Range('C3').AddCommentThreaded('支店はここで選ぶ') } catch { Write-Warning "スレッド形式のコメントを作れない: $($_.Exception.Message.Split("`n")[0])" }
    }
    if (Want 'protect') {
        $ui.Range('B6:B7').Locked = $false
        $ui.Protect('pw', $true, $true, $true, $false, $true)
    }
    if (Want 'hyperlink') {
        [void]$misc.Hyperlinks.Add($misc.Range('A1'), 'https://example.com/', [Type]::Missing, 'サイトへ', '外部リンク')
        [void]$misc.Hyperlinks.Add($misc.Range('A2'), '', "'データ'!A1", [Type]::Missing, 'データへ')
    }
    if (Want 'extlink') {
        $other = $xl.Workbooks.Add()
        $other.Worksheets.Item(1).Range('A1').Value2 = 42
        $other.SaveAs((Join-Path $dir 'features-other.xlsx'), 51)
        $misc.Range('A3').Formula = "='$dir\[features-other.xlsx]Sheet1'!`$A`$1*2"
        $other.Close($false)
    }
    if (Want 'conditional') {
        for ($r = 5; $r -le 14; $r++) { $misc.Cells.Item($r, 1).Formula = [string]($r * 3 % 10); $misc.Cells.Item($r, 2).Formula = [string]($r * 7 % 10); $misc.Cells.Item($r, 3).Formula = [string]($r % 4); $misc.Cells.Item($r, 4).Formula = [string]$r }
        [void]$misc.Range('A5:A14').FormatConditions.AddColorScale(3)
        [void]$misc.Range('B5:B14').FormatConditions.AddDatabar()
        [void]$misc.Range('C5:C14').FormatConditions.AddIconSetCondition()
        $fc = $misc.Range('D5:D14').FormatConditions.Add(1, 5, '10')     # セルの値 > 10
        $fc.Font.Bold = $false; $fc.Font.Color = 255
    }
    if (Want 'page') {
        $misc.PageSetup.BlackAndWhite = $true
        $misc.PageSetup.PrintArea = '$A$1:$D$14'
        $misc.PageSetup.CenterHeader = '&"游ゴシック,太字"見出し'
        $misc.PageSetup.RightFooter = '&P / &N'
        $misc.PageSetup.Orientation = 2
    }
    if (Want 'outline') {
        $misc.Range('A16:A20').Formula = '1'
        [void]$misc.Rows('17:19').Group()
        $misc.Rows('17:19').Hidden = $true
        $misc.Columns('F').Hidden = $true
        $misc.Tab.Color = 0x00B050
    }
    if (Want 'image') {
        $png = Join-Path $dir 'features-image.png'
        Add-Type -AssemblyName System.Drawing
        $bmp = New-Object System.Drawing.Bitmap 40, 20
        $gr = [System.Drawing.Graphics]::FromImage($bmp); $gr.Clear([System.Drawing.Color]::SteelBlue); $gr.Dispose()
        $bmp.Save($png, [System.Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()
        $pic = $misc.Shapes.AddPicture($png, $false, $true, 300, 10, 80, 40); $pic.Name = 'ロゴ'
    }
    if (Want 'phonetic') {
        $misc.Range('E1').Formula = '東京'
        [void]$misc.Range('E1').Phonetics.Add(1, 2, 'トウキョウ')
        $misc.Range('E1').Phonetic.Visible = $true
    }
    if (Want 'names') {
        [void]$wb.Names.Add('税率', '=0.1')
        $wb.Names.Add('隠れた名前', '=その他!$A$1').Visible = $false
    }
    if (Want 'customxml') {
        [void]$wb.CustomXMLParts.Add('<?xml version="1.0"?><設定 xmlns="urn:xljs-test"><版>3</版></設定>')
    }
    if (Want 'view') {
        $misc.Activate(); $xl.ActiveWindow.DisplayGridlines = $false; $xl.ActiveWindow.Zoom = 85
    }
    $data.Activate()
    if (Want 'bookprotect') { $wb.Protect('pw', $true) }

    $wb.SaveAs($Out, 51)
    $wb.Close($false)
} finally { $xl.Quit() }
