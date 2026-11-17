//! Logic for the font name and size boxes, with no egui in it, so it can move to a shared library
//! later without changes. Ported from WordCraft's `widgets/combo.rs` (font matching and recents)
//! and its font dialog (size check). Keep this file free of UI types.

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
