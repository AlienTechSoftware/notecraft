# UI and UX user simulation research

Research date: 2026-10-31

## Executive summary

The ArtCraft applications do not appear to use one external, cross-platform
desktop automation product such as Appium, pywinauto, dogtail, SikuliX, or
Robot Framework for their ordinary UI tests.

Their established approach is layered and is especially appropriate for
Rust/egui applications:

1. Use `egui_kittest` to drive the real egui UI in-process and headlessly.
2. Use AccessKit to expose the same UI through a semantic accessibility tree.
3. Use the platform accessibility adapters supplied by eframe/winit for
   native Windows and Linux accessibility integration.
4. Use offscreen rendering and visual comparisons for layout, theme, and
   painting regressions.
5. Use application-level command/control tests for deterministic behavior that
   should not depend on window focus, timing, or a particular desktop session.
6. Add a smaller external smoke-test layer later, if needed, using Windows UI
   Automation and Linux AT-SPI against the compiled application.

For NoteCraft, the first implementation should therefore follow the existing
ArtCraft pattern rather than start with pywinauto or dogtail. External
platform drivers remain useful for release smoke tests and for proving that
the packaged binary is accessible to real desktop tools, but they should not
replace the fast, deterministic in-process suite.

## Question being answered

The original question was whether there is a well-maintained open-source
cross-platform tool for simulating users and manipulating desktop UI, with
Windows and Linux as the initial targets. The follow-up question was whether
the other ArtCraft applications already use one of the proposed tools.

The answer has two parts:

- For arbitrary applications from outside the process, Windows and Linux
  have different native accessibility systems. A universal tool is less
  mature than the web ecosystem.
- For the ArtCraft Rust/egui applications, the existing codebase already has
  a better application-specific foundation: `egui_kittest` plus AccessKit.

## Evidence inspected

### NoteCraft checkout

The NoteCraft checkout already contains the same core dependencies and test
patterns:

- The workspace manifest lists `egui_kittest = "0.36"` with `wgpu` and
  `snapshot` features.
- `crates/ui-egui/Cargo.toml` uses `egui_kittest` as a development
  dependency.
- `crates/ui-egui/tests/ui.rs` imports `Harness` and `Queryable`, drives the
  real UI headlessly, queries AccessKit roles, and verifies behavior.
- `crates/ui-egui/tests/rtl_quality.rs` uses the same harness for
  interaction and layout quality.
- `crates/ui-egui/tests/icons_visual.rs` uses the wgpu test renderer for
  visual verification.
- `crates/ui-egui/tests/arabic_layout.rs` exercises localized layout.
- `crates/ui-egui/examples/ui_shot.rs` uses the harness for UI capture.
- The lockfile contains AccessKit crates, including the Windows and Unix
  adapters, and `egui_kittest`.

The current UI test file is explicit about the intended boundary: it drives
the real window headlessly, asserts on behavior and pixels, and does not
commit golden screenshots. This is the closest local precedent for a
NoteCraft UI testing strategy.

### Craft Launcher checkout

The ArtCraft Launcher has a concrete implementation in:

- [`crates/ui-egui/tests/ui.rs`](../../../../craft-launcher/crates/ui-egui/tests/ui.rs)

The test module imports:

```rust
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
```

It uses semantic queries such as:

```rust
h.get_by_role_and_label(
    egui::accesskit::Role::TextInput,
    "Install folder",
)
```

and:

```rust
h.get_all_by_role_and_label(
    egui::accesskit::Role::Button,
    "Install",
)
```

The tests then click controls, type into text inputs, inspect values, and
assert the commands sent by the UI. The test suite also calls
`h.check_accessibility()` in multiple scenarios.

The Launcher dependency manifest includes:

```toml
egui_kittest = { workspace = true, features = ["wgpu", "snapshot"] }
```

