#!/usr/bin/env python3
"""Applies .github/web-build/patches.yaml's rules to a NoteCraft checkout, to build the web target.

A from-scratch reimplementation of the rule format daijro/openplayground's scripts/build.mjs uses (not a
copy of that script): each rule names one file, a regex (matched with MULTILINE, as build.mjs's 'm' flag),
and either `replace` (JS-style $1/$2/$& backreferences, converted here to Python's match-group semantics)
or `before`/`after` (spliced around the whole match).

Matches JS String.replace(regex-without-'g')'s real semantics exactly, which several rules rely on: only
the FIRST match in the file is touched, any further matches are left alone on purpose. The rfd::FileDialog
rules in patches.yaml are a sequence that relies on this - a generic rule picks off one occurrence, then
more specific rules (by surrounding context) pick off the rest one at a time. A rule still fails loudly if
its pattern matches nowhere at all - that case really does mean upstream changed and the rule needs updating.

Usage: python3 apply-web-patches.py <repo-root> <patches.yaml>
"""
import re
import sys
from pathlib import Path

import yaml


def js_replacement(template: str, match: "re.Match[str]") -> str:
    """JS String.replace's $1../$9/$&/$$  in `template`, against a Python match object."""

    def sub(m: "re.Match[str]") -> str:
        token = m.group(1)
        if token == "$":
            return "$"
        if token == "&":
            return match.group(0)
        idx = int(token)
        return match.group(idx) or ""

    return re.sub(r"\$(\$|&|\d)", sub, template)


def apply_rule(repo_root: Path, rule: dict) -> None:
    target = repo_root / rule["file"]
    if not target.is_file():
        raise SystemExit(f"patch rule error: {rule['file']} does not exist in {repo_root}")
    text = target.read_text(encoding="utf-8")
    pattern = re.compile(rule["find"], re.MULTILINE)
    m = pattern.search(text)
    if m is None:
        raise SystemExit(
            f"patch rule error: {rule['file']} has no match for /{rule['find']}/ "
            "- upstream source has likely changed; update the rule in patches.yaml"
        )
    if "replace" in rule:
        replacement = js_replacement(rule["replace"], m)
    else:
        replacement = (rule.get("before") or "") + m.group(0) + (rule.get("after") or "")
    new_text = text[: m.start()] + replacement + text[m.end() :]
    target.write_text(new_text, encoding="utf-8")
    print(f"patched {rule['file']} at /{rule['find']}/")


def main() -> None:
    if len(sys.argv) != 3:
        raise SystemExit("usage: apply-web-patches.py <repo-root> <patches.yaml>")
    repo_root = Path(sys.argv[1]).resolve()
    rules = yaml.safe_load(Path(sys.argv[2]).read_text(encoding="utf-8"))
    for rule in rules:
        apply_rule(repo_root, rule)
    print(f"applied {len(rules)} rules")


if __name__ == "__main__":
    main()
