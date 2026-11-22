---
name: nc-info-ocr
description: Architecture and design reference for NoteCraft's image text work: traditional OCR of printed text in pictures (companion text, line boxes, search hits) and AI image descriptions for alt text, with the engines, models, licences, status and open decisions. Use before changing the OCR crate, its companion files, the description path, or any decision about OCR engines or models.
license: MIT
metadata:
  version: "0.0.20261122"
  owner_area: image text (OCR and descriptions)
  related_skills: [nc-info-search, nc-arch, nc-arch-x-plat, nc-code-rust]
  references: [references/verified-sources.md, references/bakeoff-2026-11-22.md]
---

# nc-info-ocr - Image text: OCR and descriptions

## What this skill covers

NoteCraft pages can hold pictures (`assets/*.png`, `*.jpg`). This skill describes two different
jobs that both produce text about a picture, and why they are kept apart:

1. **Traditional OCR (content).** Reads the printed text that is in the picture, with the box of
   each line. Output is derived, machine text. It can be rebuilt at any time from the picture.
2. **Descriptions (alt text).** A short sentence about what the picture shows, for screen readers.
   Output is authored text. A person writes it or accepts a model's draft. A model never replaces
   it silently.

Keep them separate in code and on disk. Mixing them is the usual mistake: OCR text is derived and
can be overwritten; alt text is the user's and must not be.

## Status (2026-11-22)

| Part | State |
|---|---|
| OCR crate `crates/ocr` (`notecraft-ocr`, layer 1) | Built on branch `feat/ocr/agent-1`, local only, not pushed |
| Engine | `ocrs` 0.14 (MIT or Apache-2.0, pure Rust), behind the `PictureReader` trait; loads the official ONNX models |
| Companion job | Built: hash check, text and hash files written through a temporary file |
| Acceptance A1 to A5 | Pass against the POC pictures |
| Keyword search (A6, A7) | Not built. Storage choice open, see `nc-info-search` |
| UI box highlight (A8) | Not built |
| Arabic (A9) | Measured: fails with `ocrs` (English recogniser) |
| No network (A10) | No networking crate in the OCR crate's tree; not yet run with the network off |
| CI on three platforms (A11) | Not run; needs a push |
| Description model (Stage 1b) | Not started. Candidates in the references; no decision |

Open owner decisions: the model weights licence (see below), accepting an optional confidence,
Arabic in MVP scope, and the GO for new dependencies.

## Architecture

```
picture (assets/x.png)
   |
   |  decode (image crate, already in the workspace)
   v
PictureReader  (trait, crates/ocr)  <-- OcrsReader today; PP-OCRv5, platform engines plug in here
   |
   |  detect words -> group rows -> recognise lines -> join pieces of one row
   v
Vec<Line>  { text, quad: [Point; 4], confidence: Option<f32> }   (pixels, clamped to the picture)
   |
   |  companion job: FNV-1a hash of the bytes, compared with <picture>.ocr-hash
   v
<picture>.txt        derived text, one line per row      (replaced when the hash changes)
<picture>.ocr-hash   hash of the bytes it was read from  (written after the text)
```

### Why these pieces

- **The trait is the seam.** The companion job depends on `PictureReader`, not on `ocrs`. A test
  double runs the job without model files (unit test in `crates/ocr/src/lib.rs`). A later engine
  is one more `impl`, with no change to the job.
- **Derived files are never authored.** `.txt` and `.ocr-hash` are rebuilt from the picture. The
  alt text file (`<picture>.alt.txt`, RFC 0003) is never written by the OCR job.
- **Atomic replacement.** Text and hash are written to `<name>.tmp` and renamed, so a crash leaves
  the old pair, never a half-written file.
- **Stable hash.** FNV-1a 64 bit, written in code. The standard library's hasher is not stable
  across runs, so it cannot be used for a file that outlives the process.
- **Rows, not fragments.** The engine reports an item and its price as separate lines. The
  engine joins pieces that overlap vertically by at least half the shorter piece, left to right.
- **Boxes stay on the picture.** The engine can return a box a few pixels past the edge; boxes are
  clamped (criterion A3).
