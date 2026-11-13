//! Try the date and time controls on their own, before they are on a page.
//!
//! `cargo run -p notecraft --example datetime_demo`
//!
//! Three controls: a date picker, a time picker, and both together. Click the calendar button to
//! pick a day (or Today), and the clock button to pick a half hour. The values are printed under
//! them as they change.

use eframe::egui;
use notecraft_ui_egui::datetime::{Date, Time, date_picker, date_text, date_time_picker, time_picker, time_text};

struct Demo {
    today: Date,
    date: Date,
    time: Time,
    combined_date: Date,
    combined_time: Time,
}

impl Default for Demo {
    fn default() -> Self {
        let today = Date { year: 2026, month: 11, day: 12 };
        Demo { today, date: today, time: Time { hour: 15, minute: 16 }, combined_date: today, combined_time: Time { hour: 15, minute: 16 } }
    }
}

impl eframe::App for Demo {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let t = notecraft_ui_egui::theme::Theme::all()[0].tokens();
        ui.heading("Date and time controls");
        ui.add_space(8.0);
        ui.label("Date picker");
        date_picker(ui, ui.make_persistent_id("demo-date"), &t, &mut self.date, self.today);
        ui.label(format!("Value: {}", date_text(self.date)));
        ui.add_space(12.0);
        ui.label("Time picker");
        time_picker(ui, ui.make_persistent_id("demo-time"), &t, &mut self.time);
        ui.label(format!("Value: {}", time_text(self.time)));
        ui.add_space(12.0);
        ui.label("Date and time together");
        date_time_picker(ui, ui.make_persistent_id("demo-both"), &t, &mut self.combined_date, &mut self.combined_time, self.today);
        ui.label(format!("Value: {} {}", date_text(self.combined_date), time_text(self.combined_time)));
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([520.0, 420.0]).with_title("NoteCraft date and time"),
        ..Default::default()
    };
    eframe::run_native("notecraft-datetime-demo", options, Box::new(|_cc| Ok(Box::<Demo>::default())))
}
