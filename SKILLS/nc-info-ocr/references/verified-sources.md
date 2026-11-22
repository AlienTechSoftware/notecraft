# OCR and description: verified sources

Checked 2026-11-22 from the sources named. "Verified" means the value was read from that source
in this check. "Not verified" means it was taken from a secondary source or memory, or the check
failed; treat it as open until checked.

| Claim | Value | Source | Status |
|---|---|---|---|
| ocrs crate licence | MIT OR Apache-2.0 | `cargo info ocrs` (crates.io index), version 0.14.0 | Verified |
| rten crate licence | MIT OR Apache-2.0 | `cargo info rten`, version 0.28.0 | Verified |
| ocrs model weights licence | CC-BY-SA 4.0 | Hugging Face model card `robertknight/ocrs` (`license: cc-by-sa-4.0`) | Verified |
| ocrs training data | HierText, plus synthetic data | Hugging Face card; ocrs-models README | Verified |
| Official ocrs ONNX model files | `text-detection.onnx`, `text-recognition.onnx` at the ocrs model host | HTTP 200 on 2026-11-21; loaded and run locally | Verified |
| Hugging Face RTen conversions | Gave fragmented output (one character per picture) | Local run, 2026-11-21 | Verified (measured) |
| ocrs has no per-line confidence | Not in its public output types | ocrs 0.14 source (`text_items.rs`, `TextChar`, `TextLine`) | Verified |
| ocrs `text_threshold` not public | Field lives in the detector params, not `OcrEngineParams` | ocrs 0.14 source (`lib.rs`, `detection.rs`) | Verified |
| ocrs default alphabet is English | Default alphabet matches the original models | ocrs 0.14 `OcrEngineParams` docs | Verified |
| PaddleOCR code licence | Apache-2.0 | GitHub API, `PaddlePaddle/PaddleOCR` | Verified |
| PP-OCRv5 has an Arabic recogniser | `arabic_PP-OCRv5_mobile_rec` | PaddleOCR PP-OCRv5 multi-language docs (`PP-OCRv5_multi_languages.en.md`) | Verified |
| PP-OCRv5 language count | Not confirmed | Not found in the pages read | Not verified |
| PP-OCRv5 model weights licence | Not checked | Not checked | Not verified |
| ONNX Runtime licence | MIT (commonly stated) | GitHub LICENSE file not reachable in this check | Not verified |
| `ort` crate licence | MIT OR Apache-2.0 | `cargo info ort` | Verified (the wrapper; ONNX Runtime itself is a C++ library) |
| Florence-2 base licence | MIT | Hugging Face `microsoft/Florence-2-base` card | Not verified in this check (the large model was checked) |
| Florence-2 large | MIT, about 0.78 B parameters | Hugging Face `microsoft/Florence-2-large` (`params` 776,721,497) | Verified |
| Florence-2 base parameters | About 0.23 B | Stated in the report; not checked on the card here | Not verified |
| Florence-2 ONNX export exists | `onnx-community/Florence-2-base-ft`, files under `onnx/` (MIT card) | Hugging Face API, file list | Verified (existence only; not run) |
| Qwen2.5-VL 7B Instruct licence | Apache-2.0 | Hugging Face `Qwen/Qwen2.5-VL-7B-Instruct` card | Verified |
| Qwen2.5-VL 7B parameters | About 8.3 B | Hugging Face API | Verified |
| Qwen2.5-VL 3B Instruct licence | `qwen-research` | Hugging Face card metadata and README (`license_name: qwen-research`) | Verified |
| Qwen2.5-VL 72B Instruct licence | `other` | Hugging Face API | Verified |
| viveNotes pipeline | PP-OCRv5 through ONNX Runtime on Android, English recogniser only | Local code read by the supervisor (Kotlin); not an external source | Recorded by the team; not re-checked here |

## Sources

- crates.io index (`cargo info`): ocrs, rten, ort.
- Hugging Face API and cards: `robertknight/ocrs`, `microsoft/Florence-2-large`,
  `onnx-community/Florence-2-base-ft`, `Qwen/Qwen2.5-VL-3B-Instruct`, `Qwen/Qwen2.5-VL-7B-Instruct`,
  `Qwen/Qwen2.5-VL-72B-Instruct`.
- GitHub: `robertknight/ocrs` README, `robertknight/ocrs-models` README, `PaddlePaddle/PaddleOCR`.
- PaddleOCR docs: `docs/version3.x/algorithm/PP-OCRv5/PP-OCRv5_multi_languages.en.md`.
