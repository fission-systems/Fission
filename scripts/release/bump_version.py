#!/usr/bin/env python3
"""Set the version of every workspace crate, Cargo.lock and the docs nav.

  bump_version.py --version 0.5.0 [--root .]

Fission versions every member crate in lockstep (`version = "X.Y.Z"` in each
`[package]`; there is no `[workspace.package]`). This edits, as text:

* the `[package]` version of each member listed in the root Cargo.toml;
* the matching `[[package]]` entries in Cargo.lock, so `--locked` builds
  accept the tree without a resolve;
* the "Release Notes" list in mkdocs.yml, adding `changelog/vX.Y.Z.md`.

Idempotent: running it twice with the same version changes nothing the
second time. Stdlib only.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

SEMVER_RE = re.compile(
    r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(-[\da-zA-Z-]+(\.[\da-zA-Z-]+)*)?$"
)


def members(root: Path) -> list[Path]:
    text = (root / "Cargo.toml").read_text(encoding="utf-8")
    m = re.search(r"^members\s*=\s*\[(.*?)\]", text, re.MULTILINE | re.DOTALL)
    if not m:
        raise SystemExit("bump_version: no [workspace] members in Cargo.toml")
    return [root / p for p in re.findall(r'"([^"]+)"', m.group(1))]


def set_package_version(manifest: Path, version: str) -> str:
    """Rewrite the first `version =` of the `[package]` table; return the crate name."""
    lines = manifest.read_text(encoding="utf-8").split("\n")
    section = None
    name = None
    done = False
    for i, line in enumerate(lines):
        header = re.match(r"^\s*\[([^\]]+)\]\s*$", line)
        if header:
            section = header.group(1).strip()
            continue
        if section != "package":
            continue
        if name is None:
            nm = re.match(r'^name\s*=\s*"([^"]+)"', line)
            if nm:
                name = nm.group(1)
        if not done and re.match(r'^version\s*=\s*"[^"]*"', line):
            lines[i] = re.sub(r'"[^"]*"', f'"{version}"', line, count=1)
            done = True
    if name is None or not done:
        raise SystemExit(f"bump_version: {manifest} has no [package] name/version")
    manifest.write_text("\n".join(lines), encoding="utf-8")
    return name


def set_lock_versions(lock: Path, names: set[str], version: str) -> int:
    blocks = lock.read_text(encoding="utf-8").split("\n[[package]]\n")
    changed = 0
    for i, blk in enumerate(blocks):
        nm = re.search(r'^name = "([^"]+)"$', blk, re.MULTILINE)
        if nm and nm.group(1) in names and "\nsource = " not in f"\n{blk}":
            blocks[i] = re.sub(
                r'^version = "[^"]*"$', f'version = "{version}"', blk, count=1, flags=re.MULTILINE
            )
            changed += 1
    lock.write_text("\n[[package]]\n".join(blocks), encoding="utf-8")
    return changed


def add_nav_entry(mkdocs: Path, version: str) -> None:
    lines = mkdocs.read_text(encoding="utf-8").split("\n")
    entry = f"changelog/v{version}.md"
    if any(line.strip().endswith(entry) for line in lines):
        return
    for i, line in enumerate(lines):
        m = re.match(r"^(\s*)- Fission [^:]+: changelog/v[^ ]+\.md\s*$", line)
        if m:
            lines.insert(i, f"{m.group(1)}- Fission {version}: {entry}")
            mkdocs.write_text("\n".join(lines), encoding="utf-8")
            return
    raise SystemExit("bump_version: no 'Fission X.Y.Z: changelog/...' entry in mkdocs.yml")


def bump(root: Path, version: str) -> list[str]:
    version = version.removeprefix("v")
    if not SEMVER_RE.match(version):
        raise SystemExit(f"bump_version: {version!r} is not a SemVer version")
    names = {set_package_version(m / "Cargo.toml", version) for m in members(root)}
    locked = set_lock_versions(root / "Cargo.lock", names, version)
    if locked != len(names):
        raise SystemExit(
            f"bump_version: Cargo.lock has {locked} of {len(names)} workspace crates"
        )
    if (root / "mkdocs.yml").is_file():
        add_nav_entry(root / "mkdocs.yml", version)
    return sorted(names)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--version", required=True)
    parser.add_argument("--root", default=".")
    args = parser.parse_args(argv)
    names = bump(Path(args.root), args.version)
    print(f"bump_version: {len(names)} crates at {args.version.removeprefix('v')}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
