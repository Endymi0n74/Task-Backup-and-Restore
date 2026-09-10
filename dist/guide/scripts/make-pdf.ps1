# Genere un PDF a partir d'un fichier HTML avec Edge headless.
# Usage :
#   powershell -NoProfile -ExecutionPolicy Bypass -File make-pdf.ps1 `
#       -Html "dist/guide/guide.html" -Pdf "dist/Guide-tsbak.pdf"
#
# Edge headless imprime le HTML tel qu'un navigateur le rend (CSS @page
# respectee : A4, marges 13mm/12mm). Aucune installation supplementaire.
param(
  [Parameter(Mandatory = $true)][string]$Html,
  [Parameter(Mandatory = $true)][string]$Pdf
)

$ErrorActionPreference = "Stop"

$edgeCandidates = @(
  "${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe",
  "$env:ProgramFiles\Microsoft\Edge\Application\msedge.exe"
)
$edge = $edgeCandidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $edge) {
  throw "msedge.exe introuvable. Installer Microsoft Edge ou adapter le chemin."
}

$htmlPath = (Resolve-Path $Html).Path
$pdfPath = [System.IO.Path]::GetFullPath((Join-Path (Get-Location).Path $Pdf))
$pdfDir = Split-Path $pdfPath -Parent
if (-not (Test-Path $pdfDir)) { $null = New-Item -ItemType Directory -Path $pdfDir -Force }

# Profil temporaire jetable : n'accede jamais au profil Edge de l'utilisateur.
$userData = Join-Path $env:TEMP ("edge-pdf-{0}-{1}" -f $PID, [DateTime]::Now.Ticks)

$args = @(
  "--headless",
  "--disable-gpu",
  "--no-first-run",
  "--disable-extensions",
  "--user-data-dir=$userData",
  "--print-to-pdf=$pdfPath",
  "--no-pdf-header-footer",
  ("`"{0}`"" -f $htmlPath)
)

& $edge @args | Out-Null
$exit = $LASTEXITCODE

# Nettoyage du profil temporaire (best effort).
try { Remove-Item -Recurse -Force $userData -ErrorAction SilentlyContinue } catch {}

if ($exit -ne 0 -or -not (Test-Path $pdfPath)) {
  throw "La generation du PDF a echoue (code $exit)."
}

$info = Get-Item $pdfPath
Write-Output ("OK : {0} ({1:N0} octets)" -f $info.FullName, $info.Length)
