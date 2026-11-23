//! The emoji a block header holds: the choices the `+` popup offers, and the toggle that
//! adds or removes one, and the header row that draws them.

/// The emoji the popup offers: a curated set of common emoji, drawn with the system's emoji font.
pub const CHOICES: &[&str] = &[
    "\u{1F600}",
    "\u{1F603}",
    "\u{1F604}",
    "\u{1F601}",
    "\u{1F606}",
    "\u{1F605}",
    "\u{1F602}",
    "\u{1F642}",
    "\u{1F609}",
    "\u{1F60A}",
    "\u{1F60D}",
    "\u{1F970}",
    "\u{1F618}",
    "\u{1F60E}",
    "\u{1F914}",
    "\u{1F610}",
    "\u{1F634}",
    "\u{1F622}",
    "\u{1F62D}",
    "\u{1F621}",
    "\u{1F631}",
    "\u{1F92F}",
    "\u{1F973}",
    "\u{1F607}",
    "\u{1F44D}",
    "\u{1F44E}",
    "\u{1F44F}",
    "\u{1F64F}",
    "\u{1F44C}",
    "\u{270C}\u{FE0F}",
    "\u{1F91D}",
    "\u{1F4AA}",
    "\u{1F440}",
    "\u{1F64C}",
    "\u{2764}\u{FE0F}",
    "\u{1F9E1}",
    "\u{1F49B}",
    "\u{1F49A}",
    "\u{1F499}",
    "\u{1F49C}",
    "\u{1F5A4}",
    "\u{1F494}",
    "\u{1F4AF}",
    "\u{2B50}",
    "\u{1F389}",
    "\u{1F38A}",
    "\u{1F525}",
    "\u{2728}",
    "\u{2705}",
    "\u{274C}",
    "\u{26A0}\u{FE0F}",
    "\u{1F4A1}",
    "\u{1F4CC}",
    "\u{1F4DD}",
    "\u{1F4C5}",
    "\u{1F4CE}",
    "\u{1F512}",
    "\u{1F680}",
    "\u{1F3AF}",
    "\u{1F3C6}",
    "\u{2615}",
    "\u{1F355}",
    "\u{1F31F}",
    "\u{1F308}",
    "\u{1F331}",
    "\u{2600}\u{FE0F}",
    "\u{1F319}",
    "\u{1F3E0}",
    "\u{1F4F7}",
    "\u{1F3B5}",
];

/// The tabs above the grid: each has a name and the range of `CHOICES` it shows, in order. Together
/// they cover every choice, and a search looks through all of them.
pub const TABS: &[(&str, std::ops::Range<usize>)] = &[("Smileys", 0..24), ("Hands", 24..34), ("Hearts", 34..44), ("Symbols", 44..70)];

/// A short English name for each choice, in the same order as `CHOICES`; the search matches them.
pub const CHOICE_NAMES: &[&str] = &[
    "grinning face",
    "smiling face",
    "grinning",
    "beaming",
    "grinning squinting",
    "sweat smile",
    "joy",
    "slightly smiling",
    "wink",
    "blush",
    "heart eyes",
    "smiling with hearts",
    "kiss",
    "sunglasses",
    "thinking",
    "neutral face",
    "sleeping",
    "cry",
    "sob",
    "angry",
    "scream",
    "exploding head",
    "partying face",
    "halo",
    "thumbs up",
    "thumbs down",
    "clap",
    "pray",
    "ok hand",
    "victory hand",
    "handshake",
    "flexed biceps",
    "eyes",
    "raised hands",
    "red heart",
    "orange heart",
    "yellow heart",
    "green heart",
    "blue heart",
    "purple heart",
    "black heart",
    "broken heart",
    "hundred points",
    "star",
    "party popper",
    "confetti ball",
    "fire",
    "sparkles",
    "check mark",
    "cross mark",
    "warning",
    "light bulb",
    "pushpin",
    "memo",
    "calendar",
    "paperclip",
    "lock",
    "rocket",
    "bullseye",
    "trophy",
    "hot beverage",
    "pizza",
    "glowing star",
    "rainbow",
    "seedling",
    "sun",
    "crescent moon",
    "house",
    "camera",
    "musical notes",
];

/// The height of the header row at 100 percent zoom, in points.
pub const HEADER_ROW: f32 = 24.0;

/// The id under which a box says which of its blocks has its header drawn above the box's bar.
pub fn box_first_header_id() -> egui::Id {
    egui::Id::new("box-first-header")
}

