#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
//! NoteCraft for Android.
//!
//! The APK is a plain `android.app.NativeActivity` with no Java code. Android loads this
//! library and calls `android_main` ([`activity`]), which finds the app's folders, refreshes
//! the bundled Welcome notebook payload ([`welcome`]) and runs the same egui window as the
//! desktop app. `android/build.sh` builds the APK; `android/README.md` says how.
//!
//! [`welcome`] and [`storage`] are platform neutral, so the host runs their tests too.

#[cfg(target_os = "android")]
mod activity;
pub mod storage;
pub mod welcome;
