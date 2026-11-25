# NoteCraft OCR lab

A developer tool for testing and labelling OCR. It runs the product's own OCR code
(`notecraft-ocr`) on a folder of test pictures, shows what the engine reads over each picture, and lets
you draw and correct the true boxes and text. It is never shipped.

## Start it

```bash
run.cmd --ocr-lab
```

Flags are passed through: `--folder <dir>` for the test pictures, `--models <dir>` for the OCR model
folder. Without flags it reopens the last folder and models. You can also run
`cargo run -p notecraft-ocr-lab`.

Headless, with no window, for scripts:

```bash
cargo run -p notecraft-ocr-lab -- --check --folder <dir> --models <dir>
```

It prints one line per picture and exits 1 if any picture fails.

## The window

![The lab with a picture, its OCR results, metrics and picture sliders](docs/screenshots/overview.png)

- **Title row.** Drag the bare bar to move the window, double-click it to maximise. The buttons at the
  right close, maximise and minimise. The sun or moon opens the theme chooser (the same 14 themes as
  the app). The light shows the model state.
- **Left: test cases.** The pictures grouped by language (from the folder names), then by how they were
  made (**Picture (photo or scan)** or **Digital drawing (pen or vector)**), then by what they contain
  (**Handwriting (text)**, **Handwriting (math)**, **Shapes**, or **Other** when untagged). A sample
  with several content tags appears under each of them. Every content node has **+ A page here**:
  **New blank drawing** or **Import picture…**, which put the new sample in `<test folder>/<Language>/`
  with that node's tags. A pencil marks a picture that has labels.
- **Tags.** Each picture's source and content are in `<picture>.tags.json` beside it, for example
  `{"source": "drawing", "content": ["shapes"]}`. A picture without the file is a picture with no
  content tags.
- **Middle: the picture.** Orange boxes are the engine's lines. Blue boxes are your labels. Yellow is
  the box you have selected.
- **Right: run and labels.** Run OCR, copy the report, switch between select and draw, and see the
  score: how many labels the engine found, how many text matches, and how many lines have no label.
  Under it are the metrics (CPU and memory of this process while it runs), the picture sliders, your
  labels and the engine's lines.

![The lab at start, with a picture open](docs/screenshots/start.png)

## Models

The header light is green when both OCR model files are present and match their checksums, amber when
one is missing, red when one is wrong, and grey when no folder is set. Hover it for the reason.
**Download models** fetches the two public files (about 12 MB) into `tools/ocr-lab/local/models`, checks
them, and loads them. Nothing downloads until you press it. The weights are CC BY-SA 4.0; see the OCR
skill.

## Labels

- **Draw a label:** choose *Draw a label*, then drag on the picture.
- **Select, move, resize:** choose *Select / move / resize*, click a label, drag it, or drag a corner.
  Press Delete to remove the selected label.
- **Text and colour:** select a label in the right panel. Type its text, and use the colour button next
  to it to change its box colour.
- Labels are saved beside each picture as `<picture>.label.json` as soon as you change them.

## Models section

The right panel's **Models** section names the model format (ONNX), the model family (the official
`ocrs` detection and recognition models), the engine (`ocrs`) and the inference runtime that loads
the ONNX files (`rten`, pure Rust). It links to their sources, the model repository and the model
card, and gives the licence. For each model it shows the expected size, what
the model does, and what it can read. The state line says whether the engine is loaded, and the header
light's reason is repeated there.

## Drawings

- **+ Blank drawing** under a group adds a white 1200 × 800 PNG to that group's folder, named
  `drawing-1.png`, `drawing-2.png`, and so on, and opens it with the pen ready.
- **Pens** appear in the right panel only for a digital drawing (a sample tagged as a drawing). They are
  the app's own pens: the same tools, colours and widths as the app's default rack (pen, pencil,
  highlighter). Choose one and draw on the picture. A highlighter lets the picture show through.
- The picture on screen changes at once, but the file does not until you press **Save drawing**. Until
  then you cannot open another picture.
- Pens draw full pressure, since a mouse has none. Undo, more pens, and saving the drawing as the app
  stores it (SVG with the pen's readings) are not built yet.
- Saving removes the old OCR run for that picture, so the next **Run OCR** reads the new drawing.

## Picture sliders

Under **Picture** in the right panel: **Hue**, **Contrast** and **Brightness**, with **Reset picture**.
They change only the picture on screen, so you can see faint handwriting. OCR reads the original file.

## Runs

A run is saved beside its picture as `<picture>.ocr-run.json` with the picture's checksum. Reopening the
picture shows the last result without reading it again. If the picture file changes, the old run is
ignored.

## Where things live

Everything local is under `tools/ocr-lab/local/`, which is git-ignored:

| Path | What it is |
|---|---|
| `settings.json` | The folder, models and theme you last used |
| `ocr-lab.log` | The event log |
| `reports/` | Reports written by **Copy report** |
| `models/` | Models downloaded by **Download models** |

The test pictures stay in their own folder. Each case folder has a `SOURCES.txt` that records where every
picture came from and its licence.

## Language of the lab

The lab's own labels and messages are English only. It is a developer tool that is never shipped, so
it is outside the app's translation audit (`cargo xtask i18n-audit` covers the app's source only). This
is a decision, not an oversight; a translation would need a reason to ship.

## Checks

```bash
cargo test -p notecraft-ocr-lab
cargo xtask ocr-check
```

`ocr-check` runs these tests and, when the models are set, the headless read of the smoke pictures.
