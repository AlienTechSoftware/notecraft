//! Optional OfficeIMO bridge discovery and invocation.
//!
//! The bridge is deliberately an external process. This keeps the NoteCraft core Rust-only,
//! lets releases carry a platform-specific OfficeIMO build, and gives us a stable replacement
//! point if the importer implementation changes later.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The page serialization requested from the bridge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Html,
    Cmgfm,
}

impl Format {
    /// Parses the public page format names used by the CLI and bridge contract.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "html" => Ok(Self::Html),
            "cmgfm" => Ok(Self::Cmgfm),
            other => Err(format!("unknown OfficeIMO format '{other}'; use html or cmgfm")),
        }
    }
}

impl Format {
    fn argument(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Cmgfm => "cmgfm",
        }
    }
}

/// Locates an explicitly installed or bundled OfficeIMO bridge.
pub fn locate() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("NOTECRAFT_OFFICEIMO_BRIDGE") {
        let path = PathBuf::from(path);
        return executable_or_error(path, "NOTECRAFT_OFFICEIMO_BRIDGE");
    }

    let root = std::env::var_os("NOTECRAFT_DATA_DIR")
        .map(PathBuf::from)
        .or_else(data_dir)
        .ok_or_else(|| "cannot determine the NoteCraft data directory".to_string())?;
    let path = root.join("importers").join("officeimo").join(target_name()).join(executable_name());
    executable_or_error(path, "the installed OfficeIMO bridge")
}

/// Invokes the bridge using the stable command-line contract.
pub fn run(source: &Path, output: &Path, format: Format) -> Result<String, String> {
    if !source.exists() {
        return Err(format!("the OfficeIMO source {} does not exist", source.display()));
    }
    let bridge = locate()?;
    let result = Command::new(&bridge)
        .arg("--source")
        .arg(source)
        .arg("--out")
        .arg(output)
        .arg("--format")
        .arg(format.argument())
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot start OfficeIMO bridge {}: {e}", bridge.display()))?;
    let stdout = String::from_utf8_lossy(&result.stdout).into_owned();
    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(format!("OfficeIMO bridge failed with {}: {}", result.status, stderr.trim()));
    }
    Ok(stdout)
}

fn executable_or_error(path: PathBuf, source: &str) -> Result<PathBuf, String> {
    if path.is_file() { Ok(path) } else { Err(format!("{source} was not found at {}", path.display())) }
}

fn data_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from).map(|p| p.join("NoteCraft"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local").join("share")))
            .map(|p| p.join("NoteCraft"))
    }
}

fn target_name() -> &'static str {
    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "win-x64"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "linux-x64"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "osx-x64"
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "osx-arm64"
    } else {
        "unsupported"
    }
}

fn executable_name() -> OsString {
    if cfg!(windows) { OsString::from("officeimo-notecraft.exe") } else { OsString::from("officeimo-notecraft") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_names_are_release_rids() {
        assert!(matches!(target_name(), "win-x64" | "linux-x64" | "osx-x64" | "osx-arm64" | "unsupported"));
    }
}
