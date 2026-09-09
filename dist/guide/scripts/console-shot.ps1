param([string]$CmdFile, [string]$Out, [int]$WaitSec = 10)
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class ConS {
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int n);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr lp);
  public delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  public static IntPtr FindWindowByPid(int pid) {
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
# minimize the GUI app so it does not overlap the console
$gui = Get-Process TaskBackupRestore -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if ($gui) { [ConS]::ShowWindow($gui.MainWindowHandle, 6) | Out-Null }

$w = Start-Process -FilePath "cmd.exe" -ArgumentList @('/k', ('"' + $CmdFile + '"')) -PassThru
$h = [IntPtr]::Zero
for ($i = 0; $i -lt 20; $i++) {
  Start-Sleep -Milliseconds 500
  $p = Get-Process -Id $w.Id -ErrorAction SilentlyContinue
  if (-not $p) { break }
  if ($p.MainWindowHandle -ne 0) { $h = $p.MainWindowHandle; break }
  $h = [ConS]::FindWindowByPid($w.Id)
  if ($h -ne [IntPtr]::Zero) { break }
}
if ($h -eq [IntPtr]::Zero) { Write-Error "console window not found"; Stop-Process -Id $w.Id -Force -ErrorAction SilentlyContinue; exit 1 }
[ConS]::SetWindowPos($h, [IntPtr]::Zero, 150, 60, 1080, 660, 0x0040) | Out-Null
Start-Sleep -Seconds $WaitSec
[ConS]::SetForegroundWindow($h) | Out-Null
Start-Sleep -Milliseconds 600
$r = New-Object ConS+RECT
[ConS]::GetWindowRect($h, [ref]$r) | Out-Null
$wd = $r.R - $r.L; $ht = $r.B - $r.T
$bmp = New-Object System.Drawing.Bitmap($wd, $ht)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($r.L, $r.T, 0, 0, $bmp.Size)
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
Write-Output "saved $Out (${wd}x${ht})"
Stop-Process -Id $w.Id -Force -ErrorAction SilentlyContinue
# restore the GUI window
if ($gui) { [ConS]::ShowWindow($gui.MainWindowHandle, 9) | Out-Null }