# NoteCraft v0.0.10

**Status:** In progress: the Android build, packaging and phone layout exist (see Progress)  
**Base Branch:** `next`  
**Target:** Android platform support

## Scope

Version 0.0.10 introduces Android platform support, making NoteCraft available on mobile devices with the same cross-platform architecture established in v0.0.7-v0.0.9.

### Deliverables
- Android app packaging (APK/AAB)
- Touch-optimized UI for small screens
- Mobile-specific file handling
- Android-specific container implementations
- Store deployment (Google Play or F-Droid)

### Architecture Decisions
- **Framework:** egui with native Android bindings
- **Storage:** Android app directory + synced folder support
- **File Sync:** Cloud sync via user's synced folders (Drive, OneDrive, Syncthing)
- **Platform Code:** `<feature>.android.rs` only for code shared with desktop; Android-only code lives in `android/` with no suffix; `.droid.*` is retired. Desktop code stays where it is until the owner approves a move

## Progress

The Android app lives in [`android/`](../../../android/README.md): a `NativeActivity` APK with no
Java code that runs the desktop's egui window. CI checks it with clippy for the arm64 target; the
Packaging workflow builds and verifies the APK, and the nightly release publishes it. What it does
not do yet is listed under Android in [`docs/gaps.md`](../../gaps.md). Ticked items below are
built and tested on the host; none is checked on a device yet.

## Phase 1: Foundation (v0.0.10a)
- [x] Android build pipeline (`android/build.sh`, `android/test.sh`, CI and Packaging jobs)
- [x] Basic egui rendering on Android (wgpu through eframe's native activity; device check pending)
- [ ] File access on Android (ContentProvider). Notebooks live in the app's own external storage;
  the file pickers and other folders need a Java shell
- [x] Touch input handling (egui touch; finger drag scrolls the panes, two fingers scroll the page)

## Phase 2: UI Adaptation (v0.0.10b)
- [x] Phone-width layout (issue #12): the notebooks or the page, one at a time; folded ribbon
- [ ] Touch-friendly container controls
- [ ] Gesture support: pinch zoom on the page works through egui; long-press does not yet
- [x] Keyboard integration: the soft keyboard opens for the page caret and text boxes (key by
  key; composing input needs GameActivity)
- [x] Orientation handling (rotation resizes the window in place; the activity is not restarted)

## Phase 3: Feature Parity (v0.0.10c)
- [ ] All v0.0.9 features working on Android
- [ ] HTML and Markdown rendering
- [ ] Image handling
- [ ] Container resize and positioning

## Phase 4: Release (v0.0.10)
- [ ] Store submission
- [ ] Documentation
- [ ] Release notes
- [ ] Analytics/telemetry

## Timeline
- **Planning:** TBD
- **Phase 1-2:** TBD
- **Phase 3:** TBD
- **Release:** TBD

## Known Challenges
- egui mobile support still evolving
- Touch input coordination with canvas interactions
- File system access restrictions on Android
- Build complexity with cross-compilation

## Related Documents
- `android_platform_plan.md` - Detailed implementation plan
- `architecture.md` - Cross-platform architecture
- `testing.md` - Android testing strategy

## Links
- **Branch:** `feat/android/12`
- **Related:** v0.0.8, v0.0.9
- **egui Android:** https://github.com/emilk/egui
