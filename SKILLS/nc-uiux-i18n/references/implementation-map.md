# NoteCraft i18n implementation map

This map reflects the `feat/v0.0.9-i18n` implementation at the time the skill was added. Check
the code before relying on it. The local catalog embedding is a pilot, not the final ArtCraft
catalog build pipeline.

## NoteCraft files

| Path | Responsibility | Current state |
|---|---|---|
| `Cargo.toml` | Workspace Rust minimum and pinned `craft-i18n` dependency | Rust 1.95 and exact shared runtime revision approved for this work |
| `crates/ui-egui/Cargo.toml` | UI crate dependency on `craft-i18n` | Runtime dependency declared |
| `crates/ui-egui/src/i18n.rs` | Language preference normalization, locale selection, lookup adapter, embedded catalogs | Local `include_str!` catalogs for French and catalog-backed `en-XA`/`ar-XB` pseudo locales |
| `crates/ui-egui/src/lib.rs` | `tl!` convenience macro | English source text is passed to the i18n adapter |
| `crates/ui-egui/src/app.rs` | Reads and persists `ui_language`; applies selected locale | Setting defaults to `auto`; verify reset and reconstruction behavior when changing it |
| `crates/ui-egui/src/backstage.rs` | File screen content and language picker | Several headings, controls and formatted messages use lookup; a full surface audit is still outstanding |
| `crates/ui-egui/src/ribbon.rs` | Ribbon tabs, groups, controls, and tooltips | Tab names, group headings, and shared big/small control helpers use lookup; individual controls and catalog coverage remain incomplete |
| `i18n/fr.tsv` | NoteCraft French catalog rows | Local, partial catalog using Craft's TSV format; screenshot-audited Home ribbon, page header, status summary, and picker labels |
| `i18n/en-XA.tsv`, `i18n/ar-XB.tsv` | Catalog-backed pseudo translations | English source rows with LTR markers or RTL-isolated markers; missing rows remain plain English to expose coverage gaps |
| `work/requests/app-localization.md` in private host | Scope, accepted design, estimates, and known gaps | Canonical planning record for this work |
| `work/rtl-and-caret-bar.md` in private host | RTL text and whole-window RTL acceptance | Separate plan; localization does not close its gaps |
| `docs/demo/scripts/content/<locale>/` | Authored Welcome notebook pages | Source for generated demo notebooks; not the app UI catalog |
| `docs/how-to/` | Standalone User Assistance | Separate content set from the demo; translations have their own deliverable |

## ArtCraft reference files

The private workspace's `.repos/storytold` tree is a read-only mirror. Inspect these references
without editing them:

- `.repos/storytold/wordcraft/crates/ui-egui/src/i18n.rs`: app adapter, lookup helpers, language
  list, and catalog behavior.
- `.repos/storytold/wordcraft/crates/ui-egui/src/i18n/audit.rs`: pseudo-language rendered-text
  audit and catalog-gap report.
- `.repos/storytold/wordcraft/crates/ui-egui/build.rs`: build-time embedding from
  `CRAFT_I18N_DIR` and release-required behavior.
- `.repos/storytold/wordcraft/crates/ui-egui/Cargo.toml`: exact shared runtime pin used by that
  checkout; compare it to NoteCraft rather than assuming it stays current.
- `.repos/storytold/craft-i18n/docs/catalog-format.md`: TSV schema and validation rules.
- `.repos/storytold/craft-i18n/docs/integration.md`: pre-cutover local catalogs, build-time
  embedding, catalog sync, and the post-cutover data flow.
- `.repos/storytold/craft-libs/crates/i18n/README.md`: shared runtime API and lookup semantics.

These are conventions and implementation evidence, not permission to change the mirrored
repositories or publish catalogs.
