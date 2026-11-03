# NoteCraft v0.0.9

**Status:** Planning  
**Base Branch:** `next`  
**Theme:** measurable performance, reliable infinite-canvas interaction, and durable UI quality

## Scope

Version 0.0.9 focuses on three high-impact product and engineering outcomes.

### Committed wishlist

- **Infinite canvas** - drawing must work reliably outside text blocks and across empty page space.
- **Performance markers** - add useful, low-overhead markers and repeatable measurements for startup,
  page loading, editing, saving, search, and rendering.
- **UI test automation** - make important user journeys executable and diagnosable through the
  existing control and screenshot infrastructure.

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

## Links
- **GitHub Issue #20:** https://github.com/AlienTechSoftware/notecraft/issues/20
- **Related:** v0.0.8, v0.0.10