The launcher is therefore using semantic UI interaction and accessibility
checks in-process. It is not using pywinauto, dogtail, Appium, SikuliX, or
Robot Framework as its main desktop test mechanism.

### Storytold sibling applications

The Storytold mirror contains the same pattern across many applications.
The following manifests were found to use `egui_kittest`, commonly with
`wgpu` and `snapshot` features:

- `cadcraft/crates/ui-egui/Cargo.toml`
- `deckcraft/crates/ui-egui/Cargo.toml`
- `designcraft/crates/ui-egui/Cargo.toml`
- `effectcraft/crates/ui-egui/Cargo.toml`
- `filmcraft/crates/ui-egui/Cargo.toml`
- `gridcraft/crates/ui-egui/Cargo.toml`
- `lightcraft/crates/ui-egui/Cargo.toml`
- `pdfcraft/crates/ui-egui/Cargo.toml`
- `photocraft/crates/ui-egui/Cargo.toml`
- `soundcraft/crates/ui-egui/Cargo.toml`
- `vectorcraft/crates/ui-egui/Cargo.toml`
- `wordcraft/crates/ui-egui/Cargo.toml`
- `3p/mailcraft/crates/ui-egui/Cargo.toml`
- `craft-ai-integrations/crates/ai-egui/Cargo.toml`
- `craft-libs/crates/ui/Cargo.toml`

The desktop application manifests also commonly enable AccessKit through
eframe. Examples include:

- `cadcraft/apps/cadcraft/Cargo.toml`
- `effectcraft/apps/effectcraft/Cargo.toml`
- `filmcraft/apps/filmcraft/Cargo.toml`
- `gridcraft/apps/gridcraft/Cargo.toml`
- `pdfcraft/apps/pdfcraft/Cargo.toml`
- `photocraft/apps/photocraft/Cargo.toml`
- `soundcraft/apps/soundcraft/Cargo.toml`
- `vectorcraft/apps/vectorcraft/Cargo.toml`
- `wordcraft/apps/wordcraft/Cargo.toml`

A typical desktop feature set is:

```toml
eframe = {
    workspace = true,
    features = [
        "default_fonts",
        "wgpu",
        "accesskit",
        "wayland",
        "x11",
    ],
}
```

The exact feature list differs by application, but the important facts are
that AccessKit is enabled and Linux X11/Wayland support is considered at the
application boundary.

## What the shared Craft UI library contributes

The tagged reference,
[`craft-libs/crates/ui/README.md`](../../../../storytold/craft-libs/crates/ui/README.md),
describes reusable egui components and their accessibility contracts.

### Accessible controls

The shared `IconButton` component:

- accepts an accessible label;
- reports the label and selection state to AccessKit;
- respects the disabled state in painting, interaction, and accessibility;
- supports keyboard activation through egui;
- returns an egui response so the application owns command dispatch;
- paints a focus outline supplied by the application's theme.

The shared `Tab` component:

- reports the Tab role, label, and selection;
- supports keyboard activation;
- paints a focus outline;
- uses a stable ID independent of translated text.

The shared resize handle:

- supports pointer dragging;
- supports focused axis-arrow keys;
- supports AccessKit increment/decrement actions;
- can expose numeric bounds and SetValue semantics.

These are not merely visual widgets. They establish the semantic contract
that makes both a screen reader and a semantic test harness able to find and
operate controls.

### Stable identifiers

The shared guidance says that IDs should come from the surrounding UI scope
and position rather than translated labels. Reordering controls should use a
stable `push_id` value. This matters for both accessibility and testing:

- localized labels can change without invalidating identity;
- a test can query a role and label while the widget identity remains stable;
- controls do not accidentally inherit a different identity when a list
  changes order.

### Visual testing

The shared library has visual regression tests in:

[`craft-libs/crates/ui/tests/visual_regression.rs`](../../../../storytold/craft-libs/crates/ui/tests/visual_regression.rs)

