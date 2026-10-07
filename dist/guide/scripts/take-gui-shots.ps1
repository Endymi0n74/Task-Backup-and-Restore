# Prise de vue des captures d'interface du guide, par pilotage du DOM
# WebView2 via le port de debug Chrome DevTools (CDP).
#
# Pourquoi : les anciennes captures passaient par des clics simules
# (click.ps1) puis une capture d'ecran — coordonnees, DPI et focus pouvaient
# deriver. Ici l'etat de l'interface est pose en JS (clics/fill reels sur le
# DOM), la capture est le rendu exact du WebView2, et la sequence est
# entierement rejouable.
#
# Lancement : l'application doit avoir ete compilee avec l'UI courante
# (voir AGENTS.md). Le port de debug est ouvert par la variable
# d'environnement WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS.
#
# Usage :
#   powershell -NoProfile -ExecutionPolicy Bypass -File take-gui-shots.ps1
param(
  [string]$App = "D:\Codex\tsbak-gui\dist\TaskBackupRestore.exe",
  [string]$OutDir = "D:\Codex\tsbak-gui\dist\guide\shots",
  [string]$DemoDir = "D:\Codex\tsbak-gui\dist\guide\demo\export-demo",
  [string]$DemoPwDir = "D:\Codex\tsbak-gui\dist\guide\demo\demo-password",
  [int]$Port = 9333,
  [int]$WinW = 1140,
  [int]$WinH = 800
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class GuiShot {
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int cx, int cy, uint f);
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
}
"@

# ---------------------------------------------------------------------------
# 1. Lancement de l'application avec le port de debug
# ---------------------------------------------------------------------------
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$Port"
$p = Start-Process -FilePath $App -PassThru
Write-Host "application lancee (pid $($p.Id))"

$target = $null
for ($i = 0; $i -lt 40; $i++) {
  Start-Sleep -Milliseconds 500
  try {
    $targets = Invoke-RestMethod "http://127.0.0.1:$Port/json/list" -TimeoutSec 2
    $target = $targets | Where-Object { $_.type -eq "page" } | Select-Object -First 1
    if ($target) { break }
  } catch {}
}
if (-not $target) {
  Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
  throw "cible WebView2 (port $Port) introuvable"
}
Write-Host "page cible : $($target.title)"

# Fenetre dimensionnee de facon deterministic (meme taille a chaque capture).
if ($p.MainWindowHandle -ne 0) {
  [GuiShot]::SetWindowPos($p.MainWindowHandle, [IntPtr]::Zero, 60, 40, $WinW, $WinH, 0x0014) | Out-Null
}
Start-Sleep -Milliseconds 800

# ---------------------------------------------------------------------------
# 2. Client WebSocket CDP minimal
# ---------------------------------------------------------------------------
$wsUrl = $target.webSocketDebuggerUrl
$ws = New-Object System.Net.WebSockets.ClientWebSocket
$ct = [System.Threading.CancellationToken]::None
$ws.ConnectAsync([Uri]$wsUrl, $ct).Wait()
Write-Host "connecte : $wsUrl"

$script:cdpId = 0

function Invoke-Cdp {
  param([string]$Method, [hashtable]$Params = @{})
  $script:cdpId++
  $myId = $script:cdpId
  $payload = @{ id = $myId; method = $Method }
  if ($Params.Count -gt 0) { $payload.params = $Params }
  $json = $payload | ConvertTo-Json -Depth 8 -Compress
  $bytes = [System.Text.Encoding]::UTF8.GetBytes($json)
  $seg = New-Object System.ArraySegment[byte] -ArgumentList @(, $bytes)
  $ws.SendAsync($seg, [System.Net.WebSockets.WebSocketMessageType]::Text, $true, $ct).Wait()

  # Lecture jusqu'a la reponse portant notre id (les evenements recus en
  # chemin — console, reseau — sont ignores). Les gros messages (capture
  # d'ecran) arrivent fractionnes : on reassemble jusqu'a EndOfMessage.
  $buffer = New-Object byte[] (1024 * 1024)
  $sb = New-Object System.Text.StringBuilder
  for (;;) {
    $null = $sb.Clear()
    for (;;) {
      $seg = New-Object System.ArraySegment[byte] -ArgumentList @(, $buffer)
      $res = $ws.ReceiveAsync($seg, $ct)
      $res.Wait()
      $null = $sb.Append([System.Text.Encoding]::UTF8.GetString($buffer, 0, $res.Result.Count))
      if ($res.Result.EndOfMessage) { break }
    }
    $msg = $sb.ToString() | ConvertFrom-Json
    if ($msg.id -eq $myId) {
      if ($msg.error) { throw "CDP $Method : $($msg.error.message)" }
      return $msg.result
    }
  }
}

function Invoke-CdpEval {
  param([string]$Expr)
  $r = Invoke-Cdp "Runtime.evaluate" @{
    expression          = $Expr
    awaitPromise        = $true
    returnByValue       = $true
    userGesture         = $true
  }
  if ($r.exceptionDetails) {
    throw "JS : $($r.exceptionDetails.exception.description)"
  }
  return $r.result.value
}

