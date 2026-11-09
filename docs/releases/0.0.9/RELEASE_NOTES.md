# NoteCraft v0.0.9 Release Notes

## Highlights

- **Welcome wizard:** First-run setup flow on Windows guides new users through initial configuration.
- **13-locale i18n:** Complete UI localization with 13 language catalogs and script markers for Arabic/Hebrew.
- **Expanded page context menu:** More actions available in the page right-click menu.
- **Import improvements:** Preserved OneNote container colors during import.
- **Demo notebook separation:** Cleaner packaging with separate demo notebook setup.

## Platform status

| Platform | Status | Notes |
|---|---|---|
| web-WASM | Built locally | Deployment blocked by missing welcome-notebook.zip on hosted site |
| Windows x64 MSI | Built locally | Unsigned; install test pending |
| Windows x64 portable | Built locally | Launch test pending |
| Linux x64 tar.gz | Built locally | Smoke test passed; upload pending |
| macOS | Build blocked | Requires macOS runner |
| Android APK | Not rebuilt | External builder needed |

## Known gaps

- macOS build requires a macOS CI runner not yet configured.
- Android APK must be built through the external Android builder.
- Packages are unsigned in this candidate.
- Clean-install tests and artifact uploads are pending for all platforms.

## Contributors

Thank you to everyone who contributed code, reviews, testing, design, documentation, and ideas to NoteCraft v0.0.9. See [CONTRIBUTORS.md](CONTRIBUTORS.md) for the full list.