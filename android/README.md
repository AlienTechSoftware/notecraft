# NoteCraft for Android

The Android app: the same egui window as the desktop, in an APK for 64-bit ARM phones and
tablets with Android 8 (API 26) or later.

## How it is put together

- **No Java code.** The APK's activity is Android's own `android.app.NativeActivity`
  ([`AndroidManifest.xml`](AndroidManifest.xml)). It loads `libnotecraft_android.so`, built from
  this crate, and calls its `android_main` ([`src/activity.rs`](src/activity.rs)).
- **The window** is eframe's native-activity backend drawing with wgpu (Vulkan, or OpenGL ES 3).
  It runs `NoteCraftApp` from `crates/ui-egui`, unchanged. Narrower than 600 points the window uses
  the phone layout ([`crates/ui-egui/src/compact.rs`](../crates/ui-egui/src/compact.rs)).
- **Storage** ([`src/storage.rs`](src/storage.rs)): settings and logs in the app's internal
  storage; notebooks in `Android/data/com.alientechsoftware.notecraft/files/NoteCraft`, which can
  be copied off the device over USB.
- **The Welcome notebook** ([`src/welcome.rs`](src/welcome.rs)): both editions travel in one APK
  asset, are unpacked into internal storage once per version, and the first-use wizard copies the
  chosen edition into the notebooks folder, as on the desktop.
- **Back** closes a dialog, the File screen, the Tags Summary or the page templates, then turns a full-window page back
  into the notebooks list.
- **The soft keyboard** opens while a block of the page has the caret, and for text boxes.
- **Unsafe code:** one attribute. Android finds `android_main` by its unmangled name, and
  `#[unsafe(no_mangle)]` counts as unsafe code. The crate denies unsafe code and allows it on that
  one function ([`Cargo.toml`](Cargo.toml) says why).

What is missing (file pickers, the system clipboard, folders outside the app's storage, composing
input, the screen reader) is listed under Android in [`docs/gaps.md`](../docs/gaps.md).

## Build

Set up once with [`SKILLS/nc-devx/scripts/setup.android.sh`](../SKILLS/nc-devx/scripts/setup.android.sh)
(Linux or WSL2), then from the repository root:

```bash
bash android/build.sh   # dist/android/notecraft-<version>-android-arm64.apk
bash android/test.sh    # checks the APK
```

`build.sh` signs with the key in `ANDROID_KEYSTORE` when it is set, and otherwise with a key made
for that build only (see [`docs/releasing.md`](../docs/releasing.md), Android signing).
[`ci-setup.sh`](ci-setup.sh) prepares a GitHub runner; the CI workflow runs clippy for the arm64
target and the Packaging workflow builds, checks and uploads the APK, which the nightly release
publishes.

## Run on a device

```bash
adb install -r dist/android/notecraft-<version>-android-arm64.apk
adb shell am start -n com.alientechsoftware.notecraft/android.app.NativeActivity
adb logcat -s NoteCraft
```

More in [`SKILLS/nc-devx/references/android.md`](../SKILLS/nc-devx/references/android.md).
