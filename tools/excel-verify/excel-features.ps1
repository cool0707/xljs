# excel-features.ps1 <in.xlsx> <out.txt>: Excel から見たセル以外の機能を 1 行ずつ出す（開いて保存する前と後で比べる用）。
# スパークライン・ピボットテーブル・スライサー・図形とフォームコントロール・ハイパーリンク・メモ・入力規則・条件付き書式・
# テーブル・保護・印刷の設定・アウトライン・名前・外部リンク・カスタム XML・表示の設定
param([string]$Src, [string]$Out)
$ErrorActionPreference = 'Stop'
$xl = New-Object -ComObject Excel.Application
$xl.DisplayAlerts = $false
$xl.AskToUpdateLinks = $false
function T($f) { try { & $f } catch { "<$($_.Exception.Message.Split("`n")[0])>" } }
try {
    $wb = $xl.Workbooks.Open((Resolve-Path $Src).Path, 0, $true)
    $lines = [Collections.Generic.List[string]]::new()
    $add = { param($s) $lines.Add($s) }
    & $add "book|protect structure=$($wb.ProtectStructure)"
    foreach ($n in $wb.Names) { & $add "name|$($n.Name)|$($n.RefersTo)|visible=$($n.Visible)" }
    $links = $wb.LinkSources(1)
    if ($null -ne $links) { foreach ($l in @($links)) { & $add "external link|$([IO.Path]::GetFileName($l))" } }
    foreach ($p in $wb.CustomXMLParts) { if (-not $p.BuiltIn) { & $add "custom xml|$($p.NamespaceURI)|$($p.XML.Length)" } }
    foreach ($s in $wb.TableStyles) { if (-not $s.BuiltIn) { & $add "table style|$($s.Name)|header bold=$($s.TableStyleElements.Item(5).Font.Bold)|fill=$($s.TableStyleElements.Item(5).Interior.Color)" } }
    foreach ($c in $wb.SlicerCaches) {
        foreach ($sl in $c.Slicers) { & $add "slicer|$($c.SourceName)|$($sl.Name)|$($sl.Caption)|type=$(T { $c.SlicerCacheType })|$($sl.Shape.Left),$($sl.Shape.Top)" }
    }
    foreach ($ws in $wb.Worksheets) {
        $w = $ws.Name
        & $add "sheet|$w|visible=$($ws.Visible)|tab=$($ws.Tab.Color)|protect=$($ws.ProtectContents)"
        # スパークライン
        $groups = $ws.Cells.SparklineGroups
        for ($i = 1; $i -le $groups.Count; $i++) {
            $g = $groups.Item($i)
            & $add "sparkline|$w|$($g.Location.Address($false, $false))|$($g.SourceData)|type=$($g.Type)|color=$($g.SeriesColor.Color)|markers=$(T { $g.Points.Markers.Visible })"
        }
        # ピボットテーブル
        $pts = $ws.PivotTables()
        for ($i = 1; $i -le $pts.Count; $i++) {
            $pt = $pts.Item($i)
            $rows = T { ($pt.RowFields | % { $_.Name }) -join ',' }
            $cols = T { ($pt.ColumnFields | % { $_.Name }) -join ',' }
            & $add "pivot|$w|$($pt.Name)|$(T { $pt.TableRange1.Address($false, $false) })|rows=$rows|cols=$cols|total=$(T { $pt.GetPivotData('合計 / 金額').Value2 })"
        }
        # 図形とフォームコントロール
        foreach ($s in $ws.Shapes) {
            $fc = T { $s.FormControlType }
            $ctl = if ($s.Type -eq 8) { "form=$fc|linked=$(T { $s.ControlFormat.LinkedCell })|list=$(T { $s.ControlFormat.ListFillRange })|value=$(T { $s.ControlFormat.Value })" } else { '' }
            & $add "shape|$w|$($s.Name)|type=$($s.Type)|$ctl|$(T { $s.TextFrame2.TextRange.Text })|$([math]::Round($s.Left)),$([math]::Round($s.Top)),$([math]::Round($s.Width))x$([math]::Round($s.Height))"
        }
        foreach ($h in $ws.Hyperlinks) { & $add "hyperlink|$w|$($h.Range.Address($false, $false))|$($h.Address)|$($h.SubAddress)|$($h.TextToDisplay)|$($h.ScreenTip)" }
        foreach ($c in $ws.Comments) { & $add "note|$w|$($c.Parent.Address($false, $false))|$($c.Text())" }
        foreach ($c in (T { $ws.CommentsThreaded })) { if ($c -isnot [string]) { & $add "comment|$w|$($c.Parent.Address($false, $false))|$($c.Text())" } }
        # 入力規則・条件付き書式
        foreach ($cell in $ws.UsedRange.Cells) {
            $v = T { $cell.Validation.Type }
            if ($v -is [int]) { & $add "validation|$w|$($cell.Address($false, $false))|type=$v|$(T { $cell.Validation.Formula1 })|$(T { $cell.Validation.InputTitle })|$(T { $cell.Validation.InputMessage })|$(T { $cell.Validation.ErrorTitle })|$(T { $cell.Validation.ErrorMessage })" }
        }
        for ($i = 1; $i -le $ws.Cells.FormatConditions.Count; $i++) {
            $f = $ws.Cells.FormatConditions.Item($i)
            & $add "conditional|$w|$($f.AppliesTo.Address($false, $false))|type=$($f.Type)|bold=$(T { $f.Font.Bold })|color=$(T { $f.Font.Color })"
        }
        foreach ($lo in $ws.ListObjects) { & $add "table|$w|$($lo.Name)|$($lo.Range.Address($false, $false))|style=$(T { $lo.TableStyle.Name })" }
        # 印刷・アウトライン・表示
        $ps = $ws.PageSetup
        & $add "page|$w|bw=$($ps.BlackAndWhite)|area=$($ps.PrintArea)|orient=$($ps.Orientation)|header=$($ps.CenterHeader)|footer=$($ps.RightFooter)"
        $hiddenRows = (1..20 | ? { $ws.Rows.Item($_).Hidden }) -join ','
        $levels = (1..20 | ? { $ws.Rows.Item($_).OutlineLevel -gt 1 }) -join ','
        $hiddenCols = (1..10 | ? { $ws.Columns.Item($_).Hidden }) -join ','
        & $add "outline|$w|hidden rows=$hiddenRows|levels=$levels|hidden cols=$hiddenCols"
        $ws.Activate()
        & $add "view|$w|grid=$($xl.ActiveWindow.DisplayGridlines)|zoom=$($xl.ActiveWindow.Zoom)"
        foreach ($cell in $ws.UsedRange.Cells) {
            $ph = T { $cell.Phonetic.Text }
            if ($ph -and $ph -ne '' -and $cell.Phonetics.Count -gt 0) { & $add "phonetic|$w|$($cell.Address($false, $false))|$ph|visible=$($cell.Phonetic.Visible)" }
        }
    }
    Set-Content -Encoding utf8 -Path $Out -Value $lines
    $wb.Close($false)
} finally { $xl.Quit() }


