$ErrorActionPreference = 'Stop'
$data = [Console]::In.ReadToEnd() | ConvertFrom-Json
Add-Type -AssemblyName System.Drawing
$geometry = $data.geometry
$paperMm = [double]$geometry.paper_mm
if ($paperMm -lt 40 -or $paperMm -gt 120) { throw 'Receipt paper width must be 40–120 mm' }
function Units([double]$mm) { return [single]($mm / 25.4 * 100) }
$leftMargin=Units $geometry.left_mm
$rightMargin=Units $geometry.right_mm
$topMargin=Units $geometry.top_mm
$bottomMargin=Units $geometry.bottom_mm
$logoGap=Units $geometry.logo_gap_mm
$sectionGap=Units $geometry.section_gap_mm
$doc = New-Object System.Drawing.Printing.PrintDocument
$doc.PrinterSettings.PrinterName = $data.printer
if (-not $doc.PrinterSettings.IsValid) { throw 'Receipt printer is unavailable' }
# Start from the selected driver's saved per-user preferences.
# Receipt geometry below intentionally supplies paper size, portrait orientation and margins.
$doc.DefaultPageSettings = $doc.PrinterSettings.DefaultPageSettings.Clone()
$doc.DocumentName = $data.invoice
$doc.PrintController = New-Object System.Drawing.Printing.StandardPrintController
$paperWidth = [int][Math]::Round($paperMm / 25.4 * 100)
$doc.DefaultPageSettings.Landscape = $false
$doc.DefaultPageSettings.Margins = New-Object System.Drawing.Printing.Margins([int]$leftMargin,[int]$rightMargin,[int]$topMargin,[int]$bottomMargin)
$fonts = @{
    normal = New-Object System.Drawing.Font('Myanmar Text',9)
    item = New-Object System.Drawing.Font('Myanmar Text',9)
    title = New-Object System.Drawing.Font('Myanmar Text',13,([System.Drawing.FontStyle]::Bold))
    total = New-Object System.Drawing.Font('Myanmar Text',11,([System.Drawing.FontStyle]::Bold))
}
$pen = New-Object System.Drawing.Pen([System.Drawing.Color]::Gray,0.6)
$pen.DashStyle = [System.Drawing.Drawing2D.DashStyle]::Dash
$logo = $null
$logoStream = $null
$logoRow = $data.rows | Where-Object kind -eq 'logo' | Select-Object -First 1
if ($logoRow) {
    $encoded = [string]$logoRow.text
    if (-not $encoded.StartsWith('data:image/png;base64,')) { throw 'Invalid receipt logo format' }
    $logoBytes = [Convert]::FromBase64String($encoded.Substring(22))
    $logoStream = New-Object System.IO.MemoryStream(,$logoBytes)
    $logo = [System.Drawing.Image]::FromStream($logoStream)
}
function Get-LogoSize([single]$width) {
    $scale=[Math]::Min([Math]::Min($width,(Units $geometry.logo_width_mm))/$logo.Width,(Units $geometry.logo_height_mm)/$logo.Height)
    return @{Width=[single]($logo.Width*$scale);Height=[single]($logo.Height*$scale)}
}
$script:rowIndex = 0
$script:cellRemaining = $null

