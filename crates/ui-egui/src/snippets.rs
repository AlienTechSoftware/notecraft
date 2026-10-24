//! Text the toolbar inserts into the page source, for each format. Pure functions,
//! so they are tested without a window.

use notecraft_model::PageFormat;
use notecraft_store::assets::{MediaKind, media_kind};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snippet {
    pub label: &'static str,
    pub hint: &'static str,
    pub text: &'static str,
}

const fn s(label: &'static str, hint: &'static str, text: &'static str) -> Snippet {
    Snippet { label, hint, text }
}

/// The toolbar for a format.
const HTML: &[Snippet] = &[
    s("B", "Bold", "<b>bold</b>"),
    s("I", "Italic", "<i>italic</i>"),
    s("S", "Strikethrough", "<s>text</s>"),
    s("Code", "Inline code", "<code>code</code>"),
    s("H1", "Heading 1", "\n<h1>Heading</h1>\n"),
    s("H2", "Heading 2", "\n<h2>Heading</h2>\n"),
    s("List", "Bulleted list", "\n<ul>\n<li>one</li>\n<li>two</li>\n</ul>\n"),
    s("1. List", "Numbered list", "\n<ol>\n<li>one</li>\n<li>two</li>\n</ol>\n"),
    s("To-do", "To-do line", "\n<p>&#9744; to do</p>\n"),
    s("Quote", "Block quote", "\n<blockquote>quote</blockquote>\n"),
    s("Link", "Link", "<a href=\"https://\">text</a>"),
    s("Table", "3 x 3 table", "\n<table>\n<tr><th>A</th><th>B</th><th>C</th></tr>\n<tr><td>1</td><td>2</td><td>3</td></tr>\n</table>\n"),
    s("Equation", "Equation (TeX between dollar signs)", "<span class=\"math\">$ E = mc^2 $</span>"),
    s("Diagram", "Diagram block (mermaid source)", "\n<pre class=\"diagram\">graph TD;\n  A--&gt;B;</pre>\n"),
];

const CMGFM: &[Snippet] = &[
    s("B", "Bold", "**bold**"),
    s("I", "Italic", "*italic*"),
    s("S", "Strikethrough", "~~text~~"),
    s("Code", "Inline code", "`code`"),
    s("H1", "Heading 1", "\n# Heading\n"),
    s("H2", "Heading 2", "\n## Heading\n"),
    s("List", "Bulleted list", "\n- one\n- two\n"),
    s("1. List", "Numbered list", "\n1. one\n2. two\n"),
    s("To-do", "Task list item", "\n- [ ] to do\n"),
    s("Quote", "Block quote", "\n> quote\n"),
    s("Link", "Link", "[text](https://)"),
    s("Table", "3 x 3 table", "\n| A | B | C |\n|---|---|---|\n| 1 | 2 | 3 |\n"),
    s("Equation", "Equation block (TeX)", "\n$$\nE = mc^2\n$$\n"),
    s("Diagram", "Diagram block (mermaid source)", "\n```mermaid\ngraph TD;\n  A-->B;\n```\n"),
];

/// The toolbar for a format.
pub fn toolbar(format: PageFormat) -> &'static [Snippet] {
    match format {
        PageFormat::Html => HTML,
        PageFormat::Cmgfm => CMGFM,
    }
}

/// Inserts `snippet` into `text` at the character index `cursor` (clamped to the
/// text). Returns the new text and the cursor index after the snippet.
pub fn insert_at(text: &str, cursor: usize, snippet: &str) -> (String, usize) {
    let total = text.chars().count();
    let at = cursor.min(total);
    let byte = text.char_indices().nth(at).map(|(i, _)| i).unwrap_or(text.len());
    let mut out = String::with_capacity(text.len() + snippet.len());
    out.push_str(text.get(..byte).unwrap_or(""));
    out.push_str(snippet);
    out.push_str(text.get(byte..).unwrap_or(""));
    (out, at + snippet.chars().count())
}

/// Source text that links an attachment (`link` is relative, like `assets/pic.png`).
pub fn attachment(format: PageFormat, link: &str, name: &str) -> String {
    let kind = media_kind(link);
    let icon = attachment_icon(link);
    match (format, kind) {
        (PageFormat::Html, MediaKind::Image) => format!("\n<img src=\"{link}\" alt=\"{}\">\n", notecraft_formats::html::escape(name)),
        (PageFormat::Html, MediaKind::Video) => format!("\n<video src=\"{link}\" controls></video>\n"),
        (PageFormat::Html, MediaKind::Audio) => format!("\n<audio src=\"{link}\" controls></audio>\n"),
        (PageFormat::Html, MediaKind::Other) => {
            format!("\n<p><a href=\"{link}\"><img src=\"{icon}\" alt=\"\" width=\"48\"><br>{}</a></p>\n", notecraft_formats::html::escape(name))
        }
        (PageFormat::Cmgfm, MediaKind::Image) => format!("\n![{name}]({link})\n"),
        (PageFormat::Cmgfm, _) => format!("\n[![{name}]({icon}) {name}]({link})\n"),
    }
}

fn attachment_icon(link: &str) -> String {
    let name = link.strip_prefix("assets/").unwrap_or(link);
    format!("assets/{name}{}", attachment_icon_extension())
}

#[cfg(target_os = "windows")]
fn attachment_icon_extension() -> &'static str {
    ".png"
}

#[cfg(not(target_os = "windows"))]
fn attachment_icon_extension() -> &'static str {
    ".svg"
}

