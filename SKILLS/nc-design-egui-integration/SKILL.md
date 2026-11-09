---
name: nc-design-egui-integration
description: Document and review NoteCraft's egui integration boundaries, including raw input, semantic events, platform clipboard behavior, and ArtCraft shared UI conventions.
---

# NoteCraft egui integration

Use this skill when NoteCraft behavior depends on egui, eframe, winit, native
clipboard integration, raw input hooks, text widgets, or shared ArtCraft UI
patterns.

## Integration boundary

egui is a view and input framework, not NoteCraft's command layer. Use egui to
report gestures and semantic events, then dispatch NoteCraft behavior through
the application/controller command path.

The normal input flow is:

```text
winit/eframe raw input -> raw_input_hook adapter -> egui events/widgets -> NoteCraft command -> model/store/platform service
```

Use `raw_input_hook` only for integration cases that egui cannot represent
correctly. The adapter must remove or consume the event it owns so a widget does
not perform the action a second time.

## Clipboard finding and decision

ArtCraft egui apps such as CADCraft use egui semantic clipboard events:
`Event::Copy`, `Event::Cut`, and `Event::Paste(String)`, then translate them to
application commands. That works for text or DXF text because egui's paste
event is text-only.

NoteCraft also supports image clipboard data. An image-only Windows `Ctrl+V`
can fail inside egui/winit before NoteCraft receives `Event::Paste(String)`.
The context menu succeeds because it directly dispatches NoteCraft's paste
command, which reads the clipboard image through the platform boundary.

Therefore NoteCraft should:

- keep one paste command for keyboard, context menu, ribbon, and automation;
- use egui `Event::Paste(String)` for text when it is available;
- use the raw input hook to capture image-capable native `Ctrl+V` before egui
  converts or rejects it;
- route the raw shortcut to the same paste command as the context menu;
- prevent egui's text widget from also handling the consumed shortcut;
- keep clipboard access in the existing clipboard boundary, not in a widget.

## Review checklist

- Does the change rely on `egui::Event::Paste(String)` for an image? If so, it
  is incomplete.
- Is a raw event consumed before the widget sees it?
- Can the same command be invoked from a context menu and keyboard shortcut?
- Does the integration work when no text block has focus, such as a new page?
- Are platform-specific adaptations isolated in the app or integration edge?
- Are tests using the repository's clipboard boundary and permitted test data?

Related guidance: `nc-design-controller`, `nc-arch-x-plat`,
`nc-uiux-controls`, and `nc-code-rust`.
