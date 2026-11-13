$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Drawing,System.Windows.Forms
$data=[Console]::In.ReadToEnd()|ConvertFrom-Json
$rows=@($data.rows)
$script:index=0
$script:page=0
$ink=[Drawing.Brushes]::MidnightBlue
$muted=[Drawing.Brushes]::DimGray
$line=[Drawing.Pens]::LightGray
$widths=@(27,19,33,33,27,33,34,34,31)
$fonts=@{
 title=[Drawing.Font]::new('Segoe UI',19,[Drawing.FontStyle]::Bold)
 body=[Drawing.Font]::new('Segoe UI',9)
 bold=[Drawing.Font]::new('Segoe UI',9,[Drawing.FontStyle]::Bold)
 small=[Drawing.Font]::new('Segoe UI',8)
}
function Text($g,$value,$font,$brush,$x,$y,$w,$h,[bool]$right=$false) {
 $format=[Drawing.StringFormat]::new()
 try {
  $format.Alignment=if($right){[Drawing.StringAlignment]::Far}else{[Drawing.StringAlignment]::Near}
  $format.LineAlignment=[Drawing.StringAlignment]::Center
  $format.FormatFlags=[Drawing.StringFormatFlags]::NoWrap
  $format.Trimming=[Drawing.StringTrimming]::EllipsisCharacter
  $g.DrawString([string]$value,$font,$brush,[Drawing.RectangleF]::new($x,$y,$w,$h),$format)
 } finally {$format.Dispose()}
}
function Row($g,$cells,$y,$font) {
 $x=13
 for($i=0;$i -lt $widths.Count;$i++){
  Text $g $cells[$i] $font $ink ($x+2) $y ($widths[$i]-4) 7 ($i -gt 0)
  $x+=$widths[$i]
 }
}
$doc=[Drawing.Printing.PrintDocument]::new()
try {
 $doc.DocumentName='Daily Sales Summary'
 if($data.mode -eq 'pdf'){
  $doc.PrinterSettings.PrinterName='Microsoft Print to PDF'
  if(!$doc.PrinterSettings.IsValid){throw 'Microsoft Print to PDF is unavailable.'}
  $doc.PrinterSettings.PrintToFile=$true
  $doc.PrinterSettings.PrintFileName=$data.path
  $doc.PrintController=[Drawing.Printing.StandardPrintController]::new()
 }else{
  $dialog=[Windows.Forms.PrintDialog]::new()
  $dialog.Document=$doc
  $dialog.UseEXDialog=$true
  try{if($dialog.ShowDialog() -ne [Windows.Forms.DialogResult]::OK){Write-Output 'CANCELLED';return}}finally{$dialog.Dispose()}
 }
 $a4=@($doc.PrinterSettings.PaperSizes|Where-Object {$_.Kind -eq [Drawing.Printing.PaperKind]::A4})
 if(!$a4.Count){throw 'Selected printer does not support A4 paper.'}
 $doc.DefaultPageSettings.PaperSize=$a4[0]
 $doc.DefaultPageSettings.Landscape=$true
 $doc.DefaultPageSettings.Margins=[Drawing.Printing.Margins]::new(0,0,0,0)
 $doc.add_PrintPage({param($sender,$e)
  $g=$e.Graphics
  $g.PageUnit=[Drawing.GraphicsUnit]::Millimeter
  $g.TranslateTransform([single](-$e.PageSettings.HardMarginX*0.254),[single](-$e.PageSettings.HardMarginY*0.254))
  $script:page++
  Text $g 'Daily Sales Summary' $fonts.title $ink 13 11 271 11
  Text $g ("Date range: "+$data.from+" to "+$data.to) $fonts.body $muted 13 24 271 7
  $g.FillRectangle([Drawing.Brushes]::AliceBlue,13,35,271,7)
  Row $g $data.columns 35 $fonts.bold
  $y=42
  if(!$rows.Count){Text $g 'No sales in this date range.' $fonts.body $muted 15 $y 267 7;$y+=7}
  while($script:index -lt $rows.Count -and $y -lt 175){
   Row $g $rows[$script:index] $y $fonts.body
   $y+=7
   $g.DrawLine($line,13,$y,284,$y)
   $script:index++
  }
  if($script:index -ge $rows.Count){
   $g.FillRectangle([Drawing.Brushes]::AliceBlue,13,$y,271,7)
   Row $g $data.total $y $fonts.bold
   if($data.estimated -gt 0){Text $g ("Estimated cost: "+$data.estimated+" item lines use current product/variant cost.") $fonts.small $muted 13 ($y+8) 271 6}
  }
  $g.DrawLine($line,13,194,284,194)
  Text $g 'KAY POS | Sale Summary' $fonts.small $muted 13 195 200 6
  Text $g ("Page "+$script:page) $fonts.small $muted 240 195 44 6 $true
  $e.HasMorePages=($script:index -lt $rows.Count)
 })
 $doc.Print()
 Write-Output 'OK'
}finally{
 $doc.Dispose()
 foreach($font in $fonts.Values){$font.Dispose()}
}
