//! Kashida: justifying a line of Arabic by stretching the joins inside its words.
//!
//! A line of Latin is justified by widening the spaces between the words. Arabic was never
//! written that way. The scribe lengthens the stroke that joins two letters - the kashida,
//! or tatweel - so the line reaches the margin with its words still evenly spaced and the
//! colour of the page even. Widening the spaces instead pulls the words apart and leaves
//! the rivers of white that Arabic readers complain about in every word processor.
//!
//! As far as we can tell, no note taking application does this: OneNote, Obsidian and
//! Notion all justify Arabic by stretching the spaces.
//!
//! **Where a kashida may go.** Only between two letters that really join: the one before
//! has to connect forwards, and the one after has to accept a connection. Vowel marks are
//! transparent - each hangs on the letter before it - so the tatweel goes after the marks
//! and immediately before the next letter, which leaves every mark on the letter it
//! belongs to. The joins inside the lam-alef ligature and inside the name الله are never
//! stretched; they are drawn as one shape and pulling them apart breaks it.
//!
//! **Which join to stretch.** Not every join looks right stretched. The ranking in
//! [`rank_of`] is the classical one that Internet Explorer documented and that Word,
//! InDesign and LibreOffice follow, as it is written out in the documentation of the
//! `kashida-rs` crate (<https://docs.rs/kashida-rs>). Sakkal's notes on Arabic
//! justification (<https://www.sakkal.com/type/sakkal_on_justification_2010.pdf>) add the
//! two rules about spreading: in Naskh, at most one kashida to a word, and spread them
//! across the words of the line rather than piling them into one. [`justify`] does both.
//!
//! **What this does not know.** The right answer depends on the face: Nastaliq takes
//! elongation in far fewer places than Naskh, and Ruqaa in none at all. What is here is the
//! Naskh ranking, which is what the bundled Noto Naskh Arabic wants. Calligraphers also
//! disagree about the details, so this is a ranking, not a law. The other thing it cannot
//! do is stretch by a fraction: with a plain font the only lever is repeating the tatweel
//! character, so the stretch comes in steps of one tatweel. A face with a `JSTF` table or a
//! variable elongation axis would do it smoothly, and that is a later question.

/// The tatweel, U+0640: the character that *is* the stretched join.
pub const TATWEEL: char = '\u{0640}';

/// How many tatweels one join may take before the word starts to look silly.
const MAX_PER_SPOT: usize = 6;

/// How a letter joins the letters on each side of it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Join {
    /// Joins on both sides: most Arabic letters.
    Dual,
    /// Joins only to the letter before it: alef, dal, reh, waw and their family.
    Right,
    /// Joins to nothing, and breaks the word: hamza, and anything that is not a letter.
    None,
    /// A vowel or other combining mark. It hangs on the letter before it and does not break
    /// the join across it.
    Transparent,
}

/// The joining class of one character.
///
/// This is the Arabic block and the common Persian and Urdu letters, which is what the
/// bundled faces cover. Anything else - Latin, digits, punctuation, a space - is [`None`],
/// which both ends a word and keeps a tatweel away from it.
fn joining(c: char) -> Join {
    match c {
        // Harakat, the superscript alef, and the Quranic annotation marks: each hangs on
        // the letter before it.
        '\u{0610}'..='\u{061A}'
        | '\u{064B}'..='\u{065F}'
        | '\u{0670}'
        | '\u{06D6}'..='\u{06DC}'
        | '\u{06DF}'..='\u{06E8}'
        | '\u{06EA}'..='\u{06ED}' => Join::Transparent,
        // Letters that join only to the letter before them, and so end the connection:
        // the alef family, teh marbuta, dal, thal, reh, zain, waw, and the Urdu dal and
        // reh with their marks.
        'آ' | 'أ' | 'ؤ' | 'إ' | 'ا' | 'ة' | 'د' | 'ذ' | 'ر' | 'ز' | 'و' | 'ٱ' | 'ڈ' | 'ڑ' | 'ژ' | 'ے' => Join::Right,
        // Letters that join on both sides: most of the alphabet, the tatweel itself, and
        // the Persian and Urdu letters the bundled faces cover.
        '\u{0620}'
        | 'ئ'
        | 'ب'
        | '\u{062A}'..='\u{062E}'
        | '\u{0633}'..='\u{063A}'
        | '\u{0640}'..='\u{0647}'
        | 'ى'
        | 'ي'
        | 'ٹ'
        | 'پ'
        | 'چ'
        | 'ک'
        | 'گ'
        | 'ں'
        | 'ھ'
        | 'ہ'
        | 'ی' => Join::Dual,
        _ => Join::None,
    }
}

