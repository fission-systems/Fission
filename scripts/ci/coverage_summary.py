#!/usr/bin/env python3
"""Parse LCOV coverage output, check per-crate threshold, and emit status/summaries."""

from __future__ import annotations

import argparse
from dataclasses import dataclass, field
import json
import math
import os
from pathlib import Path
import sys
from typing import Sequence


class EvidenceStatus:
    """Explicit diagnostic coverage status codes."""

    MEASURED_PASS = "measured_pass"
    MEASURED_BELOW_THRESHOLD = "measured_below_threshold"
    MISSING_EVIDENCE = "missing_evidence"
    MALFORMED_EVIDENCE = "malformed_evidence"
    CHECKER_ERROR = "checker_error"


class ConfigError(ValueError):
    """Raised when threshold or CLI configuration is invalid."""


class MalformedLcovError(ValueError):
    """Raised when LCOV input is truncated or syntactically invalid."""


@dataclass
class CrateCoverage:
    """Aggregated line coverage counts for one workspace crate."""

    total: int = 0
    hit: int = 0

    @property
    def pct(self) -> float | None:
        if self.total == 0:
            return None
        return (self.hit / self.total) * 100.0


@dataclass
class CoverageReport:
    """Complete evaluation outcome for one LCOV summary run."""

    status: str
    detail: str
    target_crate: str
    threshold_pct: float | None
    target_hit: int = 0
    target_total: int = 0
    target_pct: float | None = None
    record_count: int = 0
    crates: dict[str, CrateCoverage] = field(default_factory=dict)

    def exit_code(self) -> int:
        if self.status in (
            EvidenceStatus.MEASURED_PASS,
            EvidenceStatus.MEASURED_BELOW_THRESHOLD,
        ):
            return 0
        return 2

    def to_dict(self) -> dict[str, object]:
        return {
            "status": self.status,
            "detail": self.detail,
            "target_crate": self.target_crate,
            "threshold_pct": self.threshold_pct,
            "target_hit": self.target_hit,
            "target_total": self.target_total,
            "target_pct": self.target_pct,
            "record_count": self.record_count,
            "crates": {
                name: {
                    "hit": cov.hit,
                    "total": cov.total,
                    "pct": cov.pct,
                }
                for name, cov in sorted(self.crates.items())
            },
        }


def parse_threshold(raw: str | None) -> float:
    """Parse and validate a percentage threshold in [0.0, 100.0]."""
    if raw is None or not str(raw).strip():
        raise ConfigError("missing warn threshold percentage (FISSION_IN_WARN_THRESHOLD_PCT)")
    text = str(raw).strip()
    try:
        value = float(text)
    except ValueError as exc:
        raise ConfigError(f"invalid warn threshold percentage: {text!r}") from exc
    if not math.isfinite(value) or value < 0.0 or value > 100.0:
        raise ConfigError(f"warn threshold percentage out of [0, 100]: {text!r}")
    return value


def crate_from_source_path(source_path: str) -> str | None:
    """Extract exact `<crate>` from `<repo>/crates/<crate>/...` paths."""
    normalized = source_path.strip().replace("\\", "/")
    if not normalized:
        return None
    parts = [p for p in normalized.split("/") if p]
    for idx in range(len(parts) - 2, -1, -1):
        if parts[idx] == "crates" and idx + 1 < len(parts):
            crate = parts[idx + 1]
            return crate if crate else None
    return None


def _parse_non_negative_int(raw: str, label: str, line_no: int) -> int:
    try:
        val = int(raw.strip())
    except ValueError as exc:
        raise MalformedLcovError(f"line {line_no}: invalid integer for {label}: {raw!r}") from exc
    if val < 0:
        raise MalformedLcovError(f"line {line_no}: negative count for {label}: {val}")
    return val


