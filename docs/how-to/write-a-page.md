# Write a page

Click a page in the tree and it opens. Click anywhere on the page and the caret lands where you
clicked. Type.

The page is shown the way it will be read: headings in heading type, bold and links in their own
colours, lists with their bullets. The marks that make them (`**`, `#`, `-`) stay in the text but
are dimmed, so the words come first.

## Keys

| Key | What it does |
|---|---|
| Enter | Splits the line at the caret. At the start of a line it pushes the line down. At the end of a heading it starts an ordinary paragraph. On an empty list item it ends the list |
| Backspace at the start of a line | Joins the line to the one above |
| Tab, Shift+Tab | More or less indent on a list item |
| Up, Down | Move to the line above or below |
| Escape | Leave the line |
| Ctrl+S | Save now (the page saves itself about a second after you stop typing anyway) |

## The ribbon

**Home** has the marks and the blocks: bold, italic, strikethrough, code, a link, bullets,
numbering, indent, quote and clear formatting; a Styles gallery for Heading 1 to 6, Page Title,
Citation, Quote, Code and Normal; and the Tags gallery ([tag a line](tag-a-line.md)).

**Insert** has tables, pictures, files, links, equations, diagrams and today's date.

**View** has full page, search, rule lines, zoom, the theme and the page source.

A style or a tag you ask for twice turns itself off again, as it does elsewhere.

## Equations

Insert, then **Equation**, or type TeX between dollar signs: `$E = mc^2$` inline, or a block
between `$$` lines. NoteCraft draws a TeX subset itself: fractions, roots, superscripts and
subscripts, sums, products and integrals with limits, scaled brackets, Greek letters and
common symbols. Anything it does not know stays visible as text rather than disappearing.

## The page source

The page's file is one click away: the **`< >`** button beside the page, or **View**, then
**Source**. The source opens beside the page, and what you type in either one lands in the file.

Use it for anything the editor does not do yet: raw HTML, a Mermaid diagram, a table you would
rather type than click.

## What a page is on disk

An HTML page:

```html
<h1>Project X</h1>
<p>Kickoff notes for the <b>first release</b>.</p>
<ul><li>Ship the folder format</li></ul>
```

The same page as CommonMark:

```markdown
# Project X

Kickoff notes for the **first release**.

- Ship the folder format
```

Both carry their metadata with them: the Markdown page in front matter, the HTML page in
`<meta name="notecraft:...">` tags. [Choose the page format](choose-the-page-format.md) explains
which one a new page gets.

## One limit worth knowing

A page whose HTML holds markup the editor cannot model yet (a `<div>` from a paste, say) is
shown but not written on, and says so, so that editing it cannot quietly throw that markup away.
Open its source and the whole file is yours.
