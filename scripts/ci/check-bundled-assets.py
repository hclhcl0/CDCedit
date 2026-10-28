#!/usr/bin/env python3
"""Check literal Rust/Slint bundled assets before expensive native builds."""
from pathlib import Path
import re
import sys

root = Path(__file__).resolve().parents[2]
missing = []
pattern = re.compile(
    r'(?:include_(?:bytes|str)!\s*\(\s*|@image-url\s*\(\s*|\bimport\s+)"([^"\n]+)"'
)
for crate in (root / "src/crates").iterdir():
    if not crate.is_dir():
        continue
    for source in crate.rglob("*"):
        if source.suffix not in {".rs", ".slint"}:
            continue
        for asset in pattern.findall(source.read_text(encoding="utf-8")):
            if asset.startswith(("http:", "https:")):
                continue
            if not (source.parent / asset).is_file():
                missing.append(f"{source.relative_to(root)}: {asset}")
if missing:
    print("Missing bundled assets:\n" + "\n".join(missing), file=sys.stderr)
    sys.exit(1)
print("All literal Rust and Slint bundled asset paths exist.")
