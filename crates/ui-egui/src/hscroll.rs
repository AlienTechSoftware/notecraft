//! The page's sideways scroll bar (issue #188).
//!
//! The page scrolls up and down in an ordinary scroll area, but sideways it is a canvas: the
//! words, the boxes and the ink move together by the canvas offset (Shift+wheel and a middle
//! drag change it). When what is on the page reaches past the sides of the view, for example a
//! box dragged off to the right or a page zoomed in on a narrow window, this bar shows how much
//! there is and moves the view along it. It changes the same canvas offset, so the bar, the
//! wheel and the middle drag never disagree.

use egui::{Id, Rect, Sense, pos2};

/// Where the thumb sits on the track, as fractions of the track's length.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Thumb {
    /// Where the thumb starts, from 0 (the left end of the track).
    pub start: f32,
    /// How long the thumb is, from 0 to 1 (the whole track).
    pub len: f32,
    /// Screen points of canvas per point of track: how far the view moves when the thumb moves.
    pub scale: f32,
    /// How far the view can move left (a negative number or zero) and right before it reaches
    /// the ends of what there is to see.
    pub room: (f32, f32),
}

/// The thumb for content spanning `content` (left and right on screen, as drawn this frame) seen
/// through a view spanning `view`, or `None` when everything fits and no bar is needed.
pub fn thumb(content: (f32, f32), view: (f32, f32), track: f32) -> Option<Thumb> {
    let (v0, v1) = view;
    let width = v1 - v0;
    if !(width.is_finite() && content.0.is_finite() && content.1.is_finite()) || width <= 0.0 || track <= 0.0 {
        return None;
    }
    // Everything there is to see: the content, and the view itself, which may have been panned
    // out past the content into open canvas.
    let (lo, hi) = (content.0.min(v0), content.1.max(v1));
    let total = hi - lo;
    if total <= width + 0.5 {
        return None;
    }
    Some(Thumb { start: (v0 - lo) / total, len: width / total, scale: total / track, room: (lo - v0, hi - v1) })
}

/// How far the canvas offset may move by `pan` without the view leaving what there is to see:
/// the bar scrolls to the ends of the content and no further, as a scroll bar does.
pub fn clamp_pan(pan: f32, t: &Thumb) -> f32 {
    // The view moves right by `-pan`; it may go as far left as `room.0` and as far right as `room.1`.
    let shift = (-pan).clamp(t.room.0.min(0.0), t.room.1.max(0.0));
    if shift == 0.0 { 0.0 } else { -shift }
}

/// What the bar did this frame.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Shown {
    /// Where the bar was drawn, if it was.
    pub track: Option<Rect>,
    /// Where its thumb was drawn, if it was.
    pub handle: Option<Rect>,
    /// How far to move the canvas offset sideways, in screen points.
    pub pan: f32,
}

