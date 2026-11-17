//! Home > Font Colour and Text Highlight Colour (issue #260).
//!
//! A span keeps the exact colour chosen, `#rrggbb`, in both page formats. What is drawn is
//! kept readable on every theme: a text colour too close to the page is moved towards the
//! theme's text colour until it can be read, and a highlight is laid over the page rather
//! than painted at full strength, so dark-theme text stays legible on a yellow mark. The
//! file is never changed by this; only the drawing is.

use crate::theme::Tokens;
use egui::Color32;

/// The text colours offered, as OneNote's Standard Colors row has them: name and value.
pub fn font_colours() -> [(&'static str, &'static str); 10] {
    [
        (crate::tl_id!("ui.colour_dark_red"), "#c00000"),
        (crate::tl_id!("ui.colour_red"), "#ff0000"),
        (crate::tl_id!("ui.colour_orange"), "#ffc000"),
        (crate::tl_id!("ui.colour_yellow"), "#ffff00"),
        (crate::tl_id!("ui.colour_light_green"), "#92d050"),
        (crate::tl_id!("ui.colour_green"), "#00b050"),
        (crate::tl_id!("ui.colour_light_blue"), "#00b0f0"),
        (crate::tl_id!("ui.colour_blue"), "#0070c0"),
        (crate::tl_id!("ui.colour_dark_blue"), "#002060"),
        (crate::tl_id!("ui.colour_purple"), "#7030a0"),
    ]
}

/// The highlight colours offered, as OneNote's highlighter has them.
pub fn highlights() -> [(&'static str, &'static str); 6] {
    [
        (crate::tl_id!("ui.colour_yellow"), "#ffff00"),
        (crate::tl_id!("ui.colour_bright_green"), "#00ff00"),
        (crate::tl_id!("ui.colour_turquoise"), "#00ffff"),
        (crate::tl_id!("ui.colour_pink"), "#ff00ff"),
        (crate::tl_id!("ui.colour_light_blue"), "#00b0f0"),
        (crate::tl_id!("ui.colour_gray"), "#c0c0c0"),
    ]
}

/// A `#rrggbb` value as a colour.
pub fn parse(hex: &str) -> Option<Color32> {
    let hex = notecraft_formats::markdown::css_colour(hex)?;
    let v = u32::from_str_radix(hex.trim_start_matches('#'), 16).ok()?;
    #[allow(clippy::cast_possible_truncation)]
    Some(Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

/// The colour text of `hex` is drawn in on `page`: the chosen colour, or, when it would be too
/// faint to read there, the nearest blend towards `text` that can be read (contrast 3:1).
pub fn shown_text(hex: &str, page: Color32, text: Color32) -> Option<Color32> {
    let chosen = parse(hex)?;
    if crate::theme::contrast(chosen, page) >= 3.0 {
        return Some(chosen);
    }
    for step in 1..=10 {
        #[allow(clippy::cast_precision_loss)]
        let blended = crate::theme::mix(chosen, text, step as f32 / 10.0);
        if crate::theme::contrast(blended, page) >= 3.0 {
            return Some(blended);
        }
    }
    Some(text)
}

/// The colour a highlight of `hex` is drawn in behind text on this theme.
pub fn shown_highlight(hex: &str, t: &Tokens) -> Option<Color32> {
    let chosen = parse(hex)?;
    let dark = crate::theme::luminance(t.editor) < 0.3;
    // Light pages take the mark nearly as it is; dark ones a softer wash, so the light text
    // on them stays readable.
    Some(crate::theme::mix(t.editor, chosen, if dark { 0.45 } else { 0.85 }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_are_read_and_kept_readable_on_any_page() {
        assert_eq!(parse("#c00000"), Some(Color32::from_rgb(0xc0, 0, 0)));
        assert_eq!(parse("red"), None);
        let (white, black) = (Color32::WHITE, Color32::BLACK);
        assert_eq!(shown_text("#c00000", white, black), parse("#c00000"), "dark red reads on white as it is");
        let on_dark = shown_text("#002060", Color32::from_rgb(30, 30, 30), Color32::from_rgb(230, 230, 230)).expect("colour");
        assert!(crate::theme::contrast(on_dark, Color32::from_rgb(30, 30, 30)) >= 3.0, "dark blue is lifted on a dark page");
        assert!(shown_text("#ffff00", white, black).is_some_and(|c| crate::theme::contrast(c, white) >= 3.0), "yellow text is darkened on white");
    }

    #[test]
    fn every_offered_colour_is_a_plain_value_with_a_name() {
        for (name, hex) in font_colours().iter().chain(highlights().iter()) {
            assert!(parse(hex).is_some(), "{hex}");
            assert!(!name.is_empty() && !name.starts_with("ui."), "{name} has words in the catalog");
        }
    }
}
