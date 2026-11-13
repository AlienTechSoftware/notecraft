# onenote.rs OneNote fixture corpus

This folder recognizes the work of the [`onenote.rs` Rust OneNote parser](https://github.com/msiemens/onenote.rs) and the fixture contributors credited by that project. Only test data is copied; no parser source code is included.

The full upstream `crates/parser/tests/samples/` fixture set at commit `de3a64e8159c684b137ded89c3fef3c17272ec1d` is represented under `samples/`, preserving relative paths and source bytes. It contains 34 `.one` section files and 20 `.onetoc2` files. `samples/SHA256SUMS.txt` records the SHA-256 of every sample. Paths with non-ASCII characters are percent-encoded as UTF-8 in the manifest.

The onenote.rs repository is MPL-2.0. `LICENSE-MPL-2.0.txt` is included for samples without a more specific notice. The `joplin/` subtree has its own AGPL-3.0-or-later notice and full license. The `onenote-2016/` subtree has an MIT notice from OfficeDev/Interop-TestSuites. Each subfolder README describes its source and terms.

The two large upstream samples, `Large Desktop.one` and `Large OneDrive.one`, are included unchanged through Git LFS. Their sizes and hashes are in `SHA256SUMS.txt`. They are deliberately excluded from routine CI import tests, and the repository's `.lfsconfig` keeps a clone from downloading them, so they stay as small pointer files until asked for. To hydrate them for manual tests, run:

```text
git lfs pull --include="crates/import/tests/fixtures/3p/onenote.rs/samples/Large*.one" --exclude=""
```

The empty `--exclude` is needed: without it the exclusion in `.lfsconfig` still applies and nothing is downloaded.

The regular corpus tests exercise all smaller `.one` files and report ignored `.onetoc2` files in both HTML and CMGFM modes. The large-file checks are opt-in because each file is over 100 MB and the current importer reads a complete section before parsing. Unsupported or skipped samples are recorded as such; their presence does not claim compatibility.

Source revision: https://github.com/msiemens/onenote.rs/tree/de3a64e8159c684b137ded89c3fef3c17272ec1d
