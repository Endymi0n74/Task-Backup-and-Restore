param([string]$Img)
Add-Type -AssemblyName System.Drawing
$bmp = New-Object System.Drawing.Bitmap($Img)
Write-Output "== bande onglets (blanc par ligne) =="
for ($y = 55; $y -le 115; $y += 5) {
  $w = 0
  for ($x = 0; $x -lt $bmp.Width; $x += 4) {
    $c = $bmp.GetPixel($x, $y)
    if ($c.R -gt 240 -and $c.G -gt 240 -and $c.B -gt 240) { $w++ }
  }
  Write-Output ("y={0} blanc={1}" -f $y, $w)
}
Write-Output "== segments onglets a y=80 =="
$seg = @(); $inSeg = $false
for ($x = 0; $x -lt $bmp.Width; $x++) {
  $c = $bmp.GetPixel($x, 80)
  $isW = ($c.R -gt 240 -and $c.G -gt 240 -and $c.B -gt 240)
  if ($isW -and -not $inSeg) { $seg += ,@($x); $inSeg = $true }
  elseif (-not $isW -and $inSeg) { $seg[$seg.Count-1] += $x; $inSeg = $false }
}
foreach ($s in $seg) {
  if ($s[1] - $s[0] -gt 30) { Write-Output ("onglet x {0}-{1} centre={2}" -f $s[0], $s[1], [int](($s[0]+$s[1])/2)) }
}
$bmp.Dispose()