/// Whether the fonts in use can draw this emoji. Where the system has no emoji font they cannot,
/// and drawing one would fail.
fn can_draw(ui: &egui::Ui, text: &str) -> bool {
    ui.ctx().fonts_mut(|f| f.has_glyphs(&egui::FontId::proportional(14.0), text))
}

/// The English name of a choice, shown in place of its glyph where the glyph cannot be drawn.
fn name_of(emoji: &str) -> String {
    CHOICES.iter().position(|c| *c == emoji).and_then(|at| CHOICE_NAMES.get(at)).map_or_else(|| "emoji".to_string(), |n| (*n).to_string())
}

/// The popup that is open, if any: the block it belongs to. This is view state only; the emoji
/// themselves belong to the block.
type Open = Option<(std::path::PathBuf, usize)>;

fn open_id() -> egui::Id {
    egui::Id::new("block-emoji-open")
}

/// Draws the header row above block `i`: a round `+` at the left, then the block's emoji beside
/// it. The `+` opens the popup; choosing an emoji adds it, and choosing it again removes it.
/// `emoji` is what the block holds. When `editable` is false the `+` is drawn but does nothing.
/// Returns the block's new emoji when the person chose one.
pub fn header(ui: &mut egui::Ui, page_dir: &std::path::Path, i: usize, emoji: &[String], editable: bool, zoom: f32) -> Option<Vec<String>> {
    let key = (page_dir.to_path_buf(), i);
    let size = 18.0 * zoom;
    let mut removed: Option<usize> = None;
    // The row is exactly `HEADER_ROW` tall at 100 percent and grows with the zoom. It is drawn in a
    // child area and the page's cursor moves by exactly that much: the height of an egui row follows
    // its contents, and the page's item spacing is not the same at every zoom, so neither is relied on.
    let height = HEADER_ROW * zoom;
    let area_rect = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(ui.available_width(), height));
    let mut head = ui.new_child(egui::UiBuilder::new().max_rect(area_rect).layout(egui::Layout::left_to_right(egui::Align::Center)));
    let plus_rect = {
        let ui = &mut head;
        // Forced to a square, so the round corners make a circle and not an ellipse.
        let button = ui
            .add_enabled_ui(editable, |ui| {
                // i18n-exempt: "+" is the glyph of the add button, not a word.
                ui.add_sized([size, size], egui::Button::new("+").corner_radius(egui::CornerRadius::same(255)))
            })
            .inner;
        // Named for the block it belongs to, so tests and screen readers can tell the headers apart.
        button.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, editable, format!("Add emoji to block {}", i + 1)));
        if button.clicked() {
            let now = ui.ctx().data(|d| d.get_temp::<Open>(open_id()).unwrap_or_default());
            let next: Open = if now.as_ref() == Some(&key) { None } else { Some(key.clone()) };
            if next.is_some() {
                crate::usage::record(crate::usage::Level::Usage, "block_header_plus_opened", &[("block", &i.to_string())]);
            }
            ui.ctx().data_mut(|d| d.insert_temp(open_id(), next));
        }
        for (at, e) in emoji.iter().enumerate() {
            // Clicking an emoji on the block takes it off the block.
            // Where the system has no emoji font the glyph cannot be drawn: show the name instead.
            let shown = if can_draw(ui, e) { e.clone() } else { name_of(e) };
            let chip = ui.add(egui::Label::new(shown).sense(egui::Sense::click()));
            chip.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, editable, format!("Remove {e} from block {}", i + 1)));
            // The pointing hand says the emoji can be clicked, and clicking takes it off.
            let chip = if editable { chip.on_hover_cursor(egui::CursorIcon::PointingHand) } else { chip };
            if editable && chip.clicked() {
                removed = Some(at);
            }
        }
        button.rect
    };
    ui.add_space(height);
    let plus = egui::InnerResponse::new(plus_rect, ui.interact(area_rect, ui.id().with(("block-header-row", i)), egui::Sense::hover()));
    let open = ui.ctx().data(|d| d.get_temp::<Open>(open_id()).unwrap_or_default());
    if let Some(at) = removed {
        let mut next = emoji.to_vec();
        let gone = next.remove(at);
        crate::usage::record(crate::usage::Level::Usage, "block_emoji_removed", &[("block", &i.to_string()), ("emoji", &gone)]);
        return Some(next);
    }
    if open.as_ref() != Some(&key) || !editable {
        return None;
    }
    let mut pick = None;
    let area = egui::Area::new(ui.id().with(("block-emoji-popup", i))).order(egui::Order::Foreground).fixed_pos(plus.inner.left_bottom()).show(
        ui.ctx(),
        |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                // A small search field: typing a word narrows the grid to the emoji named by it.
                let search_id = open_id().with("search");
                let mut query: String = ui.data(|d| d.get_temp::<String>(search_id)).unwrap_or_default();
                let field = ui.add(egui::TextEdit::singleline(&mut query).hint_text("Search emoji").desired_width(260.0 * zoom));
                field.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextInput, true, "Search emoji"));
                ui.data_mut(|d| d.insert_temp(search_id, query.clone()));
                let needle = query.trim().to_lowercase();
                // The tab shown: the last one picked, or the first; ignored while searching.
                let tab_id = open_id().with("tab");
                let mut tab: usize = ui.data(|d| d.get_temp::<usize>(tab_id)).unwrap_or(0);
                if needle.is_empty() {
                    ui.horizontal(|ui| {
                        for (n, (name, _)) in TABS.iter().enumerate() {
                            if ui.selectable_label(tab == n, *name).clicked() {
                                tab = n;
                            }
                        }
                    });
                    ui.data_mut(|d| d.insert_temp(tab_id, tab));
                }
                let shown = if needle.is_empty() { TABS[tab.min(TABS.len() - 1)].1.clone() } else { 0..CHOICES.len() };
                // Wide and tall enough to show every choice; the grid scrolls only past that.
                egui::ScrollArea::vertical().max_height(300.0 * zoom).show(ui, |ui| {
                    ui.set_max_width(10.0 * 36.0 * zoom);
                    ui.horizontal_wrapped(|ui| {
                        for (index, (c, name)) in CHOICES.iter().zip(CHOICE_NAMES).enumerate() {
                            if !shown.contains(&index) {
                                continue;
                            }
                            if !needle.is_empty() && !name.contains(needle.as_str()) {
                                continue;
                            }
                            let label = if can_draw(ui, c) { c.to_string() } else { name_of(c) };
                            if ui.add(egui::Button::new(label).min_size(egui::vec2(32.0 * zoom, 28.0 * zoom))).clicked() {
                                pick = Some(*c);
                            }
                        }
                    });
                });
            });
        },
    );
    if let Some(c) = pick {
        ui.ctx().data_mut(|d| d.insert_temp::<Open>(open_id(), None));
        let mut next = emoji.to_vec();
        if add(&mut next, c) {
            crate::usage::record(crate::usage::Level::Usage, "block_emoji_added", &[("block", &i.to_string()), ("emoji", c)]);
            return Some(next);
        }
        return None;
    }
    if area.response.clicked_elsewhere() && !plus.inner.contains(ui.input(|i| i.pointer.hover_pos()).unwrap_or_default()) {
        crate::usage::record(crate::usage::Level::Usage, "block_emoji_popup_closed", &[("block", &i.to_string()), ("how", "outside")]);
        ui.ctx().data_mut(|d| d.insert_temp::<Open>(open_id(), None));
    }
    None
}

