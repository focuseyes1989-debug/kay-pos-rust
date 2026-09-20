$ErrorActionPreference = 'Stop'
$data = [Console]::In.ReadToEnd() | ConvertFrom-Json
Add-Type -AssemblyName System.Drawing
$paperMm = [int]$data.paper_mm
if ($paperMm -notin @(58,80)) { throw 'Choose 58 mm or 80 mm receipt paper' }
$doc = New-Object System.Drawing.Printing.PrintDocument
$doc.PrinterSettings.PrinterName = $data.printer
if (-not $doc.PrinterSettings.IsValid) { throw 'Receipt printer is unavailable' }
$doc.DocumentName = $data.invoice
$doc.PrintController = New-Object System.Drawing.Printing.StandardPrintController
$margin = 12 # Hundredths of an inch, approximately 3 mm.
$paperWidth = [int][Math]::Round($paperMm / 25.4 * 100)
$doc.DefaultPageSettings.Landscape = $false
$doc.DefaultPageSettings.Margins = New-Object System.Drawing.Printing.Margins($margin,$margin,$margin,$margin)
$fonts = @{
    normal = New-Object System.Drawing.Font('Myanmar Text',9)
    item = New-Object System.Drawing.Font('Myanmar Text',9)
    title = New-Object System.Drawing.Font('Myanmar Text',13,([System.Drawing.FontStyle]::Bold))
    total = New-Object System.Drawing.Font('Myanmar Text',11,([System.Drawing.FontStyle]::Bold))
}
$pen = New-Object System.Drawing.Pen([System.Drawing.Color]::Gray,0.6)
$pen.DashStyle = [System.Drawing.Drawing2D.DashStyle]::Dash
$script:rowIndex = 0
$script:leftRemaining = $null
$script:rightRemaining = $null

function Get-RowFont($kind) {
    if ($fonts.ContainsKey($kind)) { return $fonts[$kind] }
    return $fonts.normal
}
function Get-ContentBounds($settings) {
    # Some thermal drivers report the roll width, not the narrower print head.
    $headMm = if ($paperMm -eq 58) { 48 } else { 72 }
    $left = [single]([Math]::Max(0,$settings.PrintableArea.Left)+$margin)
    $right = [single]([Math]::Min([double]$paperWidth,[Math]::Min([double]$settings.PrintableArea.Right,$settings.PrintableArea.Left+$headMm/25.4*100))-$margin)
    if ($right-$left -lt 80) { throw 'Printer printable width is too small' }
    return @{ Left=$left; Width=[single]($right-$left) }
}
function Measure-Text($graphics, [string]$text, $font, [single]$width, [single]$height) {
    if (-not $text) { return @{ Height=0; Chars=0 } }
    $chars=0; $lines=0
    $format=[System.Drawing.StringFormat]::GenericTypographic.Clone()
    try {
        $size=New-Object System.Drawing.SizeF($width,$height)
        $measured=$graphics.MeasureString($text,$font,$size,$format,[ref]$chars,[ref]$lines)
        return @{ Height=[single]$measured.Height; Chars=$chars }
    } finally { $format.Dispose() }
}
function Draw-Text($graphics, [string]$text, $font, [single]$x, [single]$y, [single]$width, [single]$height, $alignment) {
    if (-not $text) { return }
    $format=[System.Drawing.StringFormat]::GenericTypographic.Clone()
    $format.Alignment=$alignment
    try {
        $rect=New-Object System.Drawing.RectangleF($x,$y,$width,$height)
        $graphics.DrawString($text,$font,[System.Drawing.Brushes]::Black,$rect,$format)
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
        $height=[single](2*$margin+8+$fonts.normal.GetHeight($measure)*2)
        foreach ($row in $data.rows) {
            if ($row.kind -eq 'rule') { $height+=12; continue }
            $font=Get-RowFont $row.kind
            if ($row.kind -in @('pair','total')) {
                $a=Measure-Text $measure ([string]$row.text) $font (($width-8)*0.55) 100000
                $b=Measure-Text $measure ([string]$row.right) $font (($width-8)*0.45) 100000
                $height += [Math]::Max($a.Height,$b.Height)+3
            } else {
                $m=Measure-Text $measure ([string]$row.text) $font $width 100000
                $height += [Math]::Max($m.Height,$font.GetHeight($measure))+3
            }
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
        $y=[single][Math]::Max($margin,$event.PageSettings.HardMarginY)
        $width=$bounds.Width
        $bottom=[single][Math]::Min($event.MarginBounds.Bottom,$event.PageSettings.PrintableArea.Bottom)
        $pageStart=$y
        while ($script:rowIndex -lt $data.rows.Count) {
            $row=$data.rows[$script:rowIndex]
            if ($row.kind -eq 'rule') {
                if ($bottom-$y -lt 12) { $event.HasMorePages=$true; return }
                $graphics.DrawLine($pen,$x,($y+6),($x+$width),($y+6))
                $y+=12; $script:rowIndex++; continue
            }
            $font=Get-RowFont $row.kind
            $available=[single]($bottom-$y-3)
            if ($available -lt $font.GetHeight($graphics)*1.5) {
                if ($y -eq $pageStart) { throw 'Printer printable area is too small' }
                $event.HasMorePages=$true; return
            }
            if ($null -eq $script:leftRemaining) {
                $script:leftRemaining=[string]$row.text
                $script:rightRemaining=[string]$row.right
            }
            $pair=$row.kind -in @('pair','total')
            $leftWidth=$width; $rightWidth=0
            if ($pair) { $leftWidth=($width-8)*0.55; $rightWidth=($width-8)*0.45 }
            $a=Measure-Text $graphics $script:leftRemaining $font $leftWidth $available
            $b=@{Height=0;Chars=0}
            if ($pair) { $b=Measure-Text $graphics $script:rightRemaining $font $rightWidth $available }
            if (($script:leftRemaining -and $a.Chars -eq 0) -or ($script:rightRemaining -and $b.Chars -eq 0)) {
                if ($y -eq $pageStart) { throw 'Receipt text cannot fit the printer page' }
                $event.HasMorePages=$true; return
            }
            $align=[System.Drawing.StringAlignment]::Near
            if ($row.kind -in @('title','center')) { $align=[System.Drawing.StringAlignment]::Center }
            Draw-Text $graphics ($script:leftRemaining.Substring(0,$a.Chars)) $font $x $y $leftWidth $available $align
            if ($pair) {
                Draw-Text $graphics ($script:rightRemaining.Substring(0,$b.Chars)) $font ($x+$leftWidth+8) $y $rightWidth $available ([System.Drawing.StringAlignment]::Far)
            }
            $script:leftRemaining=$script:leftRemaining.Substring($a.Chars)
            $script:rightRemaining=$script:rightRemaining.Substring($b.Chars)
            $y += [single]([Math]::Max([Math]::Max($a.Height,$b.Height),$font.GetHeight($graphics))+3)
            if ($script:leftRemaining -or $script:rightRemaining) { $event.HasMorePages=$true; return }
            $script:leftRemaining=$null; $script:rightRemaining=$null; $script:rowIndex++
        }
        $event.HasMorePages=$false
    })
    $doc.Print()
} finally {
    foreach ($font in $fonts.Values) { $font.Dispose() }
    $pen.Dispose(); $doc.Dispose()
}
