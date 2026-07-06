#!/usr/bin/env python3
"""Report architecture-specific references in architecture-independent code."""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import asdict, dataclass
from pathlib import Path


SCAN_ROOTS = (
    "crates/fission-pcode/src/nir/normalize",
    "crates/fission-pcode/src/nir/structuring",
    "crates/fission-pcode/src/nir/types",
)

REGISTER_PATTERN = re.compile(
    r"\b(?:r(?:ax|bx|cx|dx|si|di|bp|sp|8|9|1[0-5])|e(?:ax|bx|cx|dx|si|di|bp|sp)|"
    r"(?:a|b|c|d)[lh]|r1[0-5]|r[0-9]|x[0-9]|w[0-9]|sp|lr|pc)\b"
)
ARCH_PATTERN = re.compile(r"\b(?:x86|amd64|x86_64|arm64|aarch64|arm|mips|ppc|powerpc|riscv)\b", re.I)


@dataclass(frozen=True)
class Finding:
    kind: str
    path: str
    line: int
    token: str
    detail: str


def read_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except UnicodeDecodeError:
        return path.read_text(encoding="utf-8", errors="replace")


def iter_files(root: Path) -> list[Path]:
    files: list[Path] = []
    for rel in SCAN_ROOTS:
        base = root / rel
        if not base.exists():
            continue
        files.extend(
            path
            for path in base.rglob("*.rs")
            if path.is_file() and "test" not in path.stem and "/tests/" not in path.relative_to(root).as_posix()
        )
    return sorted(files)


def quoted_or_comment_context(line: str, token: str) -> bool:
    token_index = line.find(token)
    if token_index < 0:
        return False
    before = line[:token_index]
    return "//" in before or before.count('"') % 2 == 1 or before.count("'") % 2 == 1


def scan(root: Path) -> list[Finding]:
    findings: list[Finding] = []
    for path in iter_files(root):
        rel = path.relative_to(root).as_posix()
        for lineno, line in enumerate(read_text(path).splitlines(), start=1):
            if "arch_isolation_allow" in line:
                continue
            stripped = line.strip()
            if stripped.startswith("//!") or stripped.startswith("///"):
                continue
            seen_tokens: set[tuple[str, str]] = set()
            for match in REGISTER_PATTERN.finditer(line):
                token = match.group(0)
                if len(token) <= 2 and not quoted_or_comment_context(line, token):
                    continue
                key = ("register_name", token)
                if key in seen_tokens:
                    continue
                seen_tokens.add(key)
                findings.append(
                    Finding(
                        "register_name",
                        rel,
                        lineno,
                        token,
                        "Raw register name in architecture-independent NIR code.",
                    )
                )
            for match in ARCH_PATTERN.finditer(line):
                token = match.group(0)
                key = ("architecture_name", token)
                if key in seen_tokens:
                    continue
                seen_tokens.add(key)
                findings.append(
                    Finding(
                        "architecture_name",
                        rel,
                        lineno,
                        token,
                        "Architecture name in architecture-independent NIR code.",
                    )
                )
    return findings


def render_markdown(root: Path, findings: list[Finding]) -> str:
    lines = [
        "# Architecture Isolation Scan",
        "",
        f"- Repo: `{root}`",
        f"- Findings: `{len(findings)}`",
        "",
    ]
    if findings:
        lines += ["| Kind | Location | Token | Detail |", "|---|---|---|---|"]
        for finding in findings:
            lines.append(f"| `{finding.kind}` | `{finding.path}:{finding.line}` | `{finding.token}` | {finding.detail} |")
    else:
        lines.append("No architecture-specific tokens found in scanned owner-neutral paths.")
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path("."))
    parser.add_argument("--format", choices=["text", "json", "markdown"], default="text")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--fail-on-finding", action="store_true")
    args = parser.parse_args()

    root = args.root.resolve()
    findings = scan(root)
    if args.format == "json":
        rendered = json.dumps({"root": str(root), "findings": [asdict(f) for f in findings]}, indent=2, sort_keys=True)
    else:
        rendered = render_markdown(root, findings)
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(rendered, encoding="utf-8")
    else:
        print(rendered, end="")
    return 1 if args.fail_on_finding and findings else 0


if __name__ == "__main__":
    sys.exit(main())
