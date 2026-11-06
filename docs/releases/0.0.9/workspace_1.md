# Workspace 1: Internationalization

**Release:** v0.0.9
**Checkout:** `o-NoteCraft_1`
**Updated:** 2026-11-05
**Status:** Partial implementation on `rc_0.0.9`. The shared runtime, persistent picker, French/Spanish catalogs, and `en-XA`/`ar-XB` modes are present. Full UI extraction, rendered-text audit, remaining demo locales, and User Assistance translations remain open.

## Goal

Implement [#31 Translatable UI](https://github.com/AlienTechSoftware/notecraft/issues/31) by routing every user-facing interface string through the ArtCraft-aligned localization runtime. Cover application chrome, dialogs, status text, tooltips, accessible names, and user-facing errors. Target the 13 locales represented in the demo notebook. Keep the standalone French and Spanish User Assistance work distinct from app UI localization.

## ArtCraft pattern found in WordCraft

- WordCraft embeds pinned locale catalogs at build time from `craft-i18n` and uses the shared `craft-libs` runtime for lookup, locale matching, English fallback, plurals, and pseudo-language support.
- Catalog data is separate from app code: `locales/<locale>/common.tsv` holds shared strings and `<app>.tsv` holds app strings and overrides. Rows contain context, English source text, and translation.
- There is no runtime network fetch. A build without catalog data is English-only; release builds can require the pinned catalogs. Web builds embed catalogs too.
- `cargo xtask i18n-report` reports missing or stale translations. A pseudo-language UI audit finds visible text that bypasses lookup.
- Useful read-only references in `.repos/storytold`: `wordcraft/crates/ui-egui/src/i18n.rs`, `wordcraft/crates/ui-egui/build.rs`, `wordcraft/crates/ui-egui/src/i18n/audit.rs`, `wordcraft/xtask/src/main.rs`, `craft-i18n/docs/integration.md`, and `craft-i18n/docs/catalog-format.md`.

## NoteCraft baseline and approved decisions

NoteCraft currently has no interface localization layer or locale catalogs. Its `crates/input-language` RTL keyboard detection is for editing behavior, not interface translation. Persistent app settings are managed through `notecraft_store::Settings`.

The bundled Welcome notebook already has Spanish and French content: 15 authored demo pages in each locale, generated to both HTML and CMGFM. The separate User Assistance set under `docs/how-to/` has 16 Markdown files including its index, about 8,924 whitespace-delimited source words, and currently has no French or Spanish editions. These are distinct deliverables. The existing demo translations can inform terminology, but the standalone guides need their own accurate translations and working localized links.

The owner approved reuse of the shared `craft-i18n` runtime and raising NoteCraft's minimum Rust version to 1.95. Keep NoteCraft-specific catalogs in this repository for now; do not edit the read-only `.repos/storytold` mirror or contribute catalogs to another repository without separate authorization.

A coarse scan of NoteCraft's UI source found 441 capitalized string matches across 12 of 31 Rust files; this includes non-UI strings and is not a count of translatable labels. The existing `cargo xtask text` command checks text hygiene, not localization coverage.

## Proposed v0.0.9 scope

1. Route agreed user-visible interface strings through one lookup API, keeping command IDs, document content, file formats, and automation selectors language-neutral.
2. Embed pinned catalogs at build time for desktop and WebAssembly. Keep English-only builds working when catalogs are unavailable; require catalogs in release builds only after the build pipeline supplies them reproducibly.
3. Persist an interface-language setting with automatic system-locale selection and a manual picker. Keep it separate from notebook language groups and input/proofing language.
4. Add a pseudo-language UI audit and missing-catalog report. Missing translations fall back to English and remain reportable.
5. Extract all interface strings used by the app into the English source lookup set, then use marked pseudo-locales to find remaining untranslated UI. Offer `en-XA` and `ar-XB` in the interface language picker and persist them like other UI languages. Preserve the English source between `$$$` and `@@@`; wrap `ar-XB` output in RTL isolation controls. Their only locale-set exception is that they do not participate in the demo notebook. Do not add build flags in this phase.
6. Provide translations for the 13 demo locales to the best available quality. French is the first real-language pilot; review each locale separately and do not infer RTL readiness from translated strings.
7. Translate the standalone User Assistance guides and their index into French and Spanish under `docs/how-to/fr/` and `docs/how-to/es/`, keeping headings, instructions, code samples, feature limits, and intra-language links aligned with the English sources. Link both localized indexes from the English `docs/how-to/README.md`.

Not included in this estimate: Welcome notebook translation, local date/number conventions, spell checking, IME behavior, full RTL mirroring, or all 27 WordCraft languages.

## Acceptance criteria

- All visible interface strings in the agreed scope use lookup; stable IDs and user data remain language-neutral.
- English-only builds work without external catalogs on desktop and WebAssembly.
- Catalog builds resolve translations and fall back to English for missing entries.
- Language selection persists. Automatic mode uses a supported system locale or English fallback.
- The picker offers the 13 demo languages once their UI catalogs are embedded, plus `en-XA` and `ar-XB`. Automatic system-locale matching never selects a pseudo-locale.
- All interface strings in the agreed UI surface use lookup, and the pseudo-language audit catches visible text that bypasses it; the report names missing translations without blocking ordinary development builds.
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

- 2026-11-05: Used the owner's `en-XA` and French screenshots as an audit pair. Added French entries for visible search/add-page/drop-file/status/ribbon labels, the notebook/item plurals, and the default-format status. Kept `START HERE` and page/notebook titles as notebook content, `Discord` as a product name, and `HTML`/`CommonMark + GFM` as stable format identifiers. Applied the selected locale before startup status text is created and refreshes that status on a language change. The UI and catalog regressions pass; full UI extraction and the remaining 11 demo-language catalogs remain open.
- 2026-11-05: Clarification: pseudo lookups preserve source English, using `$$$source@@@` for `en-XA` and RLI/PDI isolation around that marker for `ar-XB`; they do not distort the words. Both pseudo locales are selectable and persisted, excluded from automatic locale selection and demo content. Added code-drawn language flags to the Options and header pickers; full locale names remain visible in menus, with XA/XB codes in the header. Focused flag, locale, persistence, and pseudo tests pass, as do formatting, clippy, and release check. Full string extraction, 11 other demo-language catalogs, rendered-state audit, and visual owner review remain outstanding.
- 2026-11-05: Rebased the implementation branch onto `698943139f70211f4ce88a02f174383394d1733e`; resolved restore conflicts while retaining main's newer UI behavior. Added the shared-runtime adapter, persistent language picker, initial French catalog, and pilot UI test. The full string extraction and rendered-state audit are not complete, so the overall localization work is not ready for final inspection.
- 2026-11-05: Consolidated on `rc_0.0.9` at `d4637ce37d7e0261df29ffa138e9d171a509e3ee`. The current local pilot offers English, French, Spanish, `en-XA`, and `ar-XB`; only English is the complete fallback. Spanish and French are partial, the other demo locales are not offered, and the rendered-text audit and User Assistance translations remain outstanding. Focused locale tests and the full repository CI gate pass.
- 2026-11-04: Read-only WordCraft and Craft catalog/runtime research completed. The release README and this note record the proposed scope. No localization code or shared catalog changes have been made.
- 2026-11-04: Confirmed Spanish and French demo content exists, but the separate 16-file `docs/how-to/` guide set has no translations. Added French and Spanish User Assistance editions to this workstream; counted about 8,924 whitespace-delimited source words, including Markdown and code.