/// Draws the bar along the bottom of `view` when `content` is wider than it, and returns how far
/// the canvas should move. `reserve_right` keeps the corner free for the vertical bar.
///
/// The bar is an area of its own above the page, so a pen in hand, whose ink layer covers the
/// whole page, does not draw where the bar is pressed.
pub fn show(ctx: &egui::Context, id: Id, view: Rect, content: (f32, f32), reserve_right: f32) -> Shown {
    let style = ctx.global_style().spacing.scroll;
    let height = style.bar_width.max(6.0);
    let track = Rect::from_min_max(
        pos2(view.min.x, view.max.y - height - style.bar_outer_margin),
        pos2(view.max.x - reserve_right, view.max.y - style.bar_outer_margin),
    );
    let Some(t) = thumb(content, (view.min.x, view.max.x), track.width()) else {
        return Shown::default();
    };
    let min_len = style.handle_min_length.min(track.width());
    let len = (t.len * track.width()).max(min_len);
    // A thumb lengthened to its minimum moves over less track, so the scale follows it.
    let travel = (track.width() - len).max(1.0);
    let free = (1.0 - t.len).max(f32::EPSILON);
    let x = track.min.x + (t.start / free).clamp(0.0, 1.0) * travel;
    let handle = Rect::from_min_max(pos2(x, track.min.y), pos2(x + len, track.max.y));
    // Canvas points per point of thumb travel.
    let scale = (t.scale * track.width() * free) / travel;
    let mut pan = 0.0;
    egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(track.min).constrain(false).movable(false).show(ctx, |ui| {
        let (_, response) = ui.allocate_exact_size(track.size(), Sense::click_and_drag());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ScrollBar, true, crate::tl_id!("ui.scroll_the_page_sideways")));
        if response.dragged() {
            pan -= response.drag_delta().x * scale;
        } else if response.clicked()
            && let Some(at) = response.interact_pointer_pos()
            && !handle.contains(at)
        {
            // A click on the track beside the thumb pages one view that way, as scroll bars do.
            let page = view.width() * 0.9;
            pan += if at.x < handle.min.x { page } else { -page };
        }
        let visuals = ui.visuals();
        let state = if response.is_pointer_button_down_on() {
            &visuals.widgets.active
        } else if response.hovered() {
            &visuals.widgets.hovered
        } else {
            &visuals.widgets.inactive
        };
        let opacity = if response.hovered() || response.dragged() { style.interact_handle_opacity } else { style.active_handle_opacity };
        let painter = ui.painter();
        painter.rect_filled(track, state.corner_radius, visuals.extreme_bg_color.gamma_multiply(style.active_background_opacity.max(0.4)));
        let colour = if style.foreground_color { state.fg_stroke.color } else { state.bg_fill };
        painter.rect_filled(handle, state.corner_radius, colour.gamma_multiply(opacity.max(0.5)));
    });
    Shown { track: Some(track), handle: Some(handle), pan: clamp_pan(pan, &t) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_that_fits_needs_no_bar() {
        assert_eq!(thumb((100.0, 700.0), (0.0, 800.0), 790.0), None);
        // Exactly as wide as the view: still nothing to scroll to.
        assert_eq!(thumb((0.0, 800.0), (0.0, 800.0), 790.0), None);
    }

    #[test]
    fn content_past_the_right_edge_puts_the_thumb_at_the_left() {
        let t = thumb((0.0, 1600.0), (0.0, 800.0), 800.0).expect("a bar");
        assert_eq!(t.start, 0.0);
        assert_eq!(t.len, 0.5);
        assert_eq!(t.scale, 2.0);
        assert_eq!(t.room, (0.0, 800.0));
    }

    #[test]
    fn the_bar_stops_at_the_ends_of_the_content() {
        let t = thumb((0.0, 1600.0), (0.0, 800.0), 800.0).expect("a bar");
        // A drag far to the right moves the view only to the content's right end.
        assert_eq!(clamp_pan(-5000.0, &t), -800.0);
        // And nothing moves it left of the content's left end.
        assert_eq!(clamp_pan(300.0, &t), 0.0);
        assert_eq!(clamp_pan(-100.0, &t), -100.0);
        // A view already panned past the content can always come back.
        let past = thumb((-2000.0, -1200.0), (0.0, 800.0), 800.0).expect("a bar");
        assert_eq!(clamp_pan(5000.0, &past), 2000.0);
        assert_eq!(clamp_pan(-50.0, &past), 0.0);
    }

    #[test]
    fn a_view_panned_to_the_right_end_puts_the_thumb_at_the_right() {
        // The canvas moved 800 points left: the content now starts 800 points before the view.
        let t = thumb((-800.0, 800.0), (0.0, 800.0), 800.0).expect("a bar");
        assert_eq!(t.start + t.len, 1.0);
    }

    #[test]
    fn a_view_panned_into_open_canvas_can_still_come_back() {
        // Panned right past everything: the content is wholly to the left of the view.
        let t = thumb((-2000.0, -1200.0), (0.0, 800.0), 800.0).expect("a bar");
        assert!(t.start > 0.5, "{t:?}");
        assert!((t.start + t.len - 1.0).abs() < 1e-6, "{t:?}");
    }

    #[test]
    fn junk_sizes_give_no_bar() {
        assert_eq!(thumb((f32::NAN, 10.0), (0.0, 800.0), 800.0), None);
        assert_eq!(thumb((0.0, 1600.0), (0.0, 0.0), 800.0), None);
        assert_eq!(thumb((0.0, 1600.0), (0.0, 800.0), 0.0), None);
    }
}
