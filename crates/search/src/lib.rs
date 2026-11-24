//! Keyword search: a tokenizer and an in-memory BM25 index over text units.
//!
//! The index is a cache built from the text it is given, so it
//! can always be rebuilt. Semantic search is not here; it sits beside this index and is merged
//! with reciprocal rank fusion (see the nc-info-search skill).
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};
use unicode_properties::UnicodeEmoji;
use unicode_script::{Script, UnicodeScript};
use unicode_segmentation::UnicodeSegmentation;

use std::{collections::HashMap, ops::Range};

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

/// Produces normalized word tokens and exact emoji tokens. Matching folds Latin accents and
/// optional Arabic vowel marks, not the marks of unrelated scripts or the parts of an emoji.
pub fn tokenize(text: &str) -> Vec<String> {
    token_ranges(text).map(|(_, token)| token).collect()
}

/// Byte ranges of complete tokens in the original text that match any normalized query token.
pub fn match_ranges<'a>(text: &'a str, tokens: &'a [String]) -> impl Iterator<Item = Range<usize>> + 'a {
    token_ranges(text).filter_map(move |(range, token)| tokens.contains(&token).then_some(range))
}

fn token_ranges(text: &str) -> impl Iterator<Item = (Range<usize>, String)> + '_ {
    // Keep the cheap word splitter for ASCII notes and only segment graphemes
    // when a note actually needs Unicode handling. Both paths stream: the first snippet match
    // need not allocate or inspect tokens after it.
    let word_char = |c: char| c.is_ascii_alphanumeric();
    let mut ascii = text.is_ascii().then(|| text.split_inclusive(move |c| !word_char(c)));
    let mut ascii_offset = 0;
    let mut graphemes = text.grapheme_indices(true).peekable();
    std::iter::from_fn(move || {
        if let Some(segments) = ascii.as_mut() {
            for segment in segments {
                let start = ascii_offset;
                ascii_offset += segment.len();
                let word = segment.trim_end_matches(|c| !word_char(c));
                if !word.is_empty() {
                    return Some((start..start + word.len(), word.to_ascii_lowercase()));
                }
            }
            return None;
        }
        let mut word = String::new();
        let mut start = 0;
        while let Some(&(offset, grapheme)) = graphemes.peek() {
            let first = grapheme.chars().next().expect("graphemes are never empty");
            if is_emoji(first, grapheme) {
                if !word.is_empty() {
                    return Some((start..offset, word));
                }
                graphemes.next();
                // A variation selector, modifier, joiner, or flag component is part of the token.
                return Some((offset..offset + grapheme.len(), grapheme.to_string()));
            }
            graphemes.next();
            if first.is_alphanumeric() {
                if word.is_empty() {
                    start = offset;
                }
                append_word_grapheme(&mut word, first, grapheme);
            } else if !word.is_empty() {
                return Some((start..offset, word));
            }
        }
        (!word.is_empty()).then_some((start..text.len(), word))
    })
}

fn append_word_grapheme(word: &mut String, first: char, grapheme: &str) {
    if grapheme.is_ascii() {
        word.push(first.to_ascii_lowercase());
        return;
    }
    let script = first.script();
    match script {
        Script::Latin | Script::Arabic => {
            // Maddah and both forms of combining hamza can form distinct Arabic letters.
            let lexical_mark = |c| script == Script::Arabic && matches!(c, '\u{0653}'..='\u{0655}');
            word.extend(grapheme.nfd().filter(|&c| !is_combining_mark(c) || lexical_mark(c)).nfc().flat_map(char::to_lowercase));
        }
        _ => word.extend(grapheme.nfc().flat_map(char::to_lowercase)),
    }
}

fn is_emoji(first: char, grapheme: &str) -> bool {
    if first.is_ascii() {
        // ASCII digits, # and * are emoji bases only in a keycap sequence.
        matches!(first, '0'..='9' | '#' | '*') && grapheme.ends_with('\u{20e3}')
    } else {
        first.is_emoji_char()
    }
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
    fn words_are_lowercase_and_latin_accents_are_folded() {
        assert_eq!(tokenize("Sourdough, 6.00!"), vec!["sourdough", "6", "00"]);
        assert_eq!(tokenize("Café"), vec!["cafe"]);
        assert_eq!(tokenize("مرحبا بالعالم"), vec!["مرحبا", "بالعالم"]);
        assert!(tokenize(" -- ").is_empty());
    }

    #[test]
    fn emoji_tokens_preserve_exact_grapheme_sequences() {
        let s = "x😊👩🏽‍💻 🇨🇦 ✈️ ✈︎ 1️⃣ #️⃣ 1";
        let expected = ["x", "😊", "👩🏽‍💻", "🇨🇦", "✈️", "✈︎", "1️⃣", "#️⃣", "1"];
        assert_eq!(tokenize(s), expected);
        for (range, token) in token_ranges(s).skip(1) {
            assert_eq!(&s[range], token, "invalid byte range for {token}");
        }
        let index = Index::build(&[unit("exact", "👩🏽‍💻 🇨🇦 1️⃣ ✈️"), unit("different", "👩‍💻 🇨🇲 1 ✈︎")]);
        for query in ["👩🏽‍💻", "🇨🇦", "1️⃣", "✈️"] {
            let hits = index.search(query, 10);
            assert_eq!(hits.iter().map(|hit| hit.id.as_str()).collect::<Vec<_>>(), ["exact"], "{query}");
        }
    }

    #[test]
    fn accent_and_arabic_vowel_matching_does_not_remove_lexical_marks() {
        assert_eq!(tokenize("RéSuMé RÉSUMÉ Re\u{301}sume\u{301}"), ["resume"; 3]);
        assert_eq!(tokenize("السَّلَامُ عَلَيْكُمْ"), tokenize("السلام عليكم"));
        assert_eq!(tokenize("أُسْرَة"), tokenize("أسرة"));
        assert_ne!(tokenize("أُسْرَة"), tokenize("اسرة"));
        assert_ne!(tokenize("कि"), tokenize("क"));
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
