param([string]$Img, [string]$Ys)
Add-Type -AssemblyName System.Drawing
$bmp = New-Object System.Drawing.Bitmap($Img)
foreach ($Y in ($Ys -split ',')) {
  $Y = [int]$Y
  $segs = @()
  $cur = $null
  for ($x = 0; $x -lt $bmp.Width; $x++) {
    $c = $bmp.GetPixel($x, $Y)
    $is = ($c.R -gt 248 -and $c.G -gt 248 -and $c.B -gt 248) -or ($c.B -gt 150 -and $c.R -lt 60 -and $c.G -lt 130)
    if ($is -and $null -eq $cur) { $cur = @($x, $x) }
    elseif ($is) { $cur[1] = $x }
    elseif ($null -ne $cur) { $segs += ,$cur; $cur = $null }
  }
  if ($null -ne $cur) { $segs += ,$cur }
  Write-Output "== y=$Y =="
  foreach ($s in $segs) {
    $w = $s[1] - $s[0]
    if ($w -gt 40) { Write-Output ("  x {0}-{1} centre={2} largeur={3}" -f $s[0], $s[1], [int](($s[0]+$s[1])/2), $w) }
  }
}
$bmp.Dispose()