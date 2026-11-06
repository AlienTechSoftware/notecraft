# Android build and device test

NoteCraft's Android APK uses an Android adapter maintained in the Android builder repository.
The builder runs on Linux and is not part of the normal `cargo xtask ci` gate. The standard Linux
setup remains required for Rust and native build dependencies; the Android setup adds the SDK,
NDK, Java, and Rust cross-compilation tools.

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

The default tool versions match the Android builder CI configuration:

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
the SDK under the Linux filesystem, not a Windows drive mounted under `/mnt/`. The builder creates
and moves SDK directories during setup; the Windows mount can reject those operations.

Add the SDK tools to each shell that will run ADB or `sdkmanager`:

```bash
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
export PATH="$ANDROID_HOME/platform-tools:$ANDROID_HOME/cmdline-tools/latest/bin:$HOME/.cargo/bin:$PATH"
```

## Build an APK

Use the Android builder checkout and branch selected for the build. From its repository root,
select NoteCraft's pinned Rust toolchain explicitly. The builder itself may not contain
`rust-toolchain.toml`, and Cargo needs the source project's toolchain before it can run
`cargo-ndk`:

```bash
channel="$(sed -n 's/^[[:space:]]*channel[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' /path/to/notecraft/rust-toolchain.toml)"
RUSTUP_TOOLCHAIN="$channel" bash build.sh -notecraft
```

The builder uses the latest stable NoteCraft tag by default. To build its `main` branch instead:

```bash
RUSTUP_TOOLCHAIN="$channel" BUILD_SOURCE=main bash build.sh -notecraft
```

The APK is written to `output/` in the builder checkout. Builds use a local development signing
key; do not distribute that APK publicly or use it for a release. Public releases need a separately
managed release key and explicit release approval.

## Test on an Android device

Enable Developer options and USB debugging on the device, connect it, and approve its debugging
prompt. Confirm that ADB sees it:

```bash
adb devices -l
```

Install the APK path printed by the builder and launch the app:

```bash
adb install -r /path/to/notecraft.apk
adb shell am start -n com.artcraft.notecraft/.NoteCraftActivity
adb logcat -s com.artcraft.notecraft
```

If WSL cannot see a USB-connected device, use a configured ADB connection over wireless debugging
or run the Windows `adb` client. A successful APK build does not prove that installation, startup,
storage access, text input, or stylus interaction works on a device; check those separately.

## Troubleshooting

- `can't find crate for std` for `aarch64-linux-android`: install the target for the active
  NoteCraft toolchain, not only Rustup's default toolchain. Run the Android setup script again.
- `cargo-ndk` is installed but Cargo cannot run it: from the builder checkout, set
  `RUSTUP_TOOLCHAIN` to the channel in NoteCraft's `rust-toolchain.toml`.
- SDK setup reports a move or permission error: keep `ANDROID_SDK_ROOT` under the WSL/Linux home
  filesystem instead of `/mnt/<drive>`.
- `adb devices -l` shows no device: authorize USB debugging or connect ADB over wireless debugging.
