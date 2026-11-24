# NoteCraft patch

This is `winit` 0.30.13, the version egui and eframe 0.37 use, distributed under
the Apache-2.0 license in `LICENSE`. It is kept here because winit 0.30.13 does
not report two kinds of Wayland input that NoteCraft needs on Linux, and no
0.30.x release will add them: the 0.30 backport of pinch gestures
(rust-windowing/winit#4778) was closed without being merged.

Two changes, both in the Wayland backend only:

- `src/platform_impl/linux/wayland/seat/pointer/pointer_gesture.rs` reports
  touchpad pinch and rotation (`zwp_pointer_gestures_v1`) as
  `WindowEvent::PinchGesture` and `WindowEvent::RotationGesture`, so a pinch
  zooms the page (NoteCraft #64). It follows #4778.
- `src/platform_impl/linux/wayland/seat/tablet.rs` reports pen tablets
  (`zwp_tablet_manager_v2`) as touches with pressure, so a pen draws with
  pressure (NoteCraft #8). The pen's barrel buttons are mouse buttons, the
  lower one the middle and the upper one the right: a client that binds the
  tablet protocol gets them as tool buttons, and without this they did
  nothing, where every other program had a right click.

`src/event.rs` and `src/changelog/unreleased.md` document the events on Wayland.
Windows, macOS, X11 and the web build behave as the released crate does.

The copy is kept out of the workspace, so its tests run from its own folder, on Linux:

```sh
cd vendor/winit && cargo test --lib --features wayland,x11 --target-dir target -- tablet pointer_gesture
```

`cargo xtask pre-push` builds the copy as part of the app but does not run these tests.

When egui moves to a winit release that reports these events on Wayland, remove
this copy and its line in the workspace `[patch.crates-io]`.
