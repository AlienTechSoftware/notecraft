## What and why

<!-- One paragraph: the problem, and what this changes. Link the issue if there is one. -->

## Quality level L3 (see SKILLS/nc-devx/SKILL.md)

- [ ] `cargo xtask pre-push` passes on the current branch before pushing
- [ ] The pre-push hook confirms the validation still matches `origin/main`
- [ ] CI checks pass for this pull request
- [ ] Coverage: the `coverage` job's warnings are read; `cargo xtask coverage --update` is in
      this pull request if coverage rose
- [ ] Docs match the change: `docs/commands.md` (regenerated), `docs/gaps.md`,
      `docs/testing.md`, `docs/mcp.md`, `docs/control-protocol.md`, as they apply
- [ ] A new user action is a `CommandSpec` with a test
- [ ] Page or demo changes: `cargo xtask demo-check`, the format parity test, and a side by
      side look at both formats
- [ ] UI changes: the pictures were looked at; `cargo xtask screenshots` if they changed
- [ ] New files under `assets/` or `docs/images/` have a row in `ATTRIBUTION.md`

## How it was checked

<!-- The commands run and what they showed. Say what was not checked. -->
