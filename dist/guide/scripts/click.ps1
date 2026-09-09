param([int]$X, [int]$Y)
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class WinC {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, UIntPtr e);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hWnd, out RECT r);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hWnd, ref POINT p);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int n);
  public struct RECT { public int L, T, R, B; }
  public struct POINT { public int X, Y; }
}
"@
$p = Get-Process TaskBackupRestore -ErrorAction Stop | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $p) { Write-Error "application window not found"; exit 1 }
$h = $p.MainWindowHandle
[WinC]::ShowWindow($h, 9) | Out-Null
[WinC]::SetForegroundWindow($h) | Out-Null
Start-Sleep -Milliseconds 400
$r = New-Object WinC+RECT
[WinC]::GetClientRect($h, [ref]$r) | Out-Null
$pt = New-Object WinC+POINT
[WinC]::ClientToScreen($h, [ref]$pt) | Out-Null
$dpi = [WinC]::GetDpiForWindow($h)
$scale = $dpi / 96.0
$px = [int]($pt.X + $X * $scale)
$py = [int]($pt.Y + $Y * $scale)
[WinC]::SetCursorPos($px, $py) | Out-Null
Start-Sleep -Milliseconds 150
[WinC]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
[WinC]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
Write-Output "clicked physical $px,$py (logical $X,$Y dpi $dpi)"