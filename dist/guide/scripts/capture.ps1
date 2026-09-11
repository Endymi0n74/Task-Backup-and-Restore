# Capture d'une console Windows "classique" (conhost) avec recadrage EXACT.
#
# Pourquoi conhost.exe explicitement : depuis Windows 11, le terminal par defaut
# est Windows Terminal (classe CASCADIA_HOSTING_WINDOW_CLASS) ; lancer cmd.exe
# ouvre alors un onglet WT, et non la fenetre cmd classique utilisee dans le
# guide. "conhost.exe cmd.exe /k ..." force la fenetre ConsoleWindowClass,
# dont GetWindowRect/GetClientRect renvoient des coordonnees fiables : le
# recadrage ne derive donc plus (c'etait la cause du contenu decale et du
# navigateur visible sur les anciennes captures).
#
# Usage :
#   powershell -NoProfile -ExecutionPolicy Bypass -File capture.ps1 `
#       -Inner '"D:\...\validate.cmd"' -Out "shots\07-cmd-validate.png" `
#       -Answers "D:\...\export-demo" -WaitSec 6
param(
  [Parameter(Mandatory = $true)][string]$Inner,
  [Parameter(Mandatory = $true)][string]$Out,
  [string]$Title = "C:\Windows\System32\cmd.exe",
  [string]$Answers = "",
  [int]$WaitSec = 8,
  [int]$Cols = 132,
  [int]$Lines = 32,
  [int]$X = 60,
  [int]$Y = 120,
  [string]$WorkDir = ""
)

$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public class Cap {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr lp);
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool SetWindowText(IntPtr h, string s);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern bool PostMessage(IntPtr h, uint msg, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int cx, int cy, uint f);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RC r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  public struct RC { public int L, T, R, B; }
  public static List<IntPtr> Consoles() {
    var list = new List<IntPtr>();
    EnumWindows(delegate(IntPtr h, IntPtr l) {
      var sb = new StringBuilder(256); GetClassName(h, sb, 256);
      if (sb.ToString() == "ConsoleWindowClass" && IsWindowVisible(h)) list.Add(h);
      return true;
    }, IntPtr.Zero);
    return list;
  }
}
"@
[Cap]::SetProcessDPIAware() | Out-Null

$wshell = New-Object -ComObject WScript.Shell
function Bring-Front([IntPtr]$hwnd) {
  for ($i = 0; $i -lt 10; $i++) {
    [Cap]::SetForegroundWindow($hwnd) | Out-Null
    Start-Sleep -Milliseconds 250
    if ([Cap]::GetForegroundWindow() -eq $hwnd) { return $true }
    try { $wshell.AppActivate($hwnd.ToInt32()) | Out-Null } catch {}
    Start-Sleep -Milliseconds 250
    if ([Cap]::GetForegroundWindow() -eq $hwnd) { return $true }
  }
  return $false
}

$tag = "TSBAK-CAP-" + $PID + "-" + (Get-Random)
$full = ("title {0} & mode con: cols={1} lines={2} & " -f $tag, $Cols, $Lines) + $Inner
$spArgs = @{ FilePath = "conhost.exe"; ArgumentList = @("cmd.exe", "/k", $full); PassThru = $true }
if ($WorkDir -ne "") { $spArgs.WorkingDirectory = $WorkDir }
$proc = Start-Process @spArgs -WindowStyle Normal

# Identification de NOTRE fenetre par un titre unique : la fenetre de console
# appartient a un processus conhost qui n'est pas forcement celui lance, et sur
# un poste il peut y avoir d'autres consoles. Le titre pose par 'title' est donc
# le seul critere fiable.
$h = [IntPtr]::Zero
for ($i = 0; $i -lt 60; $i++) {
  Start-Sleep -Milliseconds 300
  foreach ($w in [Cap]::Consoles()) {
    $sb = New-Object System.Text.StringBuilder 512
    [Cap]::GetWindowText($w, $sb, 512) | Out-Null
    if ($sb.ToString() -like ($tag + '*')) { $h = $w; break }
  }
  if ($h -ne [IntPtr]::Zero) { break }
}
if ($h -eq [IntPtr]::Zero) { throw "fenetre console (titre '$tag') introuvable" }
Write-Output ("console trouvee apres " + ($i * 300) + " ms (hwnd=$h)")

Start-Sleep -Milliseconds 900
# SWP_NOSIZE(1) | SWP_NOZORDER(4) | SWP_SHOWWINDOW(0x40) : on ne fait que
# DEPLACER la fenetre : sa taille reste celle fixee par "mode con" (aucun
# retour a la ligne parasite, texte entierement visible).
[Cap]::SetWindowPos($h, [IntPtr]::Zero, $X, $Y, 0, 0, 0x0045) | Out-Null
Start-Sleep -Milliseconds 1400

# Saisie des reponses par messages WM_CHAR : aucune dependance au focus clavier
# (SendKeys pouvait partir dans la mauvaise fenetre), et les antislashs sont
# transmis tels quels.
function Send-Line([IntPtr]$h, [string]$text) {
  foreach ($ch in $text.ToCharArray()) {
    [Cap]::PostMessage($h, 0x0102, [IntPtr][int]$ch, [IntPtr]::Zero) | Out-Null
    Start-Sleep -Milliseconds 12
  }
  [Cap]::PostMessage($h, 0x0102, [IntPtr]13, [IntPtr]::Zero) | Out-Null
  Start-Sleep -Milliseconds 60
}

if ($Answers -ne "") {
  $null = Bring-Front $h
  Start-Sleep -Milliseconds 300
  foreach ($a in @($Answers -split '\|', -1)) {
    if ($a -ne "") { Send-Line $h $a } else { Send-Line $h "" }
    Start-Sleep -Milliseconds 700
  }
}

Start-Sleep -Seconds $WaitSec
$null = Bring-Front $h
Start-Sleep -Milliseconds 600
# Le titre est repose juste avant la capture : conhost reecrit le texte de la
# fenetre a chaque sortie console, un SetWindowText plus tot serait ecrase.
[Cap]::SetWindowText($h, $Title) | Out-Null
Start-Sleep -Milliseconds 400
$sbTitle = New-Object System.Text.StringBuilder 512
[Cap]::GetWindowText($h, $sbTitle, 512) | Out-Null
Write-Output ("titre fenetre : '" + $sbTitle.ToString() + "'")

$r = New-Object Cap+RC
if (-not [Cap]::GetWindowRect($h, [ref]$r)) { throw "GetWindowRect a echoue" }
$wd = $r.R - $r.L
$ht = $r.B - $r.T
if ($wd -lt 200 -or $ht -lt 150) { throw ("rect suspect : {0}x{1}" -f $wd, $ht) }
$bmp = New-Object System.Drawing.Bitmap($wd, $ht)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($r.L, $r.T, 0, 0, $bmp.Size)
$outPath = if ([System.IO.Path]::IsPathRooted($Out)) { $Out } else { Join-Path (Get-Location).Path $Out }
$bmp.Save($outPath, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
Write-Output ("capture {0} ({1}x{2} @ {3},{4})" -f $outPath, $wd, $ht, $r.L, $r.T)

$ownerPid = 0
[Cap]::GetWindowThreadProcessId($h, [ref]$ownerPid) | Out-Null
Stop-Process -Id $ownerPid -Force -ErrorAction SilentlyContinue
Stop-Process -Id $proc.Id -Force -ErrorAction SilentlyContinue
