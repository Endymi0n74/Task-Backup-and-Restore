# ============================================================
#  select-theme.ps1 - bascule du theme de l'interface tsbak
#
#  ui/index.html charge deux feuillets de palette :
#    ui/themes/legacy.css   theme publie (palette historique)
#    ui/themes/hestia.css   variante locale (bleu marine / orange)
#  Seul le feuillet sans media="not all" s'applique : ce script ne
#  bascule que cet attribut. ui/style.css (structure et composants)
#  reste commun aux deux themes, et le binaire embarque le theme
#  actif au moment de sa compilation (frontendDist = ../ui).
#
#  Le livrable publie doit rester sur legacy : tools\make-release.ps1
#  refuse de produire un artefact si la variante est active.
#
#  Actions :
#    -Action Status  (defaut) affiche le theme actif ;
#    -Action Set     -Theme legacy|hestia  bascule (idempotent) ;
#    -Action Check   -Require legacy|hestia [-ErrorOnStale]  controle
#                    sans rien ecrire ; code de sortie 4 si le theme
#                    requis n'est pas actif (et -ErrorOnStale present).
#
#  Usage :  powershell -NoProfile -ExecutionPolicy Bypass `
#                         -File tools\select-theme.ps1 -Action Set -Theme hestia
# ============================================================
[CmdletBinding()]
param(
    [ValidateSet('Status', 'Set', 'Check')][string]$Action = 'Status',
    [ValidateSet('legacy', 'hestia')][string]$Theme,
    [ValidateSet('legacy', 'hestia')][string]$Require = 'legacy',
    [switch]$ErrorOnStale
)

$ErrorActionPreference = 'Stop'

$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$index = Join-Path $root 'ui\index.html'
$themes = @('legacy', 'hestia')
$inactif = 'media="not all"'

$texte = [System.IO.File]::ReadAllText($index)

# Le feuillet d'un theme : un unique <link ... href="themes/<nom>.css" ... />
# dont on sait retirer ou ajouter media="not all".
function Get-Feuillet([string]$Nom) {
    $motif = '(?m)^(?<avant>[ \t]*<link\b[^>]*?href="themes/{0}\.css"[^>]*?)(?<media>[ \t]+media="not all")?(?<fin>[ \t]*/>)' -f [regex]::Escape($Nom)
    $trouves = [regex]::Matches($script:texte, $motif)
    if ($trouves.Count -ne 1) {
        throw ("ui\index.html doit contenir exactement un <link> vers themes\{0}.css (trouve : {1})." -f $Nom, $trouves.Count)
    }
    return $trouves[0]
}

function Get-ThemeActif {
    $actifs = @()
    foreach ($nom in $themes) {
        if (-not (Get-Feuillet $nom).Groups['media'].Success) { $actifs += $nom }
    }
    if ($actifs.Count -ne 1) {
        throw ("Un seul theme doit etre actif dans ui\index.html (actifs : {0})." -f ($actifs -join ', '))
    }
    return $actifs[0]
}

function Set-Feuillet([string]$Nom, [bool]$Actif) {
    $feuillet = Get-Feuillet $Nom
    $lien = $feuillet.Groups['avant'].Value
    if (-not $Actif) { $lien += ' ' + $inactif }
    $lien += $feuillet.Groups['fin'].Value
    $script:texte = $script:texte.Remove($feuillet.Index, $feuillet.Length).Insert($feuillet.Index, $lien)
}

# --- 0. Lecture de l'etat courant -----------------------------------------
$actif = Get-ThemeActif

switch ($Action) {
    'Status' {
        Write-Host ("Theme actif : {0}  (ui\themes\{0}.css)" -f $actif)
        if ($actif -ne 'legacy') {
            Write-Host "Variante locale : ne pas livrer en l'etat (tools\make-release.ps1 la refuse)."
        }
    }

    'Set' {
        if (-not $Theme) { throw "-Theme est obligatoire avec -Action Set (legacy ou hestia)." }
        $feuillet = Join-Path $root ("ui\themes\{0}.css" -f $Theme)
        if (-not (Test-Path $feuillet)) { throw ("Feuillet de theme introuvable : {0}." -f $feuillet) }

        foreach ($nom in $themes) { Set-Feuillet $nom ($nom -eq $Theme) }

        if ($texte -eq [System.IO.File]::ReadAllText($index)) {
            Write-Host ("ui\index.html : le theme {0} est deja actif (inchange)." -f $Theme)
        }
        else {
            [System.IO.File]::WriteAllText($index, $texte, (New-Object System.Text.UTF8Encoding($false)))
            Write-Host ("ui\index.html : theme actif -> {0}" -f $Theme)
            if ($Theme -eq 'legacy') {
                Write-Host "Theme publie retabli : la prochaine compilation embarque la palette d'origine."
            }
            else {
                Write-Host "Variante locale active : le binaire compile maintenant l'embarque (non publiable)."
            }
        }

        # Controle apres ecriture : un index.html remanie doit echouer bruyamment.
        $apres = Get-ThemeActif
        if ($apres -ne $Theme) { throw ("Bascule ratee : theme actif = {0}." -f $apres) }
    }

    'Check' {
        if ($actif -eq $Require) {
            Write-Host ("Theme actif : {0} (conforme a -Require {0})." -f $actif)
        }
        else {
            Write-Host ("Theme actif : {0} ; attendu : {1}." -f $actif, $Require)
            if ($ErrorOnStale) { exit 4 }
        }
    }
}
