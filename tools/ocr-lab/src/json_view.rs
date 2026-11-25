//! Syntax colours for a JSON file shown in the lab's main area. The lab's own code, so it does not
//! touch the app. Keys, strings, numbers and the words true, false and null each get a colour; the
//! rest of the text is plain.

use eframe::egui;
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId};

pub const KEY: Color32 = Color32::from_rgb(120, 180, 255);
pub const STRING: Color32 = Color32::from_rgb(140, 205, 120);
pub const NUMBER: Color32 = Color32::from_rgb(240, 170, 90);
pub const WORD: Color32 = Color32::from_rgb(200, 130, 230);

/// One run of text and its colour, in order. `None` is plain text.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub text: String,
    pub colour: Option<Color32>,
}

/// Splits JSON text into coloured runs. The runs, joined, give back the text exactly.
pub fn runs(text: &str) -> Vec<Run> {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<Run> = Vec::new();
    let mut i = 0;
    let push = |out: &mut Vec<Run>, s: String, colour: Option<Color32>| {
        if !s.is_empty() {
            out.push(Run { text: s, colour });
        }
    };
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            // A string runs to the next unescaped quote. A key is a string followed by a colon.
            let start = i;
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                i += if chars[i] == '\\' { 2 } else { 1 };
            }
            i = (i + 1).min(chars.len());
            let s: String = chars[start..i].iter().collect();
            let mut j = i;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            let colour = if j < chars.len() && chars[j] == ':' { KEY } else { STRING };
            push(&mut out, s, Some(colour));
        } else if c.is_ascii_digit() || (c == '-' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit()) {
            let start = i;
            i += 1;
            while i < chars.len() && (chars[i].is_ascii_digit() || matches!(chars[i], '.' | 'e' | 'E' | '+' | '-')) {
                i += 1;
            }
            push(&mut out, chars[start..i].iter().collect(), Some(NUMBER));
        } else if c.is_ascii_alphabetic() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_alphabetic() {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            let colour = matches!(word.as_str(), "true" | "false" | "null").then_some(WORD);
            push(&mut out, word, colour);
        } else {
            // Whitespace and punctuation: plain, one character at a time, so runs stay simple.
            push(&mut out, c.to_string(), None);
            i += 1;
        }
    }
    out
}

/// The text as a layout job for the UI, in the given font and plain colour.
pub fn job(text: &str, font: FontId, plain: Color32) -> LayoutJob {
    let mut job = LayoutJob::default();
    for run in runs(text) {
        job.append(&run.text, 0.0, TextFormat::simple(font.clone(), run.colour.unwrap_or(plain)));
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    fn joined(runs: &[Run]) -> String {
        runs.iter().map(|r| r.text.as_str()).collect()
    }

    #[test]
    fn the_runs_give_back_the_text_exactly() {
        let text = "{\n  \"version\": 1,\n  \"text\": \"a \\\"quoted\\\" word\",\n  \"alt\": null,\n  \"ok\": true\n}\n";
        assert_eq!(joined(&runs(text)), text);
    }

    #[test]
    fn keys_strings_numbers_and_words_get_their_colours() {
        let r = runs("{\"version\": 1, \"text\": \"hi\", \"alt\": null}");
        let colour_of = |word: &str| r.iter().find(|x| x.text == word).and_then(|x| x.colour);
        assert_eq!(colour_of("\"version\""), Some(KEY), "a string before a colon is a key");
        assert_eq!(colour_of("1"), Some(NUMBER));
        assert_eq!(colour_of("\"hi\""), Some(STRING), "a string as a value");
        assert_eq!(colour_of("null"), Some(WORD));
        assert_eq!(colour_of("{"), None, "punctuation is plain");
    }

    #[test]
    fn negative_and_decimal_numbers_are_one_run() {
        let r = runs("[-12.5e3]");
        assert!(r.iter().any(|x| x.text == "-12.5e3" && x.colour == Some(NUMBER)));
    }

    #[test]
    fn an_escaped_quote_does_not_end_a_string() {
        let r = runs(r#""a\"b": 2"#);
        assert_eq!(r[0].text, r#""a\"b""#);
        assert_eq!(r[0].colour, Some(KEY));
    }
}
