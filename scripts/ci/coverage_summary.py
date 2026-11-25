#!/usr/bin/env python3
"""Parse LCOV coverage output, check per-crate threshold, and emit status/summaries.

Evidence contract
-----------------
Line coverage is counted from ``DA`` records; ``LF``/``LH`` are summaries that
must agree with them. The producer (``cargo llvm-cov report --lcov``, i.e.
``llvm-cov export -format=lcov``) writes one ``DA`` per instrumented line and
then ``LF``/``LH`` equal to the unique ``DA`` line counts, so every rule below
holds for intact producer output:

* ``LF``/``LH`` present next to ``DA`` must equal the number of unique ``DA``
  lines / unique ``DA`` lines with a non-zero count. Disagreement is
  ``malformed_evidence``: the summary is never trusted over the line data.
* Repeated ``DA`` entries for one line (within a record, or across records
  for the same ``SF`` path) are one line whose counts are summed -- the
  merge rule ``lcov`` itself uses -- so a line is hit if any entry hit it.
* A record without ``DA`` may only declare zero lines (``LF:0``/``LH:0`` or
  nothing). A summary-only record claiming lines cannot be checked and is
  ``malformed_evidence``, never a measurement.
* Only known LCOV directives are accepted; anything else (garbage, an
  unknown tag, a line cut short) is ``malformed_evidence``.
* A failed generation step is ``measurement_failed`` even when a file exists;
  an absent/empty file or no lines for the target crate is
  ``missing_evidence``; a configuration or internal checker fault is
  ``checker_error``. Below-threshold coverage is advisory (exit 0).

LCOV has no trailer, so a file cut exactly at a record boundary is
indistinguishable from a complete one; truncation anywhere else is caught.

The step summary, ``GITHUB_OUTPUT`` and ``--json-out`` are all rendered from
one payload (``CoverageReport.to_dict``).
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass, field
import json
import math
import os
from pathlib import Path
import sys
from typing import Sequence


SCHEMA = "fission.coverage_summary.v1"


class EvidenceStatus:
    """Explicit diagnostic coverage status codes."""

    MEASURED_PASS = "measured_pass"
    MEASURED_BELOW_THRESHOLD = "measured_below_threshold"
    MISSING_EVIDENCE = "missing_evidence"
    MALFORMED_EVIDENCE = "malformed_evidence"
    MEASUREMENT_FAILED = "measurement_failed"
    CHECKER_ERROR = "checker_error"

    ALL = (
        MEASURED_PASS,
        MEASURED_BELOW_THRESHOLD,
        MISSING_EVIDENCE,
        MALFORMED_EVIDENCE,
        MEASUREMENT_FAILED,
        CHECKER_ERROR,
    )


class ConfigError(ValueError):
    """Raised when threshold or CLI configuration is invalid."""


class MalformedLcovError(ValueError):
    """Raised when LCOV input is truncated or syntactically invalid."""


@dataclass
class CrateCoverage:
    """Aggregated line coverage counts for one workspace crate."""

    total: int = 0
    hit: int = 0
    files: int = 0

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
    duplicate_da_merged: int = 0
    duplicate_source_records: int = 0
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
            "schema": SCHEMA,
            "status": self.status,
            "detail": self.detail,
            "target_crate": self.target_crate,
            "threshold_pct": self.threshold_pct,
            "target_hit": self.target_hit,
            "target_total": self.target_total,
            "target_pct": self.target_pct,
            "record_count": self.record_count,
            "duplicate_da_merged": self.duplicate_da_merged,
            "duplicate_source_records": self.duplicate_source_records,
            "crates": {
                name: {
                    "files": cov.files,
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
    text = raw.strip()
    # int() also accepts "+5", "1_000" and surrounding whitespace; LCOV counts
    # are plain decimal digits.
    if not text.isdigit() or not text.isascii():
        raise MalformedLcovError(f"line {line_no}: invalid count for {label}: {raw!r}")
    return int(text)


# Directives `llvm-cov export -format=lcov` and lcov/geninfo write. Only SF, DA,
# LF, LH and end_of_record carry line coverage; the rest are accepted and not
# interpreted. Anything else is not LCOV this checker understands.
_RECORD_DIRECTIVES = frozenset(
    {"FN", "FNDA", "FNF", "FNH", "FNL", "FNA", "BRDA", "BRF", "BRH", "MCDC", "DA", "LF", "LH"}
)
_TOP_LEVEL_DIRECTIVES = frozenset({"TN", "VER"})


@dataclass
class LcovSummary:
    """Line coverage per source file and crate, after consistency checks."""

    crates: dict[str, CrateCoverage]
    record_count: int
    duplicate_da_merged: int = 0
    duplicate_source_records: int = 0


def _close_record(
    *,
    line_no: int,
    source: str,
    da: dict[int, int],
    lf: int | None,
    lh: int | None,
) -> None:
    """Check one record's LF/LH against its unique DA lines."""
    if da:
        unique_found = len(da)
        unique_hit = sum(1 for hits in da.values() if hits > 0)
        if lf is not None and lf != unique_found:
            raise MalformedLcovError(
                f"line {line_no}: {source}: LF:{lf} disagrees with {unique_found} unique DA lines"
            )
        if lh is not None and lh != unique_hit:
            raise MalformedLcovError(
                f"line {line_no}: {source}: LH:{lh} disagrees with {unique_hit} unique "
                f"executed DA lines"
            )
        return
    if (lf or 0) != 0 or (lh or 0) != 0:
        raise MalformedLcovError(
            f"line {line_no}: {source}: summary LF:{lf} LH:{lh} without DA line data "
            "cannot be verified"
        )


