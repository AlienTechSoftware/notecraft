//! Where the system bars are, as egui's safe area.
//!
//! NativeActivity hands NoteCraft the whole window's surface, the strips behind the status bar
//! and the navigation bar included, so without this the title band and the ribbon tabs sit
//! under the clock and the status bar sits under Back and Home. Android reports the part of
//! the window that the bars leave free as the content rectangle (`AndroidApp::content_rect`);
//! the rest is passed to egui as `RawInput::safe_area_insets`, and egui lays every panel and
//! window out inside it.

/// Insets in points, one per edge of the window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Insets {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

/// The insets, in points, that leave `content` (in pixels, as `left, top, right, bottom`) free
/// in a window `width` by `height` pixels at `pixels_per_point`. `None` while Android has not
/// reported a content rectangle yet, or reports one that does not fit the window.
pub fn safe_area(content: (i32, i32, i32, i32), width: f32, height: f32, pixels_per_point: f32) -> Option<Insets> {
    let (left, top, right, bottom) = content;
    let fits = 0 <= left && left < right && 0 <= top && top < bottom && right as f32 <= width + 0.5 && bottom as f32 <= height + 0.5;
    if !fits || pixels_per_point <= 0.0 {
        return None;
    }
    let points = |px: f32| (px / pixels_per_point).max(0.0);
    Some(Insets { left: points(left as f32), top: points(top as f32), right: points(width - right as f32), bottom: points(height - bottom as f32) })
}

#[cfg(test)]
mod tests {
    use super::safe_area;

    /// A 1080 × 2424 phone at 2.625 px per point, with a 63 px status bar and a 63 px
    /// navigation bar, like the Pixel in issue #315's screenshot.
    #[test]
    fn the_status_and_navigation_bars_become_the_top_and_bottom_insets() {
        let insets = safe_area((0, 63, 1080, 2361), 1080.0, 2424.0, 2.625).expect("insets");
        assert!((insets.top - 24.0).abs() < 0.01, "{insets:?}");
        assert!((insets.bottom - 24.0).abs() < 0.01, "{insets:?}");
        assert_eq!((insets.left, insets.right), (0.0, 0.0));
    }

    /// In landscape the navigation bar can sit at a side, and a camera cut-out can take a
    /// side too.
    #[test]
    fn a_bar_at_the_side_becomes_a_side_inset() {
        let insets = safe_area((0, 63, 2276, 1080), 2424.0, 1080.0, 2.0).expect("insets");
        assert!((insets.right - 74.0).abs() < 0.01, "{insets:?}");
        assert_eq!(insets.left, 0.0);
        assert!((insets.top - 31.5).abs() < 0.01, "{insets:?}");
        assert_eq!(insets.bottom, 0.0);
    }

    /// Before Android reports the content rectangle it is empty: no insets rather than a
    /// window squeezed to nothing.
    #[test]
    fn no_content_rectangle_yet_means_no_insets() {
        assert_eq!(safe_area((0, 0, 0, 0), 1080.0, 2424.0, 2.625), None);
        assert_eq!(safe_area((0, 63, 4000, 2361), 1080.0, 2424.0, 2.625), None, "wider than the window");
        assert_eq!(safe_area((0, 63, 1080, 2361), 1080.0, 2424.0, 0.0), None);
    }

    /// A window that fills the screen with no bars over it has no insets.
    #[test]
    fn a_window_with_no_bars_has_zero_insets() {
        let insets = safe_area((0, 0, 1080, 2424), 1080.0, 2424.0, 2.625).expect("insets");
        assert_eq!((insets.left, insets.top, insets.right, insets.bottom), (0.0, 0.0, 0.0, 0.0));
    }
}
