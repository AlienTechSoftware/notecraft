# Workspace 1: Internationalization

**Release:** v0.0.9
**Checkout:** `o-NoteCraft_1`
**Updated:** 2026-11-04
**Status:** Research and French/Spanish UA scope recorded; app-interface implementation awaits the runtime/MSRV decision.

## Goal

Prepare NoteCraft's interface for localization, following the ArtCraft app pattern. This covers application chrome such as ribbon and menus, File screens, dialogs, status text, tooltips, command labels, and user-facing errors. Also add French and Spanish versions of the standalone User Assistance guides in `docs/how-to/`. This does not add new Welcome notebook translations or translate other authored notes.

## ArtCraft pattern found in WordCraft

- WordCraft embeds pinned locale catalogs at build time from `craft-i18n` and uses the shared `craft-libs` runtime for lookup, locale matching, English fallback, plurals, and pseudo-language support.
- Catalog data is separate from app code: `locales/<locale>/common.tsv` holds shared strings and `<app>.tsv` holds app strings and overrides. Rows contain context, English source text, and translation.
- There is no runtime network fetch. A build without catalog data is English-only; release builds can require the pinned catalogs. Web builds embed catalogs too.
- `cargo xtask i18n-report` reports missing or stale translations. A pseudo-language UI audit finds visible text that bypasses lookup.
- Useful read-only references in `.repos/storytold`: `wordcraft/crates/ui-egui/src/i18n.rs`, `wordcraft/crates/ui-egui/build.rs`, `wordcraft/crates/ui-egui/src/i18n/audit.rs`, `wordcraft/xtask/src/main.rs`, `craft-i18n/docs/integration.md`, and `craft-i18n/docs/catalog-format.md`.

## NoteCraft baseline and decision needed

NoteCraft currently has no interface localization layer or locale catalogs. Its `crates/input-language` RTL keyboard detection is for editing behavior, not interface translation. Persistent app settings are managed through `notecraft_store::Settings`.

The bundled Welcome notebook already has Spanish and French content: 15 authored demo pages in each locale, generated to both HTML and CMGFM. The separate User Assistance set under `docs/how-to/` has 16 Markdown files including its index, about 8,924 whitespace-delimited source words, and currently has no French or Spanish editions. These are distinct deliverables. The existing demo translations can inform terminology, but the standalone guides need their own accurate translations and working localized links.

NoteCraft declares Rust 1.90. The exact `craft-libs` revision WordCraft pins (`92d392cff132eaf53e13afff77f38b5c11b73aa0`) declares Rust 1.95. I checked all 13 available `craft-libs` revisions that change the i18n crate manifest (2026-10-18 through 2026-10-31); each declares Rust 1.95, so no compatible 1.90 revision was found in the available history. Direct reuse therefore raises NoteCraft's minimum Rust version. Catalog data is maintained in another repository; contributions there require separate authorization. Do not edit the read-only `.repos/storytold` mirror.

A coarse scan of NoteCraft's UI source found 441 capitalized string matches across 12 of 31 Rust files; this includes non-UI strings and is not a count of translatable labels. The existing `cargo xtask text` command checks text hygiene, not localization coverage.

**Owner CHECKPOINT before code:** approve the shared runtime/catalog integration and Rust 1.95 minimum. If Rust 1.90 must remain, investigate a compatible pinned runtime or another non-duplicating option and revise the estimate. Do not change the MSRV or contribute catalogs publicly without approval.

## Proposed v0.0.9 scope

1. Route agreed user-visible interface strings through one lookup API, keeping command IDs, document content, file formats, and automation selectors language-neutral.
2. Embed pinned catalogs at build time for desktop and WebAssembly. Keep English-only builds working when catalogs are unavailable; require catalogs in release builds only after the build pipeline supplies them reproducibly.
3. Persist an interface-language setting with automatic system-locale selection and a manual picker. Keep it separate from notebook language groups and input/proofing language.
4. Add a pseudo-language UI audit and missing-catalog report. Missing translations fall back to English and remain reportable.
5. Prove the path with English, one LTR translation, and Arabic. Review Arabic in the real UI. Catalog translation alone does not deliver whole-window RTL mirroring or the RTL/caret requirements in `work/rtl-and-caret-bar.md`.
6. Translate the 15 standalone User Assistance guides and their index into French and Spanish under `docs/how-to/fr/` and `docs/how-to/es/`, keeping headings, instructions, code samples, feature limits, and intra-language links aligned with the English sources. Link both localized indexes from the English `docs/how-to/README.md`.

Not included in this estimate: Welcome notebook translation, local date/number conventions, spell checking, IME behavior, full RTL mirroring, or all 27 WordCraft languages.

## Acceptance criteria

- All visible interface strings in the agreed scope use lookup; stable IDs and user data remain language-neutral.
- English-only builds work without external catalogs on desktop and WebAssembly.
- Catalog builds resolve translations and fall back to English for missing entries.
- Language selection persists. Automatic mode uses a supported system locale or English fallback.
- The picker lists only locales actually embedded and supported by NoteCraft.
- The pseudo-language audit catches visible text that bypasses lookup; the report names missing translations without blocking ordinary development builds.
- Arabic rendering is reviewed in the real UI. Complete RTL layout claims require separate verification against the RTL requirements.
- `docs/how-to/` provides equivalent English, French, and Spanish indexes and guide coverage; localized links resolve within the same language set and technical instructions match current product behavior.
- Tests cover fallback, locale choice, persisted setting, representative UI text, and desktop/WASM builds.

## Estimate

Assuming shared runtime/catalog use and approval to raise the minimum Rust version:

| Work | Agent estimate | Owner review |
|---|---:|---:|
| Runtime, build embedding, locale setting and picker | 1 session, 30-50k tokens | 30-45 min |
| UI string extraction, English baseline, pseudo-language audit and report | 2-3 sessions, 100-180k tokens | 1-2 hours |
| LTR and Arabic proof, regressions, desktop/WASM verification | 1-2 sessions, 50-90k tokens | 45-90 min plus native-speaker review |
| French and Spanish User Assistance (16 Markdown files, ~8,924 source words) | 1-2 sessions, 40-80k tokens | 1.5-2.5 hours plus native-speaker review |
| **Total** | **5-8 sessions, 220-400k tokens** | **about 4-6.5 hours plus native-speaker review** |

Main unknowns are the MSRV/repository-boundary decision and the amount of manual work needed for dynamically composed UI labels. The UI string count remains unmeasured; the 8,924-word UA count includes Markdown and code and is a sizing measure, not a translation-ready word count.

## Coordination

- `_2` owns infinite canvas; `_3` owns performance markers and the voted-issue review.
- Coordinate changes to shared UI/test infrastructure before integration.
- Branch from `next` for implementation per the release branching guide. Preserve unrelated work in this checkout.
- Do not merge, push, or contribute to the shared catalog repository without explicit authorization.

## Progress

- 2026-11-04: Read-only WordCraft and Craft catalog/runtime research completed. The release README and this note record the proposed scope. No localization code or shared catalog changes have been made.
- 2026-11-04: Confirmed Spanish and French demo content exists, but the separate 16-file `docs/how-to/` guide set has no translations. Added French and Spanish User Assistance editions to this workstream; counted about 8,924 whitespace-delimited source words, including Markdown and code.
