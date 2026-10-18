//! New page files.
//!
//! A new page carries its title and the date it was started, the way OneNote opens a page
//! with a title and a timestamp under it.

use crate::html::escape;
use notecraft_model::kv::quote;
use notecraft_model::time::long_from_iso;

pub fn markdown_page(title: &str, id: &str, order: u32, now: &str) -> String {
    let stamp = long_from_iso(now).unwrap_or_else(|| now.to_string());
    format!("---\nid: {id}\ntitle: {}\norder: {order}\ncreated: {now}\nupdated: {now}\ntags: []\n---\n\n# {title}\n\n*{stamp}*\n\n", quote(title))
}

pub fn html_page(title: &str, id: &str, order: u32, now: &str) -> String {
    let t = escape(title);
    let stamp = escape(&long_from_iso(now).unwrap_or_else(|| now.to_string()));
    format!(
        "<!doctype html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>{t}</title>\n<meta name=\"notecraft:id\" content=\"{id}\">\n<meta name=\"notecraft:order\" content=\"{order}\">\n<meta name=\"notecraft:created\" content=\"{now}\">\n<meta name=\"notecraft:updated\" content=\"{now}\">\n</head>\n<body>\n<h1>{t}</h1>\n<p class=\"page-date\"><i>{stamp}</i></p>\n</body>\n</html>\n"
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
        assert!(body.contains("# My \"Page\""));
        assert!(body.contains("*Saturday, 17 October 2026, 00:00*"), "the page starts with its date: {body}");
    }

    #[test]
    fn a_page_with_a_broken_timestamp_still_works() {
        let page = markdown_page("T", "id", 10, "not a time");
        assert!(page.contains("*not a time*"));
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