def parse_lcov(text: str) -> tuple[dict[str, CrateCoverage], int]:
    """Strictly parse LCOV records and aggregate counts by exact crate name."""
    crate_stats: dict[str, CrateCoverage] = {}
    in_record = False
    current_crate: str | None = None
    lf: int | None = None
    lh: int | None = None
    da_found = 0
    da_hit = 0
    record_count = 0

    for line_no, raw_line in enumerate(text.splitlines(), start=1):
        line = raw_line.strip()
        if not line:
            continue
        if line.startswith("TN:"):
            continue
        if line.startswith("SF:"):
            if in_record:
                raise MalformedLcovError(
                    f"line {line_no}: nested SF record without preceding end_of_record"
                )
            in_record = True
            current_crate = crate_from_source_path(line[3:])
            lf = None
            lh = None
            da_found = 0
            da_hit = 0
            continue
        if line == "end_of_record":
            if not in_record:
                raise MalformedLcovError(f"line {line_no}: unexpected end_of_record")
            total = lf if lf is not None else da_found
            hit = lh if lh is not None else da_hit
            if hit > total:
                raise MalformedLcovError(
                    f"line {line_no}: hit count ({hit}) exceeds found count ({total})"
                )
            record_count += 1
            if current_crate is not None:
                entry = crate_stats.setdefault(current_crate, CrateCoverage())
                entry.total += total
                entry.hit += hit
            in_record = False
            current_crate = None
            lf = None
            lh = None
            da_found = 0
            da_hit = 0
            continue

        if not in_record:
            raise MalformedLcovError(f"line {line_no}: directive outside SF..end_of_record: {line!r}")

        if line.startswith("LF:"):
            if lf is not None:
                raise MalformedLcovError(f"line {line_no}: duplicate LF entry inside record")
            lf = _parse_non_negative_int(line[3:], "LF", line_no)
        elif line.startswith("LH:"):
            if lh is not None:
                raise MalformedLcovError(f"line {line_no}: duplicate LH entry inside record")
            lh = _parse_non_negative_int(line[3:], "LH", line_no)
        elif line.startswith("DA:"):
            fields = line[3:].split(",")
            if len(fields) < 2:
                raise MalformedLcovError(f"line {line_no}: malformed DA directive: {line!r}")
            _parse_non_negative_int(fields[0], "DA line", line_no)
            hits = _parse_non_negative_int(fields[1], "DA hits", line_no)
            da_found += 1
            if hits > 0:
                da_hit += 1

    if in_record:
        raise MalformedLcovError("unterminated LCOV record at EOF (missing end_of_record)")

    return crate_stats, record_count


def evaluate_coverage(
    lcov_path: Path,
    target_crate: str,
    threshold_raw: str | None,
    generate_outcome: str = "success",
) -> CoverageReport:
    """Evaluate LCOV coverage for `target_crate` and return explicit status."""
    if generate_outcome != "success":
        return CoverageReport(
            status=EvidenceStatus.CHECKER_ERROR,
            detail=f"coverage generation did not succeed (outcome={generate_outcome})",
            target_crate=target_crate,
            threshold_pct=None,
        )

    try:
        threshold = parse_threshold(threshold_raw)
    except ConfigError as exc:
        return CoverageReport(
            status=EvidenceStatus.CHECKER_ERROR,
            detail=str(exc),
            target_crate=target_crate,
            threshold_pct=None,
        )

    if not lcov_path.exists():
        return CoverageReport(
            status=EvidenceStatus.MISSING_EVIDENCE,
            detail=f"LCOV file not found at {lcov_path}",
            target_crate=target_crate,
            threshold_pct=threshold,
        )

    try:
        text = lcov_path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as exc:
        return CoverageReport(
            status=EvidenceStatus.MALFORMED_EVIDENCE,
            detail=f"failed to read LCOV file {lcov_path}: {exc}",
            target_crate=target_crate,
            threshold_pct=threshold,
        )

    if not text.strip():
        return CoverageReport(
            status=EvidenceStatus.MISSING_EVIDENCE,
            detail=f"LCOV file {lcov_path} is empty",
            target_crate=target_crate,
            threshold_pct=threshold,
        )

    try:
        crate_stats, record_count = parse_lcov(text)
    except MalformedLcovError as exc:
        return CoverageReport(
            status=EvidenceStatus.MALFORMED_EVIDENCE,
            detail=str(exc),
            target_crate=target_crate,
            threshold_pct=threshold,
        )

    target_cov = crate_stats.get(target_crate)
    if record_count == 0 or target_cov is None or target_cov.total == 0:
        return CoverageReport(
            status=EvidenceStatus.MISSING_EVIDENCE,
            detail=f"no executable LCOV line data found for crate '{target_crate}'",
            target_crate=target_crate,
            threshold_pct=threshold,
            record_count=record_count,
            crates=crate_stats,
        )

    pct = target_cov.pct
    assert pct is not None
    if pct < threshold:
        status = EvidenceStatus.MEASURED_BELOW_THRESHOLD
        detail = (
            f"{target_crate} coverage {pct:.1f}% ({target_cov.hit}/{target_cov.total} lines) "
            f"is below advisory threshold {threshold:.1f}%"
        )
    else:
        status = EvidenceStatus.MEASURED_PASS
        detail = (
            f"{target_crate} coverage {pct:.1f}% ({target_cov.hit}/{target_cov.total} lines) "
            f"meets threshold {threshold:.1f}%"
        )

    return CoverageReport(
        status=status,
        detail=detail,
        target_crate=target_crate,
        threshold_pct=threshold,
        target_hit=target_cov.hit,
        target_total=target_cov.total,
        target_pct=pct,
        record_count=record_count,
        crates=crate_stats,
    )