Those tests use `egui_kittest` and a wgpu test renderer. They compare
independently painted references and rendered output, including geometry and
pixels. They deliberately do not depend on committed golden screenshots.
The test comments describe the suite as paired same-renderer comparisons,
rather than portable GPU golden files.

This is complementary to semantic interaction tests:

- semantic tests answer whether a user can find and activate a control;
- behavior tests answer whether the expected command and state transition
  occur;
- visual tests answer whether the control is laid out and painted correctly.

## Tool comparison for Windows and Linux desktop UI

### `egui_kittest`

Best first choice for NoteCraft's own egui UI.

Strengths:

- in-process and fast;
- deterministic and headless;
- queries roles, labels, and the AccessKit tree;
- can click, type, focus, and inspect controls;
- can simulate pointer movement for hover states;
- can simulate pointer press, move, and release sequences for drag behavior;
- supports wgpu rendering and snapshots;
- works naturally with Rust tests and existing application state;
- does not require a running Windows or Linux desktop session;
- catches missing accessible names early.

Limitations:

- it tests the egui UI at the application/test-harness boundary, not the
  final packaged executable from another process;
- it cannot prove that a particular installed screen reader or desktop
  accessibility bridge behaves correctly;
- custom-painted regions need explicit semantic exposure and may need
  specialized tests;
- it is specific to egui rather than a universal desktop automation API.

#### Mouse, hover, and pointer simulation

`egui_kittest` simulates input inside the egui harness. It can exercise the
same interaction path used by egui widgets without moving the physical
Windows or Linux cursor.

The relevant interaction categories are:

| Interaction | In-process `egui_kittest` support |
|---|---|
| Click a semantic control | Yes |
| Type into a text input | Yes |
| Move the pointer over a control | Yes, through pointer movement/events |
| Test hover-dependent rendering or state | Yes, after rendering the resulting frame |
| Drag, resize, or select with the pointer | Yes, through press/move/release events |
| Keyboard shortcuts and focus | Yes |
| Move the real OS cursor | No |
| Manipulate an external packaged application | No |

For a hover test, the intended pattern is:

1. Render the control in its normal state.
2. Send pointer movement to the control's bounds.
3. Render the next frame.
4. Assert the hover-dependent behavior or compare the rendered output.
5. Move the pointer away and verify that the state clears.

Prefer semantic queries for finding the target. Use pointer coordinates only
for the input event itself when the interaction specifically tests geometry,
hover, dragging, or another pointer-dependent behavior.

This distinction is important. An in-process pointer event proves that the
egui control responds correctly. It does not prove that the packaged
executable receives native mouse events through Windows or Linux desktop
integration.

### AccessKit

Best accessibility foundation for the native application.

AccessKit is not a scenario runner by itself. It is the semantic bridge
between application widgets and native accessibility technologies. With
eframe/winit integration it can expose the application to platform
consumers, including Windows UI Automation and Linux AT-SPI-related paths.

Strengths:

- one application-side semantic model for multiple platforms;
- useful to real users, screen readers, and automation tools;
- integrates with the existing egui/eframe stack;
- encourages stable roles, names, values, focus, and actions.

Limitations:

- does not launch applications or author test scenarios by itself;
- a good AccessKit tree still requires platform-level smoke testing;
- custom canvases and nonstandard interactions need deliberate modeling.

### Windows UI Automation with pywinauto

Good external Windows smoke-test option.

`pywinauto` can drive native Windows controls through the Win32 and UI
Automation backends. It is appropriate when the requirement is:

```text
launch the installed .exe
find a control through Windows accessibility metadata
click/type/assert from outside the process
```

The UIA backend is generally the relevant starting point for modern
applications. The Win32 backend is useful for older traditional controls.

Strengths:

- external black-box testing of the packaged executable;
- semantic control lookup instead of coordinates when UIA data is present;
- useful for Windows installer and release smoke tests;
- Python-based and easy to run separately from the Rust build.

