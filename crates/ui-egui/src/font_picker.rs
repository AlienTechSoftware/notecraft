//! Logic for the font name and size boxes, with no egui in it, so it can move to a shared library
//! later without changes. Ported from WordCraft's `widgets/combo.rs` (font matching and recents)
//! and its font dialog (size check). Keep this file free of UI types.

/// Separates the names of the recent list when it is saved. A family name cannot contain it.
const LIST_SEPARATOR: char = '|';

/// The saved recent list: names separated by [`LIST_SEPARATOR`], most recent first. Blank entries and
/// entries that could not be a family name are dropped.
pub fn parse_list(text: &str) -> Vec<String> {
    text.split(LIST_SEPARATOR).map(str::trim).filter(|name| !name.is_empty()).map(str::to_owned).take(RECENT_CAP).collect()
}

/// The recent list as it is saved.
pub fn format_list(names: &[String]) -> String {
    names.iter().filter(|name| !name.contains(LIST_SEPARATOR)).map(String::as_str).collect::<Vec<_>>().join(&LIST_SEPARATOR.to_string())
}

/// The most recent families kept in the Recently Used list.
pub const RECENT_CAP: usize = 8;

/// Indices of the `items` matching `query`, ignoring case: exact matches, then names starting with
/// it, then names containing it, each group in list order. A blank query matches everything.
/// (WordCraft's ranking; NoteCraft used list order only before this.)
pub fn matches(query: &str, items: &[String]) -> Vec<usize> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return (0..items.len()).collect();
    }
    let (mut exact, mut prefix, mut inner) = (Vec::new(), Vec::new(), Vec::new());
    for (i, item) in items.iter().enumerate() {
        let name = item.to_lowercase();
        if name == q {
            exact.push(i);
        } else if name.starts_with(&q) {
            prefix.push(i);
        } else if name.contains(&q) {
            inner.push(i);
        }
    }
    exact.extend(prefix);
    exact.extend(inner);
    exact
}

/// Records `name` as the most recently used family: it moves to the front, appears once, and the
/// list keeps at most [`RECENT_CAP`] names.
pub fn remember(recent: &mut Vec<String>, name: &str) {
    recent.retain(|n| !n.eq_ignore_ascii_case(name));
    recent.insert(0, name.to_owned());
    recent.truncate(RECENT_CAP);
}

/// True when `name` is not among the installed families, so the page will show a fallback font.
/// An empty name is not missing; it means "the default".
pub fn is_missing(name: &str, installed: &[String]) -> bool {
    let name = name.trim();
    !name.is_empty() && !installed.iter().any(|f| f.eq_ignore_ascii_case(name))
}

/// A typed size the page can hold: a finite whole number of points from `sizes`. Text such as
/// "NaN", "inf" or "1.5" is refused, so a non-finite size never reaches the layout.
pub fn size_is_usable(text: &str, sizes: &[u8]) -> bool {
    text.trim().parse::<u8>().is_ok_and(|size| sizes.contains(&size))
}

#[cfg(test)]
mod persist_tests {
    use super::*;

    #[test]
    fn the_recent_list_round_trips_in_order() {
        let names = vec!["Georgia".to_owned(), "Arial Black".to_owned()];
        assert_eq!(parse_list(&format_list(&names)), names);
    }