def parse_lcov_summary(text: str) -> LcovSummary:
    """Strictly parse LCOV and aggregate unique-line counts by exact crate name."""
    # Source path -> line -> summed execution count, across all records.
    files: dict[str, dict[int, int]] = {}
    seen_sources: set[str] = set()
    record_count = 0
    duplicate_da_merged = 0
    duplicate_source_records = 0

    in_record = False
    source = ""
    da: dict[int, int] = {}
    lf: int | None = None
    lh: int | None = None

    for line_no, raw_line in enumerate(text.splitlines(), start=1):
        line = raw_line.strip()
        if not line:
            continue
        if line == "end_of_record":
            if not in_record:
                raise MalformedLcovError(f"line {line_no}: unexpected end_of_record")
            _close_record(line_no=line_no, source=source, da=da, lf=lf, lh=lh)
            record_count += 1
            if source in seen_sources:
                duplicate_source_records += 1
            seen_sources.add(source)
            merged = files.setdefault(source, {})
            for da_line, hits in da.items():
                merged[da_line] = merged.get(da_line, 0) + hits
            in_record = False
            continue

        tag, sep, value = line.partition(":")
        if not sep:
            raise MalformedLcovError(f"line {line_no}: not an LCOV directive: {line!r}")
        if tag in _TOP_LEVEL_DIRECTIVES:
            continue
        if tag == "SF":
            if in_record:
                raise MalformedLcovError(
                    f"line {line_no}: nested SF record without preceding end_of_record"
                )
            source = value.strip().replace("\\", "/")
            if not source:
                raise MalformedLcovError(f"line {line_no}: SF directive with an empty path")
            in_record = True
            da = {}
            lf = None
            lh = None
            continue
        if tag not in _RECORD_DIRECTIVES:
            raise MalformedLcovError(f"line {line_no}: unknown LCOV directive: {line!r}")
        if not in_record:
            raise MalformedLcovError(
                f"line {line_no}: directive outside SF..end_of_record: {line!r}"
            )

        if tag == "LF":
            if lf is not None:
                raise MalformedLcovError(f"line {line_no}: duplicate LF entry inside record")
            lf = _parse_non_negative_int(value, "LF", line_no)
        elif tag == "LH":
            if lh is not None:
                raise MalformedLcovError(f"line {line_no}: duplicate LH entry inside record")
            lh = _parse_non_negative_int(value, "LH", line_no)
        elif tag == "DA":
            # DA:<line>,<count>[,<checksum>]
            fields = value.split(",")
            if len(fields) not in (2, 3):
                raise MalformedLcovError(f"line {line_no}: malformed DA directive: {line!r}")
            da_line = _parse_non_negative_int(fields[0], "DA line", line_no)
            if da_line == 0:
                raise MalformedLcovError(f"line {line_no}: DA line numbers start at 1: {line!r}")
            hits = _parse_non_negative_int(fields[1], "DA hits", line_no)
            if da_line in da:
                duplicate_da_merged += 1
            da[da_line] = da.get(da_line, 0) + hits

    if in_record:
        raise MalformedLcovError("unterminated LCOV record at EOF (missing end_of_record)")

    crate_stats: dict[str, CrateCoverage] = {}
    for path in sorted(files):
        crate = crate_from_source_path(path)
        if crate is None:
            continue
        entry = crate_stats.setdefault(crate, CrateCoverage())
        lines = files[path]
        entry.files += 1
        entry.total += len(lines)
        entry.hit += sum(1 for hits in lines.values() if hits > 0)

    return LcovSummary(
        crates=crate_stats,
        record_count=record_count,
        duplicate_da_merged=duplicate_da_merged,
        duplicate_source_records=duplicate_source_records,
    )


