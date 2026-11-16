//! Tags on a line, the way OneNote marks one: a to-do box, an important star, a question.
//!
//! A tag has to live in the page file, and the file has to stay a plain Markdown or HTML
//! page that any other tool can read. So a tag is written as a marker word at the start of
//! the line, `#important `, and the app draws it as its icon instead of showing the word.
//! To-do is the exception: it is a task list item (`- [ ]`), which is what every Markdown
//! reader already understands.
//!
//! A person can define tags of their own (issue #259): a name, a symbol and a colour, kept in
//! their settings. A custom tag is written the same way, as a marker word made from its name
//! (`#followup `), so a page means the same thing in HTML and in CommonMark. The marker word
//! never changes once a tag exists: renaming a tag changes what the app calls it, and every
//! line that carries it keeps it. A line whose tag is not defined here (a tag that was removed,
//! or one made on another computer) keeps its marker as ordinary words, so nothing is lost and
//! defining the tag again brings it back.

use crate::color::NodeColor;
use std::sync::RwLock;

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
    /// A tag the person defined, by its marker word (see [`CustomTag`]).
    Custom(&'static str),
}

/// A tag the person defined: the marker word written in the page, what it is called, the
/// symbol drawn in front of the line and the colour of that symbol.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomTag {
    pub id: String,
    pub name: String,
    pub symbol: String,
    pub color: NodeColor,
}

/// The symbols a custom tag can show. Each one is in the app's fonts and reads at line height.
pub const SYMBOLS: [&str; 16] = ["★", "●", "■", "▲", "◆", "♥", "♦", "♣", "✓", "!", "?", "☎", "⚠", "♪", "☀", "→"];

static CUSTOM: RwLock<Vec<Defined>> = RwLock::new(Vec::new());
static NAMES: RwLock<Vec<&'static str>> = RwLock::new(Vec::new());

#[derive(Clone, Copy)]
struct Defined {
    id: &'static str,
    name: &'static str,
    symbol: &'static str,
    color: NodeColor,
}

/// The same text as a `'static` string, made once: a tag is `Copy`, so its words must live for
/// the whole run. The few names a person types are kept, never freed.
fn keep(text: &str) -> &'static str {
    if let Some(found) = NAMES.read().ok().and_then(|n| n.iter().copied().find(|k| *k == text)) {
        return found;
    }
    let kept: &'static str = Box::leak(text.to_string().into_boxed_str());
    if let Ok(mut names) = NAMES.write() {
        names.push(kept);
    }
    kept
}

/// Makes `list` the custom tags, in that order. Entries whose marker word is not a plain word,
/// is a built-in tag's or repeats an earlier one are left out.
pub fn set_custom(list: &[CustomTag]) {
    let mut out: Vec<Defined> = Vec::new();
    for c in list {
        let id = c.id.trim().to_ascii_lowercase();
        if !CustomTag::valid_id(&id) || out.iter().any(|d| d.id == id) {
            continue;
        }
        out.push(Defined { id: keep(&id), name: keep(c.name.trim()), symbol: keep(c.symbol.trim()), color: c.color });
    }
    if let Ok(mut custom) = CUSTOM.write() {
        *custom = out;
    }
}

/// The custom tags, in the order the person put them.
pub fn custom() -> Vec<CustomTag> {
    let custom = CUSTOM.read().map(|c| c.clone()).unwrap_or_default();
    custom.into_iter().map(|d| CustomTag { id: d.id.into(), name: d.name.into(), symbol: d.symbol.into(), color: d.color }).collect()
}

fn defined(id: &str) -> Option<Defined> {
    CUSTOM.read().ok()?.iter().copied().find(|d| d.id == id)
}

impl CustomTag {
    /// A new tag called `name`, with a marker word made from it that no other tag uses.
    pub fn named(name: &str, taken: &[CustomTag]) -> CustomTag {
        let base: String = name.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
        let base = if base.is_empty() { "tag".to_string() } else { base };
        let free = |id: &str| CustomTag::valid_id(id) && !taken.iter().any(|t| t.id == id);
        let id = if free(&base) { base } else { (2..).map(|n| format!("{base}{n}")).find(|id| free(id)).unwrap_or_default() };
        CustomTag { id, name: name.trim().to_string(), symbol: SYMBOLS[0].to_string(), color: NodeColor::Blue }
    }