- **Confidence is optional.** `ocrs` 0.14 does not report a per-line confidence. The field is
  `Option<f32>`; when present it must be in 0 to 1. Owner decision pending.

### Traditional OCR: the engines

| Engine | Licence (checked) | Status here |
|---|---|---|
| `ocrs` 0.14 (`robertknight/ocrs`) | MIT or Apache-2.0 (crate); its trained weights: CC-BY-SA 4.0 (model card) | Used. English recogniser. Measured |
| PP-OCRv5 through ONNX Runtime | PaddleOCR code: Apache-2.0 (GitHub). PP-OCRv5 ships a separate Arabic recogniser (`arabic_PP-OCRv5_mobile_rec`, PaddleOCR docs) | Fallback candidate. Needs an ONNX Runtime binding, a C++ library: owner exception to the all-Rust rule |
| Platform engines (Apple Vision, Windows.Media.Ocr) | OS APIs | Optional fast paths, behind the trait. Not built |

**Model weights licence is the blocker for shipping `ocrs`.** The Hugging Face card for the ocrs
weights says CC-BY-SA 4.0 (trained on HierText). The code licence does not cover the weights.
Shipping them needs an owner decision. Measuring them does not.

### Descriptions (alt text)

- **Authored alt text is the MVP.** Attachment panel field; stored as `<picture>.alt.txt`
  (RFC 0003 option A). A person writes or accepts it.
- **Model drafts are Stage 1b, after the MVP is measured.** A model writes a draft; a person
  accepts or edits it before it becomes alt text. Never silent.
- **Candidates (licences checked on Hugging Face cards, 2026-11-22):**
  - Florence-2 base (`microsoft/Florence-2-base`): MIT and about 0.23 billion parameters are
    stated in the OCR report, but not yet checked on its card. Captions; English-centred. An ONNX
    export exists on Hugging Face (`onnx-community/Florence-2-base-ft`, MIT card, `onnx/*.onnx`
    files listed). Whether it runs from Rust with the ONNX route is not yet tested.
  - Florence-2 large: MIT, about 0.78 billion parameters.
  - Qwen2.5-VL 7B Instruct: Apache-2.0 on its card, about 8.3 billion parameters. Multilingual.
    Large: needs a capable machine.
  - Qwen2.5-VL 3B Instruct: its card names the licence `qwen-research`, which is not an open licence.
    Do not ship without reading it.
  - Qwen2.5-VL 72B Instruct: card says `other`.
- **Test before choosing (Stage 1b):** run Florence-2 base and Qwen2.5-VL 7B on the POC pictures,
  including the Arabic receipt, and record the result. Check the ONNX route for each first.

### Stage plan (from the Stage 1 plan and the OCR report)

- **Stage 1 (MVP):** printed OCR with boxes, companion text, alt text by hand, keyword search, box
  highlight. Descriptions by a model are Stage 1b.
- **Stage 2:** handwriting (lines with TrOCR or Kraken), equations (pix2tex or Pix2Text).
- **Stage 3:** drawings and shapes to Mermaid (experimental).

## Rules for changes in this area

- Test first. Each criterion (A1 to A11) has a test, written before the behaviour, per `AGENTS.md`.
- Tests that need the POC pictures or the model files read them from environment variables
  (`NOTECRAFT_OCR_FIXTURES`, `NOTECRAFT_OCR_MODELS`). Never commit the POC notebook or model weights.
- Scratch tests copy the picture into a temporary folder; they never write companion files into
  the fixtures.
- New engines implement `PictureReader`. New dependencies need the owner's GO.
- Do not copy code from other apps. viveNotes (Android, Kotlin) was checked for its pipeline
  shape only; its recogniser is English-only.
- Licence first: record the weights licence from the model card before any model is used in a
  release.

## Commands

```
NOTECRAFT_OCR_FIXTURES=<POC Images folder> NOTECRAFT_OCR_MODELS=<folder with text-detection.onnx and text-recognition.onnx> cargo test -p notecraft-ocr
cargo clippy -p notecraft-ocr --all-targets -- -D warnings
```

## References

- `references/verified-sources.md`: every licence and fact used here, with its source and whether
  it was checked.
- `references/bakeoff-2026-11-22.md`: the measured `ocrs` results on the POC pictures, timings,
  and what failed.
