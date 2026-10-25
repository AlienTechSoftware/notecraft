//! The global settings file. It lives in the user's config folder, outside any
//! notebook, so notebooks carry no personal settings.

use crate::error::{Result, StoreError};
use notecraft_model::{PageFormat, kv};
use std::path::{Path, PathBuf};

/// Where the format for a new page came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormatSource {
    /// Chosen explicitly for this page.
    Explicit,
    /// A `_node.toml` in the notebook tree (notebook, section group or section).
    Tree,
    /// The global setting.
    Global,
    /// The built-in default.
    BuiltIn,
}

impl FormatSource {
    pub fn label(self) -> &'static str {
        match self {
            FormatSource::Explicit => "chosen for this page",
            FormatSource::Tree => "the notebook or section setting",
            FormatSource::Global => "the global setting",
            FormatSource::BuiltIn => "the built-in default",
        }
    }
}

/// The default notebooks folder: `NOTECRAFT_NOTEBOOKS`, else `Documents/NoteCraft` in the home folder.
pub fn default_notebooks_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("NOTECRAFT_NOTEBOOKS") {
        return PathBuf::from(p);
    }
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    home.join("Documents").join("NoteCraft")
}

#[derive(Clone, Debug)]
pub struct Settings {
    path: PathBuf,
}

impl Settings {
    /// Settings stored in `path` (used by tests and by `NOTECRAFT_SETTINGS`).
    pub fn at(path: impl Into<PathBuf>) -> Settings {
        Settings { path: path.into() }
    }

    /// The usual place: `NOTECRAFT_SETTINGS`, else `%APPDATA%\NoteCraft\settings.toml`,
    /// else `$XDG_CONFIG_HOME/notecraft/settings.toml`, else `$HOME/.config/notecraft/settings.toml`.
    pub fn default_location() -> Settings {
        if let Some(p) = std::env::var_os("NOTECRAFT_SETTINGS") {
            return Settings::at(p);
        }
        // Development mode: prefer local folder next to executable/cwd
        let local_dev = PathBuf::from("local/settings.toml");
        if local_dev.exists() {
            return Settings::at(local_dev);
        }
        // Check if we're in a development environment (local/ folder exists nearby)
        if let Ok(cwd) = std::env::current_dir()
            && (cwd.join("local").exists() || cwd.join("Cargo.toml").exists())
        {
            return Settings::at(local_dev);
        }
        // Production: use platform-specific locations
        let path = if let Some(appdata) = std::env::var_os("APPDATA") {
            PathBuf::from(appdata).join("NoteCraft").join("settings.toml")
        } else if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
            PathBuf::from(xdg).join("notecraft").join("settings.toml")
        } else if let Some(home) = std::env::var_os("HOME") {
            PathBuf::from(home).join(".config").join("notecraft").join("settings.toml")
        } else {
            PathBuf::from("notecraft-settings.toml")
        };
        Settings::at(path)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The global default format, if one was saved and is valid.
    pub fn default_format(&self) -> Option<PageFormat> {
        let text = std::fs::read_to_string(&self.path).ok()?;
        kv::parse(&text).get("default_format").and_then(|v| PageFormat::parse(v).ok())
    }

    pub fn set_default_format(&self, format: PageFormat) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| StoreError::io("create", dir, e))?;
        }
        let mut map = std::fs::read_to_string(&self.path).map(|t| kv::parse(&t)).unwrap_or_default();
        map.insert("default_format".into(), format.id().into());
        std::fs::write(&self.path, kv::render(&map)).map_err(|e| StoreError::io("write", &self.path, e))
    }

    /// Any saved setting by key (for example `theme_mode`). Unknown keys are kept by every write.
    pub fn get(&self, key: &str) -> Option<String> {
        let text = std::fs::read_to_string(&self.path).ok()?;
        kv::parse(&text).get(key).cloned()
    }

    pub fn set(&self, key: &str, value: &str) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| StoreError::io("create", dir, e))?;
        }
        let mut map = std::fs::read_to_string(&self.path).map(|t| kv::parse(&t)).unwrap_or_default();
        map.insert(key.to_string(), value.to_string());
        std::fs::write(&self.path, kv::render(&map)).map_err(|e| StoreError::io("write", &self.path, e))
    }

    /// The format used for new pages when nothing nearer says otherwise.
    pub fn global_format(&self) -> (PageFormat, FormatSource) {
        match self.default_format() {
            Some(f) => (f, FormatSource::Global),
            None => (PageFormat::DEFAULT, FormatSource::BuiltIn),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    #[test]
    fn built_in_default_is_html() {
        let t = TempDir::new("settings1");
        let s = Settings::at(t.path().join("settings.toml"));
        assert_eq!(s.global_format(), (PageFormat::Html, FormatSource::BuiltIn));
    }

    #[test]
    fn saves_and_reads_the_global_format() {
        let t = TempDir::new("settings2");
        let s = Settings::at(t.path().join("sub").join("settings.toml"));
        s.set_default_format(PageFormat::Cmgfm).expect("save");
        assert_eq!(s.global_format(), (PageFormat::Cmgfm, FormatSource::Global));
        s.set_default_format(PageFormat::Html).expect("save");
        assert_eq!(s.default_format(), Some(PageFormat::Html));
    }

    #[test]
    fn generic_keys_round_trip_and_do_not_disturb_the_format() {
        let t = TempDir::new("settings4");
        let s = Settings::at(t.path().join("settings.toml"));
        assert_eq!(s.get("theme_mode"), None);
        s.set("theme_mode", "dark").expect("set");
        s.set_default_format(PageFormat::Cmgfm).expect("set");
        s.set("theme_accent", "red").expect("set");
        assert_eq!(s.get("theme_mode").as_deref(), Some("dark"));
        assert_eq!(s.get("theme_accent").as_deref(), Some("red"));
        assert_eq!(s.default_format(), Some(PageFormat::Cmgfm));
    }

    #[test]
    fn keeps_unknown_keys_and_ignores_bad_values() {
        let t = TempDir::new("settings3");
        let p = t.path().join("settings.toml");
        std::fs::write(&p, "other = \"keep\"\ndefault_format = \"docx\"\n").expect("write");
        let s = Settings::at(&p);
        assert_eq!(s.default_format(), None);
        s.set_default_format(PageFormat::Cmgfm).expect("save");
        let text = std::fs::read_to_string(&p).expect("read");
        assert!(text.contains("other = \"keep\""));
        assert!(text.contains("default_format = \"cmgfm\""));
    }
}
