# Search: verified sources

Checked 2026-11-22 from the sources named. "Verified" means the value was read from that source in
this check. "Not verified" means it comes from memory or a secondary source, or the check failed.

| Claim | Value | Source | Status |
|---|---|---|---|
| Tantivy licence and language | MIT; pure Rust; BM25 | GitHub API `quickwit-oss/tantivy` (`spdx_id` MIT, description); its `Cargo.toml` `license = "MIT"` | Verified |
| Tantivy Arabic stemmer | An `Arabic` stemmer algorithm exists | `src/tokenizer/stemmer.rs` in the Tantivy repository | Verified (stemming only; Arabic tokenisation and normalisation are not checked) |
| Tantivy minimum Rust version | 1.91 | Its `Cargo.toml` | Verified (our toolchain is 1.95) |
| redb licence | Apache-2.0; embedded key-value database in pure Rust | GitHub API `cberner/redb` | Verified |
| SQLite licence | Public domain | sqlite.org copyright page | Verified |
| SQLite language | C | Common knowledge of the project; not re-checked here | Not verified in this check |
| ChromaDB licence | Apache-2.0 | Its README badge and link to LICENSE | Verified |
| ChromaDB local persistence | `PersistentClient` in the Python package (`chromadb/__init__.py`) | Its source | Verified |
| ChromaDB local storage backend | SQLite (the `chroma-sqlite` Rust crate uses sqlx-sqlite) | `rust/sqlite/Cargo.toml` in its repository | Verified |
| ChromaDB keyword BM25 in local mode | Not confirmed. Python exposes `FtsIndexConfig`; the README points full-text search at the hosted service | README and Python source | Not verified |
| BGE-M3 licence | MIT | Hugging Face `BAAI/bge-m3` card (`license: mit`) | Verified |
| BGE-M3 languages | More than 100 | Its card, multi-linguality section | Verified (the claim); quality for Arabic, Hebrew, Persian, Urdu not tested |
| BGE-M3 modes | Dense, multi-vector and sparse retrieval from one model | Its card | Verified |
| SigLIP base licence and size | Apache-2.0; about 0.20 B parameters | Hugging Face `google/siglip-base-patch16-224` (`params` 203,155,970) | Verified |
| hnsw_rs licence | MIT OR Apache-2.0; version 0.3.5 | `cargo info hnsw_rs` | Verified |
| LanceDB licence | Apache-2.0; described as an embedded retrieval library | GitHub API `lancedb/lancedb`; `cargo info lancedb` (version 0.41.0) | Verified |
| fastembed licence | Apache-2.0; version 7.7.1 | `cargo info fastembed` | Verified. Its ONNX dependency and its C++ runtime are not checked here |
| Reciprocal rank fusion formula | `score += 1 / (k + rank)` for each list containing the document | Elasticsearch reference, RRF retriever section | Verified |
| RRF default constant | `k = 60` | Elasticsearch reference (`rank_constant` defaults to 60) | Verified |
| RRF original paper | Cormack, Clarke and Buttcher, SIGIR 2009 | Link on the Elastic page (`cormacksigir09-rrf`) | Partly verified: the link names Cormack; the full author list is from memory |
| ONNX Runtime licence | MIT (commonly stated) | Not verified: the LICENSE file was not reachable in this check | Not verified |
| `ort` crate licence | MIT OR Apache-2.0 (wrapper) | `cargo info ort` | Verified (the library it wraps is C++) |

## Sources used

- crates.io index (`cargo info`): hnsw_rs, lancedb, fastembed, ort.
- GitHub API and repositories: `quickwit-oss/tantivy`, `cberner/redb`, `lancedb/lancedb`,
  `chroma-core/chroma` (README, `rust/sqlite/Cargo.toml`, `chromadb/__init__.py`).
- Hugging Face cards: `BAAI/bge-m3`, `google/siglip-base-patch16-224`.
- sqlite.org copyright page.
- Elasticsearch reference: RRF retriever.