/// Adds `pick` to a block's emoji, after the ones it has. Returns false, and changes nothing, when
/// the block already holds it.
pub fn add(emojis: &mut Vec<String>, pick: &str) -> bool {
    if emojis.iter().any(|e| e == pick) {
        return false;
    }
    emojis.push(pick.to_string());
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn choosing_an_emoji_adds_it_to_the_block() {
        let mut e = Vec::new();
        assert!(add(&mut e, CHOICES[0]));
        assert_eq!(e, s(&[CHOICES[0]]));
    }

    #[test]
    fn choosing_an_emoji_the_block_already_holds_changes_nothing() {
        let mut e = s(&[CHOICES[0]]);
        assert!(!add(&mut e, CHOICES[0]));
        assert_eq!(e, s(&[CHOICES[0]]));
    }

    #[test]
    fn new_emoji_go_after_the_existing_ones() {
        let mut e = s(&[CHOICES[3]]);
        assert!(add(&mut e, CHOICES[4]));
        assert_eq!(e, s(&[CHOICES[3], CHOICES[4]]));
    }

    #[test]
    fn every_choice_has_a_name_to_search_by() {
        assert_eq!(CHOICES.len(), CHOICE_NAMES.len());
        assert!(CHOICE_NAMES.iter().all(|n| !n.is_empty()));
    }

    #[test]
    fn the_tabs_cover_every_choice_once() {
        let mut next = 0;
        for (_, range) in TABS {
            assert_eq!(range.start, next, "the tabs run on in order");
            next = range.end;
        }
        assert_eq!(next, CHOICES.len());
    }

    #[test]
    fn the_choices_are_distinct() {
        for (i, a) in CHOICES.iter().enumerate() {
            assert!(!CHOICES[i + 1..].contains(a), "duplicate choice {a}");
        }
    }
}
