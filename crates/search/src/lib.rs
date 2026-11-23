//! Keyword search: a tokenizer and an in-memory BM25 index over text units.
//!
//! Pure Rust with no dependencies. The index is a cache built from the text it is given, so it
//! can always be rebuilt. Semantic search is not here; it sits beside this index and is merged
//! with reciprocal rank fusion (see the nc-info-search skill).
use unicode_normalization::UnicodeNormalization;

use std::collections::HashMap;

/// BM25 parameters: the usual defaults.
const K1: f64 = 1.2;
const B: f64 = 0.75;

/// A piece of text to index, identified by the caller (for example a page path and a kind).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    pub id: String,
    pub text: String,
}

/// One ranked result: the unit's id and its BM25 score (higher is better).
#[derive(Debug, Clone, PartialEq)]
pub struct Ranked {
    pub id: String,
    pub score: f64,
}

/// Splits text into lowercase words. A word is a run of letters or digits, so Arabic, Hebrew,
/// Persian and Urdu words are kept whole. Anything else separates words.
pub fn tokenize(text: &str) -> Vec<String> {
    // Composed form first, so 'e' + combining accent (macOS text) matches the composed 'é' a query types.
    let text: String = text.nfc().collect();
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(|word| word.chars().flat_map(char::to_lowercase).collect())
        .collect()
}

/// Byte ranges of complete words in the original text that match any normalized token.
pub fn match_ranges<'a>(text: &'a str, tokens: &'a [String]) -> impl Iterator<Item = std::ops::Range<usize>> + 'a {
    use unicode_normalization::char::is_combining_mark;

    let is_word = |c: char| c.is_alphanumeric() || is_combining_mark(c);
    let mut offset = 0;
    text.split_inclusive(move |c| !is_word(c)).filter_map(move |segment| {
        let start = offset;
        offset += segment.len();
        let word = segment.trim_end_matches(|c| !is_word(c));
        tokenize(word).iter().any(|token| tokens.contains(token)).then_some(start..start + word.len())
    })
}

/// An in-memory BM25 index over a fixed set of units.
#[derive(Debug, Clone)]
pub struct Index {
    ids: Vec<String>,
    lengths: Vec<usize>,
    average_length: f64,
    /// For each word, the units that contain it with their term counts.
    postings: HashMap<String, Vec<(usize, u32)>>,
}

impl Index {
    /// Builds the index. Units with no words are kept but can never match.
    pub fn build(units: &[Unit]) -> Index {
        let mut ids = Vec::with_capacity(units.len());
        let mut lengths = Vec::with_capacity(units.len());
        let mut postings: HashMap<String, Vec<(usize, u32)>> = HashMap::new();
        for (index, unit) in units.iter().enumerate() {
            let words = tokenize(&unit.text);
            lengths.push(words.len());
            ids.push(unit.id.clone());
            let mut counts: HashMap<String, u32> = HashMap::new();
            for word in words {
                *counts.entry(word).or_insert(0) += 1;
            }
            for (word, count) in counts {
                postings.entry(word).or_default().push((index, count));
            }
        }
        let total: usize = lengths.iter().sum();
        let average_length = if lengths.is_empty() { 0.0 } else { total as f64 / lengths.len() as f64 };
        Index { ids, lengths, average_length, postings }
    }

