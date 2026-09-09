Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public class WinT {
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowPlacement(IntPtr h, ref WINDOWPLACEMENT wp);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
  public struct POINT { public int X, Y; }
  public struct RECT { public int L, T, R, B; }
  [StructLayout(LayoutKind.Sequential)]
  public struct WINDOWPLACEMENT {
    public int length; public int flags; public int showCmd;
    public POINT ptMinPosition; public POINT ptMaxPosition; public RECT rcNormalPosition;
  }
}
"@
$p = Get-Process TaskBackupRestore -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $p) { Write-Error "none"; exit 1 }
$h = $p.MainWindowHandle
$sb = New-Object System.Text.StringBuilder 256
[WinT]::GetWindowText($h, $sb, 256) | Out-Null
$r = New-Object WinT+RECT
[WinT]::GetWindowRect($h, [ref]$r) | Out-Null
$wp = New-Object WinT+WINDOWPLACEMENT
$wp.length = [Runtime.InteropServices.Marshal]::SizeOf($wp)
[WinT]::GetWindowPlacement($h, [ref]$wp) | Out-Null
Write-Output ("hwnd=0x{0:X} visible={1} iconic={2} showCmd={3} rect={4},{5}-{6},{7} title='{8}'" -f $h, [WinT]::IsWindowVisible($h), [WinT]::IsIconic($h), $wp.showCmd, $r.L, $r.T, $r.R, $r.B, $sb.ToString())