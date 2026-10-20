# Draw diagrams and charts

A diagram in NoteCraft is Mermaid source: plain text that says what to draw. The page keeps the
text, and the code view draws it for you, above the text, in your theme's colours. Nothing is a
picture until you look at it, and nothing is sent anywhere.

![The code view with a pie chart, a bar and line chart and a quadrant chart drawn above their source](../images/diagrams.png)

## See one

1. Put a diagram in a page. Insert, then **Diagram**, adds a starter one; or write your own.
2. Press the **`< >`** button beside the page. The code view opens, and the diagrams of the page
   are drawn at the top, one under another, with the page's text under them.
3. Edit the text. The drawing follows what you type. Press the button again to go back to the page.

The live page does not draw them yet. It shows the source, as it always has. See "Not built yet".

## How a diagram is kept

In either page format, so the same diagram looks the same:

| Page format | The diagram is |
|---|---|
| Markdown | a fenced block: three backticks and `mermaid`, the diagram, three backticks |
| HTML | `<pre class="diagram">` as NoteCraft writes it, or `<pre class="mermaid">` or `<div class="mermaid">` as Mermaid's own pages do. `<` and `>` are written as `&lt;` and `&gt;`, as in any HTML |

A diagram can also be a file of its own in the page's folder, with the name ending `.mmd` or
`.mermaid`, linked from the page like a picture. Pick that file in the list and it is drawn above
its source, which you can type in.

## What can be drawn

Flowcharts, sequence diagrams, class diagrams, state diagrams, entity-relationship diagrams, Gantt
charts, mind maps, timelines, pie charts, bar and line charts (`xychart-beta`), quadrant charts,
and more: whatever the renderer, `mermaid-rs-renderer`, can read. In the tests, the ones the page
checks are the kinds above. Arabic, Hebrew and mixed labels are drawn joined and in the right
direction, and a right-to-left flowchart runs right to left.

Charts are Mermaid's own: a pie, bars with a line, a quadrant. They are not a general plotting
tool; a Vega-Lite chart from data in a file is planned and not built.

## When it cannot be drawn

- **A mistake in the source.** The code view says "Not drawn" and the reason, in one line, and the
  text is still there to fix.
- **Too big.** A diagram of more than 150 lines or 20,000 characters is not drawn, and says so
  at once. A very large graph can take the renderer seconds to lay out.
- **Slow.** Drawing happens off to one side, so the window never waits: you see "Drawing..." and
  then the picture. A diagram that takes more than eight seconds is given up on.

A diagram is never lost. Whatever happens, the text is the page's.

## Not built yet

- The live page drawing a diagram where it stands, with a double click to open its code, as an
  embedded object in an old word processor did.
- Saving the drawn picture beside the page, for other programs to show.
- Vega-Lite charts, and other diagram languages.
- Zoom and pan on a large drawing.

The plan, and why the renderer was chosen, are in `docs/gaps.md` and `docs/architecture.md`.
