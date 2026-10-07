# Recompose chaque figure du guide (image source + pastilles .mark) pour
# verification visuelle, sans navigateur : memes coordonnees % que guide.html.
# Sortie : dist\guide\figcheck\fig-NN.png (a supprimer apres controle).
param(
  [string]$Repo = "D:\Codex\tsbak-gui"
)
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing

$guide = Join-Path $Repo "dist\guide\guide.html"
$outDir = Join-Path $Repo "dist\guide\figcheck"
if (-not (Test-Path $outDir)) { New-Item -ItemType Directory -Path $outDir | Out-Null }
Get-ChildItem $outDir -Filter *.png | Remove-Item -Force

$html = Get-Content -Raw -Encoding UTF8 $guide

# Un bloc <figure> = image + marques + legende.
$figRe = [regex]'(?s)<figure>\s*<div class="shot">\s*<img src="(?<src>[^"]+)"[^>]*>(?<marks>(?s:.*?)?)</div>\s*<figcaption>(?<cap>.*?)</figcaption>\s*</figure>'
$markRe = [regex]'<span class="mark (?<cls>[a-z]+)" style="left:(?<l>[0-9.]+)%;\s*top:(?<t>[0-9.]+)%">(?<n>\d+)</span>'

$n = 0
foreach ($m in $figRe.Matches($html)) {
  $n++
  $src = Join-Path $Repo ("dist\guide\" + $m.Groups["src"].Value -replace '/', '\')
  $img = [Drawing.Bitmap]::FromFile($src)
  $g = [Drawing.Graphics]::FromImage($img)
  $g.SmoothingMode = [Drawing.Drawing2D.SmoothingMode]::AntiAlias

  foreach ($mk in $markRe.Matches($m.Groups["marks"].Value)) {
    $cx = [float]::Parse($mk.Groups["l"].Value, [Globalization.CultureInfo]::InvariantCulture) / 100 * $img.Width + 13
    $cy = [float]::Parse($mk.Groups["t"].Value, [Globalization.CultureInfo]::InvariantCulture) / 100 * $img.Height + 13
    $color = switch ($mk.Groups["cls"].Value) { "blue" { "#1e5aa8" } "green" { "#2f9e44" } "warn" { "#f08c00" } default { "#e03131" } }
    $brush = New-Object Drawing.SolidBrush ([Drawing.ColorTranslator]::FromHtml($color))
    $pen = New-Object Drawing.Pen ([Drawing.Color]::White), 2
    $g.FillEllipse($brush, ($cx - 13), ($cy - 13), 26, 26)
    $g.DrawEllipse($pen, ($cx - 13), ($cy - 13), 26, 26)
    $txt = $mk.Groups["n"].Value
    $font = New-Object Drawing.Font "Segoe UI", 13, ([Drawing.FontStyle]::Bold), ([Drawing.GraphicsUnit]::Point)
    $sf = New-Object Drawing.StringFormat
    $sf.Alignment = [Drawing.StringAlignment]::Center
    $sf.LineAlignment = [Drawing.StringAlignment]::Center
    $rect = New-Object Drawing.RectangleF (($cx - 13), ($cy - 13), 26, 26)
    $white = [Drawing.Brushes]::White
    $g.DrawString($txt, $font, $white, $rect, $sf)
    $brush.Dispose(); $pen.Dispose(); $font.Dispose()
  }
  $g.Dispose()
  $out = Join-Path $outDir ("fig-{0:d2}.png" -f $n)
  $img.Save($out, [Drawing.Imaging.ImageFormat]::Png)
  $img.Dispose()
  "fig-{0:d2} <- {1}" -f $n, $m.Groups["src"].Value
}
"OK : $n figures dans $outDir"
