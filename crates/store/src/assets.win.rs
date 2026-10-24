//! Windows Shell icon extraction for attachment sidecars.
//!
//! `SHGetFileInfoW` supplies the icon registered with the Windows Shell for the
//! attachment's file type. A hidden PowerShell process uses WPF's PNG encoder
//! because the installed PowerShell host does not provide `System.Drawing`.

use std::os::windows::process::CommandExt;
use std::path::Path;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Saves the Windows Shell icon associated with `source` as a PNG at `target`.
pub(super) fn shell_icon(source: &Path, target: &Path) -> Result<(), String> {
    let source_text = source.to_string_lossy();
    let target_text = target.to_string_lossy();
    let source_text = source_text.strip_prefix(r"\\?\").unwrap_or(&source_text);
    let target_text = target_text.strip_prefix(r"\\?\").unwrap_or(&target_text);
    let output = std::process::Command::new("powershell.exe")
        .env("NC_ICON_SOURCE", source_text)
        .env("NC_ICON_TARGET", target_text)
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-NonInteractive", "-STA", "-ExecutionPolicy", "Bypass", "-Command", SCRIPT])
        .output()
        .map_err(|error| format!("could not start hidden PowerShell: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if !stderr.is_empty() { stderr } else { stdout };
        return Err(format!("PowerShell exited with {}: {detail}", output.status));
    }
    if !target.is_file() {
        return Err("PowerShell reported success but wrote no PNG".to_string());
    }
    Ok(())
}

const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName PresentationCore
Add-Type -AssemblyName WindowsBase
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class NoteCraftShellIcon {
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct SHFILEINFO {
        public IntPtr hIcon;
        public int iIcon;
        public uint dwAttributes;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 260)] public string szDisplayName;
        [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 80)] public string szTypeName;
    }
    [DllImport("shell32.dll", CharSet = CharSet.Unicode)]
    private static extern IntPtr SHGetFileInfo(string path, uint attributes, out SHFILEINFO info, uint size, uint flags);
    [DllImport("user32.dll")] public static extern bool DestroyIcon(IntPtr icon);
    public static IntPtr GetIcon(string source) {
        SHFILEINFO info;
        const uint SHGFI_ICON = 0x100;
        const uint SHGFI_LARGEICON = 0x0;
        if (SHGetFileInfo(source, 0, out info, (uint)Marshal.SizeOf(typeof(SHFILEINFO)), SHGFI_ICON | SHGFI_LARGEICON) == IntPtr.Zero || info.hIcon == IntPtr.Zero)
            throw new InvalidOperationException("Windows Shell returned no associated icon");
        return info.hIcon;
    }
}
"@
$icon = [NoteCraftShellIcon]::GetIcon($env:NC_ICON_SOURCE)
try {
    $bitmap = [System.Windows.Interop.Imaging]::CreateBitmapSourceFromHIcon(
        $icon,
        [System.Windows.Int32Rect]::Empty,
        [System.Windows.Media.Imaging.BitmapSizeOptions]::FromEmptyOptions()
    )
    $encoder = New-Object System.Windows.Media.Imaging.PngBitmapEncoder
    [void]$encoder.Frames.Add([System.Windows.Media.Imaging.BitmapFrame]::Create($bitmap))
    $stream = [System.IO.File]::Open(
        $env:NC_ICON_TARGET,
        [System.IO.FileMode]::Create,
        [System.IO.FileAccess]::Write,
        [System.IO.FileShare]::None
    )
    try { $encoder.Save($stream) } finally { $stream.Dispose() }
} finally {
    [void][NoteCraftShellIcon]::DestroyIcon($icon)
}
"#;
