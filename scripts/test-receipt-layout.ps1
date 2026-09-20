param([string]$Printer='GA-E200 Series',[int]$PaperMm=80,[int]$ItemCount=4)
$ErrorActionPreference='Stop'
# Uses the real renderer with PreviewPrintController; never submits a physical job.
$rows=@(
    @{kind='title';text='KAY POS';right=''},
    @{kind='center';text='09771000510, 09674506492';right=''},
    @{kind='center';text='မြန်မာစာ စမ်းသပ်ဘောင်ချာ';right=''},
    @{kind='rule';text='';right=''},
    @{kind='center';text='TEST ONLY - NOT A SALE';right=''},
    @{kind='pair';text='Payment method';right='Cash'},
    @{kind='rule';text='';right=''}
)
for($i=1;$i -le $ItemCount;$i++) {
    $rows+=@{kind='item';text="Item $i - Nestle Strawberry Milk / မြန်မာစာ ပစ္စည်းအမည်";right=''}
    $rows+=@{kind='pair';text='2 x 2,000 Ks';right='4,000 Ks'}
}
$rows+=@{kind='rule';text='';right=''}
$rows+=@{kind='total';text='TOTAL';right="$($ItemCount*4000) Ks"}
$rows+=@{kind='pair';text='Received';right="$($ItemCount*4000+500) Ks"}
$rows+=@{kind='pair';text='Change';right='500 Ks'}
$rows+=@{kind='rule';text='';right=''}
$rows+=@{kind='center';text='ကျေးဇူးတင်ပါသည်';right=''}
$data=@{printer=$Printer;invoice='KAY-LAYOUT-TEST';paper_mm=$PaperMm;rows=$rows}
$source=Get-Content (Join-Path $PSScriptRoot '../crates/pos_desktop/assets/print-receipt.ps1') -Raw
$source=$source.Replace('$data = [Console]::In.ReadToEnd() | ConvertFrom-Json','')
$source=$source.Replace('New-Object System.Drawing.Printing.StandardPrintController','New-Object System.Drawing.Printing.PreviewPrintController')
$source=$source.Replace('$doc.Print()',@'
$doc.Print()
$bounds=Get-ContentBounds $doc.DefaultPageSettings
$headMm=if ($PaperMm -eq 58) { 48 } else { 72 }
$safeRight=$doc.DefaultPageSettings.PrintableArea.Left+$headMm/25.4*100-$margin
if ($bounds.Left+$bounds.Width -gt $safeRight+0.01) { throw 'Receipt exceeds safe print-head width' }
if ($fonts.item.Bold) { throw 'Product names must use regular weight' }
$pages=$doc.PrintController.GetPreviewPageInfo()
if($pages.Count -eq 0){throw 'No preview pages rendered'}
for($n=0;$n -lt $pages.Count;$n++) {
    $bitmap=New-Object System.Drawing.Bitmap(($pages[$n].Image.Width*2),($pages[$n].Image.Height*2))
    $g=[System.Drawing.Graphics]::FromImage($bitmap)
    try {
        $g.Clear([System.Drawing.Color]::White)
        $g.DrawImage($pages[$n].Image,0,0,$bitmap.Width,$bitmap.Height)
        $path=Join-Path $env:TEMP "kay-receipt-$PaperMm-$ItemCount-$n.png"
        $bitmap.Save($path,[System.Drawing.Imaging.ImageFormat]::Png)
        Write-Output $path
    } finally {$g.Dispose();$bitmap.Dispose();$pages[$n].Image.Dispose()}
}
Write-Output "Rendered $($pages.Count) pages; rows completed: $script:rowIndex / $($data.rows.Count)"
if($script:rowIndex -ne $data.rows.Count){throw 'Receipt was truncated'}
'@)
& ([scriptblock]::Create($source))
