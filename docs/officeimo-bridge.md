# OfficeIMO bridge

The OfficeIMO importer is optional. NoteCraft remains a Rust application; the bridge is a
separate process that converts a OneNote input into a NoteCraft folder.

## Contract

The bridge must accept:

```text
--source PATH --out PATH --format html|cmgfm
```

It must return exit code zero only after the output folder is complete. Its standard output is
diagnostic JSON or text for the caller. A failure must use a non-zero exit code and write a useful
diagnostic to standard error.

The NoteCraft caller accepts either one `.one` section file or a folder tree. For a folder tree,
NoteCraft uses the selected folder name as the notebook, nested folders as section groups, and
each `.one` file as a section. The bridge itself remains a single-section converter.

## Installation

For development, set `NOTECRAFT_OFFICEIMO_BRIDGE` to an explicitly built executable. Installed
artifacts use this layout under the NoteCraft data directory:

```text
importers/officeimo/<target>/officeimo-notecraft[.exe]
```

The initial target names are `win-x64`, `linux-x64`, `osx-x64` and `osx-arm64`. Release tooling
must pin an artifact URL, SHA-256, bridge version and license manifest. Cargo builds must not
download an unpinned binary implicitly.

Release bundles are built with [`tools/officeimo/package.ps1`](../tools/officeimo/package.ps1).
The script publishes a self-contained bridge for one runtime identifier and records its SHA-256
in a manifest. Release packaging must run it for each target runtime and include the OfficeIMO
and transitive dependency license files beside the manifest. Generated binaries stay out of the
source repository.
