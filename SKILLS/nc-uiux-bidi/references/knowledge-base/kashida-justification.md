# Kashida justification knowledge base

**Status:** Research note; not an implementation specification
**Added:** 2026-11-01
**Skill:** `nc-uiux-bidi`

This note records useful public guidance for future Arabic, Persian, and Urdu
justification work. It is deliberately a summary, not a copy of either source.

## Core conclusions

1. Justification is a writing-system decision, not one universal spacing rule.
   Possible strategies include inter-word or inter-cluster spacing, inter-letter
   spacing where the script permits it, inter-ideograph spacing, and character
   elongation/compression.
2. Arabic-script justification may use kashida, also called tatweel. U+0640 is
   the Unicode character commonly used to represent that elongation, but the
   typographic operation is broader than blindly inserting U+0640.
3. Kashida insertion belongs only at script-valid joins. Candidate sites need a
   priority policy and must be measured in the actual shaped font. Do not stretch
   arbitrary glyphs or split connected forms.
4. Font behavior differs. Naskh faces may respond cleanly to tatweel while some
   Nastaliq faces ignore it or require different calligraphic treatment. Detect
   the result per font and degrade gracefully rather than forcing a distorted line.
5. The last line of a paragraph normally uses the paragraph alignment instead of
   being expanded with large gaps. Hyphenation and other line-breaking rules affect
   how much justification pressure exists.
6. Mixed-script paragraphs may need more than one strategy on the same line. A
   Latin run, Arabic run, punctuation, and numbers must retain their logical text
   and shaping boundaries while the line is fitted.

## NoteCraft implications

- Keep logical text offsets unchanged. Justification must be a rendering/layout
  decision, not a mutation of the user's stored prose.
- Keep paragraph direction, page direction, and UI direction separate.
- Measure shaped advances after font selection. A fixed "one tatweel equals N
  pixels" assumption is not safe across fonts, sizes, scripts, or platforms.
- Treat kashida as a candidate operation with tests for valid joins, marks,
  lam-alef, word boundaries, mixed scripts, minimum/maximum pressure, and the
  last line.
- Preserve a fallback policy for fonts that do not support useful elongation:
  permitted spacing adjustments, alignment fallback, or an explicit limitation.
- Do not put kashida into the page file unless the product design explicitly
  chooses persisted authored justification. The current direction is to keep it
  in layout/rendering first.
- The owner bar currently records kashida as engine work measured but not yet in
  the window. This remains future work and is not a v0.0.8 release criterion.

## Source notes

### Harfsaz: Kashida justification

Source: <https://harfsaz.com/kashida-justification>

Useful points: kashida is placed only at allowed joins; candidate sites are
ranked; the result is measured until the line fits; and behavior is checked per
font. The source specifically distinguishes Naskh behavior from Nastaliq behavior
and describes graceful fallback when a font ignores tatweel.

### Microsoft Learn: Text justification

Source: <https://learn.microsoft.com/en-us/globalization/fonts-layout/text-justification>

Useful points: justification adjusts letter and word spacing within the writing
system's rules; connected glyphs should not be separated by generic letter
spacing; the last line may use ordinary alignment; and Arabic justification can
use character elongation/kashida. The article also describes different
justification strategies and low/medium/high kashida control as a product model.

## Capture and licensing boundary

The web pages are recorded here as linked research sources. Do not copy their
full text, screenshots, illustrations, fonts, or book pages into this skill
without checking licence and redistribution rights. If the owner adds a free
book or other reference later, store only the permitted excerpt or a local
summary, record its licence and attribution, and link to the original.

## Follow-up questions

- Should NoteCraft expose low/medium/high kashida strength, or only a single
  justified mode initially?
- Should a font that ignores tatweel fall back to word spacing, or report that
  the requested justification cannot be achieved faithfully?
- Which fonts are the acceptance set for Arabic, Persian, and Urdu on Windows,
  Linux, and macOS?
- Should PDF/export use the same shaping and justification path as the canvas?
