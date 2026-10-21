---
id: caa02c95caf144c30593
title: "Diagrammes Mermaid"
order: 82
created: 2026-10-18T00:00:00Z
updated: 2026-10-18T00:00:00Z
tags: []
---

Write a diagram as Mermaid text in a fenced block. NoteCraft draws the block on the page in the
current theme; open the page source with **< >** to edit the text.

## Flowchart

```mermaid
flowchart TD
  Start[Write a note] --> Save{Saved?}
  Save -->|yes| Ready[Keep going]
  Save -->|not yet| Wait[Pause briefly]
  Wait --> Save
```

## Sequence

```mermaid
sequenceDiagram
  Writer->>Page: Type a line
  Page->>Files: Save the page
  Files-->>Page: Saved
  Page-->>Writer: Keep writing
```

## Mind map

```mermaid
mindmap
  root((A notebook))
    Pages
      Words
      Equations
    Drawings
      Shapes
      Ink
    Files
      Pictures
      Attachments
```

## State diagram

```mermaid
stateDiagram-v2
  [*] --> Ready
  Ready --> Editing: type
  Editing --> Saved: pause
  Saved --> Editing: type again
  Saved --> [*]
```

## Pie diagram

```mermaid
pie title Pages in this tour
  "Writing" : 4
  "Drawing" : 2
  "Files" : 2
```

Each block is its own diagram. The text stays in the page file, so it can be changed or copied
without an image editor.

Next: [Charts, Vega and D3](../charts-vega-d3/).
