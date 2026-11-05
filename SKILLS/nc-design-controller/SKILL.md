---
name: nc-design-controller
description: Design and review NoteCraft user actions so they remain MVC-aligned, identify when behavior is a command, and recommend the correct command, model, controller, and view boundaries.
---

# NoteCraft design controller

Use this skill when a request adds or changes a user action, keyboard shortcut,
context-menu item, ribbon action, toolbar action, or another interaction that
may cross the MVC boundary.

## Core decision

Treat a user-visible action as a command when it changes application state,
changes page content, reads or writes a platform service, or must be reachable
from more than one UI entry point. Keyboard shortcuts and context-menu items
that mean the same thing must dispatch the same command.

## Design review

Before implementation:

1. Name the user action and its expected result.
2. Identify every entry point: keyboard, ribbon, context menu, toolbar, control
   protocol, and automation where applicable.
3. Find the existing command or `CommandSpec`. Reuse it when the behavior is
   already defined.
4. If no command exists, recommend where to add it in the engine/controller,
   what state it changes, and which views should only dispatch it.
5. Keep storage, format, and platform-service work behind the controller or
   lower-layer boundary. Views must not implement a second version of the
   action.

## Clipboard decision

Paste is one application action even though the platform exposes different
clipboard representations. Text, HTML, and image paste must converge on the
same NoteCraft paste command. The command may choose image, HTML, then text
based on available clipboard data.

egui's `Event::Paste(String)` is text-only. Image-only `Ctrl+V` may therefore
need an earlier native/raw-input adapter, but that adapter should queue the
same paste command used by the context menu. It must not create a separate
image-paste behavior.

## Acceptance checks

- Keyboard and menu entry points produce the same state transition.
- The command is testable without a particular widget or pointer location.
- The view only detects the gesture and dispatches the command.
- Errors and unavailable platform services use the command's normal status or
  alert path.
- The command is documented when it is added or changed.

Consult `nc-arch`, `nc-hl-design`, and `nc-code-rust` for architecture,
feature design, and implementation details respectively.
