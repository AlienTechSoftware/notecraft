# Installation artifacts

Status: product contract updated by the owner on 2026-11-07. This is the single authority for
installation artifact responsibilities and the User Assistance demo notebook. The application
bundles the generated notebook and asks once per app version before creating or replacing the
per-user Welcome notebook.

## Separate installation artifacts

Every distribution has two logically separate artifacts:

1. **Application binaries and bundled source content**: executable code plus the two generated
   Welcome notebook formats, deployed to the platform's normal application location.
2. **Managed Welcome notebook**: a per-user copy in the target user's normal `NoteCraft`
   notebooks folder, created or replaced by the app only after the user chooses Yes.

Each installer carries the generated content with the app so NoteCraft can provision it without a
separate setup download or network request. The content remains ordinary notebook files; it is not
executable code.

The authored notebook source is `docs/demo/scripts/content`. `cargo xtask demo` generates the
official HTML and CMGFM notebooks under `docs/demo/DemoNotebook`, each with all 13 languages.
Both are built, bundled, and tested for parity. Installers use this generated source, never
`my-notebooks/welcome-notebook` or another local test copy. NoteCraft supports both formats for
the Welcome notebook and for notebooks created or opened by users.

## Separation of responsibility

The running app owns the per-user copy after the user confirms:

- Install application binaries in the application location.
- Bundle the User Assistance source with the application binaries.
- Show the first-use wizard once: on the first launch after installation, or after the preferences
  are reset. It is one dialog with five pages and Next at the bottom right: the welcome page with
  the brand banner; the local logging level; the page format, HTML or CommonMark + GFM, saved as the
  default; whether to create the demo notebook in that format; and a thank-you page with Finish.
- The wizard does not reopen for later app versions. Installation copies nothing into the notebooks
  folder; the package ships the generated editions only as the wizard's source.
- On Yes on the demo page, create or completely replace only `welcome-to-notecraft` from the bundled
  edition for the chosen format when Finish is pressed. The edition's notebook folder becomes the
  managed notebook, so the notebook list finds it, and the Welcome page opens. The other format is
  not installed.
- On No, leave any existing Welcome notebook unchanged and create none.
- Preserve every sibling notebook and its contents.
- On uninstall, remove application files only; leave user notebooks in place.
- If creating or replacing the Welcome notebook fails, report the error and preserve the existing
  copy.

Desktop apps copy the chosen edition from the installed package or app bundle into the selected
user notebook root only after Yes in step 3. The desktop setup launcher installs nothing; it starts
NoteCraft. Android and the browser have separate startup paths; each must
apply the same per-version confirmation before replacing a user's Welcome notebook. Android
app-specific storage follows Android's normal uninstall lifecycle.

## Platform installation contract

The expected notebook destination is the platform's normal user notebook location. Desktop setup
uses the app's saved notebook root when present, otherwise its current Documents/NoteCraft default.
On Android, it is the same app-specific
`NoteCraft` folder used for notebooks created on-device. Use platform APIs to find these locations;
do not guess a home or storage path when the platform provides an API.

| Platform | Application binaries | User Assistance notebook | Installation owner |
|---|---|---|---|
| Website demo | Browser app (JavaScript and WebAssembly) | `/NoteCraft/welcome-to-notecraft`, containing HTML and CMGFM | Browser prompt must ask before copying from the two-format archive |
| Windows | Normal per-machine or per-user app location | Current user's Documents/NoteCraft/welcome-to-notecraft | First-use wizard copies the chosen edition from the package's bundled source after Yes |
| Linux | Normal package or portable-app location | Current user's Documents/NoteCraft/welcome-to-notecraft | First-use wizard copies the chosen edition from the package's bundled source after Yes |
| macOS | `NoteCraft.app` copied to Applications | Current user's Documents/NoteCraft/welcome-to-notecraft | First-use wizard copies the chosen edition from app resources after Yes |
| Android | Installed APK application package | App-specific external files/NoteCraft/welcome-to-notecraft | APK startup must ask before copying the asset formats |

The browser archive contains both directories under `docs/demo/DemoNotebook`, is version-checked
in the website workflow, and is loaded under the same `/NoteCraft` root as other browser notebooks.
The generic engine sample remains a separate sample notebook and is not the release Welcome
notebook.

The Android launch activity refreshes the managed notebook from both APK asset formats before
opening the native app.

## Platform scope for 0.0.9 (owner, 2026-11-08)

The first-use wizard is implemented and verified on Windows only for this release. macOS, Linux,
the website and Android are deferred for the wizard flow: their packages keep shipping the generated
editions as the wizard's source, but no platform-specific wizard work or sign-off is claimed for them
in 0.0.9. Installation copies no notebook content on any platform.

## Current implementation status

Desktop and mobile package staging follows the app-managed refresh contract:

- The Windows MSI and portable ZIP include both generated formats. Clean VM refresh and
  sibling-preservation checks remain required.
- Linux packages include both generated formats. Installed-run refresh and sibling-preservation
  checks remain required.
- macOS bundles both formats in app resources; native launch verification remains required.
- The desktop setup launcher no longer installs the Welcome notebook. The first-use wizard does,
  and only for the format the user chose. Windows, Linux and macOS checks of the wizard remain
  required. The macOS setup app now shows a notice and does not install anything; agent 3 must
  revalidate that path.

Android stages both formats as APK assets and refreshes them before opening NoteCraft. Build and
emulator checks remain required.

## Installation verification

For each supported artifact and platform, signoff proves:

1. The package contains both generated formats and all 13 languages.
2. App startup creates or replaces only `welcome-to-notecraft` in the platform notebook root.
3. A sentinel sibling notebook stays byte-for-byte unchanged.
4. Both format variants retain their page tree, assets, and metadata.
5. Yes in step 3 replaces the managed Welcome notebook with the chosen edition only; No preserves
   its current files.
6. The wizard opens once after installation or a preferences reset, and does not reopen after an
   upgrade. Its format page saves the default format, and Finish saves every answer.
7. Desktop uninstall removes app binaries and leaves the user's notebook untouched. Android
   uninstall removes app-specific notebook files as required by the OS storage model.
8. A failed replacement is reported and does not replace the existing copy with a partial notebook.

Run `cargo xtask demo-check` and
`cargo test -p notecraft-formats --test format_parity` for the source outputs. Then inspect the
final installer artifacts and installed result. Package staging alone does not prove deployment
to the user's notebook folder.
