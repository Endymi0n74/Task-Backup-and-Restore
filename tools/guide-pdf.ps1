# ============================================================
#  guide-pdf.ps1 - fraicheur des PDF livres (guide + memo)
#
#  Les PDF de dist/ sont generes depuis dist/guide/ par Edge
#  headless (dist/guide/scripts/make-pdf.ps1). Ce script enregistre
#  l'empreinte des sources de chaque PDF dans
#  dist/guide/pdf-sources.sha256, puis compare : un PDF non regenere
#  apres une modification des sources est donc detectable.
#
#  Actions :
#    -Action Check   (defaut) compare les empreintes, n'ecrit rien ;
#    -Action Build   regenere les PDF puis enregistre les empreintes ;
#    -Action Update  enregistre les empreintes SANS regenerer (PDF
#                    retouche a la main : declare les sources couvertes).
#
#  Sortie : rapport lisible ; si GITHUB_OUTPUT / GITHUB_STEP_SUMMARY
#  sont definis (GitHub Actions), les sorties perime / aJour / perimes
#  et un resume y sont ajoutes.
#  Code de sortie : 0 ; 4 si -ErrorOnStale et au moins un PDF perime.
#
#  Usage :  powershell -NoProfile -ExecutionPolicy Bypass `
#                         -File tools\guide-pdf.ps1 -Action Check
# ============================================================
[CmdletBinding()]
param(
    [ValidateSet('Check', 'Build', 'Update')][string]$Action = 'Check',
    [switch]$ErrorOnStale
)

$ErrorActionPreference = 'Stop'

$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$manifeste = Join-Path $root 'dist\guide\pdf-sources.sha256'
$makePdf = Join-Path $root 'dist\guide\scripts\make-pdf.ps1'
$sep = [System.IO.Path]::DirectorySeparatorChar

# Generateur commun : toute modification perime tous les PDF.
$generateur = 'dist/guide/scripts/make-pdf.ps1'

# PDF livre -> source HTML + dossier des captures.
$cibles = @(
    [pscustomobject]@{
        Pdf     = 'dist/Guide-tsbak.pdf'
        Html    = 'dist/guide/guide.html'
        Sources = @('dist/guide/guide.html', 'dist/guide/shots')
    },
    [pscustomobject]@{
        Pdf     = 'dist/Memo-motifs.pdf'
        Html    = 'dist/guide/memo-motifs.html'
        Sources = @('dist/guide/memo-motifs.html', 'dist/guide/shots')
    }
)

# Extensions dont les fins de ligne sont normalisees en LF avant hachage :
# le depot n'embarque pas de .gitattributes et core.autocrlf vaut true ici,
# une meme source peut donc arriver en LF ou en CRLF selon la machine.
$extensionsTexte = @(
    '.html', '.css', '.js', '.json', '.svg', '.txt', '.ps1', '.cmd', '.yml', '.yaml', '.md'
)

function Get-HashFichier([string]$Chemin) {
    $ext = [System.IO.Path]::GetExtension($Chemin).ToLowerInvariant()
    if ($extensionsTexte -contains $ext) {
        $texte = [System.IO.File]::ReadAllText($Chemin)
        $texte = $texte.Replace("`r`n", "`n").Replace("`r", "`n")
        $octets = [System.Text.Encoding]::UTF8.GetBytes($texte)
    }
    else {
        $octets = [System.IO.File]::ReadAllBytes($Chemin)
    }
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try { return (($sha.ComputeHash($octets) | ForEach-Object { $_.ToString('x2') }) -join '') }
    finally { $sha.Dispose() }
}

function Get-FichiersSuivis([string[]]$Chemins) {
    $liste = New-Object System.Collections.Generic.List[string]
    foreach ($chemin in $Chemins) {
        $suivis = @(& git -C $root ls-files -- $chemin)
        if ($LASTEXITCODE -ne 0) { throw "git ls-files a echoue pour '$chemin'." }
        foreach ($f in $suivis) { if ($f) { $liste.Add($f) } }
    }
    return ($liste | Sort-Object -Unique)
}

