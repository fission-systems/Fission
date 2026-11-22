#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Fail for new MkDocs warnings or a broken executable evaluation example."""

from __future__ import annotations

import argparse
from collections import Counter
import io
import os
from pathlib import Path
import re
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[2]
SNIPPET = re.compile(r"^<!-- cli-smoke: ([a-z-]+) -->\n\n```bash\n([\s\S]*?)^```$", re.M)
EXAMPLES = ["setup", "info", "list", "disasm", "decomp", "decomp-json"]


def site_warnings(root: Path, destination: Path) -> Counter[str]:
    build = subprocess.run(
        ["mkdocs", "build", "--site-dir", str(destination)],
        cwd=root, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        timeout=120,
    )
    if build.returncode:
        raise RuntimeError(build.stdout)
    diagnostics = Counter()
    for line in build.stdout.splitlines():
        match = re.match(r"^(WARNING|INFO)\s+-\s+(.*)$", line)
        if match:
            level, message = match.groups()
            if level == "WARNING" or (
                message.startswith("Doc file ") and " contains a link " in message
            ):
                diagnostics[message] += 1
    return diagnostics


def check_links(base: str) -> None:
    with tempfile.TemporaryDirectory(prefix="fission-docs-") as dirname:
        work = Path(dirname)
        base_tree = work / "base"
        base_tree.mkdir()
        files = subprocess.check_output(
            ["git", "archive", base, "--", "docs", "mkdocs.yml", "README.md", "CONTRIBUTING.md"],
            cwd=ROOT,
        )
        with tarfile.open(fileobj=io.BytesIO(files)) as archive:
            archive.extractall(base_tree, filter="data")
        baseline = site_warnings(base_tree, work / "base-site")
        current = site_warnings(ROOT, work / "current-site")
        added = current - baseline
        if added:
            raise SystemExit("\n".join(f"{count} × {message}" for message, count in sorted(added.items())))
        print(f"MkDocs passes with no new diagnostics ({sum(current.values())} existing)")


def check_examples(cli: Path) -> None:
    text = (ROOT / "docs/EVALUATION.md").read_text()
    commands = SNIPPET.findall(text)
    if [name for name, _ in commands] != EXAMPLES:
        raise SystemExit(f"Expected these evaluation blocks in order: {', '.join(EXAMPLES)}")
    subprocess.run(
        ["bash", "-euo", "pipefail"],
        input="\n".join(command for _, command in commands), text=True, cwd=ROOT,
        env={**os.environ, "FISSION_DOC_CLI": str(cli.resolve())}, check=True, timeout=120,
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base")
    parser.add_argument("--cli", type=Path)
    args = parser.parse_args()
    if not args.base and args.cli is None:
        parser.error("specify --base, --cli, or both")
    if args.base:
        check_links(args.base)
    if args.cli is not None:
        check_examples(args.cli)


if __name__ == "__main__":
    main()