    /// Number of units in the index.
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    /// True when the index holds no units.
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Ranks the units that contain at least one query word, best first, at most `limit`.
    /// Ties keep the order the units were given, so the result is deterministic.
    pub fn search(&self, query: &str, limit: usize) -> Vec<Ranked> {
        // Each query word counts once, however often it is typed: 'bread bread' scores like 'bread'.
        let mut words = tokenize(query);
        words.sort();
        words.dedup();
        if words.is_empty() || limit == 0 || self.ids.is_empty() {
            return Vec::new();
        }
        let n = self.ids.len() as f64;
        let mut scores: HashMap<usize, f64> = HashMap::new();
        for word in &words {
            let Some(list) = self.postings.get(word) else { continue };
            let df = list.len() as f64;
            let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();
            for &(unit, tf) in list {
                let tf = f64::from(tf);
                let length = self.lengths[unit] as f64;
                let norm = K1 * (1.0 - B + B * length / self.average_length.max(f64::MIN_POSITIVE));
                *scores.entry(unit).or_insert(0.0) += idf * tf * (K1 + 1.0) / (tf + norm);
            }
        }
        let mut ranked: Vec<(usize, f64)> = scores.into_iter().collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        ranked.truncate(limit);
        ranked.into_iter().map(|(unit, score)| Ranked { id: self.ids[unit].clone(), score }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(id: &str, text: &str) -> Unit {
        Unit { id: id.to_string(), text: text.to_string() }
    }

    #[test]
    fn words_are_lowercase_letters_and_digits_and_non_latin_words_stay_whole() {
        assert_eq!(tokenize("Sourdough, 6.00!"), vec!["sourdough", "6", "00"]);
        assert_eq!(tokenize("Café"), vec!["café"]);
        assert_eq!(tokenize("مرحبا بالعالم"), vec!["مرحبا", "بالعالم"]);
        assert!(tokenize(" -- ").is_empty());
    }

    #[test]
    fn a_word_that_is_in_one_unit_finds_that_unit() {
        let index = Index::build(&[unit("receipt", "Sourdough 6.00 Flat white"), unit("minutes", "Book the room")]);
        let hits = index.search("sourdough", 10);
        assert_eq!(hits.iter().map(|h| h.id.as_str()).collect::<Vec<_>>(), vec!["receipt"]);
    }

    #[test]
    fn a_unit_with_the_rarer_word_ranks_first() {
        let index = Index::build(&[unit("common", "bread bread bread the the"), unit("rare", "bread sourdough")]);
        let hits = index.search("bread sourdough", 10);
        assert_eq!(hits[0].id, "rare", "the rarer word weighs more");
    }

    #[test]
    fn matching_is_case_insensitive_and_limited() {
        let index = Index::build(&[unit("a", "Alpha"), unit("b", "alpha"), unit("c", "ALPHA")]);
        assert_eq!(index.search("ALPHA", 2).len(), 2);
        assert_eq!(index.search("alpha", 10).len(), 3);
    }

    #[test]
    fn an_empty_query_or_index_finds_nothing() {
        let index = Index::build(&[unit("a", "text")]);
        assert!(index.search("", 10).is_empty());
        assert!(index.search("   ", 10).is_empty());
        assert!(index.search("text", 0).is_empty());
        assert!(Index::build(&[]).search("text", 10).is_empty());
    }

    #[test]
    fn ties_keep_the_order_the_units_were_given() {
        let index = Index::build(&[unit("first", "word"), unit("second", "word")]);
        let ids: Vec<String> = index.search("word", 10).into_iter().map(|h| h.id).collect();
        assert_eq!(ids, vec!["first", "second"]);
    }

    #[test]
    fn a_unit_with_no_words_never_matches() {
        let index = Index::build(&[unit("empty", "  "), unit("full", "word")]);
        assert_eq!(index.len(), 2);
        assert_eq!(index.search("word", 10).len(), 1);
    }

    #[test]
    fn a_word_typed_twice_scores_the_same_as_once() {
        let index = Index::build(&[unit("a", "bread and jam"), unit("b", "bread bread bread")]);
        let once = index.search("bread", 10);
        let twice = index.search("bread bread", 10);
        assert_eq!(once.len(), twice.len());
        for (a, b) in once.iter().zip(&twice) {
            assert_eq!(a.id, b.id);
            assert!((a.score - b.score).abs() < 1e-9, "{a:?} vs {b:?}");
        }
    }

    #[test]
    fn a_decomposed_accent_matches_the_composed_word_typed_in_a_query() {
        let index = Index::build(&[unit("cafe", "Cafe\u{301} au lait")]);
        let hits = index.search("café", 10);
        assert_eq!(hits.len(), 1, "the NFD text is found by the NFC query");
        assert_eq!(tokenize("Cafe\u{301}"), tokenize("Café"));
    }
}
