# `onenote.rs` test fixtures

This folder recognizes the work of the [`onenote.rs` Rust OneNote parser](https://github.com/msiemens/onenote.rs) and the upstream fixture contributors credited here. Only test data is copied; no parser source code is included.

The samples under `samples/` are copied unchanged from `onenote.rs` commit `de3a64e8159c684b137ded89c3fef3c17272ec1d`, preserving their upstream paths. The repository's MPL-2.0 license applies unless a fixture has a more specific notice. A copy is included as `LICENSE-MPL-2.0.txt`.

`handwriting_recognition.one` has separate provenance recorded by OfficeIMO: it is from `onenote.rs` commit `5138a39a3f4e72b840932f9872fecde52fa9da60`, original path `crates/parser/tests/samples/handwriting_recognition.one`, under MPL-2.0.

The upstream `onenote-2016/OneWithFileData.one` sample is recorded as vendored from OfficeDev/Interop-TestSuites under MIT. It is excluded because binary text inspection found an email-like value. The source file is not copied.

The selected files include section and `.onetoc2` samples covering page hierarchy, deleted-page/recycle-bin structure, non-legacy sections, object revisions, and handwriting. NoteCraft imports the handwriting section and currently skips the other eight `.one` files as unrecognized section files. `.onetoc2` files are reported as noticed but not imported. Tests record these limits in both output formats.

The `joplin/` fixtures are excluded because they carry separate AGPL-3.0-or-later terms and need a separate license compatibility review. `Large Desktop.one` and `Large OneDrive.one` are excluded from normal CI because together they exceed 260 MB. No included source bytes were modified.

## Selected sample paths

```text
handwriting_recognition.one
New Section 1.one
New Section Group/New Section 1.one
New Section Group/New Section 2.one
New Section Group/Open Notebook.onetoc2
OneNote_RecycleBin/OneNote_DeletedPages.one
OneNote_RecycleBin/Open Notebook.onetoc2
Open Notebook.onetoc2
Schnelle Notizen.one
non-legacy/New Section 1 2.one
non-legacy/New Section 2.one
non-legacy/New Section 3.one
non-legacy/Open Notebook.onetoc2
object-revision/Open Notebook.onetoc2
```

Source revision: https://github.com/msiemens/onenote.rs/tree/de3a64e8159c684b137ded89c3fef3c17272ec1
OfficeDev/Interop-TestSuites source: https://github.com/OfficeDev/Interop-TestSuites/
