param([string]$Out)
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class WinS {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT r);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int n);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hWnd);
  public struct RECT { public int L, T, R, B; }
}
"@
$p = Get-Process TaskBackupRestore -ErrorAction Stop | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $p) { Write-Error "application window not found"; exit 1 }
$h = $p.MainWindowHandle
[WinS]::ShowWindow($h, 9) | Out-Null
[WinS]::SetForegroundWindow($h) | Out-Null
Start-Sleep -Milliseconds 700
$r = New-Object WinS+RECT
[WinS]::GetWindowRect($h, [ref]$r) | Out-Null
$w = $r.R - $r.L; $ht = $r.B - $r.T
$dpi = [WinS]::GetDpiForWindow($h)
$bmp = New-Object System.Drawing.Bitmap($w, $ht)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($r.L, $r.T, 0, 0, $bmp.Size)
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
Set-Content -Path ($Out + ".txt") -Value "rect=$($r.L),$($r.T),$($r.R),$($r.B) size=${w}x${ht} dpi=$dpi"
Write-Output "saved $Out (${w}x${ht}, dpi $dpi)"