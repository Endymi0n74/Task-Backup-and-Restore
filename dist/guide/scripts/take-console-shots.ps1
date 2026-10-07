# Prise de vue des captures console du guide (04, 05, 07, 08, 09).
#
# Repose sur capture.ps1 (console conhost + saisie par WM_CHAR) : aucune
# dependance au focus clavier ni a SendKeys, contrairement a l'ancien
# take-shots.ps1 supprime. Chaque appel lance une console, envoie les
# reponses, attend la fin de la commande, capture puis tue la console.
#
# Usage :
#   powershell -NoProfile -ExecutionPolicy Bypass -File take-console-shots.ps1
param(
  [string]$Repo = "D:\Codex\tsbak-gui"
)

$ErrorActionPreference = "Stop"
$cap     = Join-Path $Repo "dist\guide\scripts\capture.ps1"
$shots   = Join-Path $Repo "dist\guide\shots"
$dist    = Join-Path $Repo "dist"
$demo    = Join-Path $Repo "dist\guide\demo"

# --- 04 : export filtre en ligne de commande (demo-export.cmd) ------------
# En-tete + "tsbak export .\export-demo --include \MSIAfterburner" :
# 1 tache exportee, les autres ignorees par le filtre.
& $cap -Inner (Join-Path $demo "demo-export.cmd") `
       -Out (Join-Path $shots "04-cli-export.png") -WaitSec 6

# --- 05 : validation + simulation bloquee (demo-import.cmd) ---------------
# "tsbak validate" puis "tsbak import --folder ... --dry-run --yes" :
# l'utilisateur source n'est pas mappe -> tache bloquee (voir options
# avancees : --user-map / --answer-file).
& $cap -Inner (Join-Path $demo "demo-import.cmd") `
       -Out (Join-Path $shots "05-cli-import.png") -WaitSec 6

# --- 07 : assistant validate.cmd (une question : le dossier) --------------
& $cap -Inner (Join-Path $dist "validate.cmd") `
       -Out (Join-Path $shots "07-cmd-validate.png") `
       -Answers (Join-Path $demo "export-demo") -WaitSec 5

# --- 08 : assistant export.cmd (export complet + verification) ------------
$dest = "D:\Codex\tmp\guide-export-" + (Get-Date -Format "yyyy-MM-dd")
if (Test-Path $dest) { Remove-Item $dest -Recurse -Force }
& $cap -Inner (Join-Path $dist "export.cmd") `
       -Out (Join-Path $shots "08-cmd-export.png") `
       -Answers $dest -WaitSec 12

# --- 09 : assistant import.cmd (4 questions, dossier cible vide = chemins
#            d'origine -> tache identique -> rien a faire) -----------------
& $cap -Inner (Join-Path $dist "import.cmd") `
       -Out (Join-Path $shots "09-cmd-import.png") `
       -Answers ((Join-Path $demo "export-demo") + "|||") -WaitSec 8

Write-Host "ALL DONE"
