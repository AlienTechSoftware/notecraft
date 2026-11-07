# Installation artifacts

Status: product contract updated by the owner on 2026-11-07. This is the single authority for
installation artifact responsibilities and the User Assistance demo notebook. Every app launch
refreshes the managed Welcome notebook from the generated content bundled with the application.

## Separate installation artifacts

Every distribution has two logically separate artifacts:

1. **Application binaries and bundled source content**: executable code plus the two generated
   Welcome notebook formats, deployed to the platform's normal application location.
2. **Managed Welcome notebook**: a per-user copy in the target user's normal `NoteCraft`
   notebooks folder, refreshed by the app when it starts.

Each installer carries the generated content with the app so the app can provision it without a
separate setup download, helper choice, or network request. The content remains ordinary notebook
files; it is not executable code.

The authored notebook source is `docs/demo/scripts/content`. `cargo xtask demo` generates the
official HTML and CMGFM notebooks under `docs/demo/DemoNotebook`, each with all 13 languages.
Both are built, bundled, and tested for parity. Installers use this generated source, never
`my-notebooks/welcome-notebook` or another local test copy. NoteCraft supports both formats for
the Welcome notebook and for notebooks created or opened by users.

## Separation of responsibility

The running app owns the per-user copy:

- Install application binaries in the application location.
- Install the User Assistance notebook in the user's normal notebooks folder.
- At app startup, create or replace only `welcome-to-notecraft` from the bundled HTML and CMGFM
  formats. This refreshes app-authored content on every launch.
- Preserve every sibling notebook and its contents.
- On uninstall, remove application files only; leave user notebooks in place.
- If refreshing the Welcome notebook fails, report the error and preserve the existing copy.

Desktop apps copy the generated source from the installed package or app bundle into the selected
user notebook root. Android refreshes from APK assets before opening the native app. Browser startup
refreshes it from the website archive on each non-invited session. The Welcome notebook's content
and edits are replaced with the release-authored version on app start; sibling notebooks are
untouched. Android app-specific storage follows Android's normal uninstall lifecycle.

## Platform installation contract

The expected notebook destination is the platform's normal user notebook location. Desktop setup
uses the app's saved notebook root when present, otherwise its current Documents/NoteCraft default.
On Android, it is the same app-specific
`NoteCraft` folder used for notebooks created on-device. Use platform APIs to find these locations;
do not guess a home or storage path when the platform provides an API.

| Platform | Application binaries | User Assistance notebook | Installation owner |
|---|---|---|---|
| Website demo | Browser app (JavaScript and WebAssembly) | `/NoteCraft/welcome-to-notecraft`, containing HTML and CMGFM | Browser startup refreshes it from the two-format archive |
| Windows | Normal per-machine or per-user app location | Current user's Documents/NoteCraft/welcome-to-notecraft | App startup copies from the package's bundled source |
| Linux | Normal package or portable-app location | Current user's Documents/NoteCraft/welcome-to-notecraft | App startup copies from the package's bundled source |
| macOS | `NoteCraft.app` copied to Applications | Current user's Documents/NoteCraft/welcome-to-notecraft | App startup copies from app resources |
| Android | Installed APK application package | App-specific external files/NoteCraft/welcome-to-notecraft | APK setup activity refreshes both asset formats before opening NoteCraft |

The browser archive contains both directories under `docs/demo/DemoNotebook`, is version-checked
in the website workflow, and is loaded under the same `/NoteCraft` root as other browser notebooks.
The generic engine sample remains a separate sample notebook and is not the release Welcome
notebook.

The Android launch activity refreshes the managed notebook from both APK asset formats before
opening the native app.

## Current implementation status

Desktop and mobile package staging follows the app-managed refresh contract:

- The Windows MSI and portable ZIP include both generated formats. Clean VM refresh and
  sibling-preservation checks remain required.
- Linux packages include both generated formats. Installed-run refresh and sibling-preservation
  checks remain required.
- macOS bundles both formats in app resources; native launch verification remains required.

Android stages both formats as APK assets and refreshes them before opening NoteCraft. Build and
emulator checks remain required.

## Installation verification

For each supported artifact and platform, signoff proves:

1. The package contains both generated formats and all 13 languages.
2. App startup creates or replaces only `welcome-to-notecraft` in the platform notebook root.
3. A sentinel sibling notebook stays byte-for-byte unchanged.
4. Both format variants retain their page tree, assets, and metadata.
5. The managed Welcome notebook is refreshed from app content on restart.
6. Desktop uninstall removes app binaries and leaves the user's notebook untouched. Android
   uninstall removes app-specific notebook files as required by the OS storage model.
7. A failed refresh is reported and does not replace the existing copy with a partial notebook.

Run `cargo xtask demo-check` and
`cargo test -p notecraft-formats --test format_parity` for the source outputs. Then inspect the
final installer artifacts and installed result. Package staging alone does not prove deployment
to the user's notebook folder.
