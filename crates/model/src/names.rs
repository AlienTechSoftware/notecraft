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

/// The longest an attachment's file name is made, in characters.
pub const MAX_FILE_NAME: usize = 80;

/// A file name for an attachment: no path parts, only safe characters, never reserved, and at
/// most [`MAX_FILE_NAME`] characters. A name that is too long loses the end of its stem, not
/// its extension, so `report.pdf` stays a PDF however long it is (issue #130). Applying it
/// again changes nothing.
pub fn safe_file_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let cleaned: String = base.chars().map(|c| if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '-' }).collect();
    let trimmed = cleaned.trim_matches(|c| c == '.' || c == '-');
    let mut out = if trimmed.is_empty() { "file".to_string() } else { trimmed.to_string() };
    let stem = out.split('.').next().unwrap_or("").to_string();
    if is_reserved_name(&stem) {
        out = format!("file-{out}");
    }
    let (stem, extension) = split_extension(&out);
    fit_file_name(stem, extension, "")
}

/// `name` split before its extension (the dot included), when it has one worth keeping: a
/// short run of letters and digits after the last dot.
pub fn split_extension(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(dot) if dot > 0 && (2..=17).contains(&(name.len() - dot)) && name[dot + 1..].chars().all(|c| c.is_ascii_alphanumeric()) => {
            name.split_at(dot)
        }
        _ => (name, ""),
    }
}

/// `stem`, `suffix` and `extension` joined into a name of at most [`MAX_FILE_NAME`] characters,
/// cutting the stem when they do not fit. The uniqueness suffix of a second copy, `-2`, goes
/// in through here too, so the name it makes is still a safe name (issue #131).
pub fn fit_file_name(stem: &str, extension: &str, suffix: &str) -> String {
    let room = MAX_FILE_NAME.saturating_sub(extension.chars().count() + suffix.chars().count());
    let stem: String = stem.chars().take(room).collect();
    let stem = stem.trim_end_matches(['.', '-']);
    let stem = if stem.is_empty() { "file" } else { stem };
    format!("{stem}{suffix}{extension}")
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

    /// Issue #130: a long name keeps its extension, and is never longer than the limit.
    #[test]
    fn a_long_attachment_name_keeps_its_extension() {
        for extension in ["pdf", "png", "docx", "tar.gz"] {
            let name = safe_file_name(&format!("{}.{extension}", "x".repeat(90)));
            assert!(name.ends_with(&format!(".{}", extension.rsplit('.').next().expect("ext"))), "{name}");
            assert_eq!(name.chars().count(), MAX_FILE_NAME, "{name}");
        }
        assert_eq!(safe_file_name(&format!("{}.txt", "x".repeat(76))), format!("{}.txt", "x".repeat(76)), "a name at the limit is kept");
        assert_eq!(safe_file_name(&"x".repeat(90)), "x".repeat(80), "a name with no extension is cut as before");
        assert_eq!(safe_file_name(&format!("{}.not an extension at all", "x".repeat(90))).chars().count(), MAX_FILE_NAME);
        assert_eq!(safe_file_name(&format!("CON.{}.png", "y".repeat(90))).chars().count(), MAX_FILE_NAME, "a reserved stem made safe still fits");
        for name in ["../evil name.txt", "CON.png", "...", &format!("{}.pdf", "z".repeat(200)), &format!("é{}.docx", "ü".repeat(100))] {
            let once = safe_file_name(name);
            assert_eq!(safe_file_name(&once), once, "applying it again changes nothing: {name}");
        }
    }

    /// Issue #131: the suffix of a second copy fits inside the limit with the extension kept, so
    /// the name made is a safe name in its own right.
    #[test]
    fn a_suffixed_name_fits_and_stays_safe() {
        let original = format!("{}.txt", "x".repeat(76));
        let (stem, extension) = split_extension(&original);
        for n in [2, 10, 1234] {
            let name = fit_file_name(stem, extension, &format!("-{n}"));
            assert!(name.ends_with(&format!("-{n}.txt")) && name.chars().count() == MAX_FILE_NAME, "{name}");
            assert_eq!(safe_file_name(&name), name);
        }
        assert_eq!(fit_file_name("pic", ".png", "-2"), "pic-2.png");
        assert_eq!(split_extension("archive"), ("archive", ""));
        assert_eq!(split_extension(".hidden"), (".hidden", ""));
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
