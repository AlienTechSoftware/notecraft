from __future__ import annotations

import tempfile
import tomllib
import unittest
import zipfile
from pathlib import Path

from bundle_demo import bundle_demo


class BundleDemoTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.root = Path(__file__).resolve().parents[2]
        with (cls.root / "Cargo.toml").open("rb") as manifest:
            cls.version = tomllib.load(manifest)["workspace"]["package"]["version"]

    def test_bundles_both_official_formats_with_release_version(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            archive = Path(temporary) / "welcome-notebook.zip"
            bundle_demo(self.root / "docs" / "demo" / "DemoNotebook", archive, self.version)

            with zipfile.ZipFile(archive) as bundle:
                names = set(bundle.namelist())
                self.assertIn("html/_node.toml", names)
                self.assertIn("html/00-start/welcome/index.html", names)
                self.assertIn("cmgfm/_node.toml", names)
                self.assertIn("cmgfm/00-start/welcome/index.md", names)
                for fmt in ("html", "cmgfm"):
                    languages = {name.split("/")[1] for name in names if name.startswith(f"{fmt}/") and len(name.split("/")) > 2}
                    languages = {name for name in languages if len(name) == 5 and name[:2].isdigit() and name[2] == "-"}
                    self.assertEqual(len(languages), 13, f"{fmt} should include every translated notebook")
                html = bundle.read("html/00-start/welcome/index.html").decode()
                cmgfm = bundle.read("cmgfm/00-start/welcome/index.md").decode()
                for page in (html, cmgfm):
                    self.assertIn(f"Welcome to NoteCraft (Ver {self.version})", page)
                    self.assertIn(f"NoteCraft {self.version}. This notebook", page)
            self.assertTrue((self.root / "docs/demo/DemoNotebook/cmgfm/00-start/welcome/index.md").is_file())

    def test_rejects_a_demo_generated_for_a_different_release(self) -> None:
        wrong_version = self.version + ".1"
        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaisesRegex(ValueError, "workspace version"):
                bundle_demo(self.root / "docs" / "demo" / "DemoNotebook", Path(temporary) / "demo.zip", wrong_version)


    def test_first_run_installs_only_the_html_edition_at_the_notebook_root(self) -> None:
        patch = (self.root / ".github" / "web-build" / "web.diff").read_text(encoding="utf-8")
        self.assertIn("with_welcome_notebook_setup(|_root, format| install_edition(format))", patch)
        self.assertIn('session.settings.set("privacy_welcome_seen", "true")', patch)
        self.assertIn('relative.strip_prefix(format)', patch)
        self.assertIn("Ok(Some(notebook.to_string_lossy().into_owned()))", patch)
        self.assertNotIn('Ok(Some(notebook.join("00-start/welcome/index.html")', patch)

if __name__ == "__main__":
    unittest.main()
