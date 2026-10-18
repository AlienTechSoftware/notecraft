//! A tiny reader and writer for the simple `key = "value"` text used by `_node.toml`
//! and the settings file. It handles only the lines NoteCraft writes itself.

use std::collections::HashMap;

pub fn quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

pub fn unquote(value: &str) -> String {
    let value = value.trim();
    if let Some(rest) = value.strip_prefix('"') {
        let (mut out, mut escaped) = (String::new(), false);
        for c in rest.chars() {
            if escaped {
                out.push(c);
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                break;
            } else {
                out.push(c);
            }
        }
        out
    } else {
        value.split('#').next().unwrap_or("").trim().to_string()
    }
}

/// Parses `key = value` lines. Blank lines and lines starting with `#` are skipped.
pub fn parse(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            map.insert(key.trim().to_string(), unquote(value));
        }
    }
    map
}

/// Writes the keys in sorted order as `key = "value"` lines.
pub fn render(map: &HashMap<String, String>) -> String {
    let mut keys: Vec<&String> = map.keys().collect();
    keys.sort();
    keys.iter().filter_map(|k| map.get(*k).map(|v| format!("{k} = {}\n", quote(v)))).collect()
}

/// Replaces the first line that starts with `prefix`, keeping its line ending.
/// Returns the text unchanged if no line matches.
pub fn replace_line(text: &str, prefix: &str, new_line: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut done = false;
    for line in text.split_inclusive('\n') {
        if !done && line.starts_with(prefix) {
            let ending = if line.ends_with("\r\n") {
                "\r\n"
            } else if line.ends_with('\n') {
                "\n"
            } else {
                ""
            };
            out.push_str(new_line);
            out.push_str(ending);
            done = true;
        } else {
            out.push_str(line);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_round_trips() {
        let s = "a \"quoted\" \\ title";
        assert_eq!(unquote(&quote(s)), s);
        assert_eq!(unquote("\"html\"   # comment"), "html");
        assert_eq!(unquote("20"), "20");
    }

    #[test]
    fn parse_and_render() {
        let m = parse("# c\nkind = \"notebook\"\nformat = \"html\"  # note\n\norder = 20\n");
        assert_eq!(m.get("kind").map(String::as_str), Some("notebook"));
        assert_eq!(m.get("format").map(String::as_str), Some("html"));
        assert_eq!(m.get("order").map(String::as_str), Some("20"));
        assert!(render(&m).contains("kind = \"notebook\"\n"));
    }

    #[test]
    fn replace_keeps_endings() {
        assert_eq!(replace_line("a: 1\r\nb: 2\r\n", "b:", "b: 9"), "a: 1\r\nb: 9\r\n");
        assert_eq!(replace_line("a: 1\n", "x:", "x: 1"), "a: 1\n");
    }
}
