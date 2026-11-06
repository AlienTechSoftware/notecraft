# NoteCraft v0.0.9

**Status:** Planning  
**Base Branch:** `rc_0.0.9`
**Theme:** measurable performance, reliable infinite-canvas interaction, and durable UI quality

## Scope

Version 0.0.9 focuses on four high-impact product and engineering outcomes.

### Committed wishlist

- **Infinite canvas** - drawing must work reliably outside text blocks and across empty page space.
- **Performance markers** - add useful, low-overhead markers and repeatable measurements for startup,
  page loading, editing, saving, search, and rendering.
- **UI test automation** - make important user journeys executable and diagnosable through the
  existing control and screenshot infrastructure.
- **Internationalization** - prepare the app interface for translated catalogs, with persistent
  language selection, English fallback, and an initial language proof. Add French and Spanish
  versions of the standalone User Assistance guides as well. Follow the ArtCraft shared catalog
  and runtime pattern; scope, estimate, and the compiler-version decision are recorded in
  [`workspace_1.md`](workspace_1.md).

### Issue candidates

`_3` will review current GitHub votes and propose a limited set of issues. Add the selected issue
numbers and acceptance criteria here after verifying the current list.

### Cross-platform quality gate

Every pull request should build and test NoteCraft on Linux, Windows, and macOS. Packaging and
deployment remain separate workflows: ordinary PRs do not create installers or publish artifacts.
Packaging runs manually, for release branches/tags, or when packaging inputs change; installation
validation is release-only or explicitly requested.

See the private host proposal:
`work/release-0.0.9-cross-platform-ci-packaging-proposal.md`.

## Timeline
- **Planning:** TBD
- **Development:** TBD
- **Release:** TBD

## Related Documents
- `enhancement_bidi_uiux_alignment.md` - Bidi UI alignment candidate
- `rc_perf.md` - Performance instrumentation and profiling plan
- `feature_plan.md` - Implementation plan

## Parallel Workspaces

- [`workspace_1.md`](workspace_1.md) - Internationalization
- [`workspace_2.md`](workspace_2.md) - Infinite canvas
- [`workspace_3.md`](workspace_3.md) - Performance markers and voted GitHub issues

## Links
- **GitHub Issue #20:** https://github.com/AlienTechSoftware/notecraft/issues/20
- **GitHub Issue #31:** https://github.com/AlienTechSoftware/notecraft/issues/31
- **Related:** v0.0.8, v0.0.10