Limitations:

- Windows-only;
- requires a desktop session and careful process/window cleanup;
- UI Automation semantic actions are not the same as moving the physical
  mouse. Use native input facilities only when actual cursor movement,
  pointer capture, or hit-testing behavior must be verified;
- more timing and focus sensitive than in-process tests;
- failures can depend on display scale, window state, permissions, and
  desktop configuration;
- does not provide Linux coverage.

### Linux AT-SPI with dogtail or a direct AT-SPI client

Good external Linux smoke-test option when the target exposes a useful
accessibility tree.

AT-SPI is the Linux accessibility protocol. `dogtail` is a Python
automation library built around it. A direct AT-SPI client may be preferable
when the test suite needs tighter control or a smaller dependency surface.

Strengths:

- semantic automation of the actual Linux application;
- uses the same general accessibility concepts as the application-side
  AccessKit model;
- can validate behavior through a real desktop accessibility bus.

Limitations:

- Linux desktop and toolkit configuration matters;
- X11 and Wayland environments can differ;
- the accessibility bus must be available and correctly configured;
- AT-SPI semantic actions are not the same as moving the physical mouse.
  X11 and Wayland input facilities differ when real cursor movement or
  pointer injection must be verified;
- application toolkit support varies;
- external tests are slower and less deterministic than
  `egui_kittest`;
- dogtail and similar tools do not create a universal Windows/Linux API.

For a CI policy, pin the Linux environment rather than claiming generic
Linux support. Record the distribution, desktop environment, display server,
and toolkit conditions used by the test.

### Appium

Appium is valuable for mobile, browser, and broad WebDriver-style automation.
It can also be extended to desktop platforms, but it is not the natural first
choice for this Rust/egui Windows/Linux application.

Use Appium if a project has a deliberate requirement for a shared WebDriver
protocol or if the same test infrastructure already covers mobile devices.
Otherwise, pywinauto/UIA and AT-SPI are more direct for native desktop
accessibility smoke tests.

### SikuliX

SikuliX is a visual, image-based fallback. It can click or locate a control
when semantic accessibility metadata is unavailable.

It is useful for:

- custom-rendered controls with no accessibility node;
- third-party dialogs that cannot be queried semantically;
- a last-resort packaging smoke test.

It is a poor primary strategy because it is sensitive to DPI, theme, font
rendering, resolution, window position, animation, and timing. It should not
be used when a stable role, label, value, or action can be exposed instead.

### Robot Framework

Robot Framework is an orchestration and scenario language, not the native
Windows/Linux driver itself. It can organize readable scenarios and call
platform-specific libraries, but it adds another layer.

It is reasonable when:

- non-Rust QA contributors need keyword-oriented scenarios;
- one reporting system must coordinate web, mobile, and desktop tests;
- the organization already standardizes on Robot Framework.

It is not needed for the initial NoteCraft UI suite. Rust tests with
`egui_kittest` are closer to the code and provide stronger type and state
integration.

## Recommended NoteCraft test pyramid

### Level 1: pure command and engine tests

Test command semantics, storage, and state transitions without a GUI. These
tests are the most deterministic and should cover behavior that is not
specific to layout or input method.

Examples:

- creating and opening a notebook;
- adding a section or page;
- saving and reloading;
- undo/redo;
- command enablement;
- error handling.

### Level 2: in-process semantic UI tests

Use `egui_kittest::Harness` against the real `NoteCraftApp`.

Tests should prefer:

- `get_by_role_and_label`;
- `query_all_by_role`;
- accessible names and values;
- pointer movement over control bounds for hover behavior;
- pointer press/move/release sequences for drag and resize behavior;
- keyboard actions;
- focus transitions;
- command/control observations;
- real temporary folders and checked-in fixtures.

