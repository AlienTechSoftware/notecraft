# NoteCraft v0.0.10 Readiness

## Release identity

| Field | Value |
|---|---|
| Release version | 0.0.10 (planning label; workspace version not verified) |
| Release branch | `rc_0.0.10` |
| Source commit | Not selected |
| Previous release | v0.0.9 |
| Signoff owner | NoteCraft owner |
| Release status | Planning; no platform is signed off |

This file is a readiness record, not release approval. No build, package, upload,
installation test, or deployment evidence is recorded here yet. Update this file
with command output and immutable artifact hashes when a release candidate is selected.

## Platform readiness

| Platform | Build | Tests | Package | Install test | Deployment | Ready |
|---|---|---|---|---|---|---|
| Windows | NOT RUN | NOT RUN | NOT RUN | NOT RUN | N/A | BLOCKED |
| macOS | NOT RUN | NOT RUN | NOT RUN | NOT RUN | N/A | BLOCKED |
| Linux | NOT RUN | NOT RUN | NOT RUN | NOT RUN | N/A | BLOCKED |
| Android | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | BLOCKED |
| Web/WASM | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | BLOCKED |

The platform list is a validation checklist, not a claim that v0.0.10 has approved
support or a committed release scope on every platform. Confirm scope and available
platform branches before signing off.

## Release checks

| Check | Result | Evidence |
|---|---|---|
| Approved release scope and version | NOT VERIFIED | Owner decision required |
| `cargo xtask ci` | NOT RUN | |
| `cargo xtask demo-check` | NOT RUN | |
| `cargo test -p notecraft-formats --test format_parity` | NOT RUN | |
| Platform builds and tests | NOT RUN | Record exact commands per target |
| Package provenance and hashes | NOT RUN | Record source commit, artifact path, and SHA-256 |
| Clean install or launch checks | NOT RUN | Record target OS/device and result |
| Release notes and contributor credits | DRAFT | Update from verified shipped changes |
| Upload or deployment | NOT RUN | Requires explicit owner authorization |

## Known planning inputs

- `README.md` describes Android support as the release theme, but the scope and
  timeline are not approved in this record.
- `issues.md` and `parser-choice-analysis.md` contain Android import analysis and
  open decisions. Treat proposed plans and estimates there as unverified until
  confirmed against current code and owner decisions.
- `text_editing_issues.md` records a reported caret issue and proposed diagnosis;
  validate the behavior and code before treating the diagnosis as confirmed.
- Wishlist items under `wishlist/` are candidates, not committed release scope.

## Signoff rule

Mark a platform ready only when the exact candidate commit has passed the required
quality gates, its artifact has traceable provenance, and a clean-target install or
launch test has passed. Record any unavailable check as blocked with the reason.
Do not infer readiness from a successful compile alone.