def format_markdown_summary(report: CoverageReport) -> str:
    """Format diagnostic coverage status and per-crate table as Markdown."""
    lines = [
        "## Code Coverage Report",
        "",
        f"- Status: `{report.status}`",
        f"- Target crate: `{report.target_crate}`",
        f"- Detail: {report.detail}",
        "",
    ]
    rows = [(name, cov) for name, cov in sorted(report.crates.items()) if cov.total > 0]
    if rows:
        lines.append("| Crate | Lines Hit | Total Lines | Coverage % |")
        lines.append("|---|---:|---:|---:|")
        for name, cov in rows:
            pct = cov.pct
            pct_str = f"{pct:.1f}%" if pct is not None else "N/A"
            lines.append(f"| `{name}` | {cov.hit} | {cov.total} | {pct_str} |")
    else:
        lines.append("_No per-crate LCOV measurements recorded._")
    return "\n".join(lines) + "\n"


def _sanitize_output_value(value: str) -> str:
    return value.replace("\r", " ").replace("\n", " ").strip()


def write_github_output(path: Path, report: CoverageReport) -> None:
    """Append coverage status outputs to `GITHUB_OUTPUT`."""
    path.parent.mkdir(parents=True, exist_ok=True)
    pct_str = f"{report.target_pct:.2f}" if report.target_pct is not None else ""
    payload = (
        f"coverage_status={_sanitize_output_value(report.status)}\n"
        f"coverage_detail={_sanitize_output_value(report.detail)}\n"
        f"target_pct={pct_str}\n"
    )
    with path.open("a", encoding="utf-8") as handle:
        handle.write(payload)


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Summarize and check LCOV code coverage.")
    parser.add_argument("--lcov", default="coverage/lcov.info", help="Path to LCOV info file")
    parser.add_argument(
        "--target-crate",
        default="fission-pcode",
        help="Exact crate directory name under crates/ to evaluate",
    )
    parser.add_argument(
        "--threshold",
        default=None,
        help="Advisory warn percentage threshold (defaults to FISSION_IN_WARN_THRESHOLD_PCT)",
    )
    parser.add_argument(
        "--generate-outcome",
        default="success",
        help="Outcome of the upstream coverage generation step",
    )
    parser.add_argument("--json-out", default=None, help="Optional JSON summary output path")
    parser.add_argument(
        "--step-summary",
        default=None,
        help="Markdown summary path (defaults to GITHUB_STEP_SUMMARY)",
    )
    parser.add_argument(
        "--github-output",
        default=None,
        help="GitHub Actions output file (defaults to GITHUB_OUTPUT)",
    )
    args = parser.parse_args(list(argv) if argv is not None else None)

    threshold_raw = (
        args.threshold
        if args.threshold is not None
        else os.environ.get("FISSION_IN_WARN_THRESHOLD_PCT")
    )
    report = evaluate_coverage(
        lcov_path=Path(args.lcov),
        target_crate=args.target_crate,
        threshold_raw=threshold_raw,
        generate_outcome=args.generate_outcome,
    )

    markdown = format_markdown_summary(report)
    print(f"[{report.status}] {report.detail}")

    step_summary = (
        args.step_summary
        if args.step_summary is not None
        else os.environ.get("GITHUB_STEP_SUMMARY", "")
    )
    if step_summary:
        summary_path = Path(step_summary)
        summary_path.parent.mkdir(parents=True, exist_ok=True)
        with summary_path.open("a", encoding="utf-8") as handle:
            handle.write(markdown)
    else:
        print(markdown, end="")

    if args.json_out:
        json_path = Path(args.json_out)
        json_path.parent.mkdir(parents=True, exist_ok=True)
        json_path.write_text(json.dumps(report.to_dict(), indent=2) + "\n", encoding="utf-8")

    github_output = (
        args.github_output
        if args.github_output is not None
        else os.environ.get("GITHUB_OUTPUT", "")
    )
    if github_output:
        write_github_output(Path(github_output), report)

    return report.exit_code()


if __name__ == "__main__":
    sys.exit(main())
