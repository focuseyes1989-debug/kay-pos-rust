$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Drawing,System.Windows.Forms
$data=[Console]::In.ReadToEnd()|ConvertFrom-Json
$script:sectionIndex=0
$script:index=0
$script:page=0
$ink=[Drawing.Brushes]::MidnightBlue
$muted=[Drawing.Brushes]::DimGray
$line=[Drawing.Pens]::LightGray

$fonts=@{
 title=[Drawing.Font]::new('Segoe UI',19,[Drawing.FontStyle]::Bold)
 body=[Drawing.Font]::new('Segoe UI',9)
 bold=[Drawing.Font]::new('Segoe UI',9,[Drawing.FontStyle]::Bold)
 small=[Drawing.Font]::new('Segoe UI',8)
}
function Text($g,$value,$font,$brush,$x,$y,$w,$h,[bool]$right=$false,[bool]$wrap=$false) {
 $format=[Drawing.StringFormat]::new()
 try {
  $format.Alignment=if($right){[Drawing.StringAlignment]::Far}else{[Drawing.StringAlignment]::Near}
  $format.LineAlignment=[Drawing.StringAlignment]::Center
  if(!$wrap){$format.FormatFlags=[Drawing.StringFormatFlags]::NoWrap}
  $format.Trimming=[Drawing.StringTrimming]::EllipsisCharacter
  $g.DrawString([string]$value,$font,$brush,[Drawing.RectangleF]::new($x,$y,$w,$h),$format)
 } finally {$format.Dispose()}
}
function Row($g,$cells,$y,$font,$widths,$height) {
 $x=13
 for($i=0;$i -lt $widths.Count;$i++){
  Text $g $cells[$i] $font $ink ($x+2) $y ($widths[$i]-4) $height ($i -gt 0) ($i -eq 0)
  $x+=$widths[$i]
 }
}
function RowHeight($g,$row,$widths) {
 $height=[Math]::Ceiling($g.MeasureString([string]$row[0],$fonts.body,[Drawing.SizeF]::new(($widths[0]-4),100)).Height)+2
 [Math]::Min(110,[Math]::Max(7,$height))
}
$doc=[Drawing.Printing.PrintDocument]::new()
try {
 $doc.DocumentName='Sale Summary'
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
  Text $g 'Sale Summary' $fonts.title $ink 13 11 271 11
  Text $g ("Date range: "+$data.from+" to "+$data.to) $fonts.body $muted 13 24 271 7
  $y=33
  $bottom=189
  while($script:sectionIndex -lt $data.sections.Count){
   $section=$data.sections[$script:sectionIndex]
   $rows=@($section.rows)
   $widths=switch($section.columns.Count){
    3 {151;45;75}
    9 {27;19;33;33;27;33;34;34;31}
    10 {66;15;17;24;25;24;25;24;25;26}
    default {throw 'Unexpected report columns.'}
   }
   $showEstimate=$script:sectionIndex -eq 0 -and $data.estimated -gt 0
   $tail=7
   if($showEstimate){$tail+=8}
   $firstHeight=7
   if($rows.Count){$firstHeight=RowHeight $g $rows[$script:index] $widths}
   $needed=15+$firstHeight
   if($script:index+1 -ge $rows.Count){$needed+=$tail}
   if($y+$needed -gt $bottom){break}
   $heading=$section.title
   if($script:index -gt 0){$heading+=' (continued)'}
   Text $g $heading $fonts.bold $ink 13 $y 271 7
   $y+=7
   $g.FillRectangle([Drawing.Brushes]::AliceBlue,13,$y,271,8)
   Row $g $section.columns $y $fonts.small $widths 8
   $y+=8
   if(!$rows.Count){Text $g 'No records in this date range.' $fonts.body $muted 15 $y 267 7;$y+=7}
   while($script:index -lt $rows.Count){
    $row=$rows[$script:index]
    $height=RowHeight $g $row $widths
    $needed=$height
    if($script:index+1 -eq $rows.Count){$needed+=$tail}
    if($y+$needed -gt $bottom){break}
    Row $g $row $y $fonts.body $widths $height
    $y+=$height
    $g.DrawLine($line,13,$y,284,$y)
    $script:index++
   }
   if($script:index -lt $rows.Count){break}
   $g.FillRectangle([Drawing.Brushes]::AliceBlue,13,$y,271,7)
   Row $g $section.total $y $fonts.bold $widths 7
   if($showEstimate){
    Text $g ("Estimated cost: "+$data.estimated+" item lines use current product/variant cost.") $fonts.small $muted 13 ($y+8) 271 6
   }
   $y+=$tail+5
   $script:sectionIndex++
   $script:index=0
  }
  $g.DrawLine($line,13,194,284,194)
  Text $g 'KAY POS | Sale Summary' $fonts.small $muted 13 195 200 6
  Text $g ("Page "+$script:page) $fonts.small $muted 240 195 44 6 $true
  $e.HasMorePages=($script:sectionIndex -lt $data.sections.Count)
 })
 $doc.Print()
 Write-Output 'OK'
}finally{
 $doc.Dispose()
 foreach($font in $fonts.Values){$font.Dispose()}
}
