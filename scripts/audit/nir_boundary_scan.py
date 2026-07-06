#!/usr/bin/env python3
"""Report NIR owner-to-owner dependencies that block future crate splits."""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import asdict, dataclass
from pathlib import Path


NIR_ROOT = Path("crates/fission-pcode/src/nir")
TARGET_PATTERN = re.compile(
    r"crate::nir::(abi|abstract_location|action_pipeline|builder|cfg|cspec|normalize|pass|render|stats|"
    r"structuring|support|telemetry|types|var_rename|vsa)\b"
)

RULES: dict[str, dict[str, str]] = {
    "abi": {"builder": "violation", "normalize": "violation", "structuring": "violation", "render": "violation"},
    "abstract_location": {
        "builder": "violation",
        "normalize": "violation",
        "structuring": "violation",
        "render": "violation",
    },
    "action_pipeline": {
        "builder": "violation",
        "normalize": "migration",
        "structuring": "violation",
        "render": "violation",
    },
    "builder": {"normalize": "violation", "structuring": "migration", "render": "violation"},
    "cfg": {"builder": "violation", "normalize": "violation", "structuring": "violation", "render": "violation"},
    "cspec": {"builder": "violation", "normalize": "violation", "structuring": "violation", "render": "violation"},
    "normalize": {"builder": "violation", "structuring": "violation", "render": "violation"},
    "pass": {"builder": "migration", "normalize": "migration", "structuring": "migration", "render": "violation"},
    "render": {"builder": "violation", "normalize": "violation", "structuring": "migration"},
    "stats": {"builder": "violation", "normalize": "violation", "structuring": "violation", "render": "violation"},
    "structuring": {"builder": "migration", "normalize": "migration", "render": "violation"},
    "support": {"builder": "violation", "normalize": "violation", "structuring": "migration", "render": "violation"},
    "telemetry": {"builder": "violation", "normalize": "violation", "structuring": "violation", "render": "violation"},
    "types": {"builder": "violation", "normalize": "violation", "structuring": "violation", "render": "violation"},
    "var_rename": {"builder": "violation", "normalize": "violation", "structuring": "violation", "render": "violation"},
    "vsa": {"builder": "violation", "normalize": "migration", "structuring": "violation", "render": "violation"},
}


@dataclass(frozen=True)
class Finding:
    severity: str
    source_layer: str
    target_layer: str
    path: str
    line: int
    detail: str


def source_layer(path: Path) -> str | None:
    try:
        rel = path.relative_to(NIR_ROOT)
    except ValueError:
        return None
    if len(rel.parts) == 1:
        return rel.stem
    return rel.parts[0]


def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8", errors="replace")


def iter_files(root: Path) -> list[Path]:
    base = root / NIR_ROOT
    files = []
    for path in base.rglob("*.rs"):
        rel = path.relative_to(root).as_posix()
        if "/tests/" in rel or "test" in path.stem:
            continue
        files.append(path)
    return sorted(files)


def scan(root: Path) -> list[Finding]:
    findings: list[Finding] = []
    for path in iter_files(root):
        src = source_layer(path.relative_to(root))
        if src is None:
            continue
        rel = path.relative_to(root).as_posix()
        rules = RULES.get(src, {})
        for lineno, line in enumerate(read_text(path).splitlines(), start=1):
            if "nir_boundary_allow" in line:
                continue
            seen: set[str] = set()
            for match in TARGET_PATTERN.finditer(line):
                target = match.group(1)
                if target == src or target in seen:
                    continue
                seen.add(target)
                severity = rules.get(target)
                if not severity:
                    continue
                detail = (
                    "Owner-to-owner dependency should be moved through substrate facts."
                    if severity == "violation"
                    else "Known boundary debt; do not copy this pattern for new fixes."
                )
                findings.append(Finding(severity, src, target, rel, lineno, detail))
    return findings


def render_markdown(root: Path, findings: list[Finding]) -> str:
    counts: dict[str, int] = {}
    for finding in findings:
        counts[finding.severity] = counts.get(finding.severity, 0) + 1
    lines = [
        "# NIR Boundary Scan",
        "",
        f"- Repo: `{root}`",
        f"- Findings: `{len(findings)}`",
        f"- Violations: `{counts.get('violation', 0)}`",
        f"- Migration debt: `{counts.get('migration', 0)}`",
        "",
    ]
    if findings:
        lines += ["| Severity | Edge | Location | Detail |", "|---|---|---|---|"]
        for finding in findings:
            edge = f"{finding.source_layer} -> {finding.target_layer}"
            lines.append(f"| `{finding.severity}` | `{edge}` | `{finding.path}:{finding.line}` | {finding.detail} |")
    else:
        lines.append("No NIR boundary findings.")
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path("."))
    parser.add_argument("--format", choices=["text", "json", "markdown"], default="text")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--fail-on-violation", action="store_true")
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

    if args.fail_on_violation and any(f.severity == "violation" for f in findings):
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
