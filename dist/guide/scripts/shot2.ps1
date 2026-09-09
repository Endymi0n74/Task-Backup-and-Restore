param([string]$Out)
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public class WinP {
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hwnd, IntPtr hdc, uint flags);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern int GetClassName(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr parent, EnumProc cb, IntPtr lp);
  public delegate bool EnumProc(IntPtr hWnd, IntPtr lParam);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  public struct RECT { public int L, T, R, B; }
}
"@
$p = Get-Process TaskBackupRestore -ErrorAction Stop | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
$h = $p.MainWindowHandle

$list = New-Object System.Collections.ArrayList
$cb = [WinP+EnumProc]{ param($c, $lp)
  $sb = New-Object System.Text.StringBuilder 256
  [WinP]::GetClassName($c, $sb, 256) | Out-Null
  $r = New-Object WinP+RECT
  [WinP]::GetWindowRect($c, [ref]$r) | Out-Null
  $null = $list.Add(@{ h = $c; cls = $sb.ToString(); w = $r.R - $r.L; hgt = $r.B - $r.T; vis = [WinP]::IsWindowVisible($c) })
  return $true
}
[WinP]::EnumChildWindows($h, $cb, [IntPtr]::Zero) | Out-Null
foreach ($e in $list) { Write-Output ("child 0x{0:X} class='{1}' {2}x{3} visible={4}" -f $e.h, $e.cls, $e.w, $e.hgt, $e.vis) }

$cand = $list | Where-Object { $_.vis -and ($_.cls -like "Chrome_WidgetWin*" -or $_.cls -like "*WebView*") -and $_.w -gt 500 } | Sort-Object { $_.w * $_.hgt } -Descending | Select-Object -First 1
if (-not $cand) { $cand = $list | Where-Object { $_.vis -and $_.w -gt 500 } | Sort-Object { $_.w * $_.hgt } -Descending | Select-Object -First 1 }
if (-not $cand) { Write-Error "no suitable child window"; exit 1 }
Write-Output "capturing 0x{0:X} '{1}' {2}x{3}" -f $cand.h, $cand.cls, $cand.w, $cand.hgt

$bmp = New-Object System.Drawing.Bitmap($cand.w, $cand.hgt)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$hdc = $g.GetHdc()
$ok = [WinP]::PrintWindow($cand.h, $hdc, 0x2)
$g.ReleaseHdc($hdc)
$g.Dispose()
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Output "saved $Out (${$cand.w}x$($cand.hgt), printwindow=$ok)"