//! Which way a piece of text reads.
//!
//! The Unicode bidirectional algorithm picks a paragraph's direction from its first strong
//! character ([UAX #9], rules P2 and P3): the first letter that is itself left-to-right or
//! right-to-left decides, and digits, spaces and punctuation do not vote. That is what this
//! does. It is not the whole algorithm: reordering a mixed line is a separate job that
//! NoteCraft does not do yet (see `docs/gaps.md`).
//!
//! Deriving the direction from the text, rather than storing it, means it survives a copy,
//! a sync and an edit in another program, which is the point of keeping notes as plain files.

/// Which way a line reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Dir {
    #[default]
    Ltr,
    Rtl,
}

impl Dir {
    pub fn is_rtl(self) -> bool {
        self == Dir::Rtl
    }

    /// The value an HTML `dir` attribute would carry.
    pub fn id(self) -> &'static str {
        match self {
            Dir::Ltr => "ltr",
            Dir::Rtl => "rtl",
        }
    }
}

/// True for a character that is strongly right-to-left: the Arabic, Hebrew, Syriac, Thaana,
/// N'Ko, Samaritan, Mandaic and Adlam ranges, and the Arabic presentation forms.
fn strong_rtl(c: char) -> bool {
    matches!(c as u32,
        0x0590..=0x05FF   // Hebrew
        | 0x0600..=0x06FF // Arabic
        | 0x0700..=0x074F // Syriac
        | 0x0750..=0x077F // Arabic Supplement
        | 0x0780..=0x07BF // Thaana
        | 0x07C0..=0x07FF // N'Ko
        | 0x0800..=0x083F // Samaritan
        | 0x0840..=0x085F // Mandaic
        | 0x0860..=0x08FF // Syriac Supplement, Arabic Extended
        | 0xFB1D..=0xFDFF // Hebrew and Arabic presentation forms A
        | 0xFE70..=0xFEFF // Arabic presentation forms B
        | 0x10800..=0x10FFF
        | 0x1E800..=0x1EFFF)
}

/// The invisible mark that says a line reads right to left. It is a real character, strong
/// in the Unicode sense, so every editor and viewer that knows the algorithm obeys it, and a
/// page keeps its direction when it is copied, synced or opened somewhere else.
pub const RLM: char = '\u{200F}';
/// The same, the other way.
pub const LRM: char = '\u{200E}';

/// The direction written into the text by a mark at its start, if there is one. `None`
/// means nobody said, so the text itself decides.
pub fn explicit(text: &str) -> Option<Dir> {
    match text.chars().next() {
        Some(RLM) => Some(Dir::Rtl),
        Some(LRM) => Some(Dir::Ltr),
        _ => None,
    }
}

/// `text` with a direction mark at the start, replacing one already there. `None` takes the
/// mark off and lets the words decide again.
pub fn set_direction(text: &str, dir: Option<Dir>) -> String {
    let rest = without_mark(text);
    match dir {
        Some(Dir::Rtl) => format!("{RLM}{rest}"),
        Some(Dir::Ltr) => format!("{LRM}{rest}"),
        None => rest.to_string(),
    }
}

/// `text` without the direction mark at its start, which is what the reader sees anyway.
pub fn without_mark(text: &str) -> &str {
    text.strip_prefix(RLM).or_else(|| text.strip_prefix(LRM)).unwrap_or(text)
}

/// Which way a line reads: the mark at its start if it has one, else its first strong
/// character. Text with no strong character at all, such as "123" or "...", reads left to
/// right.
pub fn direction(text: &str) -> Dir {
    for c in text.chars() {
        if c == RLM || strong_rtl(c) {
            return Dir::Rtl;
        }
        if c == LRM {
            return Dir::Ltr;
        }
        // A letter that is not in a right-to-left range settles it the other way. Digits,
        // spaces, punctuation and other marks are neutral and are skipped.
        if c.is_alphabetic() {
            return Dir::Ltr;
        }
    }
    Dir::Ltr
}

