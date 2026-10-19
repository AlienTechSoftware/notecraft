<#
Takes a picture of one window (not the desktop) with the Windows PrintWindow call, so other
windows on top of it do not appear. Windows only.

  scripts\capture-window.ps1 -Process notecraft -Out shot.png

The window must be open and not minimized. A window drawn with some graphics drivers can come
out black; the headless screenshot tests (see docs/testing.md) do not have that limit.
#>
param(
    [Parameter(Mandatory = $true)][string]$Process,
    [Parameter(Mandatory = $true)][string]$Out
)

Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @"
using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.Runtime.InteropServices;
public class WinShot {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  public static string Capture(IntPtr h, string path) {
    RECT r; if (!GetWindowRect(h, out r)) return "no rect";
    int w = r.R - r.L, hgt = r.B - r.T; if (w <= 0 || hgt <= 0) return "empty window " + w + "x" + hgt;
    using (var bmp = new Bitmap(w, hgt)) {
      using (var g = Graphics.FromImage(bmp)) { IntPtr dc = g.GetHdc(); bool ok = PrintWindow(h, dc, 2); g.ReleaseHdc(dc); if (!ok) return "PrintWindow failed"; }
      bmp.Save(path, ImageFormat.Png); return "ok " + w + "x" + hgt;
    }
  }
}
"@

$p = Get-Process $Process -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $p) { Write-Error "No window found for process '$Process'."; exit 1 }
[WinShot]::Capture($p.MainWindowHandle, (Resolve-Path -LiteralPath (Split-Path -Parent (Join-Path (Get-Location) $Out))).Path + "\" + (Split-Path -Leaf $Out))
