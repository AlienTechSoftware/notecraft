from __future__ import annotations

import argparse
import re
import zipfile
from pathlib import Path


REQUIRED_FILES = {
    "html": ("_node.toml", "00-start/welcome/index.html"),
    "cmgfm": ("_node.toml", "00-start/welcome/index.md"),
}


def bundle_demo(source: Path, destination: Path, version: str) -> None:
    html_source = source / "html"
    cmgfm_source = source / "cmgfm"
    for required_root in (html_source, cmgfm_source):
        if not (required_root / "_node.toml").is_file():
            raise ValueError(f"official demo format is missing metadata: {required_root}")
        language_folders = [path for path in required_root.iterdir() if path.is_dir() and re.fullmatch(r"(?:1\d|2\d)-[a-z]{2}", path.name)]
        if len(language_folders) != 13:
            raise ValueError(f"official {required_root.name} demo has {len(language_folders)} language folders, expected 13")
    for format_name, required_files in REQUIRED_FILES.items():
        format_root = source / format_name
        for relative in required_files:
            if not (format_root / relative).is_file():
                raise ValueError(f"official {format_name} demo is missing {relative}")

    expected_title = f"Welcome to NoteCraft (Ver {version})"
    for format_name, required_files in REQUIRED_FILES.items():
        page = (source / format_name / required_files[1]).read_text(encoding="utf-8")
        if version not in page:
            raise ValueError(f"{format_name} Welcome page does not contain workspace version {version}")
        if expected_title not in page:
            raise ValueError(f"official {format_name} Welcome page does not name version {version}")

    destination.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(destination, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=6) as bundle:
        for format_name in REQUIRED_FILES:
            format_root = source / format_name
            for path in sorted(format_root.rglob("*")):
                if path.is_file():
                    bundle.write(path, (Path(format_name) / path.relative_to(format_root)).as_posix())


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("source", type=Path)
    parser.add_argument("destination", type=Path)
    parser.add_argument("--version", required=True)
    args = parser.parse_args()
    if not re.fullmatch(r"\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?", args.version):
        parser.error("--version must be a semantic version")
    bundle_demo(args.source, args.destination, args.version)
    print(f"Bundled both official Welcome notebook formats v{args.version} at {args.destination}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
