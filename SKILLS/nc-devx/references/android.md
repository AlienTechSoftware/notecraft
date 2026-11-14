# Android build and device test

NoteCraft's Android APK is built from this repository: the crate and resources in
[`android/`](../../../android/README.md), packaged by `android/build.sh`. The build runs on Linux
(or WSL2) and is not part of the normal `cargo xtask ci` gate; CI checks it in its own job, and the
Packaging workflow builds the release APK. The standard Linux setup remains required for Rust and
native build dependencies; the Android setup adds the SDK, NDK, Java, and Rust cross-compilation
tools.

## Bootstrap

On Windows, install the prerequisites in WSL2 Ubuntu 24.04:

```powershell
.\SKILLS\nc-devx\scripts\setup.android.ps1 -CheckOnly
.\SKILLS\nc-devx\scripts\setup.android.ps1
```

On Linux or from a WSL terminal:

```bash
bash SKILLS/nc-devx/scripts/setup.android.sh --check-only
bash SKILLS/nc-devx/scripts/setup.android.sh
```

Check-only mode does not install or change anything. Install mode uses the system package manager
and Rustup, downloads the Android command-line tools and `cargo-ndk`, then prompts for Android SDK
license acceptance before installing packages.

The default tool versions match `android/build.sh` and the CI jobs (`android/ci-setup.sh`):

| Tool | Version or target |
|---|---|
| Rust channel | `rust-toolchain.toml` |
| Rust Android target | `aarch64-linux-android` |
| `cargo-ndk` | 4.1.2 |
| Java | JDK 21 |
| Android platform | API 35 |
| Android Build Tools | 35.0.0 |
| Android NDK | 27.2.12479018 |
| Android platform tools | Includes `adb` |

The SDK defaults to `$HOME/Android/Sdk`. Override the Android SDK versions with
`ARTCRAFT_TARGET_SDK`, `ARTCRAFT_BUILD_TOOLS_VERSION`, and `ARTCRAFT_NDK_VERSION`. In WSL, keep
the SDK under the Linux filesystem, not a Windows drive mounted under `/mnt/`. The SDK manager creates
and moves SDK directories during setup; the Windows mount can reject those operations.

Add the SDK tools to each shell that will run ADB or `sdkmanager`:

```bash
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
export PATH="$ANDROID_HOME/platform-tools:$ANDROID_HOME/cmdline-tools/latest/bin:$HOME/.cargo/bin:$PATH"
```

## Build an APK

From the repository root, after the setup above:

```bash
bash android/build.sh   # dist/android/notecraft-<version>-android-arm64.apk
bash android/test.sh    # signature, manifest, native library, Welcome payload
```

Without `ANDROID_KEYSTORE` the APK is signed with a key made for that build only. It installs, but
an APK signed with another key cannot update it in place. Release builds use the release key from
the repository secrets (`docs/releasing.md`, Android signing). For a quick compile check without
packaging: `cargo ndk --target arm64-v8a --platform 26 clippy -p notecraft-android -- -D warnings`.

## Test on an Android device

Enable Developer options and USB debugging on the device, connect it, and approve its debugging
prompt. Confirm that ADB sees it:

```bash
adb devices -l
```

Install the APK `android/build.sh` printed and launch the app:

```bash
adb install -r dist/android/notecraft-<version>-android-arm64.apk
adb shell am start -n com.alientechsoftware.notecraft/android.app.NativeActivity
adb logcat -s NoteCraft
```

If WSL cannot see a USB-connected device, use a configured ADB connection over wireless debugging
or run the Windows `adb` client. A successful APK build does not prove that installation, startup,
storage access, text input, or stylus interaction works on a device; check those separately.

## Troubleshooting

- `can't find crate for std` for `aarch64-linux-android`: install the target for the active
  NoteCraft toolchain, not only Rustup's default toolchain. Run the Android setup script again.
- `cargo-ndk` is installed but Cargo cannot run it: run it from the repository root, so
  `rust-toolchain.toml` selects the pinned toolchain.
- SDK setup reports a move or permission error: keep `ANDROID_SDK_ROOT` under the WSL/Linux home
  filesystem instead of `/mnt/<drive>`.
- `adb devices -l` shows no device: authorize USB debugging or connect ADB over wireless debugging.
