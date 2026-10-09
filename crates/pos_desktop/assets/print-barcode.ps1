$ErrorActionPreference='Stop'
$data=[Console]::In.ReadToEnd() | ConvertFrom-Json
Add-Type -AssemblyName System.Drawing
$label=$data.label
$w=[double]$label.width; $h=[double]$label.height; $dpi=[int]$label.dpi
if ($w -lt 25 -or $w -gt 110 -or $h -lt 20 -or $h -gt 100 -or $dpi -notin @(203,300)) {throw 'Invalid label dimensions or DPI'}
function Dots([double]$mm){return [int][Math]::Round($mm*$dpi/25.4)}
$bitmap=$null; $graphics=$null; $stream=$null; $doc=$null
try {
 if ($data.mode -eq 'preview') {
  $bitmap=New-Object System.Drawing.Bitmap((Dots $w),(Dots $h))
  $bitmap.SetResolution($dpi,$dpi)
  $graphics=[System.Drawing.Graphics]::FromImage($bitmap)
  $graphics.Clear([System.Drawing.Color]::White)
  $graphics.TextRenderingHint=[System.Drawing.Text.TextRenderingHint]::SingleBitPerPixelGridFit
  $graphics.SmoothingMode=[System.Drawing.Drawing2D.SmoothingMode]::None
  $pad=if($h -le 25){Dots 1.5}else{Dots 2}; $inner=$bitmap.Width-2*$pad; $y=$pad
  $fontSize=if($h -le 25){7}else{8}
  $nameHeight=if($h -le 25){3.5}else{5}
  $priceHeight=if($h -le 25){3}else{5}
  $font=New-Object System.Drawing.Font('Myanmar Text',$fontSize,[System.Drawing.FontStyle]::Regular,[System.Drawing.GraphicsUnit]::Point)
  $format=[System.Drawing.StringFormat]::GenericDefault.Clone()
  $format.Alignment=[System.Drawing.StringAlignment]::Center
  $format.LineAlignment=[System.Drawing.StringAlignment]::Center
  $format.FormatFlags=[System.Drawing.StringFormatFlags]::NoWrap
  $format.Trimming=[System.Drawing.StringTrimming]::EllipsisCharacter
  try {
   foreach ($row in @(@{text=$(if($label.show_name){[string]$label.name}else{''}); height=$nameHeight},@{text=$(if($label.show_price){[string]$label.price}else{''}); height=$priceHeight})) {
    $text=[string]$row.text; $rowHeight=Dots $row.height
    if ($text.Length -gt 0) {
     $rect=New-Object System.Drawing.RectangleF($pad,$y,$inner,$rowHeight)
     $graphics.DrawString($text,$font,[System.Drawing.Brushes]::Black,$rect,$format)
     $y+=$rowHeight
    }
   }
   $numberHeight=if($label.show_number){if($h -le 25){Dots 3}else{Dots 4}}else{0}
   $numberGap=if($label.show_number){Dots 0.5}else{0}
   $barHeight=$bitmap.Height-$pad-$y-$numberHeight-$numberGap
   if($barHeight -lt (Dots 6)){throw 'Label is too short. Increase height or hide some text.'}
   $bars=@($data.bars); if($bars.Count -lt 30){throw 'Missing barcode modules'}
   $module=[int][Math]::Floor($bitmap.Width/$bars.Count)
   if($module -lt 2){throw 'Barcode too long for label size and DPI'}
   $left=[int][Math]::Floor(($bitmap.Width-$bars.Count*$module)/2)
   for($i=0;$i -lt $bars.Count;$i++) {
    if([int]$bars[$i] -eq 1){$graphics.FillRectangle([System.Drawing.Brushes]::Black,($left+$i*$module),$y,$module,$barHeight)}
   }
   if($label.show_number){
    $rect=New-Object System.Drawing.RectangleF($pad,($y+$barHeight+$numberGap),$inner,$numberHeight)
    $graphics.DrawString([string]$label.code,$font,[System.Drawing.Brushes]::Black,$rect,$format)
   }
  } finally {$font.Dispose();$format.Dispose()}
  $graphics.Dispose();$graphics=$null
  $stream=New-Object System.IO.MemoryStream
  $bitmap.Save($stream,[System.Drawing.Imaging.ImageFormat]::Png)
  [Console]::Write([Convert]::ToBase64String($stream.ToArray()))
 } elseif ($data.mode -eq 'print') {
  $copies=[int]$data.copies
  if($copies -lt 1 -or $copies -gt 500){throw 'Copies must be 1–500'}
  $stream=New-Object System.IO.MemoryStream(,[Convert]::FromBase64String([string]$data.png))
  $bitmap=[System.Drawing.Image]::FromStream($stream)
  if($bitmap.Width -ne (Dots $w) -or $bitmap.Height -ne (Dots $h)){throw 'Preview dimensions do not match label'}
  $doc=New-Object System.Drawing.Printing.PrintDocument
  $doc.PrinterSettings.PrinterName=[string]$data.printer
  if(-not $doc.PrinterSettings.IsValid){throw 'Barcode printer is unavailable'}
  $doc.DefaultPageSettings=$doc.PrinterSettings.DefaultPageSettings.Clone()
  $doc.DefaultPageSettings.Landscape=$false
  $doc.DefaultPageSettings.Margins=New-Object System.Drawing.Printing.Margins(0,0,0,0)
  $paperWidth=[int][Math]::Round($w/25.4*100);$paperHeight=[int][Math]::Round($h/25.4*100)
  # Prefer a real driver form when one matches the requested label.
  $matchedPaper=$doc.PrinterSettings.PaperSizes | Where-Object { [Math]::Abs($_.Width-$paperWidth) -le 2 -and [Math]::Abs($_.Height-$paperHeight) -le 2 } | Select-Object -First 1
  if($matchedPaper){$doc.DefaultPageSettings.PaperSize=$matchedPaper}
  else{$doc.DefaultPageSettings.PaperSize=New-Object System.Drawing.Printing.PaperSize('Barcode label',$paperWidth,$paperHeight)}
  $doc.OriginAtMargins=$false
  $doc.PrinterSettings.Copies=1
  $doc.DocumentName='KAY POS Barcode Labels'
  $doc.PrintController=New-Object System.Drawing.Printing.StandardPrintController
  $script:barcodePage=0
  $doc.add_PrintPage({param($sender,$event)
   $page=$event.PageSettings;$area=$page.PrintableArea;$inset=2/25.4*100
   if([Math]::Abs($event.PageBounds.Width-$paperWidth) -gt 3 -or [Math]::Abs($event.PageBounds.Height-$paperHeight) -gt 3){throw "Printer driver did not accept the $w x $h mm label. Set the same paper size in Printing Preferences."}
   if($area.Left -gt $inset+1 -or $area.Top -gt $inset+1 -or $area.Right -lt $paperWidth-$inset-1 -or $area.Bottom -lt $paperHeight-$inset-1){throw 'Printer printable area does not fit this label. Check label size in Printing Preferences.'}
   if([Math]::Abs($event.Graphics.DpiX-$dpi) -gt 5 -or [Math]::Abs($event.Graphics.DpiY-$dpi) -gt 5){throw "Selected DPI ($dpi) does not match printer DPI ($($event.Graphics.DpiX) x $($event.Graphics.DpiY)). Select the printer's actual DPI."}
   # Draw one bitmap pixel per printer dot, avoiding driver/GDI image scaling.
   $event.Graphics.PageUnit=[System.Drawing.GraphicsUnit]::Pixel
   $event.Graphics.PageScale=1
   $event.Graphics.TranslateTransform([single](-$page.HardMarginX*$event.Graphics.DpiX/100),[single](-$page.HardMarginY*$event.Graphics.DpiY/100))
   $event.Graphics.DrawImageUnscaled($bitmap,0,0)
   $script:barcodePage++
   $event.HasMorePages=$script:barcodePage -lt $copies
  })
  $doc.Print()
 } else {throw 'Unknown barcode action'}
} finally {
 if($graphics){$graphics.Dispose()};if($doc){$doc.Dispose()};if($bitmap){$bitmap.Dispose()};if($stream){$stream.Dispose()}
}
