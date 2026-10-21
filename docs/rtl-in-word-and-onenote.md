# What Word and OneNote do for right-to-left, and what NoteCraft does

Read this before changing a right-to-left control. It is the comparison the owner asked for
on 2026-10-19: what Microsoft's own applications offer a writer of Arabic or Hebrew, where
each control lives, and which of them NoteCraft has.

Sources are Microsoft's published documentation only, listed at the foot. Nothing here comes
from reading Microsoft's code or from inspecting a binary, which is the clean room rule in
[AGENTS.md](../AGENTS.md).

Status: written 2026-10-19 from the documentation. Rows marked **unverified** are what the
documentation says and nobody here has run the application to confirm; this is mostly the
older Word material, which is from 2010.

## 1. The short answer to "where are the controls"

| | Word | OneNote | NoteCraft |
|---|---|---|---|
| Paragraph direction | Home tab, Paragraph group, a pair of buttons | behind the **Paragraph Alignment** arrow | behind the alignment button, and in the Direction group |
| Page or section direction | Page Layout, Page Setup, Layout, **Section direction** | not documented | Direction group, **Page** |
| Whole document direction | Word Options, Advanced, Show Document Content, **Document View** | not documented | the page direction is the only level |
| Table direction | Table Properties, **Table direction** | not documented | not built |
| Direction of one run inside a line | the `RtlRun` and `LtrRun` macros | not documented | in the Direction group's **More...** menu |

Two things to take from the first row. OneNote keeps paragraph direction **behind the
alignment arrow**, which is where NoteCraft already had it, so that placement was right. And
in both applications the buttons **appear only when a right-to-left editing language is
enabled** (File, Options, Language). NoteCraft shows them always. For a notes application
that is the better default: there is no language list to set up first, and a writer who
types one Arabic line in an English notebook needs the control at that moment.

## 2. What decides a paragraph's direction

Word tags text with a direction **from the keyboard language it was typed on**. That is the
part NoteCraft cannot copy: it has no keyboard language to read, and a notes application
that guessed from the keyboard would be wrong for anyone who types Arabic on a US layout.

Excel and Access instead offer a **Context** direction, which takes the direction from the
**first strong character** and leaves neutral characters to follow the paragraph until a
strong one is typed. That is exactly NoteCraft's **Auto**, and it is the right model here.

NoteCraft's order, since 2026-10-19, is the paragraph's own mark, then the page, then the
first strong letter. Word's is the same shape: a paragraph button overrides the section,
which overrides the document.

## 3. Kashida justification

Word has had this for years and it is the one place where the documentation is ahead of
NoteCraft.

- ~~**Three strengths**, not one~~: **Justify Low**, **Justify Medium** and **Justify
  High**, which change how many kashidas a line is allowed. **Built on 2026-10-19**, under
  the same three names, in the alignment menu. Each is a ceiling rather than a target, so a
  line the weakest can already fill looks the same at all three and the difference shows on
  a hungry line; `crates/text/src/kashida.rs` has what each one allows and
  `crates/ui-egui/tests/rtl_quality.rs` measures the three apart.
- **Kashidas can be typed by hand**, with **Shift+J** between two Arabic letters on an
  Arabic keyboard, and several in a row give a longer stretch. NoteCraft inserts them only
  when it justifies, and `notecraft_text::kashida` already treats a kashida the author typed
  as the best place to grow (rank 1), so the engine is ready for this and the window is not.

## 4. Things Word has that NoteCraft has not

- **Digit substitution.** Which numerals are drawn: Western (123), Arabic-Indic (١٢٣), or
  from the context. Word keeps it per document and it can differ from the operating system's
  setting. NoteCraft draws whatever the writer typed and has no option.
- **Coloured diacritics.** Word can draw the harakat in a colour of their own, which is a
  real aid when teaching or proofreading vowelled text. NoteCraft draws them in the text
  colour.
- **A separate font for complex scripts.** Word keeps the font, size and style for complex
  scripts apart from the Latin ones, so a document can have a Latin face and an Arabic face
  at different sizes. NoteCraft bundles one face per script and has no setting.
- **Zero-width joiner and non-joiner**, inserted from the Symbol dialog or with Ctrl+Shift+1
  and Ctrl+Shift+2 on an Arabic keyboard, to force or break a join. NoteCraft has neither.
- **Find and replace that knows the script**: it searches in entry order rather than display
  order, and has options for matching kashida, alef hamza forms and diacritics. NoteCraft's
  search is plain.
- **Table direction**, so the first column of a table is on the right.
- **Section direction**, which also turns column flow and line numbering around.

## 5. Things NoteCraft has that the documentation does not claim for either

- **Kashida justification in a notes application.** OneNote has no kashida control in its
  documentation at all. Word has one; OneNote does not.
- **Page direction with no language pack**, and the direction controls always present.

## 6. What to build next, in the order the owner would feel it

1. ~~**Three strengths of kashida justification**~~, matching Word's Low, Medium and High.
   Built on 2026-10-19.
2. **A typed kashida.** A key that inserts a tatweel between two joined letters. The ranking
   already prefers one the author put in.
3. **Digit substitution**, at least as a page setting. A writer of Arabic who types `123`
   often wants `١٢٣`, and the demo page shows both forms side by side today.
4. **Table direction**, which is the largest of these and the one a reader notices at once
   on a page with a table.

Zero-width joiner and non-joiner, coloured diacritics and a separate complex-script font are
smaller and can wait.

## 7. Sources

All read on 2026-10-19.

- Microsoft, "Using right-to-left languages in Office":
  <https://support.microsoft.com/en-US/office/languages/using-right-to-left-languages-in-office>
- Microsoft, "Type in a bi-directional language in Office for Mac":
  <https://support.microsoft.com/en-au/topic/type-in-a-bi-directional-language-in-office-for-mac-d7bb1d52-4d82-4482-910f-d74b7c3bd468>
- Microsoft Office Global Experience team, "Bidirectional Features in Microsoft Word", 2010,
  archived: <https://learn.microsoft.com/en-us/archive/blogs/office_global_experience/bidirectional-features-in-microsoft-word>
- Microsoft, "Text directionality":
  <https://learn.microsoft.com/en-us/globalization/fonts-layout/text-directionality>

On the wider question of how an editor should behave, there is no standard to follow. The
W3C's "Additional Requirements for Bidi in HTML" is a 2011 working group draft that says in
its own status note that it has no standing, and the Chromium project's notes say plainly
that there is no standard for mapping between logical and visual positions, which is why
browsers disagree about what an arrow key does in mixed text. NoteCraft's answers are
measured in `crates/ui-egui/tests/rtl_quality.rs` rather than taken from a specification.

- W3C Internationalization, "Additional Requirements for Bidi in HTML" (editor's draft):
  <https://www.w3.org/International/docs/html-bidi-requirements/>
- The Chromium Projects, "RTL in WebKit": <https://chromium.org/developers/rtl-in-webkit>
