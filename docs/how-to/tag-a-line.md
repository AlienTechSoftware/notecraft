# Tag a line

Tags mark a line as something: a thing to do, something important, a question to answer.

Click in the line, then **Home**, and either the Tags box or the arrow beside it for the whole
list: To Do, Important, Question, Remember for later, Definition, Highlight, Contact, Address,
Phone number, Web site to visit, Idea and Critical. Asking for the same tag again takes it off.

The line then shows the familiar mark for that tag: a tick box, an amber star, a question mark, a
light bulb.

## What a tag is in the file

A tag has to survive being read by other tools, so it is written into the page as text:

| Tag | In a Markdown page | In an HTML page |
|---|---|---|
| To Do | `- [ ] book the train` | `<li>&#9744; book the train</li>` |
| Any other tag | `#important leave on Friday` | `<p>#important leave on Friday</p>` |

To Do is a task list item, which every Markdown reader already understands, and its box is
ticked by clicking it on the page.

The others are a marker word at the start of the line. NoteCraft draws the mark instead of the
word, but the word is what is in the file, so your notes stay readable in any editor and
greppable from a terminal:

```sh
grep -rn "#important" ~/Documents/NoteCraft
```

A word that only looks like a tag is left alone: `#ideas for later` is ordinary text, because
the marker is a whole word followed by a space.

## What is missing

The tag lists elsewhere are longer than this one, their tags can be customised, and they collect
every tagged line into a summary page. None of that is built; the Find Tags button says so.
