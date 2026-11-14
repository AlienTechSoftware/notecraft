//! Phone-width windows (issue #12).
//!
//! The desktop layout puts the notebooks, the ribbon and the page side by side, which needs a
//! window several hundred points wide. On a phone held upright, or any window narrower than
//! [`COMPACT_BELOW`], the window instead shows one thing at a time, as phone note apps do:
//!
//! - the notebooks list fills the window until a page is opened;
//! - an opened page then fills the window, and the page's Full page button, or Android's Back
//!   button, brings the list back;
//! - the ribbon stays folded to its tab names, and a tab shows its commands over the page;
//! - on a touch screen a finger drag scrolls the panes, and two fingers scroll the page (one
//!   finger still writes, selects and moves boxes on it).
//!
//! The same rules apply on every platform, so a narrow desktop window or the browser build
//! on a phone get the same layout as the Android app.

/// The width, in points, below which the window uses the one-thing-at-a-time layout. Narrow
/// tablets in portrait (600 points and up) keep the desktop layout.
pub const COMPACT_BELOW: f32 = 600.0;

/// Whether a window this many points wide uses the compact layout.
pub fn is_compact(width: f32) -> bool {
    width < COMPACT_BELOW
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phones_are_compact_and_tablets_are_not() {
        // A typical phone in portrait, a small phone, and a phone in landscape.
        assert!(is_compact(360.0));
        assert!(is_compact(412.0));
        assert!(is_compact(599.0));
        // A small tablet in portrait and the desktop window's smallest width.
        assert!(!is_compact(600.0));
        assert!(!is_compact(820.0));
    }
}
