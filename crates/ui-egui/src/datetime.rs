//! Date and time controls for the top line of a page: a date picker, a time picker, and both
//! together. Pure calendar arithmetic here; the widgets are drawn in the same module.

use egui::{Id, Ui};

use crate::theme::Tokens;

/// A calendar day.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

/// A time of day, 24-hour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Time {
    pub hour: u32,
    pub minute: u32,
}

/// The date as the page shows it.
pub fn date_text(date: Date) -> String {
    let iso = format!("{:04}-{:02}-{:02}T12:00:00Z", date.year, date.month, date.day);
    notecraft_model::time::date_from_iso(&iso).unwrap_or(iso)
}

/// The time as the page shows it.
pub fn time_text(time: Time) -> String {
    format!("{:02}:{:02}", time.hour, time.minute)
}

/// A date picker: a calendar button, the date in grey, and a month calendar with a Today button.
/// Returns true when the date changed.
/// The catalogue name of a weekday abbreviation (0 is Sunday).
fn weekday_name(index: usize) -> String {
    match index {
        0 => crate::tl_id!("ui.weekday_short_0"),
        1 => crate::tl_id!("ui.weekday_short_1"),
        2 => crate::tl_id!("ui.weekday_short_2"),
        3 => crate::tl_id!("ui.weekday_short_3"),
        4 => crate::tl_id!("ui.weekday_short_4"),
        5 => crate::tl_id!("ui.weekday_short_5"),
        _ => crate::tl_id!("ui.weekday_short_6"),
    }
    .to_string()
}

/// The catalogue name of `month` (1 to 12).
fn month_name(month: u32) -> String {
    match month {
        1 => crate::tl_id!("ui.month_1"),
        2 => crate::tl_id!("ui.month_2"),
        3 => crate::tl_id!("ui.month_3"),
        4 => crate::tl_id!("ui.month_4"),
        5 => crate::tl_id!("ui.month_5"),
        6 => crate::tl_id!("ui.month_6"),
        7 => crate::tl_id!("ui.month_7"),
        8 => crate::tl_id!("ui.month_8"),
        9 => crate::tl_id!("ui.month_9"),
        10 => crate::tl_id!("ui.month_10"),
        11 => crate::tl_id!("ui.month_11"),
        _ => crate::tl_id!("ui.month_12"),
    }
    .to_string()
}

pub fn date_picker(ui: &mut Ui, id: Id, t: &Tokens, value: &mut Date, today: Date) -> bool {
    let mut changed = false;
    let open_id = id.with("open");
    let month_id = id.with("month");
    let mut open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
    let mut shown = ui.data(|d| d.get_temp::<(i32, u32)>(month_id)).unwrap_or((value.year, value.month));
    ui.horizontal(|ui| {
        let button = ui.add(egui::Button::new("").min_size(egui::vec2(18.0, 18.0)));
        crate::icons::paint(&ui.painter_at(button.rect), button.rect.shrink(2.0), crate::icons::Icon::Calendar, t.text);
        let label = crate::tl_id!("ui.pick_date").to_string();
        let shown_text = date_text(*value);
        let info_text = shown_text.clone();
        button.widget_info(move || {
            let mut info = egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label.clone());
            info.current_text_value = Some(info_text.clone());
            info
        });
        ui.label(egui::RichText::new(&shown_text).color(t.muted));
        if button.clicked() {
            open = !open;
            shown = (value.year, value.month);
        }
        if open {
            let area = egui::Area::new(id.with("popup")).order(egui::Order::Foreground).fixed_pos(button.rect.left_bottom()).show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        // i18n-exempt: "<" is the arrow of the previous-month button, not a word.
                        if ui.button("<").clicked() {
                            shown = if shown.1 == 1 { (shown.0 - 1, 12) } else { (shown.0, shown.1 - 1) };
                        }
                        ui.label(format!("{} {}", month_name(shown.1), shown.0));
                        // i18n-exempt: ">" is the arrow of the next-month button, not a word.
                        if ui.button(">").clicked() {
                            shown = if shown.1 == 12 { (shown.0 + 1, 1) } else { (shown.0, shown.1 + 1) };
                        }
                    });
                    ui.horizontal(|ui| {
                        for index in 0..7 {
                            ui.label(weekday_name(index));
                        }
                    });
                    for row in month_grid(shown.0, shown.1) {
                        ui.horizontal(|ui| {
                            for cell in row {
                                match cell {
                                    Some(day) => {
                                        let picked = day == value.day && shown == (value.year, value.month);
                                        if ui.selectable_label(picked, day.to_string()).clicked() {
                                            *value = Date { year: shown.0, month: shown.1, day };
                                            changed = true;
                                            open = false;
                                        }
                                    }
                                    None => {
                                        // i18n-exempt: a blank cell keeps the calendar columns lined up.
                                        ui.label("  ");
                                    }
                                }
                            }
                        });
                    }
                    if ui.button(crate::tl_id!("ui.today")).clicked() {
                        *value = today;
                        changed = true;
                        open = false;
                    }
                });
            });
            if ui.input(|i| i.pointer.any_click()) && !area.response.hovered() && !button.hovered() {
                open = false;
            }
        }
    });
    ui.data_mut(|d| {
        d.insert_temp(open_id, open);
        d.insert_temp(month_id, shown);
    });
    changed
}

