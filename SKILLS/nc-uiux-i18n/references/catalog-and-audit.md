# Catalog, extraction, and audit workflow

## What is stored where

The target NoteCraft approach has three distinct parts:

1. **English source** is stored once in `i18n/en.tsv`, keyed by a stable UI ID. For example,
   `@id<TAB>ui.notebook.open<TAB>Open notebook` keeps the wording out of Rust.
2. **Translation data** is stored in locale TSV catalogs under the same stable IDs. Shared terms
   may live in `common.tsv`; app-specific terms live in the app's catalog. Missing locale rows
   resolve to the English value for that ID.
3. **Runtime behavior** comes from `craft-i18n`: locale matching, fallback, plural handling,
   catalog parsing, and language metadata. Apps embed the selected catalogs at build time. The
   adapter must support arbitrary UI-ID lookup and English-catalog fallback; the existing
   command-label `tid` fallback, which requires inline English, is not sufficient by itself.
   Machine-code names may use the separate `SYMBOL_NAME_ID.<domain>.<name>` namespace; resolve
   their canonical English values through generated or compile-time constants and never localize
   the values.

This means "extract all strings" means route every user-facing string through stable-ID lookup,
maintain the English source row and locale translation rows, and audit that Rust contains no
user-facing English wording. `en.tsv` is the authoritative English source, not a duplicate of
inline source literals. A generated JSON inventory may be useful for reports, but it must not
become another independently edited source of truth.

## TSV rows

Each row has exactly three tab-separated columns:

```text
@id<TAB>stable.ui.id<TAB>English source or locale translation
```

Use stable, descriptive IDs that name concepts rather than wording. Preserve the ID when text is
rephrased. Follow the current shared catalog parser for `@id`, `@msg`, plural contexts, escaping,
and placeholder checks. Verify that the pinned runtime accepts general UI IDs in the app catalog;
the special `@id language.name` row and command-label `tid` API alone do not establish general
stable-ID lookup support. Do not invent syntax from this summary.

Example:

```text
@id<TAB>ui.notebook.open<TAB>Open notebook
@id<TAB>language.name<TAB>Français
```

The locale catalog uses the same UI ID with its translated wording in the final column. Avoid
copying illustrative rows by hand when a catalog tool can validate them.

## Extraction is not translation coverage

- A stable-ID lookup shows the string is routed through lookup, even if every locale falls back
  to the English catalog.
- A locale TSV row supplies a translation for one stable UI ID.
- A runtime pseudo-language audit finds rendered text that bypassed lookup in the states it ran.
- A catalog report finds missing translations among strings that were looked up, and may identify
  stale catalog rows.

These checks answer different questions. Static `rg` searches are useful for finding candidate
literals, but cannot determine whether a value is interface text or prove that menus, tooltips,
errors, and accessibility labels were rendered.

## Source audit and exemptions

Run `cargo xtask i18n-audit` before reviewing i18n changes. The command is also part of
`cargo xtask ci` and the repository pre-commit hook. It checks English catalog structure and
duplicate IDs, verifies that `en-XA` and `ar-XB` contain the English ID and plural sets with the
expected wrappers, validates literal IDs passed through `tl_id!` and localized formatter calls,
and scans production Rust UI code for literal text passed directly into common egui text sinks.
Helpers that accept UI text must be marked directly above the function with a parameter list, for
example `// i18n-ui-text: label,tip`; callers are then checked for raw literals too. Prefer passing
`tl_id!("ui.stable_name")` through these helpers.

The audit is deliberately a static source gate, not proof of rendered coverage. It cannot infer
all project-specific helper semantics, runtime-generated text, all accessibility metadata, or
whether every hidden UI state was exercised. Keep the `en-XA`/`ar-XB` rendered-state audit in the
workflow and expand helper annotations/sinks when new patterns are added. Real-locale missing-row
counts are informational because runtime English fallback remains supported.

For a necessary non-language literal, put a narrow reason on the same line or immediately above
the literal:

```rust
ui.label("T"); // i18n-exempt: single-letter toolbar glyph
```

`i18n-exempt` is reserved for actual non-language values such as a glyph, a stable machine code,
or a keyboard shortcut when the displayed form is intentionally invariant. It is not a way to
waive ordinary prose. The audit rejects blank and stale exemptions, so keep each comment attached
to the specific literal it documents. If the value is user-facing language, add an English ID and
use the catalog lookup instead.

## Integration phases

The current NoteCraft pilot stores `i18n/<locale>.tsv` in this repository and directly embeds it
in `crates/ui-egui/src/i18n.rs`. ArtCraft's pre-cutover convention permits app-local TSV catalogs.
The aligned build path is to generate an embedded catalog table from a pinned `craft-i18n`
checkout using `CRAFT_I18N_DIR`, when NoteCraft's build and release workflows can supply that
checkout reproducibly. Development builds should remain English-capable without external catalog
data; release builds should fail if required translations were not embedded. Desktop and WASM
must use the same selected catalog set.

Until an owner-approved cutover:

- Keep NoteCraft catalog edits local.
- Do not write to the shared catalog repository or the Storytold mirror.
- Record the last synchronized state before any later catalog migration.
- Test the exact runtime and catalog revisions used by the build.

## Pseudo-language audit

Use the canonical pseudo-locale tags `en-XA` (LTR) and `ar-XB` (RTL). Offer them in the ordinary
interface language picker and persist them as normal UI preferences. Their only locale-set
exception is the demo notebook; do not add them as demo content languages. Keep real Arabic `ar`
distinct from `ar-XB`. Store pseudo translations in `i18n/en-XA.tsv` and `i18n/ar-XB.tsv` and load
them through the same catalog runtime as real locales. Every extracted source key needs rows in both
files: `en-XA` wraps each line in `$$$` and `@@@`; `ar-XB` adds RLI/PDI isolation around each marked
line. A missing pseudo row must fall back to plain English. This makes pseudo rendering validate
both extraction and pseudo-catalog coverage, while still not verifying Arabic translation quality
or shaping.

Follow WordCraft's approach rather than relying on a list of string literals alone:

1. Run the UI through selected reproducible states using `en-XA` and `ar-XB`. Every extracted
   lookup with a pseudo catalog row must be visibly marked; missing rows intentionally remain plain
   English. `ar-XB` must also exercise RTL ordering and mixed-direction text.
2. Collect rendered text from the UI output and report remaining ordinary-language text.
3. Exercise different screens, menus, dialogs, tooltips, and accessible widget metadata. A single
   default frame does not cover hidden states.
4. Whitelist only known non-interface text, such as actual notebook content, user-supplied names,
   stable codes, keyboard shortcuts, and machine-oriented status values. Name the exemption and
   its reason in the audit.
5. Keep the pseudo audit a failing check for untranslated call sites and missing pseudo rows. Keep
   missing real translations a report so normal builds can safely fall back to English.

Passing the pseudo audit checks lookup coverage and layout stress only. It does not verify real
translation quality or replace the separate RTL interaction acceptance work.

The audit is incomplete until the coverage list includes every implemented user-facing state in
its declared scope. A clean report from a narrow state set is not proof of whole-app extraction.
