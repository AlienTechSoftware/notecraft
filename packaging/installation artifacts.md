# Installation artifacts

Status: product contract approved by the owner on 2026-11-06. This is the single authority for
installation artifact responsibilities and the User Assistance demo notebook. It defines the
target behavior; the current implementation does not yet conform.

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

The application does not install the notebook. It must not unpack, seed, copy, refresh, replace,
repair, or delete the notebook at startup or in response to an app update. After setup, the app
discovers and opens the already-installed notebook through its ordinary notebook discovery and
open behavior. If setup did not deploy it, the app does not silently compensate.

After installation, the Welcome notebook is ordinary user-owned notebook data. Users can edit,
move, sync, back up, or delete it. App upgrades and uninstall must not overwrite or remove those
files.

## Platform installation contract

The expected notebook destination is the platform's normal Documents location, followed by
`NoteCraft/welcome-to-notecraft`. Use platform known-folder or user-directory APIs where
available; do not assume a hard-coded home path when the platform provides a location API.

| Platform | Application binaries | User Assistance notebook | Installation owner |
|---|---|---|---|
| Windows | Normal per-machine or per-user app location | Current user's Documents/NoteCraft/welcome-to-notecraft | MSI or its setup helper, running for the intended user |
| Linux | Normal package or portable-app location | Current user's Documents/NoteCraft/welcome-to-notecraft | A user-context setup tool; a system package script must not guess which user's home to modify |
| macOS | App bundle or package location | Current user's Documents/NoteCraft/welcome-to-notecraft | A `.pkg` installer or explicit, consented setup helper; dragging a DMG app alone does not perform this step |
| Android | Installed APK application package | The installation tool's separate User Assistance payload, deployed to the agreed user Documents/NoteCraft location | APK installation or a dedicated setup component, never NoteCraft startup code |

An installation format that cannot perform the per-user notebook step by itself must provide a
separate installation/setup component with a clear user action. The app itself is not that
component. Do not claim that an artifact installs the notebook until a clean installation proves
the notebook is present before the app launches.

## Current implementation gap

The following existing behavior violates this contract and needs a later, separately approved
implementation change:

- `crates/engine/build.rs` embeds the HTML notebook into the engine binary.
- `apps/notecraft/src/main.rs` runs `notebook.welcome` at every startup; the engine replaces the
  existing notebook folder, which can discard user edits.
- Windows and Linux package scripts stage a separate notebook payload, but desktop startup does
  not use that payload.
- The Windows MSI has a custom action that deletes the user's existing Welcome notebook before
  launching the app. This contradicts the preservation requirement.
- The Android build currently packages the notebook through the engine build; its adapter does
  not use a separately deployed User Assistance payload.

These statements record the gap. They do not authorize changes to installer or application
behavior. Implementation requires separate owner approval after this contract is reviewed.

## Installation verification

For each supported artifact and platform, signoff proves:

1. The final package contains two separately identifiable payloads: app binaries and both
   generated notebook formats.
2. A clean installation places the notebook in the expected user Documents/NoteCraft location
   before NoteCraft is launched.
3. The app opens the installed notebook without creating or copying it at startup.
4. An edit survives app restart and application upgrade.
5. A user-deleted notebook stays deleted after restart and upgrade.
6. Uninstall removes app binaries and leaves the user's notebook untouched.
7. A failed notebook installation is reported by the installation tool; the app does not install
   a fallback copy.

Run `cargo xtask demo-check` and
`cargo test -p notecraft-formats --test format_parity` for the source outputs. Then inspect the
final installer artifacts and installed result. Package staging alone does not prove deployment
to the user's notebook folder.
