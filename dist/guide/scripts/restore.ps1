Add-Type @"
using System;
using System.Runtime.InteropServices;
public class WinR {
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int n);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  public struct RECT { public int L, T, R, B; }
}
"@
$p = Get-Process TaskBackupRestore -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $p) { Write-Error "no application window"; exit 1 }
$h = $p.MainWindowHandle
[WinR]::ShowWindow($h, 9) | Out-Null   # SW_RESTORE
[WinR]::ShowWindow($h, 5) | Out-Null   # SW_SHOW
[WinR]::SetForegroundWindow($h) | Out-Null
Start-Sleep -Milliseconds 800
$r = New-Object WinR+RECT
[WinR]::GetWindowRect($h, [ref]$r) | Out-Null
Write-Output "rect $($r.L),$($r.T) -> $($r.R),$($r.B) size $($r.R-$r.L)x$($r.B-$r.T)"