function Get-EmpreinteSources($Cible) {
    $sb = New-Object System.Text.StringBuilder
    foreach ($rel in (Get-FichiersSuivis (@($Cible.Sources) + $generateur))) {
        $abs = Join-Path $root ($rel.Replace('/', $sep))
        if (-not (Test-Path -LiteralPath $abs)) { throw "Source introuvable : $rel" }
        [void]$sb.Append($rel).Append("`n").Append((Get-HashFichier $abs)).Append("`n")
    }
    $sha = [System.Security.Cryptography.SHA256]::Create()
    try {
        $octets = [System.Text.Encoding]::UTF8.GetBytes($sb.ToString())
        return (($sha.ComputeHash($octets) | ForEach-Object { $_.ToString('x2') }) -join '')
    }
    finally { $sha.Dispose() }
}

function Read-Manifeste {
    $table = @{}
    if (-not (Test-Path -LiteralPath $manifeste)) { return $table }
    foreach ($ligne in [System.IO.File]::ReadAllLines($manifeste)) {
        if ($ligne -match '^([0-9a-f]{64})\s{2}(.+?)\s*$') { $table[$Matches[2]] = $Matches[1] }
    }
    return $table
}

function Write-Manifeste($Empreintes) {
    $lignes = @(
        '# Empreintes des sources du guide PDF - genere par tools/guide-pdf.ps1',
        '# Ne pas editer a la main : modifier les sources, puis',
        '#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\guide-pdf.ps1 -Action Build',
        '# <sha256 des sources>  <PDF livre>'
    )
    foreach ($pdf in ($Empreintes.Keys | Sort-Object)) { $lignes += ('{0}  {1}' -f $Empreintes[$pdf], $pdf) }
    $contenu = ($lignes -join "`n") + "`n"
    [System.IO.File]::WriteAllText($manifeste, $contenu, (New-Object System.Text.UTF8Encoding($false)))
}

function Get-Empreintes {
    $empreintes = @{}
    foreach ($cible in $cibles) { $empreintes[$cible.Pdf] = Get-EmpreinteSources $cible }
    return $empreintes
}

# Ajout sans BOM : PowerShell 5.1 en ecrit un avec -Encoding utf8, ce qui
# corromprait la premiere cle de GITHUB_OUTPUT.
function Add-LigneSansBom([string]$Chemin, [string]$Contenu) {
    [System.IO.File]::AppendAllText($Chemin, ($Contenu + "`n"), (New-Object System.Text.UTF8Encoding($false)))
}

function Write-Resume([string]$Titre, [string[]]$Lignes) {
    if (-not $env:GITHUB_STEP_SUMMARY) { return }
    $md = @("### $Titre", '') + ($Lignes | ForEach-Object { "- $_" }) + @('', '')
    Add-LigneSansBom $env:GITHUB_STEP_SUMMARY ($md -join "`n")
}

function Write-Sorties($Perimes, $AJour) {
    if (-not $env:GITHUB_OUTPUT) { return }
    $lignes = @(
        ('perime={0}' -f $(if ($Perimes.Count -gt 0) { 'true' } else { 'false' })),
        ('perimes={0}' -f ($Perimes -join ', ')),
        ('aJour={0}' -f ($AJour -join ', '))
    )
    Add-LigneSansBom $env:GITHUB_OUTPUT ($lignes -join "`n")
}

function Test-SourcesNonSuivies {
    $nonSuivis = @(& git -C $root ls-files --others --exclude-standard -- dist/guide)
    if ($LASTEXITCODE -ne 0) { return }
    # Le manifeste lui-meme n'est pas une source : il est ecrit par ce script.
    $nonSuivis = @($nonSuivis | Where-Object { $_ -ne 'dist/guide/pdf-sources.sha256' })
    if ($nonSuivis.Count -gt 0) {
        Write-Warning ("Fichiers non suivis dans dist/guide, absents des empreintes : " +
            ($nonSuivis -join ', ') + " (penser a 'git add')")
    }
}

