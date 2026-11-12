---
name: nc-design-artcraft-integration
description: Design NoteCraft integrations with ArtCraft suite conventions, shared tools, and platform builders while preserving NoteCraft's product and architecture boundaries.
---

# NoteCraft ArtCraft integration design

Use this skill when a NoteCraft change shares behavior, UI conventions, build tooling, or platform integration with another ArtCraft app. The goal is deliberate reuse at a clear boundary. NoteCraft remains a local-file notebook application with its own product requirements and repository.

## Source and evidence

Local source comes first. Inspect the current state of the repository you cite before implementation.
Record the path, the HEAD commit and its date, and whether each claim was verified in source or assumed.

- Local ArtCraft pattern notes: `C:\dev\p-NoteCraft\ideation\code-style-guide\architecture-artcraft.md`, when available and authorized for read-only inspection.
- The actual sibling repository, shared library, build workflow, or adapter being considered. A directory listing does not replace source inspection.

Secondary, maintainer-reported only: the Remade pages below. Local source wins where they disagree.

- [Remade project directory, iOS query](https://remade.wiki/explore/?q=Ios). The directory is JavaScript-filtered; indexed results describe ArtCraft projects and tools. Treat the listing as discovery context, not certification or proof of a particular NoteCraft capability.
- [Remade ArtCraft suite](https://remade.wiki/suites/artcraft/). Its current listing includes ArtCraft apps and supporting tools, including an Android Builder described as building ARM64 packages from existing source and platform adapters.

Do not present directory text as independent validation.

## Local sources

Paths are under `C:\dev\p-NoteCraft\.repos\`. `storytold\` is a link to `C:\src\storytold`. HEAD dates are as read on 2026-11-10.

- **`storytold\` (read-only mirror).** Do not edit it. Refresh only with `storytold_sync.py`, when the owner asks.
- **`storytold\craft-libs\`** (HEAD `1ee01a4`, 2026-11-10). Shared crates that matter here: `ai-core`, `web-api`, `window`, `ui`, `i18n`, `settings`, `update`, `quit`, `menu`, `keymap`, `platform`. All exist.
  - `craft-libs\docs\ai-shared-layer.md` (proposed, 13/10/2026): the suite-wide AI contract. A registered tool is a declaration, not installed code. Apps must fail explicitly when the selected adapter is absent.
  - `craft-libs\docs\ai-declarative-adapter.md` (built 13/10/2026): one generic adapter for providers described by a data file. The spec is data only; it runs no code.
- **`storytold\craft-launcher\apps\artcraft-launcher\`** (HEAD `753a710`, 2026-11-09): tray precedent. `Cargo.toml` pins `tray-icon = "0.26"`, with the `ksni` feature on Linux and the BSDs.
- **`storytold\cadcraft\docs\control-protocol.md`** (HEAD `6f0f1fef`, 2026-11-10): the JSON control channel. Each request carries a per-session `token`, which the app writes to an owner-only file.
- **`storytold\craft-libs\crates\web-api\README.md`**: `craft-web-api`, the page command API for every Craft web build. It gives one `window.<app>` shape over the app's JSON control channel.
- **Craft app repositories** (HEAD and top-level layout, read 2026-11-10; no deeper layout checked):
  - `storytold\artcraft\` (`cfd30da`, 2026-10-20): `artcraft-native/`, `crates/`, `frontend/`, `docs/`.
  - `storytold\artcraft-services\` (`0a2dc72`, 2026-11-09): `crates/`, `frontend/`, `build/`, `docs/`.
  - `storytold\wordcraft\` (`8548b33`, 2026-11-10): `apps/`, `crates/`, `docs/`, `xtask/`.
  - `storytold\deckcraft\` (`0623b43`, 2026-11-08): `apps/`, `crates/`, `docs/`, `xtask/`.
  - `storytold\gridcraft\` (`a36e498`, 2026-11-10): `apps/`, `crates/`, `docs/`, `xtask/`.
  - `storytold\designcraft\` (`95abb2b`, 2026-11-10): `apps/`, `crates/`, `docs/`, `xtask/`.
- **Android builder checkouts** (not verified to build; read here only):
  - `.repos\artcraft-android\` (HEAD `7dba2a2`, 2026-11-06, "working on 0.6.0").
  - `.repos\artcraft-android-e3ece4b\` (HEAD `d00ce57`, 2026-11-07). It has an `adapters\notecraft\` directory, so it carries NoteCraft-specific adapter code.
  - Both READMEs make product claims (Storage Access Framework, IME, touch gestures). Those are the maintainer's statements, not verified here.
  - Build inputs, from `lib\build-common.sh`: `cargo-ndk`, Gradle, an Android SDK (`android-sdk\`), and a persistent local signing key in `keys\`. Do not read, copy or cite the key file.

Not checked: whether either Android checkout builds, or whether the e3ece4b adapter runs.

## App-to-app communication: what exists

No suite-wide app-to-app protocol is written down. Nothing in `craft-libs` docs sets channels, request
authorisation, task or result ownership, or failure propagation between apps. The `theme-infrastructure.md`
doc links a cross-app architecture discussion on GitHub; that is a discussion, not a written rule. Do not
treat any of the mechanisms below as a suite standard.

What exists, described from source. These are descriptions, not rules.

1. **Request and reply shape.** `storytold\craft-libs\crates\web-api\src\lib.rs`, lines 5-9: Craft web apps
   accept JSON control requests `{method, params}` and answer `{ok, result}` or `{ok: false, error}`. The crate
   says this is "the same protocol as the desktop control server".
2. **Method discovery.** `storytold\cadcraft\docs\control-protocol.md`, line 66: `control.methods` returns every
   method the server answers, as `{method, params, description}`.
3. **Caller tiers.** `storytold\craft-libs\crates\font-store\src\store.rs`, lines 234 and 239: `FontStore::for_user`
   is for the app's own interface, and `FontStore::for_agent` is for MCP, CLI and scripting. The crate's
   `README.md`, line 29, says `for_agent` gets `Error::NotAllowed` and that an agent can install only while the
   user has the network on.
4. **MCP over the same protocol.** `storytold\cadcraft\docs\control-protocol.md`, line 110: the MCP server
   (`cadcraft-cli mcp --connect 127.0.0.1:PORT`) wraps this protocol.
5. **Opt-in and loopback only.** `storytold\cadcraft\docs\control-protocol.md`, lines 3-4: the desktop app starts
   the channel with `--control PORT` and listens on `127.0.0.1:PORT` for JSON lines.
6. **Session token.** `storytold\cadcraft\docs\control-protocol.md`, lines 13-15: each session writes an
   unguessable token to an owner-only file, `cadcraft-control-<port>.token`, in the per-user runtime directory.
7. **Health check.** `storytold\cadcraft\docs\control-protocol.md`, line 67: `ping` replies `"pong"`.

Launcher install detection. This is the ArtCraft Launcher's mechanism, not a suite standard.

- Windows: the launcher reads Windows Installer uninstall entries under `SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall`
  in the 64-bit machine view, the 32-bit machine view and the user hive (`storytold\craft-launcher\crates\native\src\detect.rs`,
  lines 240 and 272).
- The launcher's own install record is `installed.json` in its data folder (`storytold\craft-launcher\crates\native\src\paths.rs`,
  lines 15, 22 and 57). It is keyed by app ID; the write is at `backend.rs` line 864. On Windows the folder is named
  `ArtCraft Launcher` (line 15). The `%LOCALAPPDATA%` location is Assumed, not checked.
- macOS: `Info.plist` names the bundle. GridCraft's `CFBundleIdentifier` is `ai.storyteller.gridcraft`
  (`storytold\gridcraft\packaging\macos\Info.plist.in`, line 40).

Not found in the sources read (2026-11-10 searches): discovery of running apps across the suite, a task model,
results or artifact exchange between apps, failure handling between apps, identity between apps, and remote access.
`control.methods` appears only in CADCraft sources. The `registry` and `discover` hits in `craft-libs`, `craft-launcher`,
`cadcraft` and `gridcraft` are unrelated (fonts, keymaps, printing, PDF). The AI shared layer states that the current
surface "does **not** ship a dynamic adapter registry" (`craft-libs\docs\ai-shared-layer.md`, line 25).

Any cross-app protocol design needs a separate, owner-approved proposal before it becomes guidance here.

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

## Reference: WordCraft and NoteCraft document models

See `references/wordcraft_x_notecraft_dom.md` for the comparison of the two document models and what it means for caret, runs and inline objects.
