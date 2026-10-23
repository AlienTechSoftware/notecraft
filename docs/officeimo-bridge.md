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

## Installation

For development, set `NOTECRAFT_OFFICEIMO_BRIDGE` to an explicitly built executable. Installed
artifacts use this layout under the NoteCraft data directory:

```text
importers/officeimo/<target>/officeimo-notecraft[.exe]
```

The initial target names are `win-x64`, `linux-x64`, `osx-x64` and `osx-arm64`. Release tooling
must pin an artifact URL, SHA-256, bridge version and license manifest. Cargo builds must not
download an unpinned binary implicitly.

The bridge is not yet bundled. Its OfficeIMO dependency and all transitive licenses must be
reviewed before redistribution.
