//! Themes: light and dark, each with seven accents, 14 in all. The UI reads colours
//! from `Tokens` and never uses literal colours.
//!
//! Every text and accent colour is checked against its background (WCAG contrast) when
//! the tokens are built, so no theme can ship unreadable text. The tests check all 14.

use egui::Color32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    Light,
    Dark,
}

impl Mode {
    pub fn id(self) -> &'static str {
        match self {
            Mode::Light => "light",
            Mode::Dark => "dark",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Mode::Light => "Light",
            Mode::Dark => "Dark",
        }
    }

    pub fn is_dark(self) -> bool {
        self == Mode::Dark
    }
}

/// How the mode is chosen: fixed, or following the system's light or dark setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModePref {
    Light,
    Dark,
    System,
}

impl ModePref {
    pub fn id(self) -> &'static str {
        match self {
            ModePref::Light => "light",
            ModePref::Dark => "dark",
            ModePref::System => "system",
        }
    }

    pub fn parse(text: &str) -> Option<ModePref> {
        match text.trim() {
            "light" => Some(ModePref::Light),
            "dark" => Some(ModePref::Dark),
            "system" => Some(ModePref::System),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Accent {
    /// The NoteCraft purple on neutral surfaces.
    Default,
    Red,
    Green,
    Yellow,
    Blue,
    Gray,
    /// Purple surfaces as well as a purple accent.
    Purple,
}

impl Accent {
    pub const ALL: [Accent; 7] = [Accent::Default, Accent::Red, Accent::Green, Accent::Yellow, Accent::Blue, Accent::Gray, Accent::Purple];

    pub fn id(self) -> &'static str {
        match self {
            Accent::Default => "default",
            Accent::Red => "red",
            Accent::Green => "green",
            Accent::Yellow => "yellow",
            Accent::Blue => "blue",
            Accent::Gray => "gray",
            Accent::Purple => "purple",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Accent::Default => "Default",
            Accent::Red => "Red",
            Accent::Green => "Green",
            Accent::Yellow => "Yellow",
            Accent::Blue => "Blue",
            Accent::Gray => "Gray",
            Accent::Purple => "Purple",
        }
    }

    pub fn parse(text: &str) -> Option<Accent> {
        Accent::ALL.into_iter().find(|a| a.id() == text.trim())
    }

    /// The accent colour before the contrast check.
    fn base(self, mode: Mode) -> Color32 {
        let (l, d) = match self {
            Accent::Default | Accent::Purple => ((0x7a, 0x45, 0xb8), (0xb0, 0x85, 0xe8)),
            Accent::Red => ((0xc0, 0x39, 0x2b), (0xff, 0x8a, 0x80)),
            Accent::Green => ((0x1e, 0x8a, 0x44), (0x6c, 0xd0, 0x8a)),
            Accent::Yellow => ((0xa8, 0x74, 0x00), (0xf2, 0xc9, 0x4c)),
            Accent::Blue => ((0x1a, 0x5f, 0xd0), (0x8f, 0xb8, 0xff)),
            Accent::Gray => ((0x5b, 0x64, 0x72), (0xb4, 0xbc, 0xc8)),
        };
        let (r, g, b) = if mode.is_dark() { d } else { l };
        Color32::from_rgb(r, g, b)
    }

    /// How much of the accent is mixed into the surfaces. Default and Gray stay neutral.
    fn tint(self, mode: Mode) -> f32 {
        match self {
            Accent::Default | Accent::Gray => 0.0,
            _ => {
                if mode.is_dark() {
                    0.10
                } else {
                    0.05
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Theme {
    pub mode: Mode,
    pub accent: Accent,
}

impl Theme {
    pub const DEFAULT: Theme = Theme { mode: Mode::Light, accent: Accent::Default };

    /// All 14 themes: light then dark, each with the seven accents.
    pub fn all() -> Vec<Theme> {
        [Mode::Light, Mode::Dark].into_iter().flat_map(|mode| Accent::ALL.into_iter().map(move |accent| Theme { mode, accent })).collect()
    }

    pub fn id(self) -> String {
        format!("{}-{}", self.mode.id(), self.accent.id())
    }

    pub fn label(self) -> String {
        format!("{} {}", self.mode.label(), self.accent.label())
    }

    pub fn parse(id: &str) -> Option<Theme> {
        Theme::all().into_iter().find(|t| t.id() == id.trim())
    }

    pub fn tokens(self) -> Tokens {
        Tokens::for_theme(self)
    }

    /// The egui visuals for this theme.
    pub fn visuals(self) -> egui::Visuals {
        let t = self.tokens();
        let mut v = if self.mode.is_dark() { egui::Visuals::dark() } else { egui::Visuals::light() };
        v.panel_fill = t.panel;
        v.window_fill = t.panel;
        v.extreme_bg_color = t.editor;
        v.faint_bg_color = t.faint;
        v.code_bg_color = t.code_bg;
        v.hyperlink_color = t.link;
        v.selection.bg_fill = mix(t.panel, t.accent, 0.40);
        v.selection.stroke = egui::Stroke::new(1.0, t.accent);
        v.widgets.hovered.weak_bg_fill = t.row_hover;
        v.widgets.hovered.bg_fill = t.row_hover;
        v.widgets.active.weak_bg_fill = t.row_selected;
        v.widgets.active.bg_fill = t.row_selected;
        v.widgets.noninteractive.bg_fill = t.panel;
        v.widgets.noninteractive.weak_bg_fill = t.panel;
        v.widgets.inactive.weak_bg_fill = t.faint;
        v.warn_fg_color = t.warn;
        v.error_fg_color = t.warn;
        v
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tokens {
    pub panel: Color32,
    pub editor: Color32,
    pub faint: Color32,
    pub text: Color32,
    pub accent: Color32,
    pub notebook: Color32,
    pub group: Color32,
    pub section: Color32,
    pub page: Color32,
    pub muted: Color32,
    pub strong: Color32,
    pub link: Color32,
    pub code_bg: Color32,
    pub quote_bar: Color32,
    pub row_hover: Color32,
    pub row_selected: Color32,
    /// The wash behind selected words on the page. Not `row_selected`: that one is tinted
    /// from the panel, because a selected row of the tree sits on the panel, and the page
    /// is drawn on `editor`. Where a theme puts those two colours far apart the panel's
    /// tint lands on top of the page's own colour and the selection is invisible, which is
    /// what the owner saw on 2026-10-19 in a green theme: the words were selected, the
    /// band was painted, and nothing could be seen.
    pub selection: Color32,
    pub ok: Color32,
    pub warn: Color32,
    /// Rule lines and grid lines drawn on the page itself (OneNote's View, Rule Lines).
    /// Decoration, so the target is low on purpose: visible, never competing with the text.
    pub rule_line: Color32,
    /// Every fifth grid line, or the margin line of a ruled page. A little stronger.
    pub rule_line_strong: Color32,
    /// The lines round the cells of a table. Stronger than a ruled page, because a table
    /// is read as a grid and its lines carry the shape; still quiet beside the words.
    pub table_line: Color32,
    /// The coloured band across the top of the window, where the title, the Quick Access
    /// Toolbar and the ribbon's tab names sit, as in Office. Dark enough for white writing.
    pub band: Color32,
    /// Writing and icons on the band.
    pub band_text: Color32,
    /// Greyed writing and icons on the band.
    pub band_dim: Color32,
    /// A button or tab on the band that the pointer is over.
    pub band_hover: Color32,
}

fn rgb(c: (u8, u8, u8)) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2)
}

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let f = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()))
}

fn channel(c: u8) -> f32 {
    let s = f32::from(c) / 255.0;
    if s <= 0.03928 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
}

pub fn luminance(c: Color32) -> f32 {
    0.2126 * channel(c.r()) + 0.7152 * channel(c.g()) + 0.0722 * channel(c.b())
}

/// WCAG contrast ratio, 1.0 to 21.0.
pub fn contrast(a: Color32, b: Color32) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// Moves `fg` away from `bg` (towards black on a light surface, white on a dark one)
/// until the contrast is at least `min`.
fn ensure_contrast(fg: Color32, bg: Color32, min: f32, dark: bool) -> Color32 {
    let target = if dark { Color32::WHITE } else { Color32::BLACK };
    let mut c = fg;
    for _ in 0..60 {
        if contrast(c, bg) >= min {
            return c;
        }
        c = mix(c, target, 0.05);
    }
    target
}

/// A colour the user chose (a notebook cover, a section tab), moved just far enough to be
/// readable on `bg`. The hue is kept; only the lightness changes.
pub fn readable_on(fg: Color32, bg: Color32, min: f32, dark: bool) -> Color32 {
    ensure_contrast(fg, bg, min, dark)
}

/// A line on `bg` that is just visible: mixed towards `towards` until the contrast reaches
/// `target`, then stopped. Used for rule lines and grid lines on a page, in both modes.
fn faint_line(bg: Color32, towards: Color32, target: f32) -> Color32 {
    let mut c = bg;
    for _ in 0..60 {
        if contrast(c, bg) >= target {
            return c;
        }
        c = mix(c, towards, 0.04);
    }
    towards
}

/// `mix(base, tint, t)` with `t` lowered until `text` still has contrast `min` on the result.
/// A bright accent (yellow) would otherwise wash out the text on a selected row.
fn tint_keeping_contrast(base: Color32, tint: Color32, t_max: f32, text: Color32, min: f32) -> Color32 {
    let mut t = t_max;
    while t > 0.0 {
        let c = mix(base, tint, t);
        if contrast(text, c) >= min {
            return c;
        }
        t -= 0.02;
    }
    base
}

/// The wash behind selected words on the page: the page's own colour carried towards the
/// accent, far enough to be seen against the page and not so far that the words on it stop
/// being readable.
///
/// The accent is tried first, because a selection in the theme's own colour is what a
/// reader expects. A pale accent on a pale page cannot be told apart from it, and then the
/// text colour is used instead, which every theme keeps well away from the page by
/// definition.
fn selection_wash(editor: Color32, acc: Color32, text: Color32) -> Color32 {
    for tint in [acc, text] {
        let mut t = 0.30;
        while t > 0.04 {
            let c = mix(editor, tint, t);
            if contrast(text, c) >= 7.0 && contrast(c, editor) >= 1.25 {
                return c;
            }
            t -= 0.02;
        }
    }
    editor
}

impl Tokens {
    pub fn for_theme(theme: Theme) -> Tokens {
        let dark = theme.mode.is_dark();
        let acc = theme.accent.base(theme.mode);
        let tint = theme.accent.tint(theme.mode);
        let (panel0, editor0, faint0, text) = if dark {
            (rgb((27, 25, 34)), rgb((20, 18, 26)), rgb((38, 35, 48)), rgb((236, 232, 245)))
        } else {
            (rgb((247, 247, 249)), rgb((255, 255, 255)), rgb((238, 238, 243)), rgb((30, 26, 43)))
        };
        let panel = mix(panel0, acc, tint);
        let editor = mix(editor0, acc, tint * 0.5);
        let faint = mix(faint0, acc, tint);
        let muted = ensure_contrast(if dark { rgb((165, 157, 187)) } else { rgb((95, 88, 117)) }, panel, 4.5, dark);
        let accent = ensure_contrast(acc, panel, 3.0, dark);
        // The band: the accent, taken darker until white writing reads on it (7, so it still reads under the hover tint), and in the
        // dark modes kept low, so the band is a deep colour and not a bright one.
        let band_base = if dark { mix(panel0, acc, 0.45) } else { acc };
        let band = ensure_contrast(band_base, Color32::WHITE, 7.0, false);
        let link = ensure_contrast(acc, editor, 4.5, dark);
        let group = ensure_contrast(if dark { rgb((0xe0, 0xa2, 0x3c)) } else { rgb((0xb8, 0x76, 0x0f)) }, panel, 3.0, dark);
        let section = ensure_contrast(if dark { rgb((0x3f, 0xb8, 0xa8)) } else { rgb((0x1f, 0x8f, 0x80)) }, panel, 3.0, dark);
        Tokens {
            panel,
            editor,
            faint,
            text,
            accent,
            notebook: accent,
            group,
            section,
            page: muted,
            muted,
            strong: if dark { Color32::WHITE } else { Color32::BLACK },
            link,
            code_bg: mix(panel, acc, if dark { 0.16 } else { 0.10 }),
            quote_bar: accent,
            row_hover: tint_keeping_contrast(panel, acc, 0.10, text, 7.0),
            row_selected: tint_keeping_contrast(panel, acc, 0.24, text, 7.0),
            selection: selection_wash(editor, acc, text),
            ok: ensure_contrast(if dark { rgb((0x6c, 0xd0, 0x8a)) } else { rgb((0x1e, 0x8a, 0x44)) }, panel, 3.0, dark),
            warn: ensure_contrast(if dark { rgb((0xff, 0x8a, 0x80)) } else { rgb((0xb3, 0x26, 0x1e)) }, panel, 4.5, dark),
            rule_line: faint_line(editor, acc, 1.25),
            rule_line_strong: faint_line(editor, acc, 1.8),
            table_line: faint_line(editor, text, 2.2),
            band,
            band_text: Color32::WHITE,
            band_dim: mix(band, Color32::WHITE, 0.6),
            band_hover: mix(band, Color32::WHITE, 0.18),
        }
    }

    /// The same colours seen from the band: its own surface and its own writing, so a control
    /// drawn for the page's surfaces (the Quick Access Toolbar, the theme button) reads there.
    pub fn on_band(&self) -> Tokens {
        Tokens {
            panel: self.band,
            text: self.band_text,
            accent: self.band_text,
            muted: self.band_dim,
            strong: self.band_text,
            row_hover: self.band_hover,
            row_selected: self.band_hover,
            selection: self.band_hover,
            ..*self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_are_fourteen_distinct_themes() {
        let all = Theme::all();
        assert_eq!(all.len(), 14);
        let ids: std::collections::HashSet<String> = all.iter().map(|t| t.id()).collect();
        assert_eq!(ids.len(), 14);
        let toks: Vec<Tokens> = all.iter().map(|t| t.tokens()).collect();
        for (i, a) in toks.iter().enumerate() {
            for b in toks.iter().skip(i + 1) {
                assert_ne!(a, b, "two themes look the same");
            }
        }
    }

    #[test]
    fn ids_round_trip() {
        for t in Theme::all() {
            assert_eq!(Theme::parse(&t.id()), Some(t));
        }
        assert_eq!(Theme::parse("light-purple"), Some(Theme { mode: Mode::Light, accent: Accent::Purple }));
        assert_eq!(Theme::parse("sepia-pink"), None);
        assert_eq!(ModePref::parse("system"), Some(ModePref::System));
        assert_eq!(ModePref::parse("auto"), None);
        assert_eq!(Accent::parse(" red "), Some(Accent::Red));
    }

    /// WCAG: body text 7:1 (AAA), secondary text 4.5:1 (AA), links 4.5:1, accents and icons 3:1.
    #[test]
    fn every_theme_is_readable() {
        for theme in Theme::all() {
            let t = theme.tokens();
            let name = theme.label();
            assert!(contrast(t.text, t.panel) >= 7.0, "{name}: text on panel {:.2}", contrast(t.text, t.panel));
            assert!(contrast(t.text, t.editor) >= 7.0, "{name}: text on editor {:.2}", contrast(t.text, t.editor));
            assert!(contrast(t.strong, t.panel) >= 7.0, "{name}: strong text");
            assert!(contrast(t.muted, t.panel) >= 4.5, "{name}: muted text {:.2}", contrast(t.muted, t.panel));
            assert!(contrast(t.muted, t.code_bg) >= 3.5, "{name}: muted on code background {:.2}", contrast(t.muted, t.code_bg));
            assert!(contrast(t.link, t.editor) >= 4.5, "{name}: link {:.2}", contrast(t.link, t.editor));
            assert!(contrast(t.accent, t.panel) >= 3.0, "{name}: accent {:.2}", contrast(t.accent, t.panel));
            for (what, c) in [("group icon", t.group), ("section icon", t.section), ("ok", t.ok)] {
                assert!(contrast(c, t.panel) >= 3.0, "{name}: {what} {:.2}", contrast(c, t.panel));
            }
            assert!(contrast(t.warn, t.panel) >= 4.5, "{name}: warning text {:.2}", contrast(t.warn, t.panel));
            // A selection on the page must be seen against the page and still be readable.
            assert!(
                contrast(t.selection, t.editor) >= 1.25,
                "{name}: the selection on the page is {:.2} against it, which cannot be seen",
                contrast(t.selection, t.editor)
            );
            assert!(contrast(t.text, t.selection) >= 7.0, "{name}: text on a selection {:.2}", contrast(t.text, t.selection));
            assert!(contrast(t.strong, t.selection) >= 7.0, "{name}: strong text on a selection");
            assert!(contrast(t.text, t.row_selected) >= 7.0, "{name}: text on the selected row {:.2}", contrast(t.text, t.row_selected));
            assert!(contrast(t.strong, t.row_selected) >= 7.0, "{name}: strong text on the selected row");
            assert!(contrast(t.text, t.row_hover) >= 7.0, "{name}: text on a hovered row");
            // The band across the top: white writing on it, and on it under the pointer.
            assert!(contrast(t.band_text, t.band) >= 4.5, "{name}: writing on the band {:.2}", contrast(t.band_text, t.band));
            assert!(
                contrast(t.band_text, t.band_hover) >= 4.5,
                "{name}: writing on a hovered band button {:.2}",
                contrast(t.band_text, t.band_hover)
            );
            assert!(contrast(t.band_dim, t.band) >= 2.0, "{name}: greyed writing on the band {:.2}", contrast(t.band_dim, t.band));
            // Rule and grid lines on a page: visible against the page, but not loud, and
            // they must not hurt the text that sits on top of them.
            for (what, c, min) in [("rule line", t.rule_line, 1.25), ("strong rule line", t.rule_line_strong, 1.8)] {
                let got = contrast(c, t.editor);
                assert!(got >= min, "{name}: {what} is invisible on the page {got:.2}");
                assert!(got <= 4.5, "{name}: {what} is too loud for a page background {got:.2}");
                assert!(contrast(t.text, c) >= 4.5, "{name}: text over a {what} {:.2}", contrast(t.text, c));
            }
            assert!(contrast(t.rule_line_strong, t.editor) > contrast(t.rule_line, t.editor), "{name}: the strong line is not stronger");
        }
    }

    #[test]
    fn light_themes_are_light_and_dark_themes_are_dark() {
        for theme in Theme::all() {
            let l = luminance(theme.tokens().panel);
            if theme.mode.is_dark() {
                assert!(l < 0.1, "{}: {l}", theme.label());
            } else {
                assert!(l > 0.8, "{}: {l}", theme.label());
            }
        }
    }

    #[test]
    fn visuals_follow_the_tokens() {
        for theme in Theme::all() {
            let v = theme.visuals();
            assert_eq!(v.dark_mode, theme.mode.is_dark());
            assert_eq!(v.panel_fill, theme.tokens().panel);
        }
    }

    #[test]
    fn contrast_math_is_right() {
        assert!((contrast(Color32::BLACK, Color32::WHITE) - 21.0).abs() < 0.01);
        assert!((contrast(Color32::WHITE, Color32::WHITE) - 1.0).abs() < 0.001);
        assert_eq!(mix(Color32::BLACK, Color32::WHITE, 0.5), Color32::from_rgb(128, 128, 128));
        assert_eq!(mix(Color32::RED, Color32::BLUE, 0.0), Color32::RED);
        let fixed = ensure_contrast(Color32::from_rgb(200, 200, 200), Color32::WHITE, 4.5, false);
        assert!(contrast(fixed, Color32::WHITE) >= 4.5);
    }
}
