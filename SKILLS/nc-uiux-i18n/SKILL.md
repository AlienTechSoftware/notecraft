---
name: nc-uiux-i18n
description: Design, implement, audit, and verify NoteCraft interface localization using the shared ArtCraft craft-i18n runtime and catalog conventions.
license: MIT
metadata:
  version: "0.0.20261105"
  related_skills: [nc-uiux-features, nc-uiux-dialogs, nc-uiux-controls, nc-uiux-bidi, nc-ua, nc-code-rust]
---

# nc-uiux-i18n

Use this skill whenever NoteCraft adds or changes interface text, localization catalogs,
language selection, locale matching, translation coverage, or localization verification.

## Goal

Every string presented by NoteCraft's interface must be translatable. This includes visible
labels and messages, tooltips, accessible names, validation feedback, status text, dialog copy,
and errors shown to a user. A translated label must keep the same behavior and meaning as its
English source.

The user's page content and stored notebook data are not interface strings. Keep document text,
page names supplied by the user, file names, file formats, command IDs, settings keys, control
protocol fields, CLI output, MCP responses, and diagnostic logs stable and untranslated. When an
engine error is also returned to automation, translate it only at the UI display boundary; do not
change the underlying command result.

## Approved design

- Reuse `craft-i18n` from the ArtCraft `craft-libs` repository. Do not build a parallel locale
  matcher, plural engine, or runtime translation table in NoteCraft.
- NoteCraft's approved minimum Rust version for this dependency is 1.95. The current dependency
  pin is recorded in the workspace `Cargo.toml`; verify it before changing it.
- **Strong recommendation: make the English catalog the sole source of UI wording.** Put every
  user-facing English string in `i18n/en.tsv`, keyed by a stable, descriptive ID. UI code should
  pass IDs to the localization adapter and contain no user-facing English wording; locale catalogs
  translate the same IDs, and a missing translation falls back through `en.tsv`. This makes
  extraction auditable and prevents UI copy from silently bypassing translation. Do not keep an
  English sentence inline as a fallback beside its catalog entry: that duplicates the source and
  lets the two drift. Keep document content, logs, machine-readable results, and other explicitly
  excluded data outside the UI catalog. Machine-code symbols may be cataloged separately as
  English-invariant values under `SYMBOL_NAME_ID`, as described below.
- Keep catalogs as UTF-8 TSV and use the shared `craft-i18n` runtime. Before implementing the
  stable-ID catalog flow, verify that the pinned runtime and NoteCraft adapter can look up arbitrary
  UI IDs and fall back to `en.tsv`. The current runtime's `tid` path is for command-label IDs and
  still requires an English label fallback; it does not by itself satisfy this recommendation for
  all UI strings. If that capability is absent, extend the shared runtime or add a small supported
  adapter/build-time mapping, then test ID lookup, English fallback, locale fallback, placeholders,
  and pseudo-localization. Do not work around the gap by copying English wording into Rust.
- Use a stable TSV row shape for the ID catalog, such as
  `@id<TAB>ui.settings.interface_language<TAB>Interface language`; locale files use the same ID
  with their translated value. Confirm the exact syntax and escaping against the pinned runtime
  parser before adding rows. Preserve IDs when wording changes; IDs name concepts, not sentences.
- The former `context<TAB>English source<TAB>translation` source-key format is a migration state,
  not the target for newly extracted UI strings.
- Classify machine-code identifiers separately from natural-language UI copy. Use the explicit
  `SYMBOL_NAME_ID` category for function names, MCP tool names, argument keys, protocol symbols,
  and similar code identifiers that must remain byte-for-byte stable in every locale. Keep their
  canonical English values in the source catalog and reference them from code through generated or
  compile-time constants. Use IDs in the `SYMBOL_NAME_ID.<domain>.<name>` namespace (for example,
  `SYMBOL_NAME_ID.mcp.tool.search_pages`) and store the exact identifier as the English value. Do not
  send these values through locale translation or change their spelling, casing, or punctuation.
  Keep runtime protocol behavior independent of the selected UI language.
- Keep NoteCraft-specific translations in the NoteCraft repository during the pre-cutover phase.
  The current pilot is `i18n/<locale>.tsv`, embedded by `crates/ui-egui/src/i18n.rs`. Do not write
  to the shared `craft-i18n` repository or `.repos/storytold` mirror unless the owner explicitly
  authorizes that external contribution.
- The current pilot uses a static `include_str!` catalog list. This is not the final ArtCraft
  build integration. Before replacing it, follow the pinned `craft-i18n` integration guidance:
  build-time catalog embedding, reproducible catalog revision, optional English-only development
  builds, and an explicit release check when release infrastructure can provide the catalog.
- Interface language is an app preference, independent of page language, proofing/input language,
  and page direction. `auto` follows a supported operating-system locale and otherwise resolves
  to English. Offer only languages with embedded translations and supported UI behavior.
