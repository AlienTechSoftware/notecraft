//! Tags on a line, the way OneNote marks one: a to-do box, an important star, a question.
//!
//! A tag has to live in the page file, and the file has to stay a plain Markdown or HTML
//! page that any other tool can read. So a tag is written as a marker word at the start of
//! the line, `#important `, and the app draws it as its icon instead of showing the word.
//! To-do is the exception: it is a task list item (`- [ ]`), which is what every Markdown
//! reader already understands.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tag {
    ToDo,
    Important,
    Question,
    Remember,
    Definition,
    Highlight,
    Contact,
    Address,
    Phone,
    Website,
    Idea,
    Critical,
}

impl Tag {
    pub const ALL: [Tag; 12] = [
        Tag::ToDo,
        Tag::Important,
        Tag::Question,
        Tag::Remember,
        Tag::Definition,
        Tag::Highlight,
        Tag::Contact,
        Tag::Address,
        Tag::Phone,
        Tag::Website,
        Tag::Idea,
        Tag::Critical,
    ];

    /// The word written in the page, without its `#`.
    pub fn id(self) -> &'static str {
        match self {
            Tag::ToDo => "todo",
            Tag::Important => "important",
            Tag::Question => "question",
            Tag::Remember => "remember",
            Tag::Definition => "definition",
            Tag::Highlight => "highlight",
            Tag::Contact => "contact",
            Tag::Address => "address",
            Tag::Phone => "phone",
            Tag::Website => "website",
            Tag::Idea => "idea",
            Tag::Critical => "critical",
        }
    }

    /// What the menu calls it, in OneNote's words.
    pub fn label(self) -> &'static str {
        match self {
            Tag::ToDo => "To Do",
            Tag::Important => "Important",
            Tag::Question => "Question",
            Tag::Remember => "Remember for later",
            Tag::Definition => "Definition",
            Tag::Highlight => "Highlight",
            Tag::Contact => "Contact",
            Tag::Address => "Address",
            Tag::Phone => "Phone number",
            Tag::Website => "Web site to visit",
            Tag::Idea => "Idea",
            Tag::Critical => "Critical",
        }
    }

    /// The keyboard shortcut OneNote gives it, where it has one.
    pub fn shortcut(self) -> Option<&'static str> {
        match self {
            Tag::ToDo => Some("Ctrl+1"),
            Tag::Important => Some("Ctrl+2"),
            Tag::Question => Some("Ctrl+3"),
            Tag::Remember => Some("Ctrl+4"),
            Tag::Definition => Some("Ctrl+5"),
            Tag::Highlight => Some("Ctrl+6"),
            Tag::Contact => Some("Ctrl+7"),
            Tag::Address => Some("Ctrl+8"),
            Tag::Phone => Some("Ctrl+9"),
            _ => None,
        }
    }

    /// The text written at the start of the line, with its trailing space.
    pub fn marker(self) -> String {
        format!("#{} ", self.id())
    }

    pub fn parse(id: &str) -> Option<Tag> {
        let want = id.trim().trim_start_matches('#').to_ascii_lowercase();
        Tag::ALL.into_iter().find(|t| t.id() == want)
    }

    /// The tag at the start of `text`, and the rest of the line without it. A direction
    /// mark may sit before the tag marker; it is invisible and does not hide the tag.
    pub fn split(text: &str) -> (Option<Tag>, &str) {
        let Some(rest) = crate::text::without_mark(text).strip_prefix('#') else { return (None, text) };
        let word: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
        let Some(tag) = Tag::parse(&word) else { return (None, text) };
        let after = rest.get(word.len()..).unwrap_or("");
        // A marker is a whole word: `#ideas` is ordinary text, `#idea ` is a tag.
        match after.strip_prefix(' ') {
            Some(line) => (Some(tag), line),
            None if after.is_empty() => (Some(tag), ""),
            None => (None, text),
        }
    }

    /// `text` with this tag at the start, replacing any tag already there. `None` removes
    /// the tag and leaves the words. A direction mark on the line stays in front of it.
    pub fn set(text: &str, tag: Option<Tag>) -> String {
        let mark: String = crate::text::explicit(text).map(|d| if d.is_rtl() { crate::text::RLM } else { crate::text::LRM }).into_iter().collect();
        let (_, rest) = Tag::split(text);
        // A line with no tag comes back whole, mark and all; the mark is put on once, here.
        let rest = crate::text::without_mark(rest);
        match tag {
            Some(t) => format!("{mark}{}{rest}", t.marker()),
            None => format!("{mark}{rest}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tag_and_a_direction_mark_live_together() {
        use crate::text::{self, Dir};
        let line = text::set_direction("مرحبا", Some(Dir::Rtl));
        let tagged = Tag::set(&line, Some(Tag::Important));
        assert_eq!(text::direction(&tagged), Dir::Rtl, "the mark stays in front");
        assert_eq!(Tag::split(&tagged).0, Some(Tag::Important), "and does not hide the tag");
        assert_eq!(Tag::split(&tagged).1, "مرحبا");
        let off = Tag::set(&tagged, None);
        assert_eq!(off, line, "taking the tag off leaves the mark and the words");
    }

    #[test]
    fn ids_labels_and_markers_round_trip() {
        for t in Tag::ALL {
            assert_eq!(Tag::parse(t.id()), Some(t));
            assert_eq!(Tag::parse(&format!("#{}", t.id())), Some(t));
            assert!(!t.label().is_empty());
            assert_eq!(t.marker(), format!("#{} ", t.id()));
        }
        assert_eq!(Tag::parse("nonsense"), None);
        assert_eq!(Tag::parse(""), None);
        // Every id and every label is its own.
        let ids: std::collections::HashSet<&str> = Tag::ALL.iter().map(|t| t.id()).collect();
        assert_eq!(ids.len(), Tag::ALL.len());
        let labels: std::collections::HashSet<&str> = Tag::ALL.iter().map(|t| t.label()).collect();
        assert_eq!(labels.len(), Tag::ALL.len());
    }

    #[test]
    fn a_marker_at_the_start_of_a_line_is_the_tag() {
        assert_eq!(Tag::split("#important ship it"), (Some(Tag::Important), "ship it"));
        assert_eq!(Tag::split("#idea"), (Some(Tag::Idea), ""));
        assert_eq!(Tag::split("plain words"), (None, "plain words"));
        // A word that only starts like a tag is ordinary text.
        assert_eq!(Tag::split("#ideas for later"), (None, "#ideas for later"));
        assert_eq!(Tag::split("#"), (None, "#"));
        assert_eq!(Tag::split(""), (None, ""));
        // A tag in the middle of a line is text, not a tag.
        assert_eq!(Tag::split("see #idea here"), (None, "see #idea here"));
    }

    #[test]
    fn setting_a_tag_replaces_the_one_that_is_there() {
        assert_eq!(Tag::set("ship it", Some(Tag::Important)), "#important ship it");
        assert_eq!(Tag::set("#important ship it", Some(Tag::Question)), "#question ship it");
        assert_eq!(Tag::set("#important ship it", None), "ship it");
        assert_eq!(Tag::set("ship it", None), "ship it");
        assert_eq!(Tag::set("", Some(Tag::Idea)), "#idea ");
        // The words keep their own marks.
        assert_eq!(Tag::set("**bold** words", Some(Tag::Idea)), "#idea **bold** words");
    }
}