function Invoke-Check {
    $enregistrees = Read-Manifeste
    $perimes = @()
    $aJour = @()

    foreach ($cible in $cibles) {
        $pdf = Join-Path $root ($cible.Pdf.Replace('/', $sep))
        $empreinte = Get-EmpreinteSources $cible
        $connue = $enregistrees[$cible.Pdf]

        if (-not (Test-Path -LiteralPath $pdf)) {
            $perimes += $cible.Pdf
            Write-Output ('PERIME  {0} : PDF absent de dist/' -f $cible.Pdf)
        }
        elseif (-not $connue) {
            $perimes += $cible.Pdf
            Write-Output ('PERIME  {0} : aucune empreinte enregistree dans dist/guide/pdf-sources.sha256' -f $cible.Pdf)
        }
        elseif ($connue -ne $empreinte) {
            $perimes += $cible.Pdf
            Write-Output ('PERIME  {0} : sources modifiees depuis le dernier build ({1} -> {2})' -f `
                    $cible.Pdf, $connue.Substring(0, 12), $empreinte.Substring(0, 12))
            if ($env:GITHUB_ACTIONS -eq 'true') {
                Write-Output ('::error file={0}::PDF perime : les sources du guide ont change. Regenerer avec tools/guide-pdf.ps1 -Action Build.' -f $cible.Pdf)
            }
        }
        else {
            $aJour += $cible.Pdf
            Write-Output ('OK      {0} ({1})' -f $cible.Pdf, $empreinte.Substring(0, 12))
        }
    }

    if ($perimes.Count -gt 0) {
        Write-Output ('{0} PDF perime(s) sur {1} : {2}' -f $perimes.Count, $cibles.Count, ($perimes -join ', '))
    }
    else {
        Write-Output ('Les {0} PDF livres sont a jour.' -f $cibles.Count)
    }

    Write-Sorties $perimes $aJour
    $etat = @()
    foreach ($pdf in $perimes) { $etat += "**Perime** : $pdf" }
    foreach ($pdf in $aJour) { $etat += "A jour : $pdf" }
    Write-Resume 'Guide PDF - fraicheur' $etat

    if ($perimes.Count -gt 0 -and $ErrorOnStale) { exit 4 }
}

function Invoke-Build {
    Test-SourcesNonSuivies
    $enregistrees = Read-Manifeste
    $empreintes = @{}
    $rapport = @()

    Push-Location $root
    try {
        foreach ($cible in $cibles) {
            $pdf = Join-Path $root ($cible.Pdf.Replace('/', $sep))
            $avant = if (Test-Path -LiteralPath $pdf) { Get-HashFichier $pdf } else { $null }
            # Un echec de generation ne doit pas laisser un ancien PDF en place.
            if (Test-Path -LiteralPath $pdf) { Remove-Item -LiteralPath $pdf -Force }

            Write-Output ('Generation : {0} -> {1}' -f $cible.Html, $cible.Pdf)
            try {
                & $makePdf -Html $cible.Html -Pdf $cible.Pdf
            }
            catch {
                throw ("Echec de la generation de {0} : {1}" -f $cible.Pdf, $_.Exception.Message)
            }

            if (-not (Test-Path -LiteralPath $pdf)) { throw "PDF non produit : $pdf" }
            $octets = [System.IO.File]::ReadAllBytes($pdf)
            if ($octets.Length -lt 10240) { throw "PDF suspicieusement petit ($($octets.Length) octets) : $pdf" }
            if ([System.Text.Encoding]::ASCII.GetString($octets, 0, 5) -ne '%PDF-') { throw "En-tete PDF invalide : $pdf" }

            $apres = Get-HashFichier $pdf
            $etat = if ($avant -and $avant -eq $apres) { 'inchange' } else { 'modifie' }
            Write-Output ('  {0} : {1}, {2:N0} octets' -f $cible.Pdf, $etat, $octets.Length)

            $empreintes[$cible.Pdf] = Get-EmpreinteSources $cible
            $rapport += ('{0} : {1}, {2:N0} octets' -f $cible.Pdf, $etat, $octets.Length)
        }
        $rapport += ('Empreintes ecrites dans dist/guide/pdf-sources.sha256')
        Write-Manifeste $empreintes
    }
    finally {
        Pop-Location
    }

    $perimesRestants = @()
    foreach ($pdf in $empreintes.Keys) {
        if ($enregistrees[$pdf] -and $enregistrees[$pdf] -ne $empreintes[$pdf]) { $perimesRestants += $pdf }
    }
    if ($perimesRestants.Count -gt 0) {
        Write-Output ('PDF regeneres : {0}' -f ($perimesRestants -join ', '))
    }

    Write-Resume 'Guide PDF - regeneration' $rapport
}

function Invoke-Update {
    Test-SourcesNonSuivies
    Write-Manifeste (Get-Empreintes)
    Write-Output 'Empreintes enregistrees (PDF non regeneres : sources declarees couvertes par les PDF livres).'
    Write-Resume 'Guide PDF - empreintes' @(
        'Empreintes enregistrees sans regeneration des PDF.'
        'A utiliser uniquement si les PDF livres ont ete retouches a la main.'
    )
}

switch ($Action) {
    'Check' { Invoke-Check }
    'Build' { Invoke-Build }
    'Update' { Invoke-Update }
}
