param([string]$CmdFile, [string]$Out, [string]$Answers = "", [int]$WaitSec = 8)
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class AsShot {
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr lp);
  public delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  public static IntPtr FindByPid(int pid) {
    IntPtr found = IntPtr.Zero;
    EnumWindows(delegate(IntPtr h, IntPtr lp) {
      uint p2; GetWindowThreadProcessId(h, out p2);
      if (p2 == (uint)pid && IsWindowVisible(h)) { found = h; return false; }
      return true;
    }, IntPtr.Zero);
    return found;
  }
  public struct RECT { public int L, T, R, B; }
}
"@
$AnswerList = @($Answers -split '\|', -1)
$w = Start-Process -FilePath "cmd.exe" -ArgumentList @('/k', ('"' + $CmdFile + '"')) -PassThru
$h = [IntPtr]::Zero
for ($i = 0; $i -lt 20; $i++) {
  Start-Sleep -Milliseconds 500
  $p = Get-Process -Id $w.Id -ErrorAction SilentlyContinue
  if (-not $p) { break }
  if ($p.MainWindowHandle -ne 0) { $h = $p.MainWindowHandle; break }
  $h = [AsShot]::FindByPid($w.Id)
  if ($h -ne [IntPtr]::Zero) { break }
}
if ($h -eq [IntPtr]::Zero) { Write-Error "console window not found"; Stop-Process -Id $w.Id -Force -ErrorAction SilentlyContinue; exit 1 }
[AsShot]::SetWindowPos($h, [IntPtr]::Zero, 150, 60, 1080, 660, 0x0040) | Out-Null
Start-Sleep -Milliseconds 1800

# Bring the console to the foreground (verified), with AppActivate fallback
$wshell = New-Object -ComObject WScript.Shell
function Bring-Front([IntPtr]$hwnd) {
  for ($i = 0; $i -lt 6; $i++) {
    [AsShot]::SetForegroundWindow($hwnd) | Out-Null
    Start-Sleep -Milliseconds 250
    if ([AsShot]::GetForegroundWindow() -eq $hwnd) { return $true }
    try { $wshell.AppActivate($hwnd.ToInt32()) | Out-Null } catch {}
    Start-Sleep -Milliseconds 250
    if ([AsShot]::GetForegroundWindow() -eq $hwnd) { return $true }
  }
  return $false
}
$null = Bring-Front $h
Start-Sleep -Milliseconds 500

foreach ($a in $AnswerList) {
  if ($a -ne "") {
    Set-Clipboard -Value $a
    Start-Sleep -Milliseconds 250
    $wshell.SendKeys("^v")          # Ctrl+V : preserve les antislashs
    Start-Sleep -Milliseconds 400
  }
  $wshell.SendKeys("~")
  Start-Sleep -Milliseconds 800
}
Start-Sleep -Seconds $WaitSec
$null = Bring-Front $h
Start-Sleep -Milliseconds 500
$r = New-Object AsShot+RECT
[AsShot]::GetWindowRect($h, [ref]$r) | Out-Null
$wd = $r.R - $r.L; $ht = $r.B - $r.T
$bmp = New-Object System.Drawing.Bitmap($wd, $ht)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($r.L, $r.T, 0, 0, $bmp.Size)
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
Write-Output "saved $Out (${wd}x${ht})"
Stop-Process -Id $w.Id -Force -ErrorAction SilentlyContinue