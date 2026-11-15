# ADR 0002: NoteCraft detects new releases itself, behind a source boundary

Status: accepted in part (issue #253). A manual check that also downloads and installs is
implemented; see "Update, 2026-11-15" at the end. The startup check is not.

## Context

Users need to know when a newer NoteCraft release exists. The Craft apps do this with the shared
`craft-update` crate in `storytold/craft-libs`. NoteCraft does not have that check today. Its
updates reach users only through the ArtCraft Launcher.

The Craft apps also share `craft-i18n`, which NoteCraft does not use. NoteCraft already ships 13
demo languages with its own catalogs, and the app's own interface strings use NoteCraft's i18n.
Replacing those would be a large change for one feature.

NoteCraft is not in the Craft App Suite list in `craft-update`. That list is a `const` in the
`storytold/craft-libs` repo. Adding NoteCraft there would need a change in another repository,
which this project does not own.

## Options considered

1. **Adopt `craft-update` now.** Gives the same check, dialogs and install flow as the other apps.
   - Deferred. It needs a new git dependency on another repo at a pinned revision. Its suite list
     is in that repo, so NoteCraft would have to append its own entry in code. Its dialogs and
     translation hook would then sit beside NoteCraft's own. The check itself is the part NoteCraft
     needs. The rest is a larger change than this feature.
2. **Adopt `craft-i18n` for the whole app.** Gives WordCraft's translation pattern.
   - Rejected for now. It replaces NoteCraft's own catalogs and language setup.
3. **Copy the needed messages from `craft-i18n`.**
   - Rejected. NoteCraft's catalogs are already ahead, and copies would drift.
4. **Our own release check behind a small source boundary.** One module asks a release source for
   the newest release, compares versions, and reports the result. The source is a trait, so
   `craft-update` can replace it later without changing the UI.
   - Accepted.

## Decision

NoteCraft implements its own release detection:

- A module `release_check` in the app crates. It is platform-neutral except for the HTTP call.
- A source boundary: a trait with one method, `latest(repo) -> Result<Release, Error>`, where
  `Release` has a version and a page URL. The GitHub implementation reads the newest non-draft,
  non-prerelease release of `AlienTechSoftware/notecraft`.
- Version comparison with a semver parser. Pre-release tags are handled per the rules in
  `docs/releasing.md`.
- A check at startup on desktop only, never on the web build. It runs in the background and
  reports only if a newer version exists. It can be turned off in settings.
- The message is shown with NoteCraft's own themed dialog. Its strings use NoteCraft's i18n.
- No download and no install. The dialog links to the release page.

The `craft-update` and `craft-i18n` options are kept open as a later decision. The source boundary
is the place to plug them in.

## Consequences

- NoteCraft gets an update check with no change to the app's look or language support.
- Two HTTP-related choices are needed: the crate (`ureq` is the one `craft-update` uses) and the
  semver parser. Both are new dependencies and need owner approval before they are added.
- Downloads and installs stay with the launcher and the platform installers.
- If the check fails (no network, rate limit), the app stays silent. The failure goes to the log.

## Open questions

- Owner approval for the HTTP and semver dependencies.
- Whether the check runs once per launch or once per day.
- Whether the release source should also read the launcher's catalog.

## Update, 2026-11-15 (issue #253)

A tester asked for a log could not find the option because their build was old, and nothing in
the app said so. The request that followed (from a contributor) asked for the update button to download and
install the new version, not only point at it. What was built:

- `crates/ui-egui/src/update.rs`, with the source boundary this ADR describes: a
  `ReleaseSource` trait (`latest() -> Release`) and a GitHub implementation reading
  `releases/latest`. `craft-update` can still replace it.
- `ureq` (with rustls and the system's certificate store) and `semver`, as named above, plus
  `sha2` to check downloads. These are the new dependencies the "Open questions" asked about.
- **No startup check.** The check runs only when File > About or File > Options > **Check for
  updates** is pressed, so nothing is sent unless the user asks. A startup check and its
  setting remain open.
- **It departs from "No download and no install".** When the release is newer, the button
  downloads the file for this install, checks its SHA-256, and installs it: the MSI or the
  portable zip on Windows, the AppImage on Linux, the app bundle on macOS. deb and rpm installs
  hand the package to the system's installer. The table in `docs/releasing.md` lists them. The
  web build and Android open the releases page instead.
- Failures are shown in the File screen and written to the log, rather than kept silent,
  because the user pressed the button and is waiting for an answer.
