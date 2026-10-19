//! The Quick Access Toolbar: Back, Forward, Undo and Redo in a strip of their own, the way
//! OneNote keeps them beside its title, and a small menu to choose which are shown, to put the
//! strip under the ribbon, or to hide it.
//!
//! Only what NoteCraft can do is offered. OneNote's menu also lists Print, Print Preview,
//! Touch/Mouse Mode and More Commands; none of those exist here, so they are not listed.
//!
//! This module draws the strip and says which command was pressed; the window carries it out.
//! The choices (which buttons, above or below, hidden) are plain data in [`Config`] and are kept
//! in the settings file.

use crate::floating::{self, Glyph};
use crate::icons::Icon;
use crate::ribbon_icons::Mark;
use crate::theme::Tokens;
use egui::{Sense, Ui, vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Back,
    Forward,
    Undo,
    Redo,
}

impl Command {
    pub const ALL: [Command; 4] = [Command::Back, Command::Forward, Command::Undo, Command::Redo];

    pub fn label(self) -> &'static str {
        match self {
            Command::Back => "Back",
            Command::Forward => "Forward",
            Command::Undo => "Undo",
            Command::Redo => "Redo",
        }
    }

    pub fn tip(self) -> &'static str {
        match self {
            Command::Back => "Back to the page before (Alt+Left)",
            Command::Forward => "Forward to the page after (Alt+Right)",
            Command::Undo => "Undo (Ctrl+Z)",
            Command::Redo => "Redo (Ctrl+Y)",
        }
    }

    fn glyph(self) -> Glyph {
        match self {
            Command::Back => Glyph::Mark(Mark::Back),
            Command::Forward => Glyph::Mark(Mark::Forward),
            Command::Undo => Glyph::Icon(Icon::Undo),
            Command::Redo => Glyph::Icon(Icon::Redo),
        }
    }

    fn index(self) -> usize {
        self as usize
    }

    fn key(self) -> &'static str {
        match self {
            Command::Back => "qat_back",
            Command::Forward => "qat_forward",
            Command::Undo => "qat_undo",
            Command::Redo => "qat_redo",
        }
    }
}

const BELOW_KEY: &str = "qat_below";
const HIDDEN_KEY: &str = "qat_hidden";

/// What the person chose: which buttons, where the strip sits, whether it shows at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    pub show: [bool; 4],
    /// Under the ribbon instead of beside the title.
    pub below: bool,
    pub hidden: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config { show: [true; 4], below: false, hidden: false }
    }
}

fn flag(value: Option<String>, default: bool) -> bool {
    match value.as_deref().map(str::trim) {
        Some("true") => true,
        Some("false") => false,
        _ => default,
    }
}

impl Config {
    /// Reads the choices from the settings, with `get` giving a setting's text. Anything not
    /// there, or not readable, is the default.
    pub fn load(get: impl Fn(&str) -> Option<String>) -> Config {
        let mut c = Config::default();
        for cmd in Command::ALL {
            c.show[cmd.index()] = flag(get(cmd.key()), true);
        }
        c.below = flag(get(BELOW_KEY), false);
        c.hidden = flag(get(HIDDEN_KEY), false);
        c
    }

    /// Writes the choices, with `set` keeping one setting.
    pub fn save(&self, mut set: impl FnMut(&str, &str)) {
        let text = |b: bool| if b { "true" } else { "false" };
        for cmd in Command::ALL {
            set(cmd.key(), text(self.show[cmd.index()]));
        }
        set(BELOW_KEY, text(self.below));
        set(HIDDEN_KEY, text(self.hidden));
    }

    pub fn shows(&self, cmd: Command) -> bool {
        self.show[cmd.index()]
    }
}

/// Which commands can be used right now. A button that cannot is drawn faint and does nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Enabled {
    pub back: bool,
    pub forward: bool,
    pub undo: bool,
    pub redo: bool,
}

impl Enabled {
    pub fn of(self, cmd: Command) -> bool {
        match cmd {
            Command::Back => self.back,
            Command::Forward => self.forward,
            Command::Undo => self.undo,
            Command::Redo => self.redo,
        }
    }
}

const BTN: f32 = 26.0;

