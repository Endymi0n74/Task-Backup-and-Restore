# rowextent.ps1 -Img <png> -Y0 <int> -Y1 <int> [-Dark] [-Gap <int>] [-X0 <int>] [-X1 <int>]
# Affiche les segments horizontaux "d'encre" d'une bande de lignes d'une capture.
#  - mode clair (defaut) : texte clair sur fond sombre (consoles)  -> pixel lum > 90
#  - -Dark                : texte sombre sur fond clair (GUI)      -> pixel lum < 170
# Sortie : segments x0..x1 (largeur), separes si l'espace vide > -Gap (defaut 8 px).
param(
  [Parameter(Mandatory=$true)][string]$Img,
  [Parameter(Mandatory=$true)][int]$Y0,
  [Parameter(Mandatory=$true)][int]$Y1,
  [switch]$Dark,
  [int]$Gap = 8,
  [int]$X0 = -1,
  [int]$X1 = -1
)
Add-Type -AssemblyName System.Drawing
$bmp = [System.Drawing.Bitmap]::FromFile($Img)
try {
  $w = $bmp.Width; $h = $bmp.Height
  if ($Y1 -ge $h) { $Y1 = $h - 1 }
  if ($X0 -lt 0) { $X0 = 0 }
  if ($X1 -lt 0 -or $X1 -ge $w) { $X1 = $w - 1 }
  $thr = if ($Dark) { 170 } else { 90 }
  $ink = New-Object bool[] $w
  for ($y = $Y0; $y -le $Y1; $y++) {
    for ($x = $X0; $x -le $X1; $x++) {
      if ($ink[$x]) { continue }
      $c = $bmp.GetPixel($x, $y)
      $lum = 0.299*$c.R + 0.587*$c.G + 0.114*$c.B
      $hit = if ($Dark) { $lum -lt $thr } else { $lum -gt $thr }
      if ($hit) { $ink[$x] = $true }
    }
  }
  $segs = @(); $start = -1; $last = -1
  for ($x = $X0; $x -le $X1 + 1; $x++) {
    $on = if ($x -le $X1) { $ink[$x] } else { $false }
    if ($on) { if ($start -lt 0) { $start = $x }; $last = $x }
    else {
      if ($start -ge 0) {
        $next = -1
        for ($xx = $x; $xx -le $X1; $xx++) { if ($ink[$xx]) { $next = $xx; break } }
        if ($next -lt 0 -or ($next - $last) -gt $Gap) {
          $segs += ("{0}..{1} (w={2})" -f $start, $last, ($last - $start + 1))
          $start = -1
        }
      }
    }
  }
  "$(Split-Path $Img -Leaf)  ${w}x${h}  bande y=$Y0..$Y1  mode=$(if($Dark){'sombre/fond clair'}else{'clair/fond sombre'})"
  if ($segs.Count -eq 0) { "(aucun encre)" } else { $segs -join "  |  " }
}
finally { $bmp.Dispose() }