def parse_lcov(text: str) -> tuple[dict[str, CrateCoverage], int]:
    """Compatibility wrapper: per-crate counts and the number of records."""
    summary = parse_lcov_summary(text)
    return summary.crates, summary.record_count


def evaluate_coverage(
    lcov_path: Path,
    target_crate: str,
    threshold_raw: str | None,
    generate_outcome: str = "success",
) -> CoverageReport:
    """Evaluate LCOV coverage for `target_crate` and return explicit status."""
    if generate_outcome != "success":
        # Whatever file a failed run left behind is not evidence.
        return CoverageReport(
            status=EvidenceStatus.MEASUREMENT_FAILED,
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
        parsed = parse_lcov_summary(text)
    except MalformedLcovError as exc:
        return CoverageReport(
            status=EvidenceStatus.MALFORMED_EVIDENCE,
            detail=str(exc),
            target_crate=target_crate,
            threshold_pct=threshold,
        )

    crate_stats = parsed.crates
    record_count = parsed.record_count
    provenance = {
        "record_count": record_count,
        "duplicate_da_merged": parsed.duplicate_da_merged,
        "duplicate_source_records": parsed.duplicate_source_records,
        "crates": crate_stats,
    }
    target_cov = crate_stats.get(target_crate)
    if record_count == 0 or target_cov is None or target_cov.total == 0:
        return CoverageReport(
            status=EvidenceStatus.MISSING_EVIDENCE,
            detail=f"no executable LCOV line data found for crate '{target_crate}'",
            target_crate=target_crate,
            threshold_pct=threshold,
            **provenance,
        )

    pct = target_cov.pct
    assert pct is not None
    if pct < threshold:
        status = EvidenceStatus.MEASURED_BELOW_THRESHOLD
        detail = (
            f"{target_crate} coverage {format_pct(pct)}% ({target_cov.hit}/{target_cov.total} lines) "
            f"is below advisory threshold {format_pct(threshold)}%"
        )
    else:
        status = EvidenceStatus.MEASURED_PASS
        detail = (
            f"{target_crate} coverage {format_pct(pct)}% ({target_cov.hit}/{target_cov.total} lines) "
            f"meets advisory threshold {format_pct(threshold)}%"
        )

    return CoverageReport(
        status=status,
        detail=detail,
        target_crate=target_crate,
        threshold_pct=threshold,
        target_hit=target_cov.hit,
        target_total=target_cov.total,
        target_pct=pct,
        **provenance,
    )


def format_pct(value: object) -> str:
    """The one percentage format shared by the summary and GITHUB_OUTPUT."""
    if value is None:
        return ""
    return f"{float(value):.2f}"  # type: ignore[arg-type]


def format_pct_cell(value: object) -> str:
    text = format_pct(value)
    return f"{text}%" if text else "n/a"


def _md_cell(value: object) -> str:
    return str(value).replace("|", "\\|").replace("\r", " ").replace("\n", " ")


def format_markdown_summary(report: CoverageReport | dict[str, object]) -> str:
    """Render the step summary from the same payload written as JSON."""
    payload = report.to_dict() if isinstance(report, CoverageReport) else report
    threshold = payload["threshold_pct"]
    target_pct = format_pct(payload["target_pct"])
    lines = [
        "## Code Coverage Report",
        "",
        f"- Status: `{payload['status']}`",
        f"- Target crate: `{payload['target_crate']}`",
        f"- Advisory threshold: {format_pct(threshold) + '%' if threshold is not None else 'n/a'}",
        f"- Target lines: {payload['target_hit']}/{payload['target_total']} lines "
        f"({target_pct + '%' if target_pct else 'not measured'})",
        f"- LCOV records: {payload['record_count']} "
        f"(merged duplicate DA lines: {payload['duplicate_da_merged']}, "
        f"repeated SF records: {payload['duplicate_source_records']})",
        f"- Detail: {_md_cell(payload['detail'])}",
        "",
    ]
    crates = payload["crates"]
    assert isinstance(crates, dict)
    if crates:
        lines.append("| Crate | Files | Lines Hit | Total Lines | Coverage % |")
        lines.append("|---|---:|---:|---:|---:|")
        for name, row in crates.items():
            lines.append(
                f"| `{name}` | {row['files']} | {row['hit']} | {row['total']} "
                f"| {format_pct_cell(row['pct'])} |"
            )
    else:
        lines.append("_No per-crate LCOV measurements recorded._")
    return "\n".join(lines) + "\n"


def _sanitize_output_value(value: str) -> str:
    return value.replace("\r", " ").replace("\n", " ").strip()


def write_github_output(path: Path, report: CoverageReport | dict[str, object]) -> None:
    """Append coverage status outputs to `GITHUB_OUTPUT` from the JSON payload."""
    payload = report.to_dict() if isinstance(report, CoverageReport) else report
    path.parent.mkdir(parents=True, exist_ok=True)
    payload_text = (
        f"coverage_status={_sanitize_output_value(str(payload['status']))}\n"
        f"coverage_detail={_sanitize_output_value(str(payload['detail']))}\n"
        f"target_pct={format_pct(payload['target_pct'])}\n"
    )
    with path.open("a", encoding="utf-8") as handle:
        handle.write(payload_text)


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
    try:
        report = evaluate_coverage(
            lcov_path=Path(args.lcov),
            target_crate=args.target_crate,
            threshold_raw=threshold_raw,
            generate_outcome=args.generate_outcome,
        )
    except Exception as exc:  # noqa: BLE001 - published as checker_error, not lost
        report = CoverageReport(
            status=EvidenceStatus.CHECKER_ERROR,
            detail=f"internal checker error: {type(exc).__name__}: {exc}",
            target_crate=args.target_crate,
            threshold_pct=None,
        )
    # Every published form below is rendered from this one payload.
    payload = report.to_dict()
    # The detail is single-line everywhere it is published.
    payload["detail"] = _sanitize_output_value(str(payload["detail"]))

    markdown = format_markdown_summary(payload)
    print(f"[{payload['status']}] {payload['detail']}")

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
        json_path.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")

    github_output = (
        args.github_output
        if args.github_output is not None
        else os.environ.get("GITHUB_OUTPUT", "")
    )
    if github_output:
        write_github_output(Path(github_output), payload)

    return report.exit_code()


if __name__ == "__main__":
    sys.exit(main())