    #[test]
    fn a_damaged_saved_list_keeps_only_usable_names() {
        assert_eq!(parse_list(" | Georgia ||  | Verdana |"), vec!["Georgia".to_owned(), "Verdana".to_owned()]);
        assert!(parse_list("").is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn a_blank_query_matches_every_family_in_order() {
        let items = names(&["Arial", "Georgia", "Verdana"]);
        assert_eq!(matches("  ", &items), vec![0, 1, 2]);
    }

    #[test]
    fn exact_matches_come_first_then_prefixes_then_the_rest() {
        let items = names(&["Courier New", "Arial Black", "Arial", "Arial Narrow", "Carial"]);
        // "arial": exact "Arial" (2), prefixes "Arial Black" (1) and "Arial Narrow" (3), then
        // "Carial" (4) because it only contains the query.
        assert_eq!(matches("arial", &items), vec![2, 1, 3, 4]);
    }

    #[test]
    fn matching_ignores_case_and_surrounding_spaces() {
        let items = names(&["Georgia", "Verdana"]);
        assert_eq!(matches("  GEOR ", &items), vec![0]);
    }

    #[test]
    fn a_query_with_no_match_lists_nothing() {
        let items = names(&["Arial"]);
        assert!(matches("zzz", &items).is_empty());
    }

    #[test]
    fn the_last_family_used_comes_first_and_appears_once() {
        let mut recent = Vec::new();
        remember(&mut recent, "Arial");
        remember(&mut recent, "Georgia");
        remember(&mut recent, "arial");
        assert_eq!(recent, names(&["arial", "Georgia"]));
    }

    #[test]
    fn the_recent_list_keeps_only_the_latest_names() {
        let mut recent = Vec::new();
        for i in 0..(RECENT_CAP + 3) {
            remember(&mut recent, &format!("Font {i}"));
        }
        assert_eq!(recent.len(), RECENT_CAP);
        assert_eq!(recent[0], format!("Font {}", RECENT_CAP + 2));
    }

    #[test]
    fn a_family_that_is_not_installed_is_missing_but_the_default_is_not() {
        let installed = names(&["Arial", "Georgia"]);
        assert!(is_missing("Aptos", &installed));
        assert!(!is_missing("arial", &installed));
        assert!(!is_missing("", &installed));
    }

    #[test]
    fn only_whole_sizes_from_the_list_are_usable() {
        let sizes = [8u8, 11, 12, 72];
        assert!(size_is_usable(" 12 ", &sizes));
        assert!(!size_is_usable("NaN", &sizes));
        assert!(!size_is_usable("inf", &sizes));
        assert!(!size_is_usable("10.5", &sizes));
        assert!(!size_is_usable("13", &sizes));
    }
}

/// A script the family list is grouped by. A font belongs under each script whose sample
/// character it draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Script {
    Latin,
    Greek,
    Cyrillic,
    Arabic,
    Hebrew,
    Devanagari,
    Thai,
    Han,
    Kana,
    Hangul,
}

impl Script {
    /// Every script, in the order the list shows them.
    pub const ALL: [Script; 10] = [
        Script::Latin,
        Script::Greek,
        Script::Cyrillic,
        Script::Arabic,
        Script::Hebrew,
        Script::Devanagari,
        Script::Thai,
        Script::Han,
        Script::Kana,
        Script::Hangul,
    ];

    /// One letter a font of this script must draw.
    pub fn sample(self) -> char {
        match self {
            Script::Latin => 'A',
            Script::Greek => '\u{03A9}',
            Script::Cyrillic => '\u{0416}',
            Script::Arabic => '\u{0627}',
            Script::Hebrew => '\u{05D0}',
            Script::Devanagari => '\u{0915}',
            Script::Thai => '\u{0E01}',
            Script::Han => '\u{4E2D}',
            Script::Kana => '\u{3042}',
            Script::Hangul => '\u{D55C}',
        }
    }
}

/// The scripts a font covers, judged by whether it draws each script's sample character.
pub fn scripts_of(has_glyph: impl Fn(char) -> bool) -> Vec<Script> {
    Script::ALL.into_iter().filter(|script| has_glyph(script.sample())).collect()
}

/// The list grouped by script: each script with the families that cover it, in list order.
/// Scripts with no family are left out. A family covering several scripts appears under each.
pub fn sections(families: &[String], scripts_of_family: impl Fn(&str) -> Vec<Script>) -> Vec<(Script, Vec<String>)> {
    let covered: Vec<(&String, Vec<Script>)> = families.iter().map(|name| (name, scripts_of_family(name))).collect();
    Script::ALL
        .into_iter()
        .filter_map(|script| {
            let names: Vec<String> = covered.iter().filter(|(_, scripts)| scripts.contains(&script)).map(|(name, _)| (*name).clone()).collect();
            (!names.is_empty()).then_some((script, names))
        })
        .collect()
}

#[cfg(test)]
mod section_tests {
    use super::*;

    #[test]
    fn a_font_is_listed_under_each_script_it_covers() {
        let families = vec!["Mixed".to_owned(), "Latin only".to_owned()];
        let grouped = sections(&families, |name| match name {
            "Mixed" => vec![Script::Latin, Script::Arabic],
            _ => vec![Script::Latin],
        });
        assert_eq!(grouped, vec![(Script::Latin, vec!["Mixed".to_owned(), "Latin only".to_owned()]), (Script::Arabic, vec!["Mixed".to_owned()])]);
    }

    #[test]
    fn a_script_with_no_font_gets_no_section() {
        let families = vec!["Latin only".to_owned()];
        let grouped = sections(&families, |_| vec![Script::Latin]);
        assert_eq!(grouped.len(), 1);
        assert_eq!(grouped[0].0, Script::Latin);
    }

    #[test]
    fn scripts_are_read_from_their_sample_characters() {
        assert_eq!(scripts_of(|c| c == 'A' || c == '\u{0627}'), vec![Script::Latin, Script::Arabic]);
        assert!(scripts_of(|_| false).is_empty());
    }
}
