# ============================================================
#  make-release.ps1 - reconstruit l'ensemble livrable d'une release
#
#  Depuis dist/ (binaires + assistants + guide) :
#   1. release\tsbak-<version>-windows\   (dossier portable complet)
#   2. release\tsbak-<version>-windows.zip (entrees en '/', Deflate)
#   3. empreintes SHA-256 des binaires et de l'archive
#
#  Idempotent : le dossier et l'archive sont reecrits a chaque
#  execution. Aucune compilation n'est lancee - construire d'abord
#  les binaires (voir AGENTS.md) puis copier le resultat dans dist\.
#
#  Usage :  powershell -NoProfile -ExecutionPolicy Bypass `
#                         -File tools\make-release.ps1
# ============================================================
[CmdletBinding()]
param(
    [string]$Dist,
    [string]$OutDir
)

$ErrorActionPreference = 'Stop'

$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if (-not $Dist)   { $Dist   = Join-Path $root 'dist' }
if (-not $OutDir) { $OutDir = Join-Path $root 'release' }

# --- 0. Garde-fou de theme ------------------------------------------------
# Le livrable publie garde le theme d'origine (palette de
# ui/themes/legacy.css) : l'interface est embarquee dans l'exe a la
# compilation, un theme variante ne doit jamais partir dans une release.
$global:LASTEXITCODE = 0
& (Join-Path $PSScriptRoot 'select-theme.ps1') -Action Check -Require legacy -ErrorOnStale
if ($LASTEXITCODE -eq 4) {
    Write-Host "Arret : le theme local 'hestia' est actif dans ui\index.html."
    Write-Host "  Lancer tools\select-theme.ps1 -Action Set -Theme legacy, recompiler"
    Write-Host "  l'interface, puis relancer ce script : la variante ne doit pas etre livree."
    exit 1
}
if ($LASTEXITCODE -ne 0) {
    Write-Host ("Arret : tools\select-theme.ps1 a echoue (code {0})." -f $LASTEXITCODE)
    exit 1
}

$dist = (Resolve-Path $Dist).Path

# Version unique du projet (src-tauri/tauri.conf.json, cf. AGENTS.md)
$conf = Get-Content (Join-Path $root 'src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json
$name = "tsbak-$($conf.version)-windows"
$target = Join-Path $OutDir $name
$zip = Join-Path $OutDir "$name.zip"

Write-Host "Build de $name"
Write-Host "  source : $dist"
Write-Host "  sortie : $target"

if (Test-Path $target) { Remove-Item $target -Recurse -Force }
New-Item -ItemType Directory -Path $target -Force | Out-Null

# --- 1. Binaires, assistants et documentation ------------------------------
$files = @(
    'TaskBackupRestore.exe',   # interface graphique (WebView2)
    'tsbak.exe',               # ligne de commande (2008 R2 -> 2025+)
    'export.cmd',
    'import.cmd',
    'validate.cmd',
    'Guide-tsbak.pdf',
    'Memo-motifs.pdf',
    'LISEZMOI.txt'
)
foreach ($f in $files) {
    $src = Join-Path $dist $f
    if (-not (Test-Path $src)) { throw "Fichier manquant dans dist : $f" }
    Copy-Item $src (Join-Path $target $f) -Force
}

# --- 2. Sources du guide (HTML + captures + scripts de capture) ------------
Copy-Item (Join-Path $dist 'guide') (Join-Path $target 'guide') -Recurse -Force

# --- 3. Nettoyage du dossier de livraison ---------------------------------
# Fichiers de travail des captures d'ecran...
foreach ($pattern in @('guide\_inspect.html', 'guide\tmp-*')) {
    Get-ChildItem -Path (Join-Path $target $pattern) -Force -ErrorAction SilentlyContinue |
        Remove-Item -Recurse -Force
}
# ...et donnees d'export reelles de la machine de l'auteur (.gitignore) :
# le guide les recree avec guide\demo\demo-export.cmd / demo-import.cmd.
foreach ($folder in @('guide\demo\assist-export', 'guide\demo\export-demo')) {
    $p = Join-Path $target $folder
    if (Test-Path $p) { Remove-Item $p -Recurse -Force }
}

# --- 4. Archive .zip -------------------------------------------------------
# Entrees normalisees en '/' (lisibles hors Windows) et horodatage des
# fichiers conserve.
if (Test-Path $zip) { Remove-Item $zip -Force }
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem

$stream = [System.IO.File]::Open($zip, [System.IO.FileMode]::CreateNew)
$archive = [System.IO.Compression.ZipArchive]::new(
    $stream, [System.IO.Compression.ZipArchiveMode]::Create)
try {
    $prefixLen = $target.Length + 1
    $entries = Get-ChildItem -Path $target -Recurse -File | Sort-Object FullName
    foreach ($file in $entries) {
        $rel = $file.FullName.Substring($prefixLen).Replace('\', '/')
        $entry = $archive.CreateEntry($rel, [System.IO.Compression.CompressionLevel]::Optimal)
        $entry.LastWriteTime = $file.LastWriteTime
        $input = [System.IO.File]::OpenRead($file.FullName)
        $output = $entry.Open()
        try { $input.CopyTo($output) } finally { $output.Dispose(); $input.Dispose() }
    }
} finally {
    $archive.Dispose()
    $stream.Dispose()
}

# --- 5. Empreintes ---------------------------------------------------------
Write-Host ''
Write-Host 'SHA-256'
$shipped = Get-ChildItem $target -File | Where-Object { $_.Extension -eq '.exe' }
foreach ($exe in $shipped) {
    Write-Host ('  {0}  {1}' -f (Get-FileHash $exe.FullName -Algorithm SHA256).Hash.ToLower(), $exe.Name)
}
Write-Host ('  {0}  {1}' -f (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower(), (Split-Path $zip -Leaf))
Write-Host ''
Write-Host ('{0} fichiers dans le dossier, {1} entrees dans l''archive' -f `
    (Get-ChildItem $target -Recurse -File).Count, $entries.Count)
