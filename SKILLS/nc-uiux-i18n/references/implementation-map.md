# NoteCraft i18n implementation map

This map reflects the current NoteCraft i18n pilot. Check the code before relying on it. The local
catalog embedding is a pilot, not the final ArtCraft catalog build pipeline.

## NoteCraft files

| Path | Responsibility | Current state |
|---|---|---|
| `Cargo.toml` | Workspace Rust minimum and pinned `craft-i18n` dependency | Rust 1.95 and exact shared runtime revision approved for this work |
| `crates/ui-egui/Cargo.toml` | UI crate dependency on `craft-i18n` | Runtime dependency declared |
| `i18n/en.tsv` | Stable English source catalog | Canonical UI wording; build script generates ID and reverse source maps |
| `crates/ui-egui/build.rs` | English catalog embedding and source lookup generation | Generates `english_text(id)` and `source_id(source)` for the adapter |
| `crates/ui-egui/src/i18n.rs` | Language preference normalization, locale selection, lookup adapter, embedded catalogs | `tl_id!` resolves stable IDs through `en.tsv`; other catalogs include French, Spanish, `en-XA`, and `ar-XB` |
| `crates/ui-egui/src/lib.rs` | `tl!` and `tl_id!` convenience macros | Stable IDs are preferred; `tl!` remains for dynamic source strings resolved by the generated map |
| `crates/ui-egui/src/app.rs` | Reads and persists `ui_language`; applies selected locale | Setting defaults to `auto`; verify reset and reconstruction behavior when changing it |
| `crates/ui-egui/src/backstage.rs` | File screen content and language picker | Several headings, controls and formatted messages use lookup; a full surface audit is still outstanding |
| `crates/ui-egui/src/ribbon.rs` | Ribbon tabs, groups, controls, and tooltips | Tab names, group headings, and shared big/small control helpers use lookup; individual controls and catalog coverage remain incomplete |
| `i18n/fr.tsv`, `i18n/es.tsv` | NoteCraft French and Spanish catalog rows | Partial real-language catalogs; pseudo catalogs currently have complete ID parity with `en.tsv` |
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
