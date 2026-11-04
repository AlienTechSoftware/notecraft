#!/usr/bin/env python3
"""Applies .github/web-build/web.diff's new files to a NoteCraft checkout.

Does NOT use `git apply`: web.diff's per-file @@ hunk headers are wrong for two of its six files
(playground.rs's header claims 277 lines, really 280; playground_clipboard.rs claims 129, really 206 -
confirmed by counting the actual '+' lines against each header). `git apply` trusts the header's declared
count and silently truncates the file at that point rather than erroring, which is worse than useless here.

Since every section in web.diff is a "new file" diff (no context lines, no deletions), the fix doesn't need
real patch application at all: split on 'diff --git', find each '+++ b/<path>' line, and take every
following line up to the next 'diff --git' (or end of file), stripping the leading '+'. This is immune to
header accuracy entirely.

Usage: python3 apply-web-diff.py <repo-root> <web.diff>
"""
import re
import sys
from pathlib import Path


def main() -> None:
    if len(sys.argv) != 3:
        raise SystemExit("usage: apply-web-diff.py <repo-root> <web.diff>")
    repo_root = Path(sys.argv[1]).resolve()
    diff_text = Path(sys.argv[2]).read_text(encoding="utf-8")

    sections = re.split(r"(?m)^(?=diff --git )", diff_text)
    created = 0
    for section in sections:
        if not section.startswith("diff --git "):
            continue
        path_match = re.search(r"(?m)^\+\+\+ b/(.+)$", section)
        if not path_match:
            raise SystemExit(f"web.diff: no '+++ b/<path>' line in section starting {section.splitlines()[0]!r}")
        if "\nnew file mode" not in section:
            raise SystemExit(f"web.diff: {path_match.group(1)} is not a 'new file' section - this script only handles pure additions")
        rel_path = path_match.group(1)
        lines = section.splitlines()
        start = next(i for i, l in enumerate(lines) if l.startswith("+++ b/")) + 1
        content_lines = [l[1:] for l in lines[start:] if l.startswith("+")]
        target = repo_root / rel_path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text("\n".join(content_lines) + "\n", encoding="utf-8")
        print(f"created {rel_path} ({len(content_lines)} lines)")
        created += 1
    print(f"created {created} files")


if __name__ == "__main__":
    main()
