# `<picture>.ocr.json`: the OCR and description file, schema version 1

**Status: proposed.** The lab writes this file. The product's companion job (`crates/ocr`) still writes
`<picture>.txt`, `<picture>.lines` and `<picture>.ocr-hash`. Moving the product to this file is a
separate change that needs the owner's decision (see "Migration").

## Why one JSON file

- The text alone loses the boxes. Search highlights and the lab need the polygons, so they are kept
  in the same file as the text.
- Traditional OCR (what the engine reads) and the description (alt text, from a model) are two
  sources about one picture. One file holds both, with the source named on each part.
- A version field lets readers refuse a file they do not understand, instead of misreading it.

## Where it lives

Beside the picture: the picture's full file name with `.ocr.json` added. `drawing-1.png` gives
`drawing-1.png.ocr.json`. It is a derived file. Nobody edits it by hand, and it is rewritten when
the picture's bytes change.

## Version 1

```json
{
  "schema": "notecraft.ocr",
  "version": 1,
  "picture": {
    "name": "drawing-1.png",
    "sha256": "9f2c...",
    "width": 1200,
    "height": 800
  },
  "produced": {
    "at": 1732500000,
    "by": "ocrs 0.14 through notecraft-ocr"
  },
  "text": "Testing\nSourdough 6.00",
  "lines": [
    {
      "text": "Testing",
      "polygon": [[120.0, 40.0], [620.0, 40.0], [620.0, 130.0], [120.0, 130.0]],
      "confidence": null
    }
  ],
  "alt": null,
  "timings": [
    { "step": "read picture", "ms": 162.0 }
  ]
}
```

### Fields

| Field | Type | Meaning |
|---|---|---|
| `schema` | string | Always `"notecraft.ocr"`. A reader checks this first. |
| `version` | integer | `1` for this layout. A reader that sees a higher number refuses the file. |
| `picture.name` | string | The picture's file name, so a moved pair is recognisable. |
| `picture.sha256` | string | SHA-256 of the picture's bytes. A file whose hash no longer matches is stale. |
| `picture.width`, `height` | integer | The picture's size in pixels. |
| `produced.at` | integer | Unix time in seconds when the file was written. |
| `produced.by` | string | The engine and version that read the picture. |
| `text` | string | All the lines joined with `\n`, in reading order. This is what search indexes. |
| `lines` | array | One object per recognised line, in reading order. |
| `lines[].text` | string | The line's text. |
| `lines[].polygon` | array | Four `[x, y]` corners in picture pixels, clockwise from the top left. |
| `lines[].confidence` | number or null | `null` when the engine gives no score (ocrs does not). |
| `bounds` | polygon or null | One polygon round everything the engine found: four `[x, y]` corners, clockwise from the top left. Version 1 uses the box round all the lines; a tighter outline can replace it without a version change. `null` when nothing was found. |
| `alt` | object or null | The description, when a description model has run. `null` until then. |
| `timings` | array, optional | The lab's own timings for this run (`step`, `ms`). Not product data; a reader may ignore it. |

### `alt` (defined now, written later)

```json
"alt": {
  "text": "A handwritten word, 'Testing', in blue ink.",
  "model": "name of the description model",
  "model_version": "its version",
  "produced_at": 1732500100
}
```

`alt` is produced by a description model, not by the OCR engine. It stays `null` until that model
exists. It is kept in this file so the picture's description and text are read together.

## Rules

- **Derived.** The file is written from the picture and the engine's output. It is never authored by
  hand.
- **Validated once, at creation.** The writer checks the geometry (no crossing outlines) when it
  creates the file. Readers and renderers assume the file is valid and do not re-check it.
- **Stale is ignored.** If `picture.sha256` does not match the picture's bytes, the file is out of date.
  Readers ignore it and the next OCR run replaces it.
- **Atomic.** Written through a temporary file, then renamed, so a crash leaves the old file.
- **Versioned.** A change that a version 1 reader would misread bumps `version`. An added optional
  field does not.
- **Unknown fields are ignored** by readers of the same version.
- **Polygons are in picture pixels**, not in the window's coordinates.
- **Nesting is allowed.** A polygon may sit inside another, as circular text does: concentric rings,
  one per row of text around a circle. The sign of `signed_area` tells the reader which ring is outer
  (positive: clockwise on the screen) and which is inner (negative: a hole).
- **Outlines never cross.** No edge of one polygon may cross or touch an edge of another. Nesting is not
  crossing.
  - **The writer validates.** The OCR job checks every pair when it creates the file, and logs any crossing.
    The lab's `outlines_cross` does this check and is tested.
  - **Readers and renderers do not validate.** They assume the file is correct and skip the check, for
    speed. A file that breaks the rule is a writer bug, not something a reader should repair.

## Migration (proposed, not done)

1. The lab writes `<picture>.ocr.json` (done in the lab).
2. The product's `crates/ocr` companion job writes this file in place of `.txt`, `.lines` and
   `.ocr-hash`. Its hash check moves into `picture.sha256`.
3. `crates/store/src/search.rs` reads `lines[].text` and `polygon` from this file instead of `.lines`.
4. The engine tests that check `receipt.png.lines` and `receipt.png.alt.txt` change to the new names.

Each step is its own PR. The product steps need the owner's decision, since they change the format
the app writes to users' notebooks.
