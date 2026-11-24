#!/usr/bin/env python3
"""Refuse a release whose tag, binary and resource manifest disagree (#1208).

  check_identity.py source   --tag vX.Y.Z [--root .]
      The tag, without its `v`, equals the [package] version of
      crates/fission-cli/Cargo.toml. A prerelease tag (v0.5.0-rc.1) needs the
      same prerelease version in Cargo.toml. Run before any build.

  check_identity.py status   --tag vX.Y.Z --sha <commit> [--version X.Y.Z] < status.json
      The JSON of `fission_cli resources status --json` from a packaged
      binary: its embedded build tag and commit are the release's, and the
      resource manifest it resolved is in state "match" with the same tag and
      commit.

  check_identity.py manifest <fission-resources.json> --tag vX.Y.Z --sha <commit> [--version X.Y.Z]
      A manifest file on its own (the separate resource bundle).

Exit status 1 lists every disagreement. Stdlib only.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from resource_manifest import SCHEMA, cargo_version  # noqa: E402


def check_source(tag: str, root: Path) -> list[str]:
    version = cargo_version(root)
    if tag.removeprefix("v") != version:
        return [f"tag {tag} is not crates/fission-cli/Cargo.toml version {version}"]
    return []


def expect(problems: list[str], what: str, actual, wanted) -> None:
    if wanted is not None and actual != wanted:
        problems.append(f"{what} is {actual!r}, expected {wanted!r}")


def check_manifest(manifest: dict, tag: str, sha: str, version: str | None) -> list[str]:
    problems: list[str] = []
    expect(problems, "manifest schema", manifest.get("schema"), SCHEMA)
    expect(problems, "manifest tag", manifest.get("tag"), tag)
    expect(problems, "manifest commit", manifest.get("sha"), sha)
    expect(problems, "manifest version", manifest.get("fission_version"), version)
    return problems


def check_status(status: dict, tag: str, sha: str, version: str | None) -> list[str]:
    problems: list[str] = []
    build = status.get("build") or {}
    expect(problems, "binary tag", build.get("tag"), tag)
    expect(problems, "binary commit", build.get("sha"), sha)
    expect(problems, "binary version", build.get("version"), version)
    manifest = status.get("resource_manifest") or {}
    expect(problems, "resource manifest state", manifest.get("state"), "match")
    expect(problems, "resource manifest tag", manifest.get("tag"), tag)
    expect(problems, "resource manifest commit", manifest.get("sha"), sha)
    expect(problems, "resource manifest version", manifest.get("version"), version)
    if manifest.get("state") != "match" and manifest.get("detail"):
        problems.append(f"resource manifest: {manifest['detail']}")
    return problems


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = parser.add_subparsers(dest="cmd", required=True)
    p_source = sub.add_parser("source")
    p_source.add_argument("--tag", required=True)
    p_source.add_argument("--root", type=Path, default=Path("."))
    for name in ("status", "manifest"):
        p = sub.add_parser(name)
        if name == "manifest":
            p.add_argument("file", type=Path)
        p.add_argument("--tag", required=True)
        p.add_argument("--sha", required=True)
        p.add_argument("--version")
    args = parser.parse_args(argv)

    if args.cmd == "source":
        problems = check_source(args.tag, args.root)
    elif args.cmd == "status":
        problems = check_status(json.load(sys.stdin), args.tag, args.sha, args.version)
    else:
        manifest = json.loads(args.file.read_text(encoding="utf-8"))
        problems = check_manifest(manifest, args.tag, args.sha, args.version)

    for problem in problems:
        print(f"check_identity {args.cmd}: {problem}", file=sys.stderr)
    if not problems:
        print(f"check_identity {args.cmd}: {args.tag} agrees")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