/// A link to a file that was left where it is, OneNote's "insert a link to the file".
pub fn file_link(format: PageFormat, target: &str, name: &str) -> String {
    let url = file_url(target);
    match format {
        PageFormat::Html => format!("\n<p><a href=\"{}\">{}</a></p>\n", notecraft_formats::html::escape(&url), notecraft_formats::html::escape(name)),
        PageFormat::Cmgfm => format!("\n[{name}]({url})\n"),
    }
}

/// A local path as a `file:` URL, so a link stays a link on every system.
fn file_url(path: &str) -> String {
    let slashes = path.replace('\\', "/");
    let encoded: String = slashes
        .chars()
        .map(|c| match c {
            ' ' => "%20".to_string(),
            '"' => "%22".to_string(),
            '#' => "%23".to_string(),
            '?' => "%3F".to_string(),
            other => other.to_string(),
        })
        .collect();
    if encoded.starts_with('/') { format!("file://{encoded}") } else { format!("file:///{encoded}") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notecraft_formats::{html, markdown};

    #[test]
    fn a_link_to_a_file_that_stays_where_it_is() {
        let html = file_link(PageFormat::Html, r"C:\Users\x\My Notes\plan.docx", "plan.docx");
        assert!(html.contains("file:///C:/Users/x/My%20Notes/plan.docx"), "{html}");
        assert!(html.contains(">plan.docx</a>"));
        let md = file_link(PageFormat::Cmgfm, "/home/x/plan.docx", "plan.docx");
        assert_eq!(md.trim(), "[plan.docx](file:///home/x/plan.docx)");
        assert!(file_url("/a b#c?d").contains("%20") && file_url("/a b#c?d").contains("%23"));
    }

    #[test]
    fn insert_at_the_cursor() {
        assert_eq!(insert_at("hello", 2, "XX"), ("heXXllo".to_string(), 4));
        assert_eq!(insert_at("hello", 0, "X"), ("Xhello".to_string(), 1));
        assert_eq!(insert_at("hello", 99, "X"), ("helloX".to_string(), 6), "past the end is clamped");
        assert_eq!(insert_at("", 5, "ab"), ("ab".to_string(), 2));
    }

    #[test]
    fn insert_counts_characters_not_bytes() {
        assert_eq!(insert_at("caf\u{e9}s", 4, "!"), ("caf\u{e9}!s".to_string(), 5));
        assert_eq!(insert_at("\u{1F600}\u{1F600}", 1, "-"), ("\u{1F600}-\u{1F600}".to_string(), 2));
    }

    #[test]
    fn markdown_snippets_parse_to_the_blocks_they_promise() {
        let block_count = |label: &str| {
            let sn = toolbar(PageFormat::Cmgfm).iter().find(|s| s.label == label).expect("snippet");
            markdown::parse(sn.text)
        };
        assert!(matches!(block_count("H1").first(), Some(markdown::Block::Heading { level: 1, .. })));
        assert!(matches!(block_count("H2").first(), Some(markdown::Block::Heading { level: 2, .. })));
        assert_eq!(block_count("List").len(), 2);
        assert!(matches!(block_count("To-do").first(), Some(markdown::Block::Item { task: Some(false), .. })));
        assert!(matches!(block_count("Quote").first(), Some(markdown::Block::Quote(_))));
        assert!(matches!(block_count("Table").first(), Some(markdown::Block::Table { .. })));
        assert!(matches!(block_count("Equation").first(), Some(markdown::Block::MathBlock(_))));
        assert!(matches!(block_count("Diagram").first(), Some(markdown::Block::Code { lang, .. }) if lang == "mermaid"));
    }

    #[test]
    fn html_snippets_convert_to_the_same_blocks() {
        let blocks = |label: &str| {
            let sn = toolbar(PageFormat::Html).iter().find(|s| s.label == label).expect("snippet");
            markdown::parse(&html::to_markdown(sn.text))
        };
        assert!(matches!(blocks("H1").first(), Some(markdown::Block::Heading { level: 1, .. })));
        assert_eq!(blocks("List").len(), 2);
        assert!(matches!(blocks("Quote").first(), Some(markdown::Block::Quote(_))));
        assert!(matches!(blocks("Table").first(), Some(markdown::Block::Table { .. })));
        assert!(matches!(blocks("Diagram").first(), Some(markdown::Block::Code { lang, .. }) if lang == "mermaid"));
    }

    #[test]
    fn both_toolbars_offer_the_same_buttons() {
        let a: Vec<_> = toolbar(PageFormat::Html).iter().map(|s| s.label).collect();
        let b: Vec<_> = toolbar(PageFormat::Cmgfm).iter().map(|s| s.label).collect();
        assert_eq!(a, b);
    }

    #[test]
    fn attachments_link_by_media_kind() {
        assert_eq!(attachment(PageFormat::Cmgfm, "assets/a.png", "a.png"), "\n![a.png](assets/a.png)\n");
        let icon = if cfg!(target_os = "windows") { "assets/a.pdf.png" } else { "assets/a.pdf.svg" };
        assert!(attachment(PageFormat::Cmgfm, "assets/a.pdf", "a.pdf").contains(icon));
        assert!(attachment(PageFormat::Html, "assets/a.mp4", "a.mp4").contains("<video src=\"assets/a.mp4\" controls>"));
        assert!(attachment(PageFormat::Html, "assets/a.png", "a<b>.png").contains("alt=\"a&lt;b&gt;.png\""));
        assert!(attachment(PageFormat::Html, "assets/a.mp3", "a.mp3").contains("<audio"));
    }
}