/// One place a kashida could go, and how good a place it is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Spot {
    /// The byte in the line where the tatweel goes.
    pub at: usize,
    /// 1 is the best place to stretch, 8 the last resort. See [`rank_of`].
    pub rank: u8,
    /// Which word of the line this join is in, counted from the start of the line. Used to
    /// spread the kashidas instead of piling them into one word.
    pub word: usize,
}

/// A letter of the line, with the marks that hang on it already skipped past.
struct Letter {
    ch: char,
    /// The byte the letter starts at.
    start: usize,
    /// Which word it belongs to.
    word: usize,
    /// True when no letter follows it in its word.
    last: bool,
}

/// Splits a line into its letters, dropping the marks and remembering where the words are.
fn letters(line: &str) -> Vec<Letter> {
    let mut out: Vec<Letter> = Vec::new();
    let mut word = 0usize;
    let mut in_word = false;
    for (at, ch) in line.char_indices() {
        match joining(ch) {
            Join::Transparent => {}
            Join::None => {
                if in_word {
                    word += 1;
                    in_word = false;
                }
            }
            _ => {
                if !in_word {
                    in_word = true;
                }
                out.push(Letter { ch, start: at, word, last: false });
            }
        }
    }
    // The last letter of each word is the one no other letter of that word follows.
    for i in 0..out.len() {
        let last = out.get(i + 1).is_none_or(|next| next.word != out[i].word);
        if let Some(l) = out.get_mut(i) {
            l.last = last;
        }
    }
    out
}

/// Seen and sad: the letters whose long flat tooth takes elongation best.
fn is_seen(c: char) -> bool {
    matches!(c, 'س' | 'ش' | 'ص' | 'ض')
}

/// The beh shape: the letters that are a tooth with a dot.
fn is_beh(c: char) -> bool {
    matches!(c, 'ب' | 'ت' | 'ث' | 'ن' | 'ي' | 'پ' | 'ٹ' | 'ی')
}

/// How good a join is to stretch, 1 being the best.
///
/// The order is the classical one (see the module note): an elongation the author wrote
/// themselves, then the seen tooth, then a short list of final letters whose entry stroke
/// is long enough to carry the extra, then anything else final, and a join in the middle of
/// a word only as a last resort.
pub fn rank_of(before: char, after: char, after_is_final: bool, after_that: Option<char>) -> u8 {
    if before == TATWEEL || after == TATWEEL {
        return 1;
    }
    if is_seen(before) {
        return 2;
    }
    if after_is_final {
        return match after {
            'ة' | 'ه' | 'د' | 'ذ' | 'ھ' | 'ہ' => 3,
            'ا' | 'أ' | 'إ' | 'آ' | 'ٱ' | 'ط' | 'ظ' | 'ل' | 'ك' | 'ک' | 'گ' => 4,
            'و' | 'ع' | 'غ' | 'ق' | 'ف' => 6,
            _ => 7,
        };
    }
    // A beh-shaped letter in the middle of a word, followed by one that sweeps below the
    // line: the pair is wide enough that the join before it can carry a kashida.
    if is_beh(after) && after_that.is_some_and(|c| matches!(c, 'ر' | 'ز' | 'ي' | 'ى' | 'ے' | 'ڑ' | 'ژ')) {
        return 5;
    }
    8
}

/// True for the two joins inside the name الله, which is drawn as one shape.
fn inside_allah(ls: &[Letter], i: usize) -> bool {
    // The join between `ls[i]` and `ls[i + 1]`, where the word is لله or الله.
    let word = ls[i].word;
    let same: Vec<char> = ls.iter().filter(|l| l.word == word).map(|l| l.ch).collect();
    let is_name = matches!(same.as_slice(), ['ل', 'ل', 'ه'] | ['ا', 'ل', 'ل', 'ه']);
    if !is_name {
        return false;
    }
    // Every join in that word is inside the shape.
    ls.get(i + 1).is_some_and(|next| next.word == word)
}