    /// A marker word a custom tag may use: plain letters and digits, and not a built-in tag's.
    pub fn valid_id(id: &str) -> bool {
        !id.is_empty() && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()) && !Tag::ALL.iter().any(|t| t.id() == id)
    }

    /// The tags as one line of the settings file: `id|name|symbol|colour` joined by `;`, with
    /// `%`, `|`, `;` and line breaks escaped so any name survives.
    pub fn encode_list(list: &[CustomTag]) -> String {
        fn esc(text: &str) -> String {
            text.replace('%', "%25").replace('|', "%7C").replace(';', "%3B").replace('\n', "%0A").replace('\r', "%0D")
        }
        list.iter().map(|c| format!("{}|{}|{}|{}", esc(&c.id), esc(&c.name), esc(&c.symbol), c.color.id())).collect::<Vec<_>>().join(";")
    }

    /// Reads [`CustomTag::encode_list`]. Entries that do not read are skipped.
    pub fn decode_list(text: &str) -> Vec<CustomTag> {
        fn unesc(text: &str) -> String {
            text.replace("%0D", "\r").replace("%0A", "\n").replace("%3B", ";").replace("%7C", "|").replace("%25", "%")
        }
        text.split(';')
            .filter_map(|entry| {
                let mut parts = entry.split('|');
                let id = unesc(parts.next()?.trim());
                let name = unesc(parts.next()?);
                let symbol = unesc(parts.next().unwrap_or(""));
                let color = parts.next().and_then(NodeColor::parse).unwrap_or(NodeColor::Blue);
                CustomTag::valid_id(&id).then(|| CustomTag { name: if name.trim().is_empty() { id.clone() } else { name }, id, symbol, color })
            })
            .collect()
    }
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

    /// Every tag a line can carry: the built-in ones, then the person's own.
    pub fn all() -> Vec<Tag> {
        let mut out = Tag::ALL.to_vec();
        out.extend(CUSTOM.read().map(|c| c.iter().map(|d| Tag::Custom(d.id)).collect::<Vec<_>>()).unwrap_or_default());
        out
    }

    /// A custom tag's symbol and colour; `None` for a built-in tag, which has its own mark.
    pub fn custom_look(self) -> Option<(&'static str, NodeColor)> {
        match self {
            Tag::Custom(id) => defined(id).map(|d| (d.symbol, d.color)).or(Some(("", NodeColor::Gray))),
            _ => None,
        }
    }

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
            Tag::Custom(id) => id,
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
            Tag::Custom(id) => defined(id).map(|d| d.name).unwrap_or(id),
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
        Tag::ALL.into_iter().find(|t| t.id() == want).or_else(|| defined(&want).map(|d| Tag::Custom(d.id)))
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

    /// Issue #259: a tag the person defined is written and read like a built-in one, renaming
    /// it keeps the lines that carry it, and removing it leaves their words alone.
    #[test]
    fn a_custom_tag_reads_renames_and_is_never_lost() {
        let mine = CustomTag::named("Ask legal!", &[]);
        assert_eq!(mine.id, "asklegal");
        let clash = CustomTag::named("Idea", &[]);
        assert_eq!(clash.id, "idea2", "a built-in marker word is not taken");
        assert_eq!(CustomTag::named("Ask legal", std::slice::from_ref(&mine)).id, "asklegal2");
        assert_eq!(CustomTag::named("???", &[]).id, "tag");

        let saved = CustomTag::encode_list(&[CustomTag { name: "Odd | name; 100%".into(), ..mine.clone() }]);
        let back = CustomTag::decode_list(&saved);
        assert_eq!(back[0].name, "Odd | name; 100%", "any name survives the settings file");
        assert_eq!(back[0].id, "asklegal");
        assert!(CustomTag::decode_list("idea|Clash|★|red;Bad Id|x|★|red;;").is_empty(), "bad entries are skipped");

        set_custom(std::slice::from_ref(&mine));
        assert_eq!(Tag::split("#asklegal check the contract"), (Some(Tag::Custom("asklegal")), "check the contract"));
        assert_eq!(Tag::set("check it", Some(Tag::Custom("asklegal"))), "#asklegal check it");
        assert_eq!(Tag::Custom("asklegal").label(), "Ask legal!");
        assert!(Tag::all().contains(&Tag::Custom("asklegal")));
        assert_eq!(Tag::Custom("asklegal").custom_look(), Some(("★", NodeColor::Blue)));

        set_custom(&[CustomTag { name: "Legal review".into(), ..mine }]);
        assert_eq!(Tag::split("#asklegal check").0.map(Tag::label), Some("Legal review"), "a rename keeps the tag on the line");

        set_custom(&[]);
        assert_eq!(Tag::split("#asklegal check"), (None, "#asklegal check"), "a removed tag's marker stays as words");
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
