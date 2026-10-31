# NoteCraft v0.0.10

**Status:** Planning  
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
- **Platform Code:** `.android.rs` files for Android-specific implementations

## Phase 1: Foundation (v0.0.10a)
- [ ] Android build pipeline
- [ ] Basic egui rendering on Android
- [ ] File access on Android (ContentProvider)
- [ ] Touch input handling

## Phase 2: UI Adaptation (v0.0.10b)
- [ ] Touch-friendly container controls
- [ ] Gesture support (long-press, pinch-zoom)
- [ ] Keyboard integration
- [ ] Orientation handling

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
- **Branch:** TBD (likely `feat/android`)
- **Related:** v0.0.8, v0.0.9
- **egui Android:** https://github.com/emilk/egui