function Get-RowFont($row) {
    $size=[single]$row.font_size
    $style=if ($row.bold) { [System.Drawing.FontStyle]::Bold } else { [System.Drawing.FontStyle]::Regular }
    $key="$size-$style"
    if (-not $fonts.ContainsKey($key)) { $fonts[$key]=New-Object System.Drawing.Font('Myanmar Text',$size,$style) }
    $script:lineFactor=[single]$row.line_height
    return $fonts[$key]
}
# Print coordinates use hundredths of an inch. Match CSS font-size(pt) * line-height,
# rather than GetHeight(Graphics), which includes font metrics and device DPI.
function Get-LineHeight($font) {
    return [single]($font.SizeInPoints / 72.0 * 100.0 * $script:lineFactor)
}
function Get-RowColumns($row,[single]$width) {
    $gap=Units 2
    $cells=New-Object System.Collections.Generic.List[object]
    if ($row.kind -eq 'columns') {
        $weights=0.0
        foreach ($cell in $row.cells) {$weights+=[double]$cell.weight}
        $available=$width-$gap*([Math]::Max(0,$row.cells.Count-1))
        if ($available -le 0 -or $weights -le 0) {throw 'Item columns do not fit this printer width'}
        foreach ($cell in $row.cells) {$cells.Add(@{Text=[string]$cell.text;Width=[single]($available*$cell.weight/$weights);Align=[string]$cell.align})}
    } elseif ($row.kind -in @('pair','total')) {
        $fraction=[double]$row.pair_percent/100
        $cells.Add(@{Text=[string]$row.text;Width=[single](($width-$gap)*$fraction);Align=[string]$row.align})
        $cells.Add(@{Text=[string]$row.right;Width=[single](($width-$gap)*(1-$fraction));Align='right'})
    } else {$cells.Add(@{Text=[string]$row.text;Width=$width;Align=[string]$row.align})}
    return ,$cells
}
function Get-ContentBounds($settings) {
    $headWidth=Units $geometry.printable_mm
    # Keep the configured print head centered on paper. Intersect both edges
    # with driver bounds; shifting the left edge must never shift the right edge.
    $headLeft=[Math]::Max(0,($paperWidth-$headWidth)/2)
    $headRight=[Math]::Min($paperWidth,($paperWidth+$headWidth)/2)
    $left=[single]([Math]::Max($settings.PrintableArea.Left,$headLeft)+$leftMargin)
    $right=[single]([Math]::Min($settings.PrintableArea.Right,$headRight)-$rightMargin)
    if ($right-$left -lt (Units 10)) { throw 'Printer printable width is too small' }
    return @{Left=$left;Width=[single]($right-$left)}
}
function Wrap-Text($graphics,[string]$text,$font,[single]$width) {
    # Reuse layout between measuring, pagination, and drawing in this job only.
    if ($null -eq $script:wrapCache) {$script:wrapCache=@{}}
    $key=(@($text,$font.Name,$font.SizeInPoints,$font.Style,$width,$graphics.DpiX,$graphics.DpiY,$graphics.PageUnit,$graphics.PageScale) | ConvertTo-Json -Compress)
    if ($script:wrapCache.ContainsKey($key)) {return ,$script:wrapCache[$key]}
    $result=New-Object System.Collections.Generic.List[object]
    $format=[System.Drawing.StringFormat]::GenericTypographic.Clone()
    try {
        # Most receipt cells fit on one line: one GDI measurement instead of
        # measuring every growing grapheme prefix.
        if ($text -and !$text.Contains("`n") -and !$text.Contains("`r") -and $graphics.MeasureString($text,$font,2147483647,$format).Width -le $width) {
            $result.Add(@{Text=$text;Chars=$text.Length})
            $script:wrapCache[$key]=$result
            return ,$result
        }
        $line=''; $consumed=0
        $elements=[System.Globalization.StringInfo]::GetTextElementEnumerator($text)
        while ($elements.MoveNext()) {
            $element=$elements.GetTextElement()
            if ($element -eq "`r") { $consumed+=$element.Length; continue }
            if ($element -eq "`n") {
                $result.Add(@{Text=$line;Chars=$consumed+1}); $line=''; $consumed=0; continue
            }
            $candidate=$line+$element
            if ($line -and $graphics.MeasureString($candidate,$font,2147483647,$format).Width -gt $width) {
                $result.Add(@{Text=$line;Chars=$consumed}); $line=''; $consumed=0
            }
            $line+=$element; $consumed+=$element.Length
        }
        if ($consumed -gt 0) { $result.Add(@{Text=$line;Chars=$consumed}) }
        $script:wrapCache[$key]=$result
        return ,$result
    } finally { $format.Dispose() }
}
function Measure-Text($graphics,[string]$text,$font,[single]$width,[single]$height) {
    if (-not $text) { return @{Height=0;Chars=0} }
    $lines=Wrap-Text $graphics $text $font $width
    $lineHeight=(Get-LineHeight $font)
    $count=[int][Math]::Min($lines.Count,[Math]::Floor($height/$lineHeight))
    $chars=0
    for ($i=0;$i -lt $count;$i++) { $chars+=$lines[$i].Chars }
    return @{Height=[single]($count*$lineHeight);Chars=$chars}
}
function Draw-Text($graphics,[string]$text,$font,[single]$x,[single]$y,[single]$width,[single]$height,$alignment) {
    if (-not $text) { return }
    $lines=Wrap-Text $graphics $text $font $width
    $lineHeight=(Get-LineHeight $font)
    $format=[System.Drawing.StringFormat]::GenericTypographic.Clone()
    $format.Alignment=$alignment
    $format.FormatFlags=$format.FormatFlags -bor [System.Drawing.StringFormatFlags]::NoWrap -bor [System.Drawing.StringFormatFlags]::NoClip
    # Wrap explicitly, then draw at a point. Rectangle layout can reject an
    # entire Myanmar line when glyph metrics exceed its height on the driver.
    $anchorX=switch ($alignment) {
        ([System.Drawing.StringAlignment]::Center) {$x+$width/2}
        ([System.Drawing.StringAlignment]::Far) {$x+$width}
        default {$x}
    }
    try {
        foreach ($line in $lines) {
            $point=New-Object System.Drawing.PointF([single]$anchorX,$y)
            $graphics.DrawString($line.Text,$font,[System.Drawing.Brushes]::Black,$point,$format)
            $y+=$lineHeight
        }
    } finally { $format.Dispose() }
}

