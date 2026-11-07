#!/usr/bin/env python3
"""Generate a reviewable first-parent Git change list for a NoteCraft release.

Copy this script into a versioned release folder, then run:

    python docs/releases/<version>/build_git.py <previous-release-tag>

By default, the script writes GIT_CHANGES.md beside itself. Use --to to select
the release candidate ref, --output to choose another path, and --force to
replace an existing output file. Review the result; it is not finished release
copy.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent


def git(repo: Path, *args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", "-C", str(repo), *args],
        check=check,
        capture_output=True,
        text=True,
        encoding="utf-8",
    )


def resolve_commit(repo: Path, ref: str) -> str:
    result = git(repo, "rev-parse", "--verify", f"{ref}^{{commit}}", check=False)
    if result.returncode:
        raise ValueError(f"Git ref does not resolve to a commit: {ref}")
    return result.stdout.strip()


def build_change_list(repo: Path, base_ref: str, target_ref: str) -> str:
    base = resolve_commit(repo, base_ref)
    target = resolve_commit(repo, target_ref)
    if git(repo, "merge-base", "--is-ancestor", base, target, check=False).returncode:
        raise ValueError(f"Base ref {base_ref!r} is not an ancestor of target ref {target_ref!r}.")

    result = git(
        repo,
        "log",
        "--first-parent",
        "--reverse",
        "--date=short",
        "--format=%cs%x09%h%x09%s",
        f"{base}..{target}",
    )
    rows = [line.split("\t", maxsplit=2) for line in result.stdout.splitlines() if line]
    entries = "\n".join(f"- {date} `{short_hash}` {subject}" for date, short_hash, subject in rows)
    if not entries:
        entries = "- No commits in this range."

    return (
        "# Git change list\n\n"
        f"Range: `{base_ref}..{target_ref}`\n\n"
        "Generated from first-parent history. Review each entry against merged pull requests, "
        "release scope, and verified behavior before using it in public release notes.\n\n"
        f"{entries}\n"
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("base_ref", help="previous release tag or other base ref")
    parser.add_argument("--to", dest="target_ref", default="HEAD", help="release candidate ref (default: HEAD)")
    parser.add_argument("--output", type=Path, help="output Markdown path (default: GIT_CHANGES.md beside this script)")
    parser.add_argument("--force", action="store_true", help="replace an existing output file")
    args = parser.parse_args()

    try:
        repo = Path(git(SCRIPT_DIR, "rev-parse", "--show-toplevel").stdout.strip())
        output = args.output if args.output is not None else SCRIPT_DIR / "GIT_CHANGES.md"
        if not output.is_absolute():
            output = repo / output
        output = output.resolve()
        if output.exists() and not args.force:
            raise FileExistsError(f"Output already exists: {output} (pass --force to replace it)")

        content = build_change_list(repo, args.base_ref, args.target_ref)
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(content, encoding="utf-8", newline="\n")
    except (OSError, subprocess.CalledProcessError, ValueError) as error:
        print(f"build_git.py: {error}", file=sys.stderr)
        return 1

    print(f"Wrote {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
