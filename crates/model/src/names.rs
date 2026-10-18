//! Rules for names that must work on Windows, macOS and Linux and in common sync
//! tools. The OneDrive limits come from secondary sources (a Microsoft support
//! article mirrored by university IT sites) and are not verified for every tool.

/// A full path from the sync root should stay under this many characters.
pub const MAX_PATH_FROM_ROOT: usize = 400;
/// Each folder or file name should stay under this many characters.
pub const MAX_NAME: usize = 255;
/// Folder names made from titles are cut to this length.
pub const SLUG_LEN: usize = 40;

/// Names Windows (and so most sync tools) refuse: CON, PRN, AUX, NUL, COM0-9, LPT0-9.
pub fn is_reserved_name(name: &str) -> bool {
    let n = name.to_lowercase();
    matches!(n.as_str(), "con" | "prn" | "aux" | "nul")
        || ((n.starts_with("com") || n.starts_with("lpt")) && n.chars().count() == 4 && n.chars().nth(3).is_some_and(|c| c.is_ascii_digit()))
}

/// A folder name made from a page title: lower case, letters and digits, single
/// dashes. Reserved names get a `-page` suffix. Never empty.
pub fn slug(title: &str) -> String {
    let mut out = String::new();
    for c in title.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out: String = out.trim_end_matches('-').chars().take(SLUG_LEN).collect();
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() {
        "untitled".to_string()
    } else if is_reserved_name(&out) {
        format!("{out}-page")
    } else {
        out
    }
}

/// Checks one folder or file name against the cross-platform rules.
pub fn validate_component(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("a name cannot be empty".into());
    }
    if name.chars().count() > MAX_NAME {
        return Err(format!("a name cannot be longer than {MAX_NAME} characters"));
    }
    if let Some(c) = name.chars().find(|c| matches!(c, '"' | '*' | ':' | '<' | '>' | '?' | '/' | '\\' | '|') || (*c as u32) < 0x20) {
        return Err(format!("the character '{}' is not allowed in a name", c.escape_default()));
    }
    if name.starts_with(' ') || name.ends_with(' ') || name.ends_with('.') {
        return Err("a name cannot start or end with a space, or end with a dot".into());
    }
    let stem = name.split('.').next().unwrap_or(name);
    if is_reserved_name(stem) {
        return Err(format!("'{stem}' is a reserved name on Windows"));
    }
    let lower = name.to_lowercase();
    if lower == "desktop.ini" || lower == ".lock" || lower.contains("_vti_") || name.starts_with("~$") || name.starts_with('~') {
        return Err("this name is blocked by common sync tools".into());
    }
    Ok(())
}

/// True if two names differ only by case (they cannot coexist on Windows or on
/// default macOS volumes).
pub fn collides_by_case(a: &str, b: &str) -> bool {
    a != b && a.to_lowercase() == b.to_lowercase()
}

/// A file name for an attachment: no path parts, only safe characters, never reserved.
pub fn safe_file_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let cleaned: String = base.chars().map(|c| if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '-' }).collect();
    let trimmed: String = cleaned.trim_matches(|c| c == '.' || c == '-').chars().take(80).collect();
    let mut out = if trimmed.is_empty() { "file".to_string() } else { trimmed };
    let stem = out.split('.').next().unwrap_or("").to_string();
    if is_reserved_name(&stem) {
        out = format!("file-{out}");
    }
    out
}

/// A file in a page or container folder that looks like a sync conflict copy of
/// `index.*` or `_node.toml`, for example `index-LAPTOP.html` or `index (1).md`.
/// The naming patterns come from secondary sources and are not verified for every tool.
pub fn looks_like_conflict(file_name: &str) -> bool {
    let lower = file_name.to_lowercase();
    if ["index.html", "index.md", "_node.toml"].contains(&lower.as_str()) {
        return false;
    }
    let Some((stem, ext)) = lower.rsplit_once('.') else {
        return false;
    };
    matches!(ext, "html" | "md" | "toml") && (stem.starts_with("index") || stem.starts_with("_node")) && stem.chars().count() > 5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(slug("Meeting: 1 / Notes!"), "meeting-1-notes");
        assert_eq!(slug("???"), "untitled");
        assert_eq!(slug("CON"), "con-page");
        assert_eq!(slug("Com1"), "com1-page");
        assert_eq!(slug("Console"), "console");
        assert!(slug(&"x".repeat(100)).chars().count() <= SLUG_LEN);
    }

    #[test]
    fn components() {
        assert!(validate_component("project-x").is_ok());
        assert!(validate_component("a:b").is_err());
        assert!(validate_component("nul").is_err());
        assert!(validate_component("nul.txt").is_err());
        assert!(validate_component("desktop.ini").is_err());
        assert!(validate_component("~$tmp").is_err());
        assert!(validate_component("trailing.").is_err());
        assert!(validate_component(&"x".repeat(256)).is_err());
    }

    #[test]
    fn case_collisions() {
        assert!(collides_by_case("Notes", "notes"));
        assert!(!collides_by_case("notes", "notes"));
    }

    #[test]
    fn attachment_names() {
        assert_eq!(safe_file_name("../evil name.txt"), "evil-name.txt");
        assert_eq!(safe_file_name("CON.png"), "file-CON.png");
        assert_eq!(safe_file_name("..."), "file");
    }

    #[test]
    fn conflict_names() {
        assert!(!looks_like_conflict("index.html"));
        assert!(!looks_like_conflict("_node.toml"));
        assert!(looks_like_conflict("index-MOHAMED-PC.html"));
        assert!(looks_like_conflict("index (1).md"));
        assert!(looks_like_conflict("_node-LAPTOP.toml"));
        assert!(!looks_like_conflict("photo-LAPTOP.png"));
    }
}