/// Every place in the line where a kashida could go, in the order they appear.
pub fn spots(line: &str) -> Vec<Spot> {
    let ls = letters(line);
    let mut out = Vec::new();
    for i in 0..ls.len() {
        let (Some(a), Some(b)) = (ls.get(i), ls.get(i + 1)) else { continue };
        if a.word != b.word {
            continue;
        }
        if joining(a.ch) != Join::Dual || joining(b.ch) == Join::None {
            continue;
        }
        // The lam-alef ligature and the name of God are single shapes, not two letters
        // with a join between them.
        if a.ch == 'ل' && matches!(b.ch, 'ا' | 'أ' | 'إ' | 'آ' | 'ٱ') {
            continue;
        }
        if inside_allah(&ls, i) {
            continue;
        }
        let after_that = ls.get(i + 2).filter(|c| c.word == b.word).map(|c| c.ch);
        out.push(Spot { at: b.start, rank: rank_of(a.ch, b.ch, b.last, after_that), word: b.word });
    }
    out
}

/// The spots in the order a line should use them: best rank first, and never two in the
/// same word until every word has had one.
///
/// This is Sakkal's spreading rule. A line that puts three kashidas into one word and none
/// into the rest is worse than no justification at all.
pub fn order(line: &str) -> Vec<Spot> {
    let all = spots(line);
    let mut rounds: Vec<Vec<Spot>> = Vec::new();
    let mut left: Vec<Spot> = all;
    while !left.is_empty() {
        let mut taken: Vec<Spot> = Vec::new();
        let mut rest: Vec<Spot> = Vec::new();
        for s in left {
            if taken.iter().any(|t| t.word == s.word) {
                rest.push(s);
            } else {
                taken.push(s);
            }
        }
        taken.sort_by_key(|s| (s.rank, s.at));
        rounds.push(taken);
        left = rest;
    }
    rounds.into_iter().flatten().collect()
}

/// Writes the line out with `counts[i]` tatweels at `order[i]`.
fn build(line: &str, order: &[Spot], counts: &[usize]) -> String {
    let mut out = String::with_capacity(line.len() + counts.iter().sum::<usize>() * 2);
    let mut at = 0usize;
    let mut pairs: Vec<(usize, usize)> = order.iter().zip(counts).map(|(s, n)| (s.at, *n)).filter(|(_, n)| *n > 0).collect();
    pairs.sort_unstable();
    for (byte, n) in pairs {
        if byte < at || byte > line.len() || !line.is_char_boundary(byte) {
            continue;
        }
        out.push_str(&line[at..byte]);
        for _ in 0..n {
            out.push(TATWEEL);
        }
        at = byte;
    }
    out.push_str(&line[at..]);
    out
}

