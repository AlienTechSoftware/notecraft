//! New page files.
//!
//! A new page carries the date it was started, the way OneNote opens a page with a
//! timestamp under its (empty) title.

use crate::html::escape;
use notecraft_model::kv::quote;
use notecraft_model::time::long_from_iso;

/// `title` is `None` for a page that has not been named yet: the "Add Page" case. No
/// `title` key is written at all, so the page reads back as untitled wherever one is read
/// (`notecraft_store::tree::read_node`, `notecraft_store::tree::raw_title`) rather than
/// carrying the word "Untitled" as if a writer had typed it. A page is always free to be
/// given a real title later, which writes the key in for the first time.
pub fn markdown_page(title: Option<&str>, id: &str, order: u32, now: &str) -> String {
    let stamp = long_from_iso(now).unwrap_or_else(|| now.to_string());
    let title_line = title.map(|t| format!("title: {}\n", quote(t))).unwrap_or_default();
    // The stamp is plain words, not italics: the page draws it in its own quiet grey, and
    // marks round it would be shown as marks the moment the caret landed on the line.
    //
    // The title is not in the body. A page carries its title in its head, and the window
    // draws it in the title area above the words, the way OneNote does; writing it into
    // the body as well showed every page its own name twice.
    format!("---\nid: {id}\n{title_line}order: {order}\ncreated: {now}\nupdated: {now}\ntags: []\n---\n\n{stamp}\n\n")
}

/// See `markdown_page` for what `None` means here.
pub fn html_page(title: Option<&str>, id: &str, order: u32, now: &str) -> String {
    let title_line = title.map(|t| format!("<title>{}</title>\n", escape(t))).unwrap_or_default();
    let stamp = escape(&long_from_iso(now).unwrap_or_else(|| now.to_string()));
    format!(
        "<!doctype html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n{title_line}<meta name=\"notecraft:id\" content=\"{id}\">\n<meta name=\"notecraft:order\" content=\"{order}\">\n<meta name=\"notecraft:created\" content=\"{now}\">\n<meta name=\"notecraft:updated\" content=\"{now}\">\n</head>\n<body>\n<p class=\"page-date\">{stamp}</p>\n</body>\n</html>\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontmatter;

    #[test]
    fn markdown_page_has_readable_front_matter() {
        let page = markdown_page(Some("My \"Page\""), "abc", 20, "2026-10-17T00:00:00Z");
        let (front, body) = frontmatter::split_front(&page);
        assert_eq!(frontmatter::value(&front, "title").as_deref(), Some("My \"Page\""));
        assert_eq!(frontmatter::value(&front, "order").as_deref(), Some("20"));
        // The title is in the front matter and nowhere else: the window draws it above the
        // page, so a body that said it again would show it twice.
        assert!(!body.contains("# My"), "the body does not repeat the title: {body}");
        assert!(body.contains("\nSaturday, 17 October 2026, 00:00\n"), "the page starts with its date, in plain words: {body}");
    }

    /// `None` leaves the title key out entirely, so the page reads back as untitled rather
    /// than carrying the word "Untitled" as if it were real content.
    #[test]
    fn no_title_writes_no_title_key() {
        let md = markdown_page(None, "abc", 20, "2026-10-17T00:00:00Z");
        let (front, body) = frontmatter::split_front(&md);
        assert_eq!(frontmatter::value(&front, "title"), None, "no title key at all:\n{front}");
        assert_eq!(frontmatter::value(&front, "id").as_deref(), Some("abc"), "the other keys are still there");
        assert!(body.contains("Saturday, 17 October 2026, 00:00"), "the date is still there:\n{body}");

        let html = html_page(None, "abc", 20, "2026-10-17T00:00:00Z");
        assert!(!html.contains("<title>"), "no title element at all:\n{html}");
        assert!(html.contains("name=\"notecraft:id\" content=\"abc\""), "the other metadata is still there");
    }

    #[test]
    fn a_page_with_a_broken_timestamp_still_works() {
        let page = markdown_page(Some("T"), "id", 10, "not a time");
        assert!(page.contains(
            "
not a time
"
        ));
        assert!(html_page(Some("T"), "id", 10, "not a time").contains("not a time"));
    }

    #[test]
    fn html_page_escapes_the_title() {
        let page = html_page(Some("A <b> & C"), "abc", 10, "2026-10-17T00:00:00Z");
        assert!(page.contains("<title>A &lt;b&gt; &amp; C</title>"));
        assert!(page.contains("name=\"notecraft:id\" content=\"abc\""));
        assert!(page.contains("<body>"));
    }
}
