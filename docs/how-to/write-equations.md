# Write equations

NoteCraft draws TeX maths in real maths fonts, in the page, with no browser and no network. You
write TeX; the page keeps the TeX as plain text.

## Put one in

- **Insert, then Equation** opens a list of equations to start from: fractions, roots, sums,
  integrals, limits, matrices, cases, aligned equations, the quadratic formula and more. Each is
  drawn as it will look. Pick one and its TeX goes in at the caret, between dollar signs.
- **Type it.** TeX between single dollar signs is an inline formula that flows with the words:
  `$E = mc^2$`. A block of its own goes between two lines of `$$`:

```text
$$
x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}
$$
```

Inline formulas are set small, as in a line of text, so a sum keeps its limits at the side. A
block is set large, with limits above and below.

Both page formats keep the same TeX. A Markdown page holds `$...$` and `$$` blocks; an HTML page
holds `<span class="math">`. The equation looks the same from either.

## What it draws

The engine is RaTeX, a Rust typesetter that follows KaTeX. In a test of 32 formulas it drew 31;
the one it did not draw was deliberately broken. Among them:

- fractions, nested fractions, `\binom`, roots with an index, superscripts and subscripts
- sums, products, integrals and limits, with limits above and below in a block
- Greek letters, relations, arrows and the usual symbols
- `\left` and `\right` brackets that grow to fit, and `\middle`
- matrices (`pmatrix`, `bmatrix`, `vmatrix`), `cases`, and `aligned` equations
- accents (`\hat`, `\vec`, `\bar`, `\tilde`), `\underbrace`, `\overset`, `\xrightarrow`
- `\mathbb`, `\mathcal`, `\mathbf`, `\operatorname`, and `\text{...}`
- colour with `\textcolor` and `\color`

A formula takes the page's text colour, so it follows the theme, light or dark. It only keeps a
colour you gave it.

Letters the maths fonts do not have, such as Arabic, Hebrew or Chinese inside `\text{...}`, are
drawn in the page's own font, kept together as one word so they join and read in the right order.

## When it cannot

If the TeX is wrong, or something it does not know, NoteCraft tries a smaller engine of its own,
and what that cannot read stays visible as text. A formula never disappears and never stops the
page from drawing.

Two limits keep a broken or hostile page from freezing the window: a formula longer than 4,000
characters, or nested more than 40 levels deep, is not given to the main engine, and is shown by
the smaller one.

## What is not there

- Packages and anything that is not maths: chemistry (`\ce`), TikZ, and the like.
- A visual formula editor. You edit the TeX.
- Every corner of LaTeX. KaTeX's own list of what it supports is the guide: if KaTeX does not draw
  it, NoteCraft will not either. NoteCraft has not been checked against all of that list.

The fonts are the KaTeX faces, 514 KB, under the SIL Open Font License; see `ATTRIBUTION.md`.
How the engine is built into the app is in [the architecture notes](../architecture.md).