/// Draws the strip in the layout it is given: the buttons that are shown, then the small arrow
/// that opens the menu. Returns the command that was pressed, if one was, and whether the menu
/// changed the [`Config`] (so the caller can keep it).
pub fn show(ui: &mut Ui, config: &mut Config, enabled: Enabled, t: &Tokens) -> (Option<Command>, bool) {
    let mut pressed = None;
    let mut changed = false;
    ui.spacing_mut().item_spacing.x = 1.0;
    if !config.hidden {
        for cmd in Command::ALL {
            if config.shows(cmd) && button(ui, cmd, enabled.of(cmd), t).clicked() && enabled.of(cmd) {
                pressed = Some(cmd);
            }
        }
    }
    // The arrow stays even when the strip is hidden, so it can be brought back.
    let arrow = customize_button(ui, t);
    egui::Popup::menu(&arrow).show(|ui| {
        ui.set_width(floating::MENU_W + 20.0);
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        ui.label(egui::RichText::new("Customize Quick Access Toolbar").strong());
        ui.add_space(4.0);
        for cmd in Command::ALL {
            if floating::menu_check_row(ui, config.shows(cmd), cmd.label(), t).clicked() {
                config.show[cmd.index()] = !config.shows(cmd);
                changed = true;
            }
        }
        floating::menu_separator(ui, t);
        let place = if config.below { "Show Above the Ribbon" } else { "Show Below the Ribbon" };
        if floating::menu_row(ui, None, place, t).clicked() {
            config.below = !config.below;
            changed = true;
        }
        let hide = if config.hidden { "Show Quick Access Toolbar" } else { "Hide Quick Access Toolbar" };
        if floating::menu_row(ui, None, hide, t).clicked() {
            config.hidden = !config.hidden;
            changed = true;
        }
    });
    (pressed, changed)
}

fn button(ui: &mut Ui, cmd: Command, enabled: bool, t: &Tokens) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(BTN, BTN), Sense::click());
    let painter = ui.painter_at(rect);
    if enabled && resp.hovered() {
        painter.rect_filled(rect, 3.0, t.row_hover);
    }
    // A faint button: the same picture in the muted text colour.
    let mut faint = *t;
    faint.text = t.muted;
    cmd.glyph().paint(&painter, egui::Rect::from_center_size(rect.center(), vec2(16.0, 16.0)), if enabled { t } else { &faint });
    let name = cmd.label().to_string();
    resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name.clone()));
    resp.on_hover_text(cmd.tip())
}

/// The small arrow at the end of the strip, drawn as a triangle pointing down.
fn customize_button(ui: &mut Ui, t: &Tokens) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(18.0, BTN), Sense::click());
    let painter = ui.painter_at(rect);
    if resp.hovered() {
        painter.rect_filled(rect, 3.0, t.row_hover);
    }
    let c = rect.center();
    painter.add(egui::Shape::convex_polygon(
        vec![egui::pos2(c.x - 4.0, c.y - 2.0), egui::pos2(c.x + 4.0, c.y - 2.0), egui::pos2(c.x, c.y + 2.5)],
        t.text,
        egui::Stroke::NONE,
    ));
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Customize Quick Access Toolbar"));
    resp.on_hover_text("Customize Quick Access Toolbar")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    #[test]
    fn nothing_saved_means_every_button_in_its_usual_place() {
        let c = Config::load(|_| None);
        assert_eq!(c, Config::default());
        assert!(c.show.iter().all(|s| *s) && !c.below && !c.hidden);
    }

    #[test]
    fn the_choices_are_kept_and_read_back() {
        let store = RefCell::new(HashMap::new());
        let chosen = Config { show: [true, true, false, true], below: true, hidden: false };
        chosen.save(|k, v| {
            store.borrow_mut().insert(k.to_string(), v.to_string());
        });
        let back = Config::load(|k| store.borrow().get(k).cloned());
        assert_eq!(back, chosen);
        assert_eq!(store.borrow().get("qat_undo").map(String::as_str), Some("false"));
    }

    #[test]
    fn a_setting_that_makes_no_sense_is_the_default() {
        let c = Config::load(|k| (k == "qat_back").then(|| "maybe".to_string()));
        assert!(c.shows(Command::Back), "unreadable means the default, which is shown");
    }

    #[test]
    fn every_command_has_a_name_a_tip_and_its_own_setting() {
        let labels: std::collections::HashSet<&str> = Command::ALL.iter().map(|c| c.label()).collect();
        let keys: std::collections::HashSet<&str> = Command::ALL.iter().map(|c| c.key()).collect();
        assert_eq!((labels.len(), keys.len()), (4, 4));
        assert!(Command::ALL.iter().all(|c| !c.tip().is_empty()));
        let on = Enabled { back: true, forward: false, undo: true, redo: false };
        assert!(on.of(Command::Back) && !on.of(Command::Forward) && on.of(Command::Undo) && !on.of(Command::Redo));
    }
}
