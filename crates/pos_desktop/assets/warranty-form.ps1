$ErrorActionPreference='Stop'
[Console]::InputEncoding=[Text.Encoding]::UTF8
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
$data=[Console]::In.ReadToEnd() | ConvertFrom-Json
$r=$data.record
$script:ink=[Drawing.ColorTranslator]::FromHtml('#152b3c')
$script:muted=[Drawing.ColorTranslator]::FromHtml('#627581')
$script:teal=[Drawing.ColorTranslator]::FromHtml('#087f8c')
$script:line=[Drawing.ColorTranslator]::FromHtml('#dbe5e9')
$script:fonts=@{
 body=[Drawing.Font]::new('Myanmar Text',10)
 small=[Drawing.Font]::new('Myanmar Text',8)
 bold=[Drawing.Font]::new('Myanmar Text',10,[Drawing.FontStyle]::Bold)
 title=[Drawing.Font]::new('Segoe UI',20,[Drawing.FontStyle]::Bold)
 shop=[Drawing.Font]::new('Myanmar Text',16,[Drawing.FontStyle]::Bold)
 serial=[Drawing.Font]::new('Segoe UI',15,[Drawing.FontStyle]::Bold)
}
$script:logo=$null;$script:logoStream=$null
if ($data.logo -and $data.logo.StartsWith('data:image/png;base64,')) {
 $script:logoStream=[IO.MemoryStream]::new([Convert]::FromBase64String($data.logo.Substring(22)))
 $script:logo=[Drawing.Image]::FromStream($script:logoStream)
}
function Text($g,$text,$font,$color,$x,$y,$w,$h=12) {
 $brush=[Drawing.SolidBrush]::new($color)
 try {$g.DrawString([string]$text,$font,$brush,[Drawing.RectangleF]::new($x,$y,$w,$h))} finally {$brush.Dispose()}
}
function Lines($g,[string]$text,$font,[single]$width) {
 $result=[Collections.Generic.List[string]]::new();$line=''
 $elements=[Globalization.StringInfo]::GetTextElementEnumerator($text)
 while($elements.MoveNext()) {
  $c=$elements.GetTextElement();if($c -eq "`r"){continue}
  if($c -eq "`n"){$result.Add($line);$line='';continue}
  if($line -and $g.MeasureString($line+$c,$font).Width -gt $width){$result.Add($line);$line=''}
  $line+=$c
 }
 if($line){$result.Add($line)}
 if(!$result.Count){$result.Add('-')}
 return ,$result
}
$script:blocks=$null;$script:index=0;$script:page=0
function Build-Blocks($g) {
 $script:blocks=[Collections.Generic.List[object]]::new()
 function Section($title) {$script:blocks.Add(@{kind='section';text=$title;height=10})}
 function Field($label,$value) {
  $script:blocks.Add(@{kind='label';text=$label;height=5})
  foreach($line in (Lines $g ([string]$value) $script:fonts.body 174)) {$script:blocks.Add(@{kind='text';text=$line;height=6})}
  $script:blocks.Add(@{kind='space';height=3})
 }
 if($script:longShop){Field 'SHOP NAME' $data.shop}
 if($script:longContacts){Field 'SHOP CONTACT' ((@($data.phone,$data.address)|Where-Object {$_}) -join ' | ')}
 Section '01  CUSTOMER DETAILS'
 $nameLines=Lines $g ([string]$r.customer) $script:fonts.body 110
 $phoneLines=Lines $g ([string]$r.phone) $script:fonts.body 56
 $script:blocks.Add(@{kind='customer';names=$nameLines;phones=$phoneLines;height=(7+6*[Math]::Max($nameLines.Count,$phoneLines.Count))})
 Field 'ADDRESS' $r.address
 Section '02  PRODUCT & SERIAL NUMBER'
 Field 'PRODUCT / MODEL' $r.product
 $script:blocks.Add(@{kind='label';text='SERIAL NUMBER';height=5})
 foreach($line in (Lines $g ([string]$r.serial) $script:fonts.serial 170)) {$script:blocks.Add(@{kind='serial';text=$line;height=9})}
 $script:blocks.Add(@{kind='space';height=5})
 Section '03  WARRANTY PERIOD'
 $script:blocks.Add(@{kind='dates';height=25})
 $a=[DateTime]::ParseExact($r.starts,'yyyy-MM-dd',$null);$b=[DateTime]::ParseExact($r.ends,'yyyy-MM-dd',$null)
 $months=($b.Year-$a.Year)*12+$b.Month-$a.Month
 Field 'WARRANTY TERM' "$months months from the original sale date (end date inclusive)"
 $script:blocks.Add(@{kind='note';text='For service, quote the serial number and invoice shown on this form. Keep this form with your original sales receipt.';height=18})
 if($r.sale_status -ne 'completed') {$script:blocks.Add(@{kind='warning';text='RECEIPT REFUNDED / NOT COMPLETED - WARRANTY NOT ACTIVE';height=12})}
 $script:blocks.Add(@{kind='signatures';height=29})
}
function Draw-Page($g) {
 $g.PageUnit=[Drawing.GraphicsUnit]::Millimeter
 $g.TextRenderingHint=[Drawing.Text.TextRenderingHint]::AntiAliasGridFit
 $g.SmoothingMode=[Drawing.Drawing2D.SmoothingMode]::HighQuality
 $g.Clear([Drawing.Color]::White)
 $script:page++
 $pen=[Drawing.Pen]::new($script:line,0.3)
 $accent=[Drawing.SolidBrush]::new($script:teal)
 $pale=[Drawing.SolidBrush]::new([Drawing.ColorTranslator]::FromHtml('#f0f6f7'))
 try {
  $g.FillRectangle($accent,16,14,2,18)
  $brandX=23
  if($script:logo){$scale=[Math]::Min(18/$script:logo.Width,18/$script:logo.Height);$g.DrawImage($script:logo,[single]23,[single]14,[single]($script:logo.Width*$scale),[single]($script:logo.Height*$scale));$brandX=45}
  $shopLines=Lines $g ([string]$data.shop) $script:fonts.shop (194-$brandX)
  $script:longShop=$shopLines.Count -gt 2
  $shopTitle=$data.shop;if($script:longShop){$shopTitle='Shop details below'}
  $shopY=18;if($shopLines.Count -gt 1 -and !$script:longShop){$shopY=13}
  Text $g $shopTitle $script:fonts.shop $script:ink $brandX $shopY (194-$brandX) 19
  $contacts=(@($data.phone,$data.address)|Where-Object {$_}) -join ' | '
  $contactLines=Lines $g $contacts $script:fonts.small 178
  $script:longContacts=$contactLines.Count -gt 2
  if($script:longContacts){$contactLines=@('Shop contact details below')}
  $cy=34
  foreach($line in $contactLines) {Text $g $line $script:fonts.small $script:muted 16 $cy 178 6;$cy+=4.5}
  $g.DrawLine($pen,16,$cy+3,194,$cy+3)
  $y=$cy+6
  Text $g 'WARRANTY FORM' $script:fonts.title $script:ink 16 $y 178 15
  $y+=14
  Text $g ("Invoice  "+$r.invoice) $script:fonts.bold $script:ink 16 $y 110 9
  Text $g ("Issued  "+$data.issued) $script:fonts.small $script:muted 139 $y 55 9
  $y+=10
  if(!$script:blocks){Build-Blocks $g}
  while($script:index -lt $script:blocks.Count) {
   $block=$script:blocks[$script:index];$h=[single]$block.height
   $needed=$h;if($block.kind -in @('section','label')) {$needed+=12}
   if($block.kind -eq 'note') {$needed=0;for($n=$script:index;$n -lt $script:blocks.Count;$n++) {$needed+=$script:blocks[$n].height}}
   if($y+$needed -gt 268){break}
   switch($block.kind) {
    'section' {$g.FillRectangle($pale,16,$y,178,8);Text $g $block.text $script:fonts.bold $script:teal 19 ($y+0.7) 172 8}
    'customer' {
     Text $g 'CUSTOMER NAME' $script:fonts.small $script:muted 17 $y 110 6
     Text $g 'PHONE' $script:fonts.small $script:muted 136 $y 56 6
     $dy=$y+5;foreach($line in $block.names){Text $g $line $script:fonts.body $script:ink 17 $dy 110 8;$dy+=6}
     $dy=$y+5;foreach($line in $block.phones){Text $g $line $script:fonts.body $script:ink 136 $dy 56 8;$dy+=6}
    }
    'label' {Text $g $block.text $script:fonts.small $script:muted 17 $y 176 6}
    'text' {Text $g $block.text $script:fonts.body $script:ink 17 $y 176 8}
    'serial' {$g.FillRectangle($pale,16,$y,178,$h);Text $g $block.text $script:fonts.serial $script:ink 19 ($y-0.5) 172 11}
    'dates' {
     $g.DrawRectangle($pen,16,$y,86,21);$g.DrawRectangle($pen,108,$y,86,21)
     Text $g 'WARRANTY START' $script:fonts.small $script:muted 20 ($y+2) 78 6
     Text $g $r.starts $script:fonts.serial $script:teal 20 ($y+8) 78 11
     Text $g 'WARRANTY END' $script:fonts.small $script:muted 112 ($y+2) 78 6
     Text $g $r.ends $script:fonts.serial $script:teal 112 ($y+8) 78 11
    }
    'note' {Text $g $block.text $script:fonts.small $script:muted 17 $y 176 18}
    'warning' {Text $g $block.text $script:fonts.bold ([Drawing.Color]::Firebrick) 17 $y 176 12}
    'signatures' {
     $g.DrawLine($pen,18,$y+16,91,$y+16);$g.DrawLine($pen,119,$y+16,192,$y+16)
     Text $g 'Customer signature' $script:fonts.small $script:muted 18 ($y+18) 73 8
     Text $g 'Authorized signature / Shop stamp' $script:fonts.small $script:muted 119 ($y+18) 73 8
    }
   }
   $y+=$h;$script:index++
  }
  $g.DrawLine($pen,16,276,194,276)
  Text $g ('Reference: '+$r.id) $script:fonts.small $script:muted 16 279 148 10
  Text $g ('Page '+$script:page) $script:fonts.small $script:muted 175 279 19 8
 } finally {$pen.Dispose();$accent.Dispose();$pale.Dispose()}
}
try {
 if($data.mode -eq 'preview') {
  $image=[Drawing.Bitmap]::new(1240,1754);$image.SetResolution(150,150);$g=[Drawing.Graphics]::FromImage($image)
  try {Draw-Page $g;$image.Save($data.path,[Drawing.Imaging.ImageFormat]::Png)}finally{$g.Dispose();$image.Dispose()}
 }else {
  $doc=[Drawing.Printing.PrintDocument]::new()
  try {
   $doc.DocumentName='Warranty '+$r.invoice
   if($data.mode -eq 'pdf') {
    $doc.PrinterSettings.PrinterName='Microsoft Print to PDF'
    if(!$doc.PrinterSettings.IsValid){throw 'Microsoft Print to PDF is unavailable. Enable this Windows printer to export PDFs.'}
    $doc.PrinterSettings.PrintToFile=$true;$doc.PrinterSettings.PrintFileName=$data.path
    $doc.PrintController=[Drawing.Printing.StandardPrintController]::new()
   }else {
    $dialog=[Windows.Forms.PrintDialog]::new();$dialog.Document=$doc;$dialog.UseEXDialog=$true
    try {if($dialog.ShowDialog() -ne [Windows.Forms.DialogResult]::OK){Write-Output 'CANCELLED';return}}finally{$dialog.Dispose()}
   }
   $a4=@($doc.PrinterSettings.PaperSizes|Where-Object {$_.Kind -eq [Drawing.Printing.PaperKind]::A4})
   if(!$a4.Count){throw 'Selected printer does not support A4 paper. Choose an A4 printer.'}
   $doc.DefaultPageSettings.PaperSize=$a4[0];$doc.DefaultPageSettings.Landscape=$false
   $doc.DefaultPageSettings.Margins=[Drawing.Printing.Margins]::new(0,0,0,0)
   $doc.add_PrintPage({param($sender,$e)
    $e.Graphics.PageUnit=[Drawing.GraphicsUnit]::Millimeter
    $e.Graphics.TranslateTransform([single](-$e.PageSettings.HardMarginX*0.254),[single](-$e.PageSettings.HardMarginY*0.254))
    Draw-Page $e.Graphics
    $e.HasMorePages=($script:index -lt $script:blocks.Count)
   })
   $doc.Print()
  }finally{$doc.Dispose()}
 }
 Write-Output 'OK'
}finally {
 foreach($font in $script:fonts.Values){$font.Dispose()}
 if($script:logo){$script:logo.Dispose();$script:logoStream.Dispose()}
}
