# nc-dom-ser

Reusable guidance for NoteCraft changes that cross the page model, HTML or
CMGFM, storage, rendering, the editor, or code view.

Read [SKILL.md](SKILL.md) when the skill applies. Read [AGENTS.md](AGENTS.md)
when maintaining or copying the skill. The canonical contracts remain in
`docs/dom_serialization/`, `docs/folder-format.md`, and `docs/testing.md`.

The skill's central rule is simple: one semantic page, many views and two file
formats. A page is not complete when only the window looks right; its files must
name and preserve what the window shows.
