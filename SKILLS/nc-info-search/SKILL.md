---
name: nc-info-search
description: Architecture and design reference for NoteCraft's search: today's plain scan, and the planned hybrid search (keyword BM25 plus semantic embeddings, merged with reciprocal rank fusion), covering what is indexed, where the index lives, the candidate engines and models with their licences, and the open decisions. Use before changing search, adding an index, choosing an embedding model, or deciding on any search dependency.
license: MIT
metadata:
  version: "0.0.20261122"
  owner_area: search (keyword and semantic)
  related_skills: [nc-info-ocr, nc-arch, nc-arch-x-plat, nc-code-rust]
  references: [references/current-search-code.md, references/verified-sources.md]
---

# nc-info-search - Hybrid search: keyword and semantic

## Local-first rule

The owner strongly prefers to stay local. Search follows the same rule as OCR:

- **Queries, indexes and embeddings stay on the device.** No query text, page text or vector is
  sent to a server. Hosted search services are excluded for this reason, including the hosted
  tier of any database with a local mode.
- **The index is local and rebuildable.** It lives in the app's local data folder and is rebuilt
  from the notebook files. It never needs a network to exist.
- **Models are local files.** An embedding model is downloaded once at setup or by the user, then
  read from disk. Search never fetches a model at query time.
- **Semantic search is optional.** Without a model installed, search runs on the keyword path
  alone and stays fully local.
- **Dependencies must also be local at runtime.** A library that needs a server, or a Python
  runtime, is excluded even when its data stays on disk, because it breaks the all-Rust,
  self-contained goal.

## What exists today

`crates/store/src/search.rs` answers a query by scanning. It walks every page folder under the
notebooks folder (and the notebooks opened from elsewhere), loads each page, and does a
case-insensitive substring match on the title and text. There is no index. It does not search
OCR text, alt text or attachments. Each query reads every page file, so the cost grows with every
page. Details: `references/current-search-code.md`.

This is the baseline the hybrid design replaces. It stays as the fallback until an index is
built and tested.

## Target design (not built)

Two retrieval paths over the same set of text units, merged by rank:

```
text units (page text, alt text, OCR companion text, later attachments and transcripts)
   |                                   |
   v                                   v
keyword index (BM25)            semantic index (embeddings)
   |                                   |
   +-------------- ranked lists -------+
                     |
          reciprocal rank fusion (RRF)
                     |
             hits: page, snippet, box (for pictures)
```

Fall back to the keyword path alone when no embedding model is installed. Semantic search is an
addition, never a requirement for finding a word.

### What is indexed

- **Page text.** The visible text of each page, from `index.html` or `index.md`, with markup removed.
- **Alt text.** The `alt` of each picture in the page markup, per RFC 0003. Indexed with the page
  as its own text; the page is the hit, and an alt hit carries no boxes.
- **OCR text.** `<picture>.txt` (derived, from `nc-info-ocr`). Indexed as its own unit, so a hit
  can open the picture with its box outlined (criterion A8).
- **Later:** attachment text, handwriting and transcripts (Stages 2 and 3).

Each unit keeps: its page path, its node id, its kind (page, alt, OCR, later others), and for
pictures the picture path and the box. Those are what a hit needs to open the right place.

### Where the index lives

- **Not in the notebook folder.** A notebook is a plain folder that syncs through cloud tools. An
  index file inside it would sync across devices and conflict. This is design reasoning; it has not
  been tested.
- **In the local app data folder**, rebuilt from the notebook when it is missing or stale. The
  notebook files are the source of truth; the index is a cache that can always be rebuilt.
- **Staleness.** Use the same content-hash idea as the OCR companion job: an index entry is
  reused only while the source file's hash is unchanged.

### Keyword path (BM25)

