param([string]$Img, [int]$X = 40)
Add-Type -AssemblyName System.Drawing
$bmp = New-Object System.Drawing.Bitmap($Img)
$prev = ""
$start = 0
for ($y = 0; $y -lt $bmp.Height; $y++) {
  $c = $bmp.GetPixel($X, $y)
  $hex = "{0:X2}{1:X2}{2:X2}" -f $c.R, $c.G, $c.B
  if ($hex -ne $prev) {
    if ($prev -ne "") { Write-Output ("{0,4}-{1,4}  #{2}" -f $start, ($y - 1), $prev) }
    $start = $y; $prev = $hex
  }
}
Write-Output ("{0,4}-{1,4}  #{2}" -f $start, ($bmp.Height - 1), $prev)
$bmp.Dispose()