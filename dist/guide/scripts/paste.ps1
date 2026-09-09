param([string]$Text)
Add-Type -AssemblyName System.Windows.Forms
[System.Windows.Forms.Clipboard]::SetText($Text)
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class WinK {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr hWnd, out RECT r);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr hWnd, ref POINT p);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, uint d, UIntPtr e);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int n);
  public struct RECT { public int L, T, R, B; }
  public struct POINT { public int X, Y; }
}
"@
$p = Get-Process TaskBackupRestore -ErrorAction Stop | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
$h = $p.MainWindowHandle
[WinK]::ShowWindow($h, 9) | Out-Null
[WinK]::SetForegroundWindow($h) | Out-Null
Start-Sleep -Milliseconds 400
$r = New-Object WinK+RECT
[WinK]::GetClientRect($h, [ref]$r) | Out-Null
$pt = New-Object WinK+POINT
[WinK]::ClientToScreen($h, [ref]$pt) | Out-Null
$dpi = [WinK]::GetDpiForWindow($h)
$scale = $dpi / 96.0
$px = [int]($pt.X + 433 * $scale)
$py = [int]($pt.Y + 220 * $scale)
[WinK]::SetCursorPos($px, $py) | Out-Null
Start-Sleep -Milliseconds 150
[WinK]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
[WinK]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
Start-Sleep -Milliseconds 300
# Ctrl+A puis Ctrl+V
[WinK]::keybd_event(0x11, 0, 0, [UIntPtr]::Zero)
[WinK]::keybd_event(0x41, 0, 0, [UIntPtr]::Zero)
[WinK]::keybd_event(0x41, 0, 0x2, [UIntPtr]::Zero)
[WinK]::keybd_event(0x11, 0, 0x2, [UIntPtr]::Zero)
Start-Sleep -Milliseconds 200
[WinK]::keybd_event(0x11, 0, 0, [UIntPtr]::Zero)
[WinK]::keybd_event(0x56, 0, 0, [UIntPtr]::Zero)
[WinK]::keybd_event(0x56, 0, 0x2, [UIntPtr]::Zero)
[WinK]::keybd_event(0x11, 0, 0x2, [UIntPtr]::Zero)
Start-Sleep -Milliseconds 400
Write-Output "pasted into import-dir"