from __future__ import annotations

import base64
import hashlib
import tempfile
import unittest
from pathlib import Path

from rehash_wasm_integrity import rehash_wasm_integrity


def sri(data: bytes) -> str:
    return "sha384-" + base64.b64encode(hashlib.sha384(data).digest()).decode()


class RehashWasmIntegrityTests(unittest.TestCase):
    def test_updates_stale_wasm_hash_and_keeps_js_hash(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            dist = Path(temporary)
            old_wasm = b"\x00asm old"
            new_wasm = b"\x00asm optimized"
            js = b"console.log('same');"
            (dist / "app_bg.wasm").write_bytes(new_wasm)
            (dist / "app.js").write_bytes(js)
            stale = sri(old_wasm)
            js_hash = sri(js)
            html = (
                '<link rel="modulepreload" href="./app.js" crossorigin="anonymous" integrity="' + js_hash + '">\n'
                '<link rel="preload" href="./app_bg.wasm" crossorigin="anonymous" integrity="' + stale + '">\n'
            )
            (dist / "index.html").write_text(html, encoding="utf-8")

            changed = rehash_wasm_integrity(dist / "index.html", dist)

            result = (dist / "index.html").read_text(encoding="utf-8")
            self.assertTrue(changed)
            self.assertIn('integrity="' + sri(new_wasm) + '"', result)
            self.assertNotIn(stale, result)
            self.assertIn('integrity="' + js_hash + '"', result)

    def test_leaves_html_alone_when_hash_is_already_current(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            dist = Path(temporary)
            wasm = b"\x00asm"
            (dist / "app_bg.wasm").write_bytes(wasm)
            html = '<link rel="preload" href="./app_bg.wasm" integrity="' + sri(wasm) + '">\n'
            (dist / "index.html").write_text(html, encoding="utf-8")

            changed = rehash_wasm_integrity(dist / "index.html", dist)

            self.assertFalse(changed)
            self.assertEqual((dist / "index.html").read_text(encoding="utf-8"), html)

    def test_fails_loudly_when_referenced_wasm_is_missing(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            dist = Path(temporary)
            (dist / "index.html").write_text(
                '<link rel="preload" href="./gone_bg.wasm" integrity="sha384-AAAA">\n', encoding="utf-8"
            )

            with self.assertRaises(FileNotFoundError):
                rehash_wasm_integrity(dist / "index.html", dist)


if __name__ == "__main__":
    unittest.main()
