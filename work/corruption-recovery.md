# Corruption tolerance and user-approved repair

Status: requirement and design reference, implementation not complete
Date: 2026-10-24

## Requirement

Old notes may contain malformed or partially written content. NoteCraft must tolerate these notes
where safe, avoid crashing, and never silently rewrite them.

## Load outcomes

1. Clean load: open normally. No repair prompt.
2. Recoverable load: show the recovered view and ask permission before writing a repair.
3. Unrecoverable load: do not write. Explain the failure and offer available version or backup
   paths.

## Repair contract

The repair prompt must identify the page, describe the issue, list what can be recovered, and say
where the original will be preserved. The actions are:

- `Repair and open`: preserve the original, atomically write the repaired content, then open it.
- `Open without repairing`: do not write; open a read-only or recovered view where possible and
  record an error-level local diagnostic.
- `Cancel`: do not write and do not replace the current page.

The original file must remain recoverable if the process stops during repair. The repair result,
denial, and unrecoverable failure must be locally observable through existing diagnostics.

## Implementation boundary

- Format parsers return diagnostics and a recovered representation instead of silently returning
  an empty body.
- Store creates a version or backup before an approved repair and writes atomically.
- Engine exposes the load and repair decision without opening UI dialogs.
- UI presents the themed modal and keeps the permission decision explicit.
- Tests cover malformed HTML, malformed CMGFM/front matter, interrupted repair, denial, retry, and
  source preservation.
