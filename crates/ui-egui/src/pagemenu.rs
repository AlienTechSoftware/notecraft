//! The page's own menu, under the right button.
//!
//! Nothing is laid over the page to carry it. A widget that senses clicks would take the
//! primary click away from the words, the ink and the caret - the same mistake that made
//! seven of eight pens unusable an hour earlier - and a widget that does not sense clicks
//! cannot carry a context menu at all. So the secondary click is watched for directly and
//! the menu is drawn where it fell.

use crate::ribbon::Act;
use egui::Ui;

/// Where the menu is open, if it is. Kept in the ui's own store so it survives frames
/// without the window having to hold it.
fn state(ui: &Ui) -> Option<egui::Pos2> {
    ui.data(|d| d.get_temp::<egui::Pos2>(id()))
}

fn id() -> egui::Id {
    egui::Id::new("page-menu")
}

/// Draws the page's menu when the page has been right-clicked, and puts what was picked
/// into `acts`.
pub fn show(ui: &mut Ui, body: egui::Rect, acts: &mut Vec<Act>) {
    let pointer = ui.ctx().pointer_latest_pos();
    if ui.input(|i| i.pointer.secondary_clicked())
        && let Some(at) = pointer.filter(|p| body.contains(*p))
    {
        ui.data_mut(|d| d.insert_temp(id(), at));
    }
    let Some(at) = state(ui) else { return };

    let mut picked = false;
    let area = egui::Area::new(id().with("area"))
        .order(egui::Order::Foreground)
        .fixed_pos(at)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(176.0);
                let mut item = |ui: &mut Ui, label: &str, act: Act| {
                    if ui.button(label).clicked() {
                        acts.push(act);
                        picked = true;
                    }
                };
                item(ui, "Paste", Act::Paste);
                item(ui, "Cut", Act::Clipboard("cut"));
                item(ui, "Copy", Act::Clipboard("copy"));
                ui.separator();
                item(ui, "Table", Act::Insert("table"));
                item(ui, "Link", Act::Insert("link"));
                item(ui, "Equation", Act::Insert("equation"));
                item(ui, "Date and time", Act::Insert("datetime"));
                ui.separator();
                item(ui, "Picture from a file", Act::AddPicture);
                item(ui, "Attach a file", Act::AddFile);
            });
        })
        .response;

    let out = ui.input(|i| i.pointer.any_click()) && !area.rect.contains(pointer.unwrap_or_default());
    if picked || out || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        ui.data_mut(|d| d.remove::<egui::Pos2>(id()));
    }
}
