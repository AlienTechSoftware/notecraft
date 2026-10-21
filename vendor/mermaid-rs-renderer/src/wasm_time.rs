//! WASM-safe timer for internal layout metrics.
//!
//! Layout timings are diagnostic only and do not affect the resulting SVG.
//! `std::time::Instant::now()` panics on `wasm32-unknown-unknown`, so use a zero-duration
//! timer there and retain the standard monotonic clock on native targets.

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use std::time::Instant;

#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy)]
pub(crate) struct Instant;

#[cfg(target_arch = "wasm32")]
impl Instant {
    #[inline]
    pub(crate) fn now() -> Self {
        Self
    }

    #[inline]
    pub(crate) fn elapsed(&self) -> std::time::Duration {
        std::time::Duration::ZERO
    }
}
