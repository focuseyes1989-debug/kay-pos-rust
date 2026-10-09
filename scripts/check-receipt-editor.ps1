# Exercise the production layout functions without opening a printer or spooling a job.
$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Drawing
$source=Join-Path $PSScriptRoot '../crates/pos_desktop/assets/print-receipt.ps1'
$errors=$null
$ast=[System.Management.Automation.Language.Parser]::ParseFile($source,[ref]$null,[ref]$errors)
if ($errors.Count) {throw ($errors | Out-String)}
$functions=$ast.FindAll({param($node) $node -is [System.Management.Automation.Language.FunctionDefinitionAst]},$true)
foreach ($function in $functions) {Invoke-Expression $function.Extent.Text}
$fonts=@{}
$bitmap=New-Object System.Drawing.Bitmap(900,1400)
$graphics=[System.Drawing.Graphics]::FromImage($bitmap)
try {
    $graphics.Clear([System.Drawing.Color]::White)
    $row=[pscustomobject]@{font_size=12;bold=$false;line_height=1.2}
    $font=Get-RowFont $row
    $expected=12.0/72*100*1.2
    if ([Math]::Abs((Get-LineHeight $font)-$expected) -gt 0.01) {throw 'Print line box differs from preview point units'}
    foreach ($dpi in @(96,203,300)) {
        $probe=New-Object System.Drawing.Bitmap(100,100)
        try {
            $probe.SetResolution($dpi,$dpi)
            $g=[System.Drawing.Graphics]::FromImage($probe)
            try {if ([Math]::Abs((Get-LineHeight $font)-$expected) -gt 0.01) {throw 'Line spacing varies with printer DPI'}} finally {$g.Dispose()}
        } finally {$probe.Dispose()}
    }
    $glyphProbe=New-Object System.Drawing.Bitmap(400,120)
    $glyphProbe.SetResolution(100,100)
    $glyphGraphics=[System.Drawing.Graphics]::FromImage($glyphProbe)
    try {
        $glyphGraphics.Clear([System.Drawing.Color]::White)
        $glyphRow=[pscustomobject]@{font_size=13;bold=$true;line_height=1.0}
        $glyphFont=Get-RowFont $glyphRow
        Draw-Text $glyphGraphics 'မြန်မာ ဆိုင်အမည်' $glyphFont 5 5 390 100000 ([System.Drawing.StringAlignment]::Near)
        $ink=0
        for($py=0;$py -lt 100;$py++){for($px=0;$px -lt 400;$px++){if($glyphProbe.GetPixel($px,$py).R -lt 128){$ink++}}}
        if($ink -lt 20){throw 'Myanmar title vanished at compact line spacing'}
    } finally {$glyphGraphics.Dispose();$glyphProbe.Dispose()}
    $font=Get-RowFont $row
    $text='မြန်မာစာ လိပ်စာ စမ်းသပ်ရန် Shop address wraps over several lines 1234567890'
    $small=Measure-Text $graphics $text $font 160 100000
    $row.line_height=2.4
    $font=Get-RowFont $row
    $large=Measure-Text $graphics $text $font 160 100000
    if ($small.Chars -ne $text.Length -or $large.Chars -ne $text.Length) {throw 'Wrapping lost text'}
    if ([Math]::Abs($large.Height-2*$small.Height) -gt 0.1) {throw 'Line spacing did not change height'}
    $partial=Measure-Text $graphics $text $font 160 ((Get-LineHeight $font)+0.01)
    if ($partial.Chars -le 0 -or $partial.Chars -ge $text.Length) {throw 'Page splitting failed'}
    Draw-Text $graphics $text $font 20 20 160 1200 ([System.Drawing.StringAlignment]::Near)
    foreach ($paper in @(58,80,112,76.5)) {
        $paperWidth=$paper/25.4*100
        $geometry=[pscustomobject]@{printable_mm=($paper-8);logo_width_mm=30;logo_height_mm=20}
        $leftMargin=Units 3; $rightMargin=Units 3
        $settings=[pscustomobject]@{PrintableArea=(New-Object System.Drawing.RectangleF(0,0,$paperWidth,1100))}
        $bounds=Get-ContentBounds $settings
        if ($bounds.Width -le 0 -or $bounds.Left+$bounds.Width -gt $paperWidth) {throw 'Custom paper bounds failed'}
    }
    $paperWidth=Units 80
    $geometry=[pscustomobject]@{printable_mm=72}
    $leftMargin=Units 2;$rightMargin=Units 2
    $headRight=($paperWidth+(Units 72))/2
    $shifted=[pscustomobject]@{PrintableArea=(New-Object System.Drawing.RectangleF((Units 6),0,(Units 74),1100))}
    $bounds=Get-ContentBounds $shifted
    if ([Math]::Abs($bounds.Left+$bounds.Width-($headRight-$rightMargin)) -gt 0.01) {throw 'Hard left margin shifted receipt right edge'}
    $narrow=[pscustomobject]@{PrintableArea=(New-Object System.Drawing.RectangleF((Units 4),0,(Units 68),1100))}
    $bounds=Get-ContentBounds $narrow
    if ($bounds.Left+$bounds.Width -gt $narrow.PrintableArea.Right-$rightMargin+0.01) {throw 'Receipt exceeds right printable margin'}
    $table=[pscustomobject]@{kind='columns';font_size=9;bold=$false;line_height=1.5;gap_mm=1;cells=@([pscustomobject]@{text=('မြန်မာစာ Long item name '*25);weight=60;align='left'},[pscustomobject]@{text='2';weight=15;align='right'},[pscustomobject]@{text='5,000 Ks';weight=25;align='right'})}
    $pair=[pscustomobject]@{kind='total';text='TOTAL';right='5,000 Ks';font_size=11;bold=$true;line_height=1.5;gap_mm=1;pair_percent=70;align='left'}
    $cells=Get-RowColumns $table 200
    $sum=0.0;foreach ($cell in $cells) {$sum+=$cell.Width}
    if ($cells.Count -ne 3 -or [Math]::Abs($sum+2*(Units 2)-200) -gt 0.01) {throw 'Column widths exceed content bounds'}
    $pairCells=Get-RowColumns $pair 200
    if ([Math]::Abs($pairCells[0].Width/$pairCells[1].Width-70/30) -gt 0.01) {throw 'Totals column ratio is incorrect'}
    $callback=$ast.Find({param($node) $node -is [System.Management.Automation.Language.InvokeMemberExpressionAst] -and $node.Member.Value -eq 'add_PrintPage'},$true).Arguments[0].ScriptBlock.GetScriptBlock()
    $paperWidth=315;$topMargin=Units 3
    $geometry=[pscustomobject]@{printable_mm=72}
    $data=[pscustomobject]@{rows=@($table,$pair)}
    $script:rowIndex=0;$script:cellRemaining=$null
    $pageSettings=[pscustomobject]@{HardMarginX=0;HardMarginY=0;PrintableArea=(New-Object System.Drawing.RectangleF(0,0,$paperWidth,160))}
    $event=[pscustomobject]@{Graphics=$graphics;PageSettings=$pageSettings;PageBounds=[pscustomobject]@{Width=$paperWidth};MarginBounds=[pscustomobject]@{Bottom=150};HasMorePages=$false}
    $pages=0
    do {$pages++; & $callback $null $event;if ($pages -gt 100) {throw 'Pagination did not finish'}} while ($event.HasMorePages)
    if ($script:rowIndex -ne 2 -or $pages -le 1) {throw 'Multi-column pagination failed'}
    'Receipt editor: syntax, Burmese wrapping, line spacing, paper widths, column ratios and production multi-column pagination passed.'

} finally {
    foreach ($font in $fonts.Values) {$font.Dispose()}
    $graphics.Dispose();$bitmap.Dispose()
}
