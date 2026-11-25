#!/usr/bin/env python3
"""Write the resource bundle manifest, `fission-resources.json` (#1208).

  resource_manifest.py <bundle_dir> [--tag vX.Y.Z] [--sha <commit>] [--root .]

`<bundle_dir>` holds sleigh-specs/, signatures/ and ghidra-data/ (an
archive's share/fission/, or the fission-resources.tar.gz staging dir). The
manifest names the release it was cut from (the Fission version from
crates/fission-cli/Cargo.toml, the tag, the commit) and, per directory, the
file count and a SHA-256 over the sorted `<path>\\0<sha256 of file>` lines.

`fission_cli resources status` compares the identity with the binary's own
(fission_core::resource_manifest); the digests are for a release-time or
install-time check, never for every invocation. The tag and commit default to
FISSION_BUILD_TAG / FISSION_BUILD_SHA, the variables the release build embeds.
Stdlib only.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
from pathlib import Path

MANIFEST = "fission-resources.json"
SCHEMA = 1
RESOURCE_DIRS = ("sleigh-specs", "signatures", "ghidra-data")


def cargo_version(root: Path) -> str:
    text = (root / "crates" / "fission-cli" / "Cargo.toml").read_text(encoding="utf-8")
    m = re.search(r'^\[package\][^\[]*?^version\s*=\s*"([^"]+)"', text, re.MULTILINE | re.DOTALL)
    if not m:
        raise SystemExit("resource_manifest: no [package] version in crates/fission-cli/Cargo.toml")
    return m.group(1)


def digest_dir(path: Path) -> dict:
    lines = []
    for file in sorted(p for p in path.rglob("*") if p.is_file()):
        rel = file.relative_to(path).as_posix()
        lines.append(f"{rel}\0{hashlib.sha256(file.read_bytes()).hexdigest()}\n")
    return {
        "files": len(lines),
        "sha256": hashlib.sha256("".join(lines).encode("utf-8")).hexdigest(),
    }


def build(bundle: Path, version: str, tag: str | None, sha: str | None) -> dict:
    missing = [d for d in RESOURCE_DIRS if not (bundle / d).is_dir()]
    if missing:
        raise SystemExit(f"resource_manifest: {bundle} lacks {', '.join(missing)}")
    return {
        "schema": SCHEMA,
        "fission_version": version,
        "tag": tag or None,
        "sha": sha or None,
        "resources": {d: digest_dir(bundle / d) for d in RESOURCE_DIRS},
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("bundle", type=Path)
    parser.add_argument("--tag", default=os.environ.get("FISSION_BUILD_TAG"))
    parser.add_argument("--sha", default=os.environ.get("FISSION_BUILD_SHA"))
    parser.add_argument("--root", type=Path, default=Path("."), help="repository root")
    args = parser.parse_args(argv)
    manifest = build(args.bundle, cargo_version(args.root), args.tag, args.sha)
    (args.bundle / MANIFEST).write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(f"resource_manifest: {args.bundle / MANIFEST} ({manifest['fission_version']}, {manifest['tag']}, {manifest['sha']})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
