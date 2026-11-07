# Installation artifacts

Status: product contract approved by the owner on 2026-11-06. This is the single authority for
installation artifact responsibilities and the User Assistance demo notebook. Windows and Linux
now use separate setup helpers. macOS DMGs carry the optional notebook beside the app and include
a separate, consented setup app. Clean installed-flow verification remains open for desktop.

## Separate installation artifacts

Every distribution has two logically separate artifacts:

1. **Application binaries**: executable code and runtime dependencies, deployed to the platform's
   normal application location.
2. **User Assistance demo notebook**: user-facing notebook files, deployed to the target user's
   normal `NoteCraft` notebooks folder.

One outer download or installer may contain both artifacts. One MSI, DMG, APK, archive, or other
package can carry both. Its manifest, staging, and verification must still identify the binaries
and notebook as separate payloads. The notebook is not executable code and must not be compiled
into the app binary as a substitute for installation.

The authored notebook source is `docs/demo/scripts/content`. `cargo xtask demo` generates the
official HTML and CMGFM versions under `docs/demo/DemoNotebook`. Both formats belong to the User
Assistance artifact and must remain in parity. Installers use these generated outputs, never
`my-notebooks/welcome-notebook` or another local test copy.

## Separation of responsibility

The installation tool owns deployment of both artifacts:

- Install application binaries in the application location.
- Install the User Assistance notebook in the user's normal notebooks folder.
- On a fresh install, create the notebook only if it is absent.
- On upgrade, preserve an existing notebook, including user edits and deletion.
- On uninstall, remove application files only; leave user notebooks in place.
- Report installation failures through the installer or setup tool.

Windows starts the separate `notecraft-setup` helper from the MSI finish action and its shortcuts.
Linux desktop entries and the AppImage launcher start the same helper. It runs in the signed-in
user's context, installs only when the User Assistance folder is absent, records completion outside
app preferences, and then opens NoteCraft. Linux system package scripts never write into a guessed
user's home. If a package omits the optional payload, the helper opens NoteCraft without copying
anything.

The application does not install the notebook. It must not unpack, seed, copy, refresh, replace,
repair, or delete the notebook at startup or in response to an app update. After setup, the app
discovers and opens the already-installed notebook through its ordinary notebook discovery and
open behavior. If setup did not deploy it, the app does not silently compensate.

After installation, the Welcome notebook is ordinary user-owned notebook data. Users can edit,
move, sync, back up, or delete it. App upgrades must not overwrite or recreate it. Android's
app-specific storage is removed by the OS when NoteCraft is uninstalled; retaining Android
notebooks after uninstall requires a future shared-storage design.

## Platform installation contract

The expected notebook destination is the platform's normal user notebook location. Desktop setup
uses the app's saved notebook root when present, otherwise its current Documents/NoteCraft default.
On Android, it is the same app-specific
`NoteCraft` folder used for notebooks created on-device. Use platform APIs to find these locations;
do not guess a home or storage path when the platform provides an API.

| Platform | Application binaries | User Assistance notebook | Installation owner |
|---|---|---|---|
| Windows | Normal per-machine or per-user app location | Current user's Documents/NoteCraft/welcome-to-notecraft | MSI or its setup helper, running for the intended user |
| Linux | Normal package or portable-app location | Current user's Documents/NoteCraft/welcome-to-notecraft | A user-context setup tool; a system package script must not guess which user's home to modify |
| macOS | `NoteCraft.app` copied to Applications | Current user's Documents/NoteCraft/welcome-to-notecraft | Separate, consented `Install NoteCraft Welcome Notebook.app` in the DMG; dragging the app alone does not perform this step |
| Android | Installed APK application package | App-specific external files/NoteCraft/welcome-to-notecraft, beside notebooks created on-device | The APK's dedicated setup activity, before it opens the NoteCraft activity |

An installation format that cannot perform the per-user notebook step by itself must provide a
separate installation/setup component with a clear user action. The Android setup activity is a
distinct installer entry point in the APK. It offers to install the optional payload or skip it,
then opens NoteCraft. The NoteCraft activity itself only discovers and opens notebooks. Do not
claim that an artifact installs the notebook until a clean installation proves the notebook is
present before the NoteCraft activity launches.

## Current implementation gap

Windows and Linux package staging now follows the separate-payload contract:

- The Windows MSI and portable ZIP include the setup helper and support builds with the optional
  notebook included or omitted. Both MSI variants compile; installation on a clean Windows profile,
  upgrade and uninstall behavior still need a VM smoke test.
- Linux tar, Debian and AppImage packages include the setup helper and support builds with the
  optional notebook included or omitted. Package builds and tar/DEB content smoke tests pass;
  running the helper from an installed package and checking upgrade/uninstall behavior still need
  a clean user-profile test.
- The helper and app share the saved notebook root. The app's current fallback is
  Documents/NoteCraft under the user's home; redirected Windows Documents folders are not yet
  resolved through the OS known-folder API.

Android's separate APK setup flow passed a fresh tablet-emulator install on 2026-11-06: setup
installed the payload, NoteCraft opened its Welcome page from the app-specific notebook folder, and
a relaunch kept the notebook and bypassed setup. Android upgrade and user-deletion flows remain
unverified.

## Installation verification

For each supported artifact and platform, signoff proves:

1. The final package contains two separately identifiable payloads: app binaries and both
   generated notebook formats.
2. A clean installation or first desktop setup places the notebook in the platform's expected
   location before NoteCraft opens it. Android setup may be skipped when the package omits the
   optional payload.
3. The app opens the installed notebook without creating or copying it at startup.
4. An edit survives app restart and application upgrade. Setup completion is separate from app
   preferences so resetting preferences does not reinstall a deleted notebook.
5. A user-deleted notebook stays deleted after restart and upgrade.
6. Desktop uninstall removes app binaries and leaves the user's notebook untouched. Android
   uninstall removes app-specific notebook files as required by the OS storage model.
7. A failed notebook installation is reported by the installation tool; the app does not install
   a fallback copy.

Run `cargo xtask demo-check` and
`cargo test -p notecraft-formats --test format_parity` for the source outputs. Then inspect the
final installer artifacts and installed result. Package staging alone does not prove deployment
to the user's notebook folder.