- Missing translations fall back to English. Never claim a locale is complete based only on its
  picker entry or catalog file.
- An RTL translation does not mean the NoteCraft window is RTL-ready. Whole-window mirroring and
  RTL interaction acceptance remain governed by `work/rtl-and-caret-bar.md` and `nc-uiux-bidi`.

The current code and the sibling reference paths are listed in
[`references/implementation-map.md`](references/implementation-map.md). If implementation moves,
update that map in the same change.

## What to localize

Localize the string at the point where the user sees it. Include text passed to egui's text and
accessibility APIs, not only the button caption.

- Menus, ribbon tabs and groups, buttons, selectors, headings, empty states, placeholders, and
  disabled-feature explanations.
- Modal titles, instructions, confirmations, warnings, errors, and button labels.
- Hover text, keyboard help, accessible widget names, and other non-painted UI text.
- Status bar messages, validation errors, import/export progress and failures, and user-facing
  command errors.
- Values formatted into translated sentences. Preserve named placeholders so translators can
  reorder them; use the shared runtime formatting helper rather than concatenating translated
  fragments.
- Plural messages through the runtime plural API. Do not choose English singular/plural forms in
  application code.

Do not mechanically translate every Rust string literal. First decide whether it is interface
copy, user/document content, a stable identifier, a format token, a test selector, a log, or a
machine-readable result. Keep the source unchanged where users or integrations depend on it.

## Workflow

1. **Read the current state.** Check the branch and working tree. Read the NoteCraft i18n adapter,
   the owning UI module, its tests, the relevant design skill, and the implementation map. Inspect
   the exact pinned `craft-i18n` API and the ArtCraft sibling integration references; do not rely
   on memory when the pin may have changed.
2. **Define the surface and acceptance criteria.** Name the screens, dialogs, states, and dynamic
   messages in scope. A complete extraction batch must cover painted text, accessibility text,
   tooltips, and errors for those states. Keep this criterion observable in tests or an audit.
3. **Add or update the regression check first.** Verify that a representative UI string goes
   through the lookup, that a translation is selected, and that a missing row falls back to
   English. Verify placeholders and plural forms where used. Persisted language changes must be
   read back from `Settings` after app reconstruction.
4. **Externalize the interface wording.** For new and migrated UI text, add the English wording to
   `i18n/en.tsv` under a stable ID, add matching locale rows, then call the adapter with that ID
   (for example, `crate::tl_id!("ui.settings.interface_language")`). Do not leave the English
   wording in Rust as a fallback. Keep identifiers, automation labels, log text, user content, and
   protocol values outside UI translation. If the pinned runtime or adapter lacks the needed ID
   lookup, first add and test a supported adapter/build-time path that reads the English catalog;
   do not silently fall back to sentence-key literals.
5. **Add catalog rows.** Put NoteCraft's localized rows in `i18n/<locale>.tsv` while this checkout
   is pre-cutover. Follow exact TSV escaping, context, placeholder, and plural rules. Keep language
   names in their own language with `@id<TAB>language.name<TAB>...`. Do not add empty or speculative
   translations.
6. **Audit actual rendered states.** Static search finds candidates but cannot prove coverage.
   Follow WordCraft's pseudo-language audit pattern: draw representative app states with marked
   pseudo text, collect rendered text, and fail on interface wording that appears unmarked. Cover
   menus, dialogs, tooltips, accessibility text, and error states; explicitly whitelist document
   text and other non-interface values. Keep the audit deterministic and report exclusions with
   their reason.
7. **Report catalog gaps.** Enumerate source strings actually looked up and compare them with
   available locale rows. Report missing and stale rows without making normal English-only builds
   impossible. Validate every catalog with `I18N.check_all()` and the shared catalog checker when
   available.
8. **Verify the real UI.** Run focused localization/UI tests, `cargo fmt --check`, clippy for the
   changed crates, and the required build targets. Review each translated screen at the same
   viewport, theme, and scale as English. Have a fluent reviewer check meaning, grammar, shortcuts,
   truncation, and tone before calling that locale reviewed.
9. **Update coverage records.** Record translated locales and known gaps in the canonical
   localization plan. Keep the Welcome demo source, generated HTML/CMGFM outputs, and standalone
   `docs/how-to/` translations distinct; use their own source and generation rules.

## Pseudo-localization

Use deliberately artificial pseudo-locales as selectable UI locales and coverage tools alongside
real translations such as French. The app treats their tags and catalogs through the ordinary
locale flow, while their text remains an audit aid rather than a natural-language translation.

Use these canonical BCP 47 tags for NoteCraft pseudo-locale modes:

- `en-XA`: left-to-right pseudo-localization. Keep the English source unchanged and wrap it in
  unmistakable markers, for example `Options` becomes `$$$Options@@@`.
- `ar-XB`: right-to-left pseudo-localization. Keep the same English source and markers, isolate
  the marked run with Unicode RTL controls, and exercise bidi ordering, punctuation, mixed-script
  runs, clipping, and layout direction. This tests bidi handling, not Arabic translation or shaping.

