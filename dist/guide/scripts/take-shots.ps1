param(
  [string]$OutDir = "D:\Codex\tsbak-gui\dist\guide\shots",
  [string]$WorkDir = "D:\Codex\tsbak-gui\dist"
)
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Take {
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr lp);
  public delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  public struct RECT { public int L, T, R, B; }
  public static IntPtr FindByPid(int pid) {
    IntPtr found = IntPtr.Zero;
    EnumWindows(delegate(IntPtr h, IntPtr lp) {
      uint p2; GetWindowThreadProcessId(h, out p2);
      if (p2 == (uint)pid && IsWindowVisible(h)) { found = h; return false; }
      return true;
    }, IntPtr.Zero);
    return found;
  }
}
"@
$wshell = New-Object -ComObject WScript.Shell

function Bring-Front([IntPtr]$hwnd) {
  for ($i = 0; $i -lt 8; $i++) {
    [Take]::SetForegroundWindow($hwnd) | Out-Null
    Start-Sleep -Milliseconds 250
    if ([Take]::GetForegroundWindow() -eq $hwnd) { return $true }
    try { $wshell.AppActivate($hwnd.ToInt32()) | Out-Null } catch {}
    Start-Sleep -Milliseconds 250
    if ([Take]::GetForegroundWindow() -eq $hwnd) { return $true }
  }
  return $false
}

function Start-Console([string]$Cmd) {
  $w = Start-Process -FilePath "cmd.exe" -ArgumentList @('/k', $Cmd) -WorkingDirectory $WorkDir -PassThru
  $h = [IntPtr]::Zero
  for ($i = 0; $i -lt 20; $i++) {
    Start-Sleep -Milliseconds 500
    $p = Get-Process -Id $w.Id -ErrorAction SilentlyContinue
    if (-not $p) { break }
    if ($p.MainWindowHandle -ne 0) { $h = $p.MainWindowHandle; break }
    $h = [Take]::FindByPid($w.Id)
    if ($h -ne [IntPtr]::Zero) { break }
  }
  if ($h -eq [IntPtr]::Zero) { throw "console window not found for $Cmd" }
  Start-Sleep -Milliseconds 1500   # let conhost finish laying out before positioning
  [Take]::SetWindowPos($h, [IntPtr]::Zero, 150, 60, 1080, 660, 0x0040) | Out-Null
  Start-Sleep -Milliseconds 1200
  return @{ Proc = $w; Hwnd = $h }
}

function Save-Shot([IntPtr]$h, [string]$Out) {
  $null = Bring-Front $h
  Start-Sleep -Milliseconds 400
  $r = New-Object Take+RECT
  [Take]::GetWindowRect($h, [ref]$r) | Out-Null
  $wd = $r.R - $r.L; $ht = $r.B - $r.T
  $bmp = New-Object System.Drawing.Bitmap($wd, $ht)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($r.L, $r.T, 0, 0, $bmp.Size)
  $bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
  Write-Output ("saved {0} ({1}x{2} at {3},{4})" -f $Out, $wd, $ht, $r.L, $r.T)
}

function Type-Answer([IntPtr]$h, [string]$Text) {
  $null = Bring-Front $h
  if ($Text -ne "") {
    $wshell.SendKeys($Text)
    Start-Sleep -Milliseconds 350
  }
  $wshell.SendKeys("~")
  Start-Sleep -Milliseconds 700
}

# ============ 1. validate.cmd ============
Write-Output "=== validate.cmd ==="
$c = Start-Console ('"' + (Join-Path $WorkDir 'validate.cmd') + '"')
Start-Sleep -Milliseconds 800
Type-Answer $c.Hwnd "D:\Codex\tsbak-gui\dist\guide\demo\export-demo"
Start-Sleep -Seconds 4
Save-Shot $c.Hwnd (Join-Path $OutDir "07-cmd-validate.png")
Stop-Process -Id $c.Proc.Id -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 800

# ============ 2. export.cmd ============
Write-Output "=== export.cmd ==="
$dest = "D:\Codex\tmp\guide-export-" + (Get-Date -Format "yyyy-MM-dd")
if (Test-Path $dest) { Remove-Item $dest -Recurse -Force }
$c = Start-Console ('"' + (Join-Path $WorkDir 'export.cmd') + '"')
Start-Sleep -Milliseconds 800
Type-Answer $c.Hwnd $dest
# wait for "Fin de l'export" marker in the newest log (up to 180 s)
$done = $false
for ($i = 0; $i -lt 180; $i++) {
  Start-Sleep -Seconds 1
  $log = Get-ChildItem (Join-Path $WorkDir "logs\export-*.log") -ErrorAction SilentlyContinue | Sort-Object LastWriteTime -Descending | Select-Object -First 1
  if ($log) {
    $tail = Get-Content $log.FullName -Tail 1 -ErrorAction SilentlyContinue
    if ($tail -match "Fin de l'export") { $done = $true; break }
  }
}
Write-Output ("export finished marker: {0} after {1}s" -f $done, $i)
Start-Sleep -Seconds 3   # validation + "OK. Archive pret a etre copiee"
Save-Shot $c.Hwnd (Join-Path $OutDir "08-cmd-export.png")
Stop-Process -Id $c.Proc.Id -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 800

# ============ 3. import.cmd (dry-run) ============
Write-Output "=== import.cmd ==="
$c = Start-Console ('"' + (Join-Path $WorkDir 'import.cmd') + '"')
Start-Sleep -Milliseconds 800
Type-Answer $c.Hwnd "D:\Codex\tsbak-gui\dist\guide\demo\export-demo"   # dossier d'export
Type-Answer $c.Hwnd "\Restauration-guide"                              # dossier cible
Type-Answer $c.Hwnd ""                                                 # pas de fichier de mots de passe
Type-Answer $c.Hwnd ""                                                 # simulation O (defaut)
Start-Sleep -Seconds 6
Save-Shot $c.Hwnd (Join-Path $OutDir "09-cmd-import.png")
Stop-Process -Id $c.Proc.Id -Force -ErrorAction SilentlyContinue
Write-Output "ALL DONE"