/// A time picker: a clock button, the time in grey, and a list of times to choose from.
/// Returns true when the time changed.
pub fn time_picker(ui: &mut Ui, id: Id, t: &Tokens, value: &mut Time) -> bool {
    let mut changed = false;
    let open_id = id.with("open");
    let mut open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
    let scroll_id = id.with("scroll");
    let mut scroll = ui.data(|d| d.get_temp::<bool>(scroll_id)).unwrap_or(false);
    ui.horizontal(|ui| {
        let button = ui.add(egui::Button::new("").min_size(egui::vec2(18.0, 18.0)));
        crate::icons::paint(&ui.painter_at(button.rect), button.rect.shrink(2.0), crate::icons::Icon::Clock, t.text);
        let label = crate::tl_id!("ui.pick_time").to_string();
        let shown_text = time_text(*value);
        let info_text = shown_text.clone();
        button.widget_info(move || {
            let mut info = egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label.clone());
            info.current_text_value = Some(info_text.clone());
            info
        });
        ui.label(egui::RichText::new(&shown_text).color(t.muted));
        if button.clicked() {
            open = !open;
            scroll = open;
        }
        if open {
            let area = egui::Area::new(id.with("popup")).order(egui::Order::Foreground).fixed_pos(button.rect.left_bottom()).show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                        // The half hour the time falls in is the current one: the list opens on it.
                        let current = Time { hour: value.hour, minute: if value.minute >= 30 { 30 } else { 0 } };
                        for step in 0..48u32 {
                            let time = Time { hour: step / 2, minute: (step % 2) * 30 };
                            let row = ui.selectable_label(time == current, time_text(time));
                            if time == current && scroll {
                                row.scroll_to_me(Some(egui::Align::Center));
                            }
                            if row.clicked() {
                                *value = time;
                                changed = true;
                                open = false;
                            }
                        }
                    });
                });
            });
            if ui.input(|i| i.pointer.any_click()) && !area.response.hovered() && !button.hovered() {
                open = false;
            }
        }
    });
    ui.data_mut(|d| {
        d.insert_temp(open_id, open);
        d.insert_temp(scroll_id, false);
    });
    changed
}

/// The date and the time together, side by side.
pub fn date_time_picker(ui: &mut Ui, id: Id, t: &Tokens, date: &mut Date, time: &mut Time, today: Date) -> bool {
    let a = date_picker(ui, id.with("date"), t, date, today);
    let b = time_picker(ui, id.with("time"), t, time);
    a || b
}

/// Days in `month` (1 to 12) of `year`, leap years included.
pub fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 31,
    }
}

fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// The weekday of the first day of `month` in `year`: 0 for Sunday to 6 for Saturday.
pub fn first_weekday(year: i32, month: u32) -> u32 {
    // Sakamoto's method: 0 is Sunday.
    const OFFSET: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if month < 3 { year - 1 } else { year };
    let day = (y + y / 4 - y / 100 + y / 400 + OFFSET[month as usize - 1] + 1).rem_euclid(7);
    day as u32
}

