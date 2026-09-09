param([string]$Img)
Add-Type -AssemblyName System.Drawing
$bmp = New-Object System.Drawing.Bitmap($Img)
Write-Output "== texte bleu (onglet actif) par bande y =="
for ($y = 62; $y -le 105; $y += 3) {
  $blue = 0; $dark = 0
  for ($x = 0; $x -lt $bmp.Width; $x += 2) {
    $c = $bmp.GetPixel($x, $y)
    if ($c.B -gt 130 -and $c.R -lt 90 -and $c.G -lt 130) { $blue++ }
    if ($c.R -lt 130 -and $c.G -lt 130 -and $c.B -lt 130) { $dark++ }
  }
  if ($blue -gt 2 -or $dark -gt 5) { Write-Output ("y={0} bleu={1} sombre={2}" -f $y, $blue, $dark) }
}
$bmp.Dispose()