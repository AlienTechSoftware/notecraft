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
}