/// The month as six weeks of seven days, Sunday first: `None` for the days outside the month.
pub fn month_grid(year: i32, month: u32) -> [[Option<u32>; 7]; 6] {
    let mut grid = [[None; 7]; 6];
    let first = first_weekday(year, month);
    for day in 1..=days_in_month(year, month) {
        let slot = first + day - 1;
        grid[(slot / 7) as usize][(slot % 7) as usize] = Some(day);
    }
    grid
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    const NOV_12: Date = Date { year: 2026, month: 11, day: 12 };
    const NOON: Time = Time { hour: 15, minute: 16 };

    fn tokens() -> Tokens {
        crate::theme::Theme::all()[0].tokens()
    }

    #[test]
    fn the_date_picker_shows_its_date_and_opens_a_calendar_to_change_it() {
        let t = tokens();
        let mut h = Harness::new_ui_state(
            |ui, state: &mut Date| {
                date_picker(ui, Id::new("d"), &t, state, NOV_12);
            },
            NOV_12,
        );
        h.run();
        assert_eq!(h.get_by_label("Pick a date").value().as_deref(), Some(date_text(NOV_12).as_str()));
        h.get_by_label("Pick a date").click();
        h.run();
        h.get_by_label("26").click();
        h.run();
        assert_eq!(*h.state(), Date { year: 2026, month: 11, day: 26 }, "picking a day in the calendar changes the date");
    }

    #[test]
    fn the_today_button_sets_the_date_to_today() {
        let t = tokens();
        let mut h = Harness::new_ui_state(
            |ui, state: &mut Date| {
                date_picker(ui, Id::new("d"), &t, state, NOV_12);
            },
            Date { year: 2026, month: 10, day: 3 },
        );
        h.run();
        h.get_by_label("Pick a date").click();
        h.run();
        h.get_by_label("Today").click();
        h.run();
        assert_eq!(*h.state(), NOV_12);
    }

    #[test]
    fn the_time_picker_shows_its_time_and_changes_it_from_a_list() {
        let t = tokens();
        let mut h = Harness::new_ui_state(
            |ui, state: &mut Time| {
                time_picker(ui, Id::new("t"), &t, state);
            },
            NOON,
        );
        h.run();
        assert_eq!(h.get_by_label("Pick a time").value().as_deref(), Some("15:16"));
        h.get_by_label("Pick a time").click();
        h.run();
        h.get_by_label("16:00").click();
        h.run();
        assert_eq!(*h.state(), Time { hour: 16, minute: 0 });
    }

    #[test]
    fn the_date_and_time_picker_has_both_controls() {
        let t = tokens();
        let mut h = Harness::new_ui_state(
            |ui, state: &mut (Date, Time)| {
                date_time_picker(ui, Id::new("dt"), &t, &mut state.0, &mut state.1, NOV_12);
            },
            (NOV_12, NOON),
        );
        h.run();
        assert!(h.query_all_by_label("Pick a date").count() == 1 && h.query_all_by_label("Pick a time").count() == 1);
    }

    #[test]
    fn months_have_their_days_and_leap_years_are_right() {
        assert_eq!(days_in_month(2026, 11), 30);
        assert_eq!(days_in_month(2026, 12), 31);
        assert_eq!(days_in_month(2026, 2), 28);
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2000, 2), 29);
        assert_eq!(days_in_month(1900, 2), 28);
    }

    #[test]
    fn the_first_day_of_the_month_has_its_weekday() {
        // 1 November 2026 is a Sunday; 1 December 2026 is a Tuesday (12 November is a Thursday).
        assert_eq!(first_weekday(2026, 11), 0);
        assert_eq!(first_weekday(2026, 12), 2);
        // 1 January 2000 was a Saturday.
        assert_eq!(first_weekday(2000, 1), 6);
    }

    #[test]
    fn the_grid_places_each_day_under_its_weekday() {
        let grid = month_grid(2026, 11);
        assert_eq!(grid[0], [Some(1), Some(2), Some(3), Some(4), Some(5), Some(6), Some(7)]);
        assert_eq!(grid[3][4], Some(26), "the 26th is a Thursday, in the fourth week");
        let days: Vec<u32> = grid.iter().flatten().flatten().copied().collect();
        assert_eq!(days, (1..=30).collect::<Vec<u32>>());
    }
}
