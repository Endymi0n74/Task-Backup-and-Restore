param([string]$Img)
Add-Type -AssemblyName System.Drawing
$bmp = New-Object System.Drawing.Bitmap($Img)
Write-Output "== lignes avec bordure (pixel gris ~border) =="
for ($y = 130; $y -le 330; $y += 2) {
  $border = 0
  for ($x = 0; $x -lt $bmp.Width; $x += 4) {
    $c = $bmp.GetPixel($x, $y)
    if ($c.R -ge 200 -and $c.R -le 225 -and $c.G -ge 205 -and $c.G -le 230 -and $c.B -ge 215) { $border++ }
  }
  if ($border -gt 60) { Write-Output ("y={0} bordure={1}" -f $y, $border) }
}
Write-Output "== boutons bleus par ligne =="
for ($y = 130; $y -le 330; $y += 2) {
  $blue = 0
  for ($x = 0; $x -lt $bmp.Width; $x += 4) {
    $c = $bmp.GetPixel($x, $y)
    if ($c.B -gt 150 -and $c.R -lt 60 -and $c.G -lt 130) { $blue++ }
  }
  if ($blue -gt 20) { Write-Output ("y={0} bleu={1}" -f $y, $blue) }
}
$bmp.Dispose()