/// Stretches one line towards `target` points wide by elongating its joins.
///
/// `measure` is how wide a string is drawn; the caller owns the font, so this crate does
/// not have to. Tatweels are added one at a time, best join first and spread across the
/// words, until the line reaches the target or there is nowhere decent left to stretch.
/// The result never overshoots by more than it would undershoot: whichever of the last two
/// tries lands closer to the target is the one returned.
///
/// A line with no Arabic in it, or with no join worth stretching, comes back unchanged -
/// the caller then has nothing but the spaces, which is where it started.
pub fn justify(line: &str, target: f32, measure: &mut dyn FnMut(&str) -> f32) -> String {
    let order = order(line);
    if order.is_empty() || !target.is_finite() {
        return line.to_string();
    }
    if measure(line) >= target {
        return line.to_string();
    }
    let mut counts = vec![0usize; order.len()];
    let mut best = line.to_string();
    for _ in 0..MAX_PER_SPOT {
        for i in 0..order.len() {
            counts[i] += 1;
            let next = build(line, &order, &counts);
            let width = measure(&next);
            if width >= target {
                // The first try that reaches the target. Keep it only if it is closer than
                // the one before, so a wide tatweel does not push the line past the margin
                // when stopping short was nearer.
                let over = width - target;
                let under = target - measure(&best);
                return if over <= under { next } else { best };
            }
            best = next;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_of(spots: &[Spot], line: &str) -> Vec<String> {
        spots.iter().map(|s| format!("{}|{} r{}", &line[..s.at], &line[s.at..], s.rank)).collect()
    }

    /// A tatweel only ever goes between two letters that really join.
    #[test]
    fn a_kashida_only_goes_where_the_letters_join() {
        // كتاب: the join after kaf (ك-ت) and after teh (ت-ا) are both real; alef joins
        // nothing after it, so there is no spot before the end.
        let line = "كتاب";
        let found = spots(line);
        assert_eq!(found.len(), 2, "{:?}", text_of(&found, line));
        // Nothing after the alef, which only joins backwards.
        assert!(found.iter().all(|s| s.at < line.len()));
        // A word of letters that never join forwards has no spot at all.
        assert!(spots("ودار").iter().all(|s| s.at > 0), "no spot before a waw that joins nothing forwards");
        assert_eq!(spots("زورد"), vec![], "every letter here joins only backwards");
        // Latin and digits are not stretched.
        assert_eq!(spots("Hello 2026"), vec![]);
    }

    /// The tatweel goes after a letter's vowel marks, never between the letter and its mark.
    #[test]
    fn a_kashida_never_splits_a_letter_from_its_vowel() {
        let line = "كِتَاب"; // kaf with kasra, teh with fatha, alef, beh
        for s in spots(line) {
            // The tatweel goes in front of the next letter, which is *after* the marks of
            // the letter before it: that is what keeps each mark on its own letter.
            let at = line[s.at..].chars().next().unwrap_or(' ');
            assert_ne!(joining(at), Join::Transparent, "a tatweel would land between a letter and its mark at {}", s.at);
            assert_ne!(joining(at), Join::None, "a tatweel would land beside something that is not a letter");
        }
        // And the marks are still there afterwards.
        let out = build(line, &order(line), &vec![1; order(line).len()]);
        assert!(out.contains('\u{0650}') && out.contains('\u{064E}'), "{out}");
    }

    /// Two shapes that are drawn as one are never pulled apart.
    #[test]
    fn the_lam_alef_and_the_name_of_god_are_left_alone() {
        // لا is one glyph: no join inside it.
        assert_eq!(spots("لا"), vec![]);
        // In قلادة the lam-alef is still one shape, so the only spots are around it.
        let line = "قلادة";
        assert!(spots(line).iter().all(|s| !line[s.at..].starts_with('ا')), "{:?}", text_of(&spots(line), line));
        // الله is one shape all the way through.
        assert_eq!(spots("الله"), vec![]);
        assert_eq!(spots("لله"), vec![]);
        // But a word that merely looks similar is stretched as usual.
        assert!(!spots("اللهم").is_empty());
    }

    /// The seen tooth is the best place to stretch, and the join before a final letter
    /// comes before a join in the middle of a word.
    #[test]
    fn the_ranking_prefers_the_seen_tooth_then_the_last_join() {
        // مسار: the join after the seen is rank 2.
        let line = "مسار";
        let best = spots(line).into_iter().min_by_key(|s| s.rank).expect("a spot");
        assert_eq!(best.rank, 2, "{:?}", text_of(&spots(line), line));
        assert!(line[best.at..].starts_with('ا'), "the stretch is the seen's own tooth: {:?}", &line[best.at..]);
        // كاتب: no seen. The join before the final beh is a final join, which beats the
        // one in the middle of the word.
        let line = "كاتب";
        let found = spots(line);
        let last = found.iter().max_by_key(|s| s.at).expect("a spot");
        let first = found.iter().min_by_key(|s| s.at).expect("a spot");
        assert!(last.rank < first.rank, "{:?}", text_of(&found, line));
        // كتاب is the case where there is no final join to be had: the alef before the
        // last letter joins nothing forwards, so both of its joins are middling ones.
        assert!(spots("كتاب").iter().all(|s| s.rank == 8), "{:?}", text_of(&spots("كتاب"), "كتاب"));
        // A tatweel the author typed is the first place to grow.
        let line = "كتــاب";
        assert_eq!(spots(line).into_iter().map(|s| s.rank).min(), Some(1));
    }

    /// The kashidas spread across the words of the line before any word takes a second.
    #[test]
    fn the_kashidas_spread_across_the_words() {
        let line = "كتاب مسار جديد";
        let got = order(line);
        let words: Vec<usize> = got.iter().take(3).map(|s| s.word).collect();
        let mut sorted = words.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 3, "the first three go to three different words: {words:?}");
    }

    /// Stretching reaches the target width, and stops rather than overshooting far.
    #[test]
    fn a_line_is_stretched_to_the_width_it_was_given() {
        let line = "كتاب مسار جديد في المكتبة";
        // A measure that is the character count: enough to test the search itself.
        let mut measure = |s: &str| s.chars().count() as f32;
        let plain = measure(line);
        let out = justify(line, plain + 6.0, &mut measure);
        assert_eq!(measure(&out), plain + 6.0, "{out}");
        assert_eq!(out.chars().filter(|c| *c == TATWEEL).count(), 6);
        // The words are all still there, in order, with nothing but tatweels added.
        let stripped: String = out.chars().filter(|c| *c != TATWEEL).collect();
        assert_eq!(stripped, line);
    }

    /// The real measurement: shaped with the bundled Noto Naskh Arabic, a justified line
    /// lands on the width it was asked for, and the only thing added is tatweels.
    ///
    /// This is the one that says the idea works. A tatweel is not a space: it is shaped
    /// into the word, joining the letters on both sides of it, so the line grows without
    /// its words moving apart. If the face ever stopped shaping it that way, this test
    /// would still pass on width but the picture in
    /// `crates/ui-egui/tests/rtl_quality.rs` would not.
    #[test]
    fn a_real_line_in_the_real_face_reaches_the_width_it_was_given() {
        let mut t = crate::Text::new();
        let line = "الملاحظات يجب أن تعيش أطول من البرنامج الذي كتبها";
        let mut width = |s: &str| t.layout(s, 18.0, None).width();
        let natural = width(line);
        assert!(natural > 100.0, "the line has glyphs: {natural}");
        for extra in [8.0_f32, 24.0, 60.0] {
            let target = natural + extra;
            let out = justify(line, target, &mut width);
            let got = width(&out);
            let tatweels = out.chars().filter(|c| *c == TATWEEL).count();
            assert!(tatweels > 0, "nothing was stretched for {extra} points");
            // One tatweel is the smallest step there is, so landing within one of the
            // target is as close as a plain face can get.
            let step = width("ـ") - width("");
            assert!((got - target).abs() <= step + 0.5, "asked for {target}, got {got} with {tatweels} tatweels (one step is {step})");
            // Nothing but tatweels was added.
            let stripped: String = out.chars().filter(|c| *c != TATWEEL).collect();
            assert_eq!(stripped, line);
        }
    }

    /// The tatweel is shaped into the word, not dropped beside it: a word with one in it is
    /// wider than the word without, and the letters still join.
    #[test]
    fn a_tatweel_widens_the_word_it_is_in() {
        let mut t = crate::Text::new();
        let plain = t.layout("مسار", 18.0, None).width();
        let long = t.layout("مسـار", 18.0, None).width();
        assert!(long > plain, "the stretched word is wider: {plain} then {long}");
        // Shaped, not laid out letter by letter: four letters, and the glyph count does
        // not grow by more than the one tatweel.
        let mut glyphs = |s: &str| -> usize { t.layout(s, 18.0, None).lines().flat_map(|l| l.runs()).map(|r| r.len()).sum() };
        assert_eq!(glyphs("مسـار"), glyphs("مسار") + 1, "one glyph more, which is the stretch itself");
    }

    /// A line that is already wide enough, or has nowhere to stretch, comes back as it was.
    #[test]
    fn a_line_that_cannot_or_need_not_stretch_is_left_alone() {
        let mut measure = |s: &str| s.chars().count() as f32;
        assert_eq!(justify("كتاب", 2.0, &mut measure), "كتاب");
        assert_eq!(justify("Hello world", 40.0, &mut measure), "Hello world");
        assert_eq!(justify("", 40.0, &mut measure), "");
        // Nowhere decent to stretch: every join is capped, so it stops instead of looping.
        let out = justify("كتاب", 1000.0, &mut measure);
        assert!(out.chars().filter(|c| *c == TATWEEL).count() <= 2 * MAX_PER_SPOT, "{out}");
    }
}
