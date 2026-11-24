# SPDX-License-Identifier: MIT OR Apache-2.0
"""Refresh the SRI hash of the WASM file in Trunk's index.html after wasm-opt rewrites it.

Trunk writes index.html with a sha384 integrity attribute for the .wasm file, before the
workflow runs wasm-opt in place. Trunk.toml sets data-wasm-opt="0", so Trunk does not re-hash.
This script recomputes the hash for every local .wasm reference that carries an integrity
attribute, and leaves other references (the JS file) untouched.

Usage: python rehash_wasm_integrity.py <index.html> <dist-dir>
"""

from __future__ import annotations

import base64
import hashlib
import re
import sys
from pathlib import Path

# A tag that names a local .wasm file and carries an integrity attribute.
_TAG = re.compile(r"<link\b[^>]*?>", re.IGNORECASE)
_HREF = re.compile(r'href="(\./)?([^"]+\.wasm)"', re.IGNORECASE)
_INTEGRITY = re.compile(r'integrity="sha384-[^"]*"', re.IGNORECASE)


def sri_sha384(data: bytes) -> str:
    return "sha384-" + base64.b64encode(hashlib.sha384(data).digest()).decode("ascii")


def rehash_wasm_integrity(index_html: Path, dist: Path) -> bool:
    """Update the wasm integrity hashes in index_html. Return True if the file changed.

    Raises FileNotFoundError if a referenced .wasm file is not in dist, so a broken build fails.
    """
    text = index_html.read_text(encoding="utf-8")

    def refresh(match: re.Match[str]) -> str:
        tag = match.group(0)
        href = _HREF.search(tag)
        if href is None or _INTEGRITY.search(tag) is None:
            return tag
        wasm = dist / href.group(2)
        if not wasm.is_file():
            raise FileNotFoundError(f"index.html refers to {href.group(2)}, which is not in {dist}")
        fresh = f'integrity="{sri_sha384(wasm.read_bytes())}"'
        return _INTEGRITY.sub(fresh, tag)

    updated = _TAG.sub(refresh, text)
    if updated == text:
        return False
    index_html.write_text(updated, encoding="utf-8")
    return True


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print("usage: rehash_wasm_integrity.py <index.html> <dist-dir>", file=sys.stderr)
        return 2
    changed = rehash_wasm_integrity(Path(argv[0]), Path(argv[1]))
    print("updated wasm integrity hash" if changed else "wasm integrity hash already current")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