These tags are pseudo-locales, not two-letter language codes. Offer them in the same interface
language picker and persist them through the same preference flow as other UI locales. Their only
locale-set exception is the demo notebook: do not add either pseudo-locale to demo content. Keep
real Arabic (`ar`) separate from `ar-XB`. Passing an `ar-XB` pseudo audit does not establish RTL
readiness; use the real RTL acceptance criteria in `work/rtl-and-caret-bar.md` and `nc-uiux-bidi`
as well.

- Treat `en-XA` and `ar-XB` as catalog-backed locales in `i18n/en-XA.tsv` and
  `i18n/ar-XB.tsv`, loaded through the same `craft-i18n` path as real languages. Do not generate
  markers automatically at lookup time: that masks missing pseudo catalog rows and makes the
  coverage check deceptive.
- Copy each extracted stable UI ID and its English wording from `en.tsv` into both pseudo
  catalogs. The `en-XA` translation wraps each displayed line in `$$$` and `@@@`; the `ar-XB`
  translation also isolates each marked line with RLI (`U+2067`) and PDI (`U+2069`). Keep
  placeholders and immutable values unchanged inside the markers.
- Add new and changed extracted stable UI IDs to both pseudo catalogs alongside real locale rows.
  A missing pseudo row must fall back to plain English, just as a missing real translation does.
  This is intentional: it exposes both extraction omissions and stale/incomplete pseudo resources.
- Expose the pseudo-locales as ordinary app language choices and persist the chosen tag. Do not add
  build flags for these modes unless a later owner-approved release policy requires one.
- Audit rendered UI in pseudo mode. Marked text proves both that the UI used the lookup and that
  the pseudo translation resource contains the source key. Plain English requires investigation;
  it may be an unextracted UI string or a missing pseudo catalog row. Classify notebook content,
  product names, and stable format identifiers separately.
- Exercise dialogs, menus, tooltips, errors, accessibility names, and longer dynamic messages,
  not just the default screen. Whitelist notebook/document content and stable non-UI values with a
  stated reason so they do not create false positives.
- Keep pseudo catalogs as first-class locale resources. The runtime must not synthesize a marker
  when a row is missing, because that would hide extraction or catalog gaps. Record uncovered
  strings and dynamic UI values in the audit report.

**Implementation status:** `en-XA` and `ar-XB` are catalog-backed locale codes. Their checked-in
TSV rows preserve English inside the markers; absent rows fall back unmarked. Treat this as an
extraction/catalog coverage check and bidi/layout probe only; it does not verify real translations
or full real-language RTL acceptance.

Example: `Options` renders as `$$$Options@@@` in `en-XA`, and as `U+2067 $$$Options@@@ U+2069`
in `ar-XB` (the two code points are invisible direction controls).

## Code rules

- Keep interface-language choices readable: show each locale's full name in the menu with a
  small flag marker. The compact header control may use a flag and locale code. Draw flag markers
  with UI primitives or bundled vector assets; do not rely on emoji glyphs being installed.
  English uses a representative English-speaking country flag, while `en-XA`/`ar-XB` retain their
  explicit pseudo-locale names and codes.
- Prefer a whole translatable sentence over concatenated fragments.
- Use named placeholders such as `{name}` or `{count}` and preserve each one in catalog rows.
- Give context only when the same English source has different meanings or a layout requires a
  different translation. Do not use context as a substitute for clear source wording.
- Keep shortcuts, command IDs, enum values, stable settings keys, and file-format tokens outside
  translated strings. Localize the human-readable label separately.
- Translate the accessible name and tooltip independently when they describe different actions or
  provide different detail. Never leave an English accessibility label behind a translated
  caption.
- Do not hide a missing translation, malformed row, invalid locale, or failed lookup. Log catalog
  data problems locally and preserve the English fallback.
- Do not make locale selection depend on notebook contents. Do not infer page direction from the
  app language.
- Do not declare Arabic, Hebrew, Persian, or Urdu ready until the separate RTL UI acceptance work
  passes in the real app.

## Verification

For each batch, run the changed crate's focused tests and lint checks. Before a localization
feature is ready for review, run the rendered-text audit and catalog-gap report, then the
repository's required CI/build targets, including desktop and WASM when the UI or catalog wiring
is shared by both. Run demo generation/parity checks only when demo source or generated notebook
outputs changed.

The required reports must show:

- No unreviewed visible interface literal bypassing the lookup in covered UI states.
- Catalog rows parse without errors; placeholders and plural forms match their sources.
- English fallback works for missing rows and English-only builds.
- Language selection persists, automatic mode resolves as designed, and the picker lists only
  supported embedded locales.
- The translated UI is legible and usable at matching viewport, theme, and scale.

See [`references/catalog-and-audit.md`](references/catalog-and-audit.md) for the catalog format,
ArtCraft data flow, and extraction/audit distinctions.