function Save-CdpPng {
  param([string]$Out)
  $r = Invoke-Cdp "Page.captureScreenshot" @{ format = "png" }
  [System.IO.File]::WriteAllBytes($Out, [Convert]::FromBase64String($r.data))
  $img = [System.Drawing.Image]::FromFile($Out)
  Write-Host ("capture {0} ({1}x{2})" -f (Split-Path $Out -Leaf), $img.Width, $img.Height)
  $img.Dispose()
}

# Helpers JS poses une seule fois : clics, remontee d'evenements, attente.
$jsHelpers = @'
window.__g = {
  wait: (ms) => new Promise(r => setTimeout(r, ms)),
  until: async (fn, timeout) => {
    const t0 = Date.now();
    const limit = timeout || 20000;
    while (Date.now() - t0 < limit) { if (fn()) return true; await new Promise(r => setTimeout(r, 150)); }
    return false;
  },
  click: (id) => document.getElementById(id).click(),
  tab: (name) => document.querySelector('.tab[data-tab="' + name + '"]').click(),
  set: (id, v) => {
    const e = document.getElementById(id);
    e.value = v;
    e.dispatchEvent(new Event("input", { bubbles: true }));
    e.dispatchEvent(new Event("change", { bubbles: true }));
  },
  check: (id, on) => {
    const e = document.getElementById(id);
    e.checked = on;
    e.dispatchEvent(new Event("change", { bubbles: true }));
  },
  text: (id) => (document.getElementById(id) || {}).textContent || ""
};
'@
Invoke-CdpEval $jsHelpers | Out-Null

# ---------------------------------------------------------------------------
# 3. Sequence de captures
# ---------------------------------------------------------------------------
function Need([bool]$Ok, [string]$What) {
  if (-not $Ok) {
    Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
    throw "echec : $What"
  }
}

# --- 02 : onglet Import, etat initial (aucune source choisie) --------------
Invoke-CdpEval 'window.__g.tab("import")' | Out-Null
Invoke-CdpEval 'window.__g.wait(300)' | Out-Null
Save-CdpPng (Join-Path $OutDir "02-import.png")

# --- 01 : onglet Taches & Export, taches chargees, options d'archive ------
Invoke-CdpEval 'window.__g.tab("tasks"); window.__g.click("btn-load-tasks")' | Out-Null
$ok = Invoke-CdpEval 'window.__g.until(() => window.__g.text("task-count").indexOf("affich") >= 0)'
Need $ok "chargement des taches"
Invoke-CdpEval 'window.__g.check("zip-enabled", true)' | Out-Null
Invoke-CdpEval 'window.__g.wait(400)' | Out-Null
$counts = Invoke-CdpEval 'window.__g.text("task-count") + " ||| " + window.__g.text("statusbar")'
Write-Host "etat taches : $counts"
Save-CdpPng (Join-Path $OutDir "01-export.png")

# --- 06 : onglet Import, archive validee + plan (archive simple) ----------
function Get-PlanShot([string]$Demo, [string]$Out) {
  Invoke-CdpEval 'window.__g.tab("import")' | Out-Null
  Invoke-CdpEval "window.__g.set('import-dir', $($Demo | ConvertTo-Json -Compress))" | Out-Null
  Invoke-CdpEval 'document.getElementById("manifest-summary").classList.add("hidden"); window.__g.click("btn-verify")' | Out-Null
  $ok = Invoke-CdpEval 'window.__g.until(() => !document.getElementById("manifest-summary").classList.contains("hidden"))'
  Need $ok "verification de l'archive $Demo"
  $summary = Invoke-CdpEval 'window.__g.text("manifest-summary")'
  Write-Host "verification : $summary"
  Invoke-CdpEval 'window.__g.click("btn-plan")' | Out-Null
  $ok = Invoke-CdpEval 'window.__g.until(() => !document.getElementById("plan-card").classList.contains("hidden") && document.querySelectorAll("#plan-table tbody tr").length > 0)'
  Need $ok "construction du plan $Demo"
  $planInfo = Invoke-CdpEval 'window.__g.text("plan-count") + " ||| " + Array.from(document.querySelectorAll("#plan-table tbody .badge")).map(b => b.textContent).join(", ")'
  Write-Host "plan : $planInfo"
  Invoke-CdpEval 'window.__g.wait(300)' | Out-Null
  Save-CdpPng $Out
}

Get-PlanShot $DemoDir (Join-Path $OutDir "06-import-plan.png")
Get-PlanShot $DemoPwDir (Join-Path $OutDir "10-import-password.png")

# --- 03 : onglet Logs (journal du jour, operations precedentes) -----------
Invoke-CdpEval 'window.__g.tab("logs")' | Out-Null
$ok = Invoke-CdpEval 'window.__g.until(() => document.querySelectorAll("#log-view .log-line-info, #log-view .log-line-warn, #log-view .log-line-error").length > 3, 6000)'
Need $ok "remplissage du journal"
Invoke-CdpEval 'window.__g.wait(1200)' | Out-Null
Save-CdpPng (Join-Path $OutDir "03-logs.png")

# ---------------------------------------------------------------------------
# 4. Fermeture
# ---------------------------------------------------------------------------
if ($p.MainWindowHandle -ne 0) {
  [GuiShot]::PostMessage($p.MainWindowHandle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
}
if (-not $p.WaitForExit(8000)) {
  Write-Host "WARN : fermeture propre incomplite, arret force"
  Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
}
$ws.Dispose()
Write-Host "ALL DONE"