/// Which way a line reads when the page it is on has a direction of its own: the line's
/// own mark or first strong letter wins, and text with nothing strong in it follows the
/// page instead of falling back to left to right.
pub fn direction_or(text: &str, page: Dir) -> Dir {
    let strong = text.chars().any(|c| c == RLM || c == LRM || strong_rtl(c) || c.is_alphabetic());
    if strong { direction(text) } else { page }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_letter_decides() {
        assert_eq!(direction("Hello"), Dir::Ltr);
        assert_eq!(direction("مرحبا"), Dir::Rtl);
        assert_eq!(direction("שלום"), Dir::Rtl);
        assert_eq!(direction("مرحبا world"), Dir::Rtl, "the first strong character is Arabic");
        assert_eq!(direction("Hello مرحبا"), Dir::Ltr);
    }

    #[test]
    fn digits_spaces_and_punctuation_do_not_vote() {
        assert_eq!(direction("123 مرحبا"), Dir::Rtl);
        assert_eq!(direction("  \t- مرحبا"), Dir::Rtl, "a list marker is neutral");
        assert_eq!(direction("## مرحبا"), Dir::Rtl, "a heading marker is neutral");
        assert_eq!(direction("1. Hello"), Dir::Ltr);
        assert_eq!(direction("123"), Dir::Ltr, "nothing strong at all");
        assert_eq!(direction(""), Dir::Ltr);
        assert_eq!(direction("...!?"), Dir::Ltr);
    }

    #[test]
    fn a_mark_at_the_start_overrules_the_words() {
        // An Arabic line the writer wants read left to right, and an English one the other way.
        let ltr = set_direction("مرحبا", Some(Dir::Ltr));
        assert_eq!(direction(&ltr), Dir::Ltr);
        assert_eq!(explicit(&ltr), Some(Dir::Ltr));
        assert_eq!(without_mark(&ltr), "مرحبا", "the mark is not part of the words");

        let rtl = set_direction("Hello", Some(Dir::Rtl));
        assert_eq!(direction(&rtl), Dir::Rtl);
        assert_eq!(rtl.chars().next(), Some(RLM));

        // Asking twice does not stack marks, and None goes back to letting the words decide.
        let twice = set_direction(&set_direction("Hello", Some(Dir::Rtl)), Some(Dir::Ltr));
        assert_eq!(twice.chars().filter(|c| *c == RLM || *c == LRM).count(), 1);
        assert_eq!(direction(&twice), Dir::Ltr);
        let off = set_direction(&twice, None);
        assert_eq!(off, "Hello");
        assert_eq!(explicit(&off), None);
        assert_eq!(direction("مرحبا"), Dir::Rtl, "with no mark, the words decide");
    }

    #[test]
    fn a_page_direction_only_decides_for_lines_that_say_nothing() {
        assert_eq!(direction_or("123", Dir::Rtl), Dir::Rtl, "nothing strong: the page decides");
        assert_eq!(direction_or("", Dir::Rtl), Dir::Rtl);
        assert_eq!(direction_or("- ", Dir::Rtl), Dir::Rtl, "an empty list item on an Arabic page");
        assert_eq!(direction_or("Hello", Dir::Rtl), Dir::Ltr, "the words win");
        assert_eq!(direction_or("مرحبا", Dir::Ltr), Dir::Rtl);
        assert_eq!(direction_or(&set_direction("Hello", Some(Dir::Rtl)), Dir::Ltr), Dir::Rtl, "the mark wins");
    }

    #[test]
    fn it_reads_the_whole_range_and_never_panics() {
        assert_eq!(direction("ߒߞߏ"), Dir::Rtl, "N'Ko");
        assert_eq!(direction("ﻣﺮﺣﺒﺎ"), Dir::Rtl, "Arabic presentation forms");
        assert_eq!(direction("𐤀"), Dir::Rtl, "Phoenician, in the supplementary range");
        assert_eq!(direction("ᚠ"), Dir::Ltr, "Runic is a left-to-right script");
        for s in ["\u{0}", "\u{FFFF}", "🙂", "\u{200F}Hello", "e\u{301}"] {
            let _ = direction(s);
        }
        assert!(Dir::Rtl.is_rtl() && !Dir::Ltr.is_rtl());
        assert_eq!((Dir::Rtl.id(), Dir::Ltr.id(), Dir::default()), ("rtl", "ltr", Dir::Ltr));
    }
}