Avoid coordinate clicks when a semantic query is available. Do not make
tests depend on translated labels if the application has a stable semantic
identifier or role-based query that expresses the intended behavior better.
Coordinates are appropriate when the test is specifically about pointer
geometry, hover, dragging, hit testing, or another spatial interaction.

These are simulated egui input events, not physical OS cursor movement. A
separate external smoke test is required to validate native mouse delivery to
the packaged application.

### Level 3: accessibility-tree checks

Run strict accessibility checks in UI tests. At minimum, verify:

- interactive controls have names;
- roles are correct;
- disabled and selected states are represented;
- inputs expose values;
- focusable controls have predictable order;
- dialogs expose their title and actions;
- custom canvas or editor regions expose meaningful structure where
  applicable.

The goal is dual-use: the same semantic information should serve automated
tests and users of screen readers or other assistive technology.

### Level 4: offscreen visual tests

Use `egui_kittest`'s wgpu renderer for bounded visual checks:

- key screens;
- normal and narrow layouts;
- all supported themes where the UI contract requires it;
- RTL and localized text expansion;
- focus, selected, disabled, hover, and pressed states;
- icons and custom-painted controls.

Do not treat a screenshot comparison as proof that the UI is accessible.
Visual correctness and semantic correctness are separate assertions.

### Level 5: external packaged-app smoke tests

Add a small number of tests against the compiled application:

- Windows: UI Automation, likely through pywinauto;
- Linux: AT-SPI, likely through dogtail or a direct client;
- optionally a visual fallback only for controls that cannot expose
  accessibility metadata.

These tests should verify the integration boundary:

- the packaged executable launches;
- the main window is discoverable;
- primary controls have expected native accessibility roles and names;
- a short create/open/save workflow works;
- the application exits cleanly.

They should not duplicate every in-process scenario.

## Recommended starting plan

### Immediate

1. Keep `egui_kittest` as the primary NoteCraft UI manipulation library.
2. Keep the existing wgpu and snapshot features available for visual tests.
3. Enable AccessKit in the native eframe application target.
4. Add accessible names, roles, values, actions, selection, and stable IDs
   as controls are implemented.
5. Add strict accessibility checks to representative screens.

### Next

1. Create a small Windows smoke-test prototype with pywinauto and the
   packaged NoteCraft executable.
2. Create a Linux smoke-test prototype with AT-SPI in one pinned CI image.
3. Test the same short user journey on both platforms.
4. Record the exact OS, desktop, display-server, scaling, and runtime
   conditions.

### Later

1. Add external tests only for integration and release confidence.
2. Add visual fallback automation only for genuinely inaccessible
   third-party or custom-rendered surfaces.
3. Reassess whether a common scenario layer is worth introducing. Do not
   add Robot Framework or Appium solely to make the two external smoke-test
   adapters look identical.

## Decision

The best starting stack for NoteCraft is:

```text
Rust/egui in-process tests:    egui_kittest
Application accessibility:     AccessKit through eframe/winit
Visual verification:           egui_kittest + wgpu renderer
Windows external smoke tests: Windows UI Automation, likely pywinauto
Linux external smoke tests:   AT-SPI, likely dogtail or a direct client
Visual fallback:               SikuliX only when semantic access is impossible
```

This follows the tooling already used by the ArtCraft applications, keeps
the fast and deterministic tests close to the Rust code, and still leaves a
clear path to verify the final Windows and Linux binaries through their
native accessibility systems.

## Important limitations and confidence

- The repository search establishes what is present in the inspected
  checkouts. It does not prove that no private CI job or untracked external
  test exists.
- The sibling repository was treated as read-only.
- The evidence strongly supports `egui_kittest`, AccessKit, and offscreen
  rendering as the established ArtCraft pattern.
- The external pywinauto and dogtail recommendation is a proposed
  integration layer, not a claim that ArtCraft already uses those tools.
- Platform accessibility behavior should be validated on real Windows and
  Linux environments before being described as certified support.