try {
    # Measure the selected width to avoid unnecessary blank roll feed.
    $doc.DefaultPageSettings.PaperSize = New-Object System.Drawing.Printing.PaperSize("Receipt $paperMm mm",$paperWidth,1100)
    $measure=$doc.PrinterSettings.CreateMeasurementGraphics()
    try {
        $measure.PageUnit=[System.Drawing.GraphicsUnit]::Display
        $bounds=Get-ContentBounds $doc.DefaultPageSettings
        $width=$bounds.Width
        $height=[single]($topMargin+$bottomMargin)
        foreach ($row in $data.rows) {
            if ($row.kind -eq 'logo') { $height+=(Get-LogoSize $width).Height+$logoGap; continue }
            if ($row.kind -eq 'rule') { $height+=$sectionGap; continue }
            $font=Get-RowFont $row
            $rowHeight=(Get-LineHeight $font)
            foreach ($cell in (Get-RowColumns $row $width)) {
                $m=Measure-Text $measure $cell.Text $font $cell.Width 100000
                $rowHeight=[Math]::Max($rowHeight,$m.Height)
            }
            $height+=$rowHeight+(Units $row.gap_mm)
        }
    } finally { $measure.Dispose() }
    # Long receipts paginate rather than exceeding typical driver length limits.
    $paperHeight=[int][Math]::Min(1100,[Math]::Max(200,[Math]::Ceiling($height)))
    $doc.DefaultPageSettings.PaperSize = New-Object System.Drawing.Printing.PaperSize("Receipt $paperMm mm",$paperWidth,$paperHeight)
    $doc.add_PrintPage({
        param($sender,$event)
        if ([Math]::Abs($event.PageBounds.Width-$paperWidth) -gt 8) {
            throw 'Printer driver did not accept the selected paper width. Check printer paper settings.'
        }
        $graphics=$event.Graphics
        $graphics.PageUnit=[System.Drawing.GraphicsUnit]::Display
        $graphics.TranslateTransform(-$event.PageSettings.HardMarginX,-$event.PageSettings.HardMarginY)
        $bounds=Get-ContentBounds $event.PageSettings
        $x=$bounds.Left
        $y=[single][Math]::Max($topMargin,$event.PageSettings.PrintableArea.Top)
        $width=$bounds.Width
        $bottom=[single][Math]::Min($event.MarginBounds.Bottom,$event.PageSettings.PrintableArea.Bottom)
        $pageStart=$y
        while ($script:rowIndex -lt $data.rows.Count) {
            $row=$data.rows[$script:rowIndex]
            if ($row.kind -eq 'logo') {
                $size=Get-LogoSize $width
                if ($bottom-$y -lt $size.Height+$logoGap) { if ($y -eq $pageStart) {throw 'Logo is taller than the printer page. Reduce logo height.'}; $event.HasMorePages=$true; return }
                $logoX=switch ($geometry.logo_align) { 'left' {$x} 'right' {$x+$width-$size.Width} default {$x+($width-$size.Width)/2} }
                $graphics.DrawImage($logo,[single]$logoX,$y,$size.Width,$size.Height)
                $y+=$size.Height+$logoGap; $script:rowIndex++; continue
            }
            if ($row.kind -eq 'rule') {
                if ($bottom-$y -lt $sectionGap) { $event.HasMorePages=$true; return }
                $graphics.DrawLine($pen,$x,($y+$sectionGap/2),($x+$width),($y+$sectionGap/2))
                $y+=$sectionGap; $script:rowIndex++; continue
            }
            $font=Get-RowFont $row
            $available=[single]($bottom-$y-(Units $row.gap_mm))
            if ($available -lt (Get-LineHeight $font)) {
                if ($y -eq $pageStart) { throw 'Printer printable area is too small' }
                $event.HasMorePages=$true; return
            }
            $columns=Get-RowColumns $row $width
            if ($null -eq $script:cellRemaining) {$script:cellRemaining=@($columns | ForEach-Object {$_.Text})}
            $measurements=New-Object System.Collections.Generic.List[object]
            $rowHeight=0.0
            for ($i=0;$i -lt $columns.Count;$i++) {
                $m=Measure-Text $graphics $script:cellRemaining[$i] $font $columns[$i].Width $available
                if ($script:cellRemaining[$i] -and $m.Chars -eq 0) {
                    if ($y -eq $pageStart) {throw 'Receipt text cannot fit the printer page'}
                    $event.HasMorePages=$true;return
                }
                $measurements.Add($m);$rowHeight=[Math]::Max($rowHeight,$m.Height)
            }
            $columnX=$x
            for ($i=0;$i -lt $columns.Count;$i++) {
                $cell=$columns[$i];$m=$measurements[$i]
                $align=switch ($cell.Align) {'right' {[System.Drawing.StringAlignment]::Far} 'center' {[System.Drawing.StringAlignment]::Center} default {[System.Drawing.StringAlignment]::Near}}
                Draw-Text $graphics ($script:cellRemaining[$i].Substring(0,$m.Chars)) $font $columnX $y $cell.Width $available $align
                $script:cellRemaining[$i]=$script:cellRemaining[$i].Substring($m.Chars)
                $columnX+=$cell.Width+(Units 2)
            }
            $y+=[single]([Math]::Max($rowHeight,(Get-LineHeight $font))+(Units $row.gap_mm))
            if (@($script:cellRemaining | Where-Object {$_}).Count -gt 0) {$event.HasMorePages=$true;return}
            $script:cellRemaining=$null;$script:rowIndex++
        }
        $event.HasMorePages=$false
    })
    $doc.Print()
} finally {
    if ($logo) { $logo.Dispose() }; if ($logoStream) { $logoStream.Dispose() }
    foreach ($font in $fonts.Values) { $font.Dispose() }
    $pen.Dispose(); $doc.Dispose()
}
