# craft-update (copied into NoteCraft)

A copy of `crates/update` from storytold/craft-libs, commit `1b34eba`, licensed MIT OR Apache-2.0
(see `LICENSE-MIT`, `LICENSE-APACHE`). It is the release check, download, install and restart
flow the Craft apps share.

## Why it is copied

The shared update card chose its button from a check that is not stable: the same installed copy
reported "installable" and then "not installable" a few seconds apart, so the card offered
Download instead of Update and Restart. NoteCraft needs one answer, decided once, so the card can
do what the user expects.

## What is changed here

- **No `egui` in this crate.** The card and its state machine (`dialog.rs`, `updater.rs`) moved to
  `crates/ui-egui/src/update_card/`, the UI layer. This crate keeps the shared logic only, so it
  sits at the bottom layer (`xtask` LAYERS).
- **One installable decision.** `Updater::with_installable` takes an answer the app decided once.
  The card uses it instead of asking the system again while it draws, so the button cannot change
  between frames.
- **Visibility.** A few helpers that the card needs are `pub` now (`button`, `record_failure`,
  `offered_release`, `TEST_AGENT`). Nothing else in the logic is changed on purpose. Any other
  change must be listed here.
- **Tests.** Upstream's tests are not copied, because they need upstream's fixtures and dev
  dependencies. The card has its own headless test in `update_card/mod.rs`.

## Keeping it up to date

Upstream is the source for everything not listed above. To bring in upstream changes, copy the
new `src/` files and re-apply the list above. Upstream tests are not copied here.

## Where it lives

`crates/` is where shared code lives in this repository (see `AGENTS.md`). It is a crate in the
workspace, not a separate repository.

## Open items

- `App::bundle_id()` builds `ai.storyteller.<id>`, ArtCraft's namespace. It is used on macOS and
  Linux only, not on Windows. NoteCraft should set its own bundle identifier there before a
  macOS or Linux release (`com.alientechsoftware.notecraft` is the one `packaging/OSX` names).
- **Ignored tests.** Three detection tests check ArtCraft's own app catalogue (PdfCraft, PrintCraft)
  against the ArtCraft publisher. They don't apply to NoteCraft, so they are marked `#[ignore]`.