Options, with the constraints that matter here (pure Rust, offline, no new dependency without the
owner's GO):

| Option | Licence (checked) | Language | Dependency | Status |
|---|---|---|---|---|
| Own BM25 over the text units, in memory, cached in the app data folder | Ours | Pure Rust | None | Recommended for the MVP. Speed at a few thousand pages is an estimate, not measured |
| Tantivy | MIT (`quickwit-oss/tantivy`, `license = "MIT"`) | Pure Rust, BM25, Lucene-like design. Includes an Arabic stemmer in its stemmer list | New crate, needs the owner's approval | Upgrade path if notebooks grow |
| redb (embedded key-value store) plus own BM25 | Apache-2.0 (`cberner/redb`) | Pure Rust | New crate | Workable, more code than Tantivy |
| SQLite FTS5 | Public domain (sqlite.org) | C | Needs a C build | Breaks the all-Rust rule |
| ChromaDB | Apache-2.0 | Python; its local mode stores to a folder backed by SQLite | Python runtime | Breaks the all-Rust rule. Built around vector search; keyword BM25 in its local mode is not confirmed |

### Semantic path (embeddings)

**Text.** BGE-M3 (`BAAI/bge-m3`, MIT on its card): more than 100 languages (card), and it provides
dense, multi-vector and sparse retrieval from one model. Its quality on our languages (Arabic,
Hebrew, Persian, Urdu) is not yet tested on our samples. Running it from Rust needs an ONNX export
or a Rust ML runtime. Not verified here.

**Images.** SigLIP (`google/siglip-base-patch16-224`, Apache-2.0 on its card, about 0.2 billion
parameters): a candidate for picture embeddings. Not tested.

**Vector index.** Starts flat and in memory, which is enough for a few thousand units. Candidates:

| Option | Licence (checked) | Notes |
|---|---|---|
| hnsw_rs (approximate nearest neighbour, pure Rust) | MIT OR Apache-2.0 (crate, version 0.3.5) | Candidate once flat search is too slow |
| LanceDB (embedded retrieval library, Rust crate) | Apache-2.0 (crate, version 0.41.0; repository says embedded) | Heavier; check its dependency tree before use |
| fastembed (embedding runtime) | Apache-2.0 (crate, version 7.7.1) | Its ONNX runtime is a C++ library. Owner exception needed. Not verified here |

### Merging: reciprocal rank fusion

Each path returns a ranked list. Each document gets `score = sum over lists of 1 / (k + rank)`,
where `rank` is its position in that list (counted from 1) and `k` is a constant. The default in
Elasticsearch is `k = 60` (its reference docs, checked). The merge needs no score calibration
between BM25 and cosine similarity, which is why it is the default choice here.

- Compute each list to a fixed depth (for example the top 100) before fusing.
- With no semantic list, the result is the BM25 list in its own order.

### Acceptance criteria (from the Stage 1 plan, search part)

- **A6.** Searching for `sourdough` finds the receipt page (integration test on the POC notebook).
- **A7.** Searching for an alt text word finds the page with the picture.
- **A8.** A hit on a picture opens it with the matching box outlined (UI or screenshot check).
- Stage 2: semantic recall tests on the same POC notebook, in the languages in scope, before any
  promise about meaning-based search.

## Rules for changes in this area

- Test first, per `AGENTS.md`. The POC notebook is private and read from an environment variable,
  never committed.
- The index is a derived cache. Deleting it must never lose user data. A test must show that.
- No new dependency without the owner's GO. Record each candidate's licence from its own source.
- Keep the keyword path working on its own. Semantic search is an addition behind a trait, the
  same pattern as the OCR `PictureReader`.
- Do not copy code from other apps. The competitor notes in the research are for the design
  shape only.

## Open decisions (owner)

1. Keyword index: own BM25 (recommended for the MVP) or Tantivy (needs approval)?
2. Semantic model: BGE-M3 for text, SigLIP for pictures? Both need testing first.
3. ONNX Runtime (C++) through an exception to the all-Rust rule, or a pure-Rust runtime?
4. Index location: confirm the local app data folder.

## References

- `references/current-search-code.md`: what the store's search does today, with the code locations.
- `references/verified-sources.md`: every licence and fact used here, with its source and status.
