//! New page files.
//!
//! A new page carries its title and the date it was started, the way OneNote opens a page
//! with a title and a timestamp under it.

use crate::html::escape;
use notecraft_model::kv::quote;
use notecraft_model::time::long_from_iso;

pub fn markdown_page(title: &str, id: &str, order: u32, now: &str) -> String {
    let stamp = long_from_iso(now).unwrap_or_else(|| now.to_string());
    // The stamp is plain words, not italics: the page draws it in its own quiet grey, and
    // marks round it would be shown as marks the moment the caret landed on the line.
    //
    // The title is not in the body. A page carries its title in its head, and the window
    // draws it in the title area above the words, the way OneNote does; writing it into
    // the body as well showed every page its own name twice.
    format!("---\nid: {id}\ntitle: {}\norder: {order}\ncreated: {now}\nupdated: {now}\ntags: []\n---\n\n{stamp}\n\n", quote(title))
}

pub fn html_page(title: &str, id: &str, order: u32, now: &str) -> String {
    let t = escape(title);
    let stamp = escape(&long_from_iso(now).unwrap_or_else(|| now.to_string()));
    format!(
        "<!doctype html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>{t}</title>\n<meta name=\"notecraft:id\" content=\"{id}\">\n<meta name=\"notecraft:order\" content=\"{order}\">\n<meta name=\"notecraft:created\" content=\"{now}\">\n<meta name=\"notecraft:updated\" content=\"{now}\">\n</head>\n<body>\n<p class=\"page-date\">{stamp}</p>\n</body>\n</html>\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontmatter;

    #[test]
    fn markdown_page_has_readable_front_matter() {
        let page = markdown_page("My \"Page\"", "abc", 20, "2026-10-17T00:00:00Z");
        let (front, body) = frontmatter::split_front(&page);
        assert_eq!(frontmatter::value(&front, "title").as_deref(), Some("My \"Page\""));
        assert_eq!(frontmatter::value(&front, "order").as_deref(), Some("20"));
        // The title is in the front matter and nowhere else: the window draws it above the
        // page, so a body that said it again would show it twice.
        assert!(!body.contains("# My"), "the body does not repeat the title: {body}");
        assert!(body.contains("\nSaturday, 17 October 2026, 00:00\n"), "the page starts with its date, in plain words: {body}");
    }

    #[test]
    fn a_page_with_a_broken_timestamp_still_works() {
        let page = markdown_page("T", "id", 10, "not a time");
        assert!(page.contains(
            "
not a time
"
        ));
        assert!(html_page("T", "id", 10, "not a time").contains("not a time"));
    }

    #[test]
    fn html_page_escapes_the_title() {
        let page = html_page("A <b> & C", "abc", 10, "2026-10-17T00:00:00Z");
        assert!(page.contains("<title>A &lt;b&gt; &amp; C</title>"));
        assert!(page.contains("name=\"notecraft:id\" content=\"abc\""));
        assert!(page.contains("<body>"));
    }
}
