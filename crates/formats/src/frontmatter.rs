//! Front matter of a Markdown page: a block of `key: value` lines between two `---` lines.

/// Splits a Markdown page into its front matter (including both `---` lines) and the
/// rest. If the text has no complete front matter block, the first part is empty and
/// the rest is the whole text.
pub fn split_front(text: &str) -> (String, String) {
    // Some editors (Notepad, PowerShell) write a byte order mark at the start of a file.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.split_inclusive('\n');
    let first = lines.next().unwrap_or("");
    if first.trim_end() != "---" {
        return (String::new(), text.to_string());
    }
    let (mut front, mut rest, mut closed) = (String::from(first), String::new(), false);
    for line in lines {
        if closed {
            rest.push_str(line);
            continue;
        }
        front.push_str(line);
        if line.trim_end() == "---" {
            closed = true;
        }
    }
    if closed { (front, rest) } else { (String::new(), text.to_string()) }
}

/// The value of `key:` in a front matter block, with surrounding quotes removed.
pub fn value(front: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    front.lines().find_map(|l| l.strip_prefix(&prefix).map(|v| notecraft_model::kv::unquote(v.trim())))
}

/// Front matter with `key: value` set: the line is replaced if it is there, added just
/// before the closing `---` if it is not. Text with no front matter is returned unchanged,
/// because adding a block would change what the rest of the file means.
pub fn set_value(front: &str, key: &str, value: &str) -> String {
    let prefix = format!("{key}:");
    if front.lines().any(|l| l.starts_with(&prefix)) {
        return notecraft_model::kv::replace_line(front, &prefix, &format!("{key}: {value}"));
    }
    let mut out = String::with_capacity(front.len() + key.len() + value.len() + 4);
    let mut added = false;
    let lines: Vec<&str> = front.split_inclusive('\n').collect();
    for (i, line) in lines.iter().enumerate() {
        // The closing `---` is the last one; everything of ours goes above it.
        if !added && i > 0 && line.trim_end() == "---" {
            out.push_str(&format!("{key}: {value}\n"));
            added = true;
        }
        out.push_str(line);
    }
    if added { out } else { front.to_string() }
}

/// Front matter without `key:` at all.
pub fn remove_value(front: &str, key: &str) -> String {
    let prefix = format!("{key}:");
    front.split_inclusive('\n').filter(|l| !l.starts_with(&prefix)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_a_complete_block() {
        let (front, rest) = split_front("---\nid: 1\ntitle: \"A\"\n---\n\n# Body\n");
        assert_eq!(front, "---\nid: 1\ntitle: \"A\"\n---\n");
        assert_eq!(rest, "\n# Body\n");
        assert_eq!(value(&front, "title").as_deref(), Some("A"));
        assert_eq!(value(&front, "id").as_deref(), Some("1"));
        assert_eq!(value(&front, "missing"), None);
    }

    #[test]
    fn no_front_matter_returns_everything_as_body() {
        assert_eq!(split_front("# Title\n"), (String::new(), "# Title\n".to_string()));
        assert_eq!(split_front(""), (String::new(), String::new()));
    }

    #[test]
    fn unclosed_block_is_not_front_matter() {
        let text = "---\nid: 1\nno closing line\n";
        assert_eq!(split_front(text), (String::new(), text.to_string()));
    }

    #[test]
    fn a_byte_order_mark_does_not_hide_the_front_matter() {
        let (front, rest) = split_front("\u{feff}---\nid: 1\n---\nbody");
        assert!(front.starts_with("---"));
        assert_eq!(value(&front, "id").as_deref(), Some("1"));
        assert_eq!(rest, "body");
    }

    #[test]
    fn windows_line_endings() {
        let (front, rest) = split_front("---\r\nid: 1\r\n---\r\nbody");
        assert!(front.ends_with("---\r\n"));
        assert_eq!(rest, "body");
    }

    #[test]
    fn a_value_can_be_set_added_and_removed() {
        let front = "---
title: Plan
id: abc
---
";
        let with = set_value(front, "dir", "rtl");
        assert_eq!(
            with,
            "---
title: Plan
id: abc
dir: rtl
---
"
        );
        assert_eq!(value(&with, "dir").as_deref(), Some("rtl"));
        let changed = set_value(&with, "dir", "ltr");
        assert_eq!(value(&changed, "dir").as_deref(), Some("ltr"));
        assert_eq!(changed.lines().filter(|l| l.starts_with("dir:")).count(), 1, "no second line");
        let gone = remove_value(&changed, "dir");
        assert_eq!(gone, front);
        assert_eq!(value(&gone, "dir"), None);
        // Text with no front matter is left alone rather than given a block.
        assert_eq!(set_value("", "dir", "rtl"), "");
        assert_eq!(remove_value("", "dir"), "");
    }
}
