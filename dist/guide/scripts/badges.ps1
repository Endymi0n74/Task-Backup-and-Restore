param([string]$Img)
Add-Type -AssemblyName System.Drawing
$bmp = New-Object System.Drawing.Bitmap($Img)
$green = 0; $red = 0; $blue = 0
for ($y = 330; $y -lt $bmp.Height; $y += 3) {
  for ($x = 0; $x -lt $bmp.Width; $x += 3) {
    $c = $bmp.GetPixel($x, $y)
    if ($c.G -gt 140 -and $c.R -lt 90 -and $c.B -lt 110) { $green++ }
    elseif ($c.R -gt 170 -and $c.G -lt 100 -and $c.B -lt 100) { $red++ }
    elseif ($c.B -gt 140 -and $c.R -lt 90 -and $c.G -lt 140) { $blue++ }
  }
}
Write-Output "vert=$green rouge=$red bleu=$blue"
$bmp.Dispose()