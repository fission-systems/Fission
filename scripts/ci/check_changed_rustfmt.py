#!/usr/bin/env python3
"""Check rustfmt only for Rust files changed by the current CI event."""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
EMPTY_TREE = "4b825dc642cb6eb9a060e54bf8d69288fbee4904"
ZERO_SHA = "0" * 40


def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()


def commit_exists(revision: str) -> bool:
    return (
        subprocess.run(
            ["git", "cat-file", "-e", f"{revision}^{{commit}}"],
            cwd=ROOT,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        ).returncode
        == 0
    )


def comparison_range() -> str:
    if len(sys.argv) > 1:
        return sys.argv[1]

    if os.environ.get("GITHUB_EVENT_NAME") == "pull_request":
        base_ref = os.environ.get("GITHUB_BASE_REF")
        if not base_ref:
            raise SystemExit("GITHUB_BASE_REF is required for pull_request formatting checks")
        subprocess.run(
            ["git", "fetch", "origin", base_ref, "--depth=1"],
            cwd=ROOT,
            check=True,
        )
        return f"{git('merge-base', 'HEAD', 'FETCH_HEAD')}..HEAD"

    before = os.environ.get("GITHUB_EVENT_BEFORE", "")
    if before and before != ZERO_SHA and commit_exists(before):
        return f"{before}..HEAD"
    if commit_exists("HEAD^"):
        return "HEAD^..HEAD"
    return f"{EMPTY_TREE}..HEAD"


def changed_rust_files(revision_range: str) -> list[Path]:
    output = git(
        "diff",
        "--name-only",
        "--diff-filter=ACMR",
        revision_range,
        "--",
        "*.rs",
    )
    return [ROOT / line for line in output.splitlines() if line]


def base_revision(revision_range: str) -> str:
    return revision_range.split("..", maxsplit=1)[0]


def baseline_is_formatted(rustfmt: str, revision: str, source: Path, edition: str) -> bool:
    relative = source.relative_to(ROOT).as_posix()
    baseline = subprocess.run(
        ["git", "show", f"{revision}:{relative}"],
        cwd=ROOT,
        capture_output=True,
        check=False,
    )
    if baseline.returncode != 0:
        return True

    with tempfile.NamedTemporaryFile(suffix=".rs") as temporary:
        temporary.write(baseline.stdout)
        temporary.flush()
        result = subprocess.run(
            [
                rustfmt,
                "--check",
                "--edition",
                edition,
                "--config",
                "skip_children=true",
                temporary.name,
            ],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
    return result.returncode == 0


def crate_edition(source: Path) -> str:
    for directory in (source.parent, *source.parents):
        manifest = directory / "Cargo.toml"
        if manifest.is_file():
            package = tomllib.loads(manifest.read_text(encoding="utf-8")).get("package", {})
            return str(package.get("edition", "2015"))
        if directory == ROOT:
            break
    raise SystemExit(f"no crate Cargo.toml found for {source.relative_to(ROOT)}")


def main() -> int:
    rustfmt = shutil.which("rustfmt")
    if rustfmt is None:
        raise SystemExit("rustfmt is not installed")

    revision_range = comparison_range()
    files = changed_rust_files(revision_range)
    if not files:
        print(f"No changed Rust files in {revision_range}")
        return 0

    print(f"Checking {len(files)} changed Rust files in {revision_range}")
    failed = False
    baseline = base_revision(revision_range)
    for source in files:
        edition = crate_edition(source)
        if baseline != EMPTY_TREE and not baseline_is_formatted(
            rustfmt, baseline, source, edition
        ):
            print(f"Skipping legacy rustfmt debt: {source.relative_to(ROOT)}")
            continue
        result = subprocess.run(
            [
                rustfmt,
                "--check",
                "--edition",
                edition,
                "--config",
                "skip_children=true",
                str(source),
            ],
            cwd=ROOT,
            check=False,
        )
        if result.returncode != 0:
            failed = True
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
