---
name: nc-design-artcraft-integration
description: Design NoteCraft integrations with ArtCraft suite conventions, shared tools, and platform builders while preserving NoteCraft's product and architecture boundaries.
---

# NoteCraft ArtCraft integration design

Use this skill when a NoteCraft change shares behavior, UI conventions, build tooling, or platform integration with another ArtCraft app. The goal is deliberate reuse at a clear boundary. NoteCraft remains a local-file notebook application with its own product requirements and repository.

## Source and evidence

Use these references as starting points, then inspect their current state before implementation:

- [Remade project directory, iOS query](https://remade.wiki/explore/?q=Ios). The directory is JavaScript-filtered; indexed results describe ArtCraft projects and tools. Treat the listing as discovery context, not certification or proof of a particular NoteCraft capability.
- [Remade ArtCraft suite](https://remade.wiki/suites/artcraft/). Its current listing includes ArtCraft apps and supporting tools, including an Android Builder described as building ARM64 packages from existing source and platform adapters.
- Local ArtCraft pattern notes referenced by `nc-hl-design`, including `C:\dev\p-NoteCraft\ideation\code-style-guide\architecture-artcraft.md`, when available and authorized for read-only inspection.
- The actual sibling repository, shared library, build workflow, or adapter being considered. A directory listing does not replace source inspection.

Record the source URL, the date checked, and which claims were verified in current source. Remade describes maintainer-reported status; do not present directory text as independent validation.

## Design rules

1. Start from NoteCraft requirements in `AGENTS.md`, `nc-prod`, and `nc-arch`. Suite similarity does not authorize a product behavior change.
2. Inspect whether a shared crate, established app pattern, builder, or platform adapter already exists. Prefer reuse when ownership, license, API, and dependency direction allow it.
3. Keep NoteCraft-specific behavior at an explicit adapter or application edge. Do not import another app's data model or business rules into NoteCraft's model or engine.
4. Keep shared contracts platform-neutral. Use platform-native capabilities through isolated implementations, with explicit `cfg` wiring and the repository's platform filename rules.
5. Apply the union-not-intersection rule: a platform can use a specialized implementation that fits it. Do not narrow every platform to the weakest common capability. Show an explicit UI limitation if a platform branch is missing.
6. Treat Android builder listings as a lead to investigate. Verify the builder's current repository, supported app structure, build inputs, signing behavior, and output before relying on it. Do not claim Android support from a listing alone.
7. Preserve NoteCraft's local folder and file ownership model. A shared installer or launcher must not silently move notebook data into a service-owned store.
8. Respect repository boundaries. Coordinate source changes in the repository that owns the code; do not copy code or assets across projects without checking license and ownership.

## Design record

For each proposed integration, state:

- User need and observable acceptance criteria.
- Existing NoteCraft path and candidate ArtCraft pattern or tool.
- Source repository, owner, license, and dependency direction.
- Shared contract and NoteCraft adapter boundary.
- Platform-specific branches and user-visible gaps.
- Failure modes, offline behavior, and local-data effects.
- Tests using real code and test data under the no-mocks rule.
- Exact source links and what was verified versus assumed.

For substantial work, route through `nc-arch` and `nc-hl-design`, then the relevant UI or MVC specialist and `nc-code-rust`. For egui input handling, also use `nc-design-egui-integration`. For dialogs, layering, or drawing, add the corresponding dialog, z-order, or drawing skill.

## Review checklist

- Does the proposed reuse exist in current source, and is its ownership clear?
- Is the dependency direction allowed by NoteCraft's crate layers?
- Are product-specific storage and command rules still owned by NoteCraft?
- Does each platform use its useful native path where supported?
- Are missing capabilities stated in the UI and in `docs/gaps.md`?
- Are license, attribution, build, and test implications recorded?
- Are directory listings kept distinct from implementation evidence?
