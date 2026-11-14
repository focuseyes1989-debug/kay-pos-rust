$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Drawing,System.Windows.Forms
$data=[Console]::In.ReadToEnd()|ConvertFrom-Json
$portrait=[bool]$data.portrait
$paperWidth=if($portrait){184}else{271}
$rightEdge=13+$paperWidth
$footer=if($portrait){281}else{194}
$rowHeight=if($portrait){5.5}else{7}
$headingHeight=if($portrait){6}else{7}
$columnHeight=if($portrait){6}else{8}
$gap=if($portrait){3}else{5}
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
  Text $g $cells[$i] $font $ink ($x+2) $y ($widths[$i]-4) $height ($i -gt 0) ($i -eq 0 -and !$portrait)
  $x+=$widths[$i]
 }
}
function RowHeight($g,$row,$widths) {
 if($portrait){return $rowHeight}
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
 $doc.DefaultPageSettings.Landscape=!$portrait
 $doc.DefaultPageSettings.Margins=[Drawing.Printing.Margins]::new(0,0,0,0)
 $doc.add_PrintPage({param($sender,$e)
  $g=$e.Graphics
  $g.PageUnit=[Drawing.GraphicsUnit]::Millimeter
  $g.TranslateTransform([single](-$e.PageSettings.HardMarginX*0.254),[single](-$e.PageSettings.HardMarginY*0.254))
  $script:page++
  Text $g 'Sale Summary' $fonts.title $ink 13 11 $paperWidth 11
  Text $g ("Date range: "+$data.from+" to "+$data.to) $fonts.body $muted 13 24 $paperWidth 7
  $y=33
  $bottom=$footer-5
  while($script:sectionIndex -lt $data.sections.Count){
   $section=$data.sections[$script:sectionIndex]
   $rows=@($section.rows)
   $widths=if($portrait){
    switch($section.columns.Count){
     2 {126;58}
     3 {104;26;54}
     5 {66;19;33;33;33}
     6 {32;19;34;30;37;32}
     default {throw 'Unexpected portrait report columns.'}
    }
   }else{switch($section.columns.Count){
    2 {181;90}
    3 {151;45;75}
    9 {27;19;33;33;27;33;34;34;31}
    10 {66;15;17;24;25;24;25;24;25;26}
    default {throw 'Unexpected report columns.'}
   }
   }
   $showEstimate=$script:sectionIndex -eq 0 -and $data.estimated -gt 0
   $tail=$rowHeight
   if($showEstimate){$tail+=8}
   if($section.note){$tail+=8}
   $firstHeight=$rowHeight
   if($rows.Count){$firstHeight=RowHeight $g $rows[$script:index] $widths}
   $needed=$headingHeight+$columnHeight+$firstHeight
   if($script:index+1 -ge $rows.Count){$needed+=$tail}
   if($y+$needed -gt $bottom){break}
   $heading=$section.title
   if($script:index -gt 0){$heading+=' (continued)'}
   Text $g $heading $fonts.bold $ink 13 $y $paperWidth $headingHeight
   $y+=$headingHeight
   $g.FillRectangle([Drawing.Brushes]::AliceBlue,13,$y,$paperWidth,$columnHeight)
   Row $g $section.columns $y $fonts.small $widths $columnHeight
   $y+=$columnHeight
   if(!$rows.Count){Text $g 'No records in this date range.' $fonts.body $muted 15 $y ($paperWidth-4) $rowHeight;$y+=$rowHeight}
   while($script:index -lt $rows.Count){
    $row=$rows[$script:index]
    $height=RowHeight $g $row $widths
    $needed=$height
    if($script:index+1 -eq $rows.Count){$needed+=$tail}
    if($y+$needed -gt $bottom){break}
    Row $g $row $y $fonts.body $widths $height
    $y+=$height
    $script:index++
   }
   if($script:index -lt $rows.Count){break}
   $g.FillRectangle([Drawing.Brushes]::AliceBlue,13,$y,$paperWidth,$rowHeight)
   Row $g $section.total $y $fonts.bold $widths $rowHeight
   if($showEstimate){
    Text $g ("Estimated cost: "+$data.estimated+" item lines use current product/variant cost.") $fonts.small $muted 13 ($y+$rowHeight+1) $paperWidth 6
   }
   if($section.note){Text $g $section.note $fonts.small $muted 13 ($y+$rowHeight+1) $paperWidth 6}
   $y+=$tail+$gap
   $script:sectionIndex++
   $script:index=0
  }
  $g.DrawLine($line,13,$footer,$rightEdge,$footer)
  Text $g 'KAY POS | Sale Summary' $fonts.small $muted 13 ($footer+1) ($paperWidth-44) 6
  Text $g ("Page "+$script:page) $fonts.small $muted ($rightEdge-44) ($footer+1) 44 6 $true
  $e.HasMorePages=($script:sectionIndex -lt $data.sections.Count)
 })
 $doc.Print()
 Write-Output 'OK'
}finally{
 $doc.Dispose()
 foreach($font in $fonts.Values){$font.Dispose()}
}
