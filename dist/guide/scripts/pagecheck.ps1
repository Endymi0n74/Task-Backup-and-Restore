param([string]$Img)
Add-Type -AssemblyName System.Drawing
$bmp = New-Object System.Drawing.Bitmap($Img)
Write-Output "== zones claires style input (blanc pur) par bande y =="
for ($y = 130; $y -lt 700; $y += 25) {
  $white = 0; $dark = 0
  for ($x = 0; $x -lt $bmp.Width; $x += 6) {
    $c = $bmp.GetPixel($x, $y)
    if ($c.R -gt 250 -and $c.G -gt 250 -and $c.B -gt 250) { $white++ }
    if ($c.R -lt 120 -and $c.G -lt 120 -and $c.B -lt 120) { $dark++ }
  }
  Write-Output ("y={0} blanc={1} sombre={2}" -f $y, $white, $dark)
}
$bmp.Dispose()