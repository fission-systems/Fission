#!/usr/bin/env python3
"""Local before/after decompilation observer for quality loops.

Purpose
-------
Make it cheap to **measure** a single-function decomp change without running the
full docker ranking harness:

  1. ``baseline``  — capture current CLI output (before a fix)
  2. ``after``     — re-capture with a (rebuilt) CLI into the same session
  3. ``show``      — print source (optional) + before + after + metric deltas

This is **local observation only**. It is not the official ranking oracle and
must not be promoted to dashboard / Pages. Full semantic claims still need the
external fission-benchmark path (see docs/BENCHMARK_DOCKER.md).

Examples
--------
  # Before editing:
  python3 scripts/quality/local_decomp_observe.py baseline \\
    --binary /path/to/bin.exe --addr 0x4015b0 \\
    --source /path/to/file.c --source-symbol sum_array

  # After rebuild:
  cargo build -p fission-cli --release
  python3 scripts/quality/local_decomp_observe.py after --session <id>
  python3 scripts/quality/local_decomp_observe.py show --session <id>

  # Import a hand-saved "before" snippet (when baseline was not captured):
  python3 scripts/quality/local_decomp_observe.py import-baseline \\
    --session <id> --nir-file before_nir.c --hir-file before_hir.c

Session artifacts live under (gitignored):
  benchmark/artifacts/local_observe/<session_id>/
"""

from __future__ import annotations

import argparse
import difflib
import json
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_OUT_ROOT = REPO_ROOT / "benchmark" / "artifacts" / "local_observe"
DEFAULT_CLI = REPO_ROOT / "target" / "release" / "fission_cli"


def utc_now() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def run_git(args: list[str]) -> str | None:
    try:
        out = subprocess.check_output(
            ["git", "-C", str(REPO_ROOT), *args],
            stderr=subprocess.DEVNULL,
            text=True,
        )
        return out.strip() or None
    except (subprocess.CalledProcessError, FileNotFoundError):
        return None


def resolve_cli(cli: Path | None) -> Path:
    path = Path(cli) if cli else DEFAULT_CLI
    if not path.is_file():
        raise SystemExit(
            f"fission_cli not found at {path}\n"
            f"Build first: cargo build -p fission-cli --release"
        )
    return path.resolve()


def parse_addr(raw: str) -> str:
    s = raw.strip().lower()
    if s.startswith("0x"):
        value = int(s, 16)
    else:
        value = int(s, 0)
    return f"0x{value:x}"


def extract_source_symbol(source_path: Path, symbol: str) -> str | None:
    """Best-effort extract of a C function body by symbol name."""
    text = source_path.read_text(encoding="utf-8", errors="replace")
    # Match `type name(...) {` at line start / after whitespace.
    pattern = re.compile(
        rf"(?ms)^[^\n{{;]*?\b{re.escape(symbol)}\s*\([^;]*?\)\s*\{{"
    )
    m = pattern.search(text)
    if not m:
        return None
    start = m.start()
    i = m.end() - 1  # at '{'
    depth = 0
    while i < len(text):
        ch = text[i]
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                return text[start : i + 1].rstrip() + "\n"
        i += 1
    return None


def code_metrics(code: str | None) -> dict[str, Any]:
    if not code:
        return {
            "lines": 0,
            "bytes": 0,
            "goto_count": 0,
            "label_count": 0,
            "for_count": 0,
            "while_count": 0,
            "do_count": 0,
            "if_count": 0,
            "return_count": 0,
            "uvar_count": 0,
            "has_for": False,
            "has_while": False,
            "has_do": False,
            "has_goto": False,
        }
    lines = code.count("\n") + (0 if code.endswith("\n") or not code else 1)
    goto_count = len(re.findall(r"\bgoto\b", code))
    label_count = len(re.findall(r"\bLAB_[A-Za-z0-9_]+\b", code))
    for_count = len(re.findall(r"\bfor\s*\(", code))
    while_count = len(re.findall(r"\bwhile\s*\(", code))
    do_count = len(re.findall(r"\bdo\b", code))
    if_count = len(re.findall(r"\bif\s*\(", code))
    return_count = len(re.findall(r"\breturn\b", code))
    uvar_count = len(re.findall(r"\buVar\d+\b", code))
    return {
        "lines": lines,
        "bytes": len(code.encode("utf-8")),
        "goto_count": goto_count,
        "label_count": label_count,
        "for_count": for_count,
        "while_count": while_count,
        "do_count": do_count,
        "if_count": if_count,
        "return_count": return_count,
        "uvar_count": uvar_count,
        "has_for": for_count > 0,
        "has_while": while_count > 0,
        "has_do": do_count > 0,
        "has_goto": goto_count > 0,
    }


def run_decomp(
    cli: Path,
    binary: Path,
    addr: str,
    *,
    layer: str = "both",
    timeout_s: float = 120.0,
) -> dict[str, Any]:
    cmd = [
        str(cli),
        "decomp",
        str(binary),
        "--addr",
        addr,
        "--layer",
        layer,
        "--json",
        "--no-header",
    ]
    proc = subprocess.run(
        cmd,
        capture_output=True,
        text=True,
        timeout=timeout_s,
        cwd=str(REPO_ROOT),
    )
    if proc.returncode != 0:
        raise SystemExit(
            f"decomp failed (exit {proc.returncode}):\n"
            f"cmd: {' '.join(cmd)}\n"
            f"stderr:\n{proc.stderr[-4000:]}"
        )
    # CLI may print non-JSON noise before the array; find first '['.
    stdout = proc.stderr + "\n" + proc.stdout if False else proc.stdout
    text = stdout.strip()
    # Drop log lines before JSON.
    bracket = text.find("[")
    brace = text.find("{")
    if bracket >= 0 and (brace < 0 or bracket <= brace):
        text = text[bracket:]
    elif brace >= 0:
        text = text[brace:]
    try:
        payload = json.loads(text)
    except json.JSONDecodeError as exc:
        raise SystemExit(
            f"failed to parse decomp JSON: {exc}\n"
            f"stdout head:\n{stdout[:2000]}"
        ) from exc
    if isinstance(payload, list):
        if not payload:
            raise SystemExit("decomp JSON list is empty")
        row = payload[0]
    elif isinstance(payload, dict):
        row = payload
    else:
        raise SystemExit(f"unexpected decomp JSON type: {type(payload)}")
    if not isinstance(row, dict):
        raise SystemExit("decomp row is not an object")
    return row


def session_id_for(binary: Path, addr: str, label: str | None) -> str:
    if label:
        slug = re.sub(r"[^A-Za-z0-9._-]+", "_", label).strip("_")
        if slug:
            return slug
    stem = re.sub(r"[^A-Za-z0-9._-]+", "_", binary.stem)[:40]
    addr_part = addr.replace("0x", "")
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    return f"{stem}_{addr_part}_{stamp}"


def resolve_session(out_root: Path, session: str) -> Path:
    p = Path(session)
    if p.is_dir():
        return p.resolve()
    candidate = out_root / session
    if candidate.is_dir():
        return candidate.resolve()
    # Allow creating new sessions only via baseline/import.
    return candidate


def write_json(path: Path, payload: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def capture_from_row(
    row: dict[str, Any],
    *,
    role: str,
    cli: Path | None,
    binary: Path | None,
    addr: str | None,
    source_text: str | None,
    source_path: str | None,
    note: str | None,
) -> dict[str, Any]:
    nir = row.get("code_nir") or (row.get("code") if row.get("layer") in (None, "nir", "both") else None)
    hir = row.get("code_hir")
    if not nir and row.get("code") and row.get("layer") == "hir":
        hir = row.get("code")
    if not nir and not hir:
        # Fall back to single code field.
        nir = row.get("code")
    capture = {
        "role": role,
        "captured_at": utc_now(),
        "git_head": run_git(["rev-parse", "HEAD"]),
        "git_short": run_git(["rev-parse", "--short", "HEAD"]),
        "git_dirty": bool(run_git(["status", "--porcelain"])),
        "cli": str(cli) if cli else None,
        "binary": str(binary) if binary else None,
        "addr": addr or row.get("address"),
        "function_name": row.get("name"),
        "engine_used": row.get("engine_used"),
        "layer": row.get("layer"),
        "note": note,
        "source_path": source_path,
        "source": source_text,
        "code_nir": nir,
        "code_hir": hir,
        "code": row.get("code"),
        "metrics_nir": code_metrics(nir if isinstance(nir, str) else None),
        "metrics_hir": code_metrics(hir if isinstance(hir, str) else None),
        "preview_build_stats": row.get("preview_build_stats"),
        "raw_keys": sorted(row.keys()),
    }
    return capture


def write_capture_dir(session_dir: Path, role: str, capture: dict[str, Any]) -> Path:
    role_dir = session_dir / role
    role_dir.mkdir(parents=True, exist_ok=True)
    write_json(role_dir / "capture.json", capture)
    if capture.get("code_nir"):
        (role_dir / "nir.c").write_text(str(capture["code_nir"]), encoding="utf-8")
    if capture.get("code_hir"):
        (role_dir / "hir.c").write_text(str(capture["code_hir"]), encoding="utf-8")
    if capture.get("source"):
        (role_dir / "source.c").write_text(str(capture["source"]), encoding="utf-8")
    # Also keep a copy at session root for the first baseline source.
    if role == "before" and capture.get("source"):
        (session_dir / "source.c").write_text(str(capture["source"]), encoding="utf-8")
    return role_dir


def load_capture(session_dir: Path, role: str) -> dict[str, Any]:
    path = session_dir / role / "capture.json"
    if not path.is_file():
        raise SystemExit(f"missing capture: {path}")
    return json.loads(path.read_text(encoding="utf-8"))


def metric_delta(before: dict[str, Any], after: dict[str, Any]) -> dict[str, Any]:
    keys = sorted(set(before) | set(after))
    out: dict[str, Any] = {}
    for key in keys:
        b = before.get(key)
        a = after.get(key)
        if isinstance(b, (int, float)) and isinstance(a, (int, float)):
            out[key] = {"before": b, "after": a, "delta": a - b}
        else:
            out[key] = {"before": b, "after": a, "delta": None}
    return out


def unified_diff(before: str | None, after: str | None, label: str) -> str:
    b = (before or "").splitlines(keepends=True)
    a = (after or "").splitlines(keepends=True)
    return "".join(
        difflib.unified_diff(
            b,
            a,
            fromfile=f"before/{label}",
            tofile=f"after/{label}",
            n=3,
        )
    )


def build_report(session_dir: Path) -> dict[str, Any]:
    before = load_capture(session_dir, "before")
    after = load_capture(session_dir, "after")
    report = {
        "session": str(session_dir),
        "generated_at": utc_now(),
        "function_name": after.get("function_name") or before.get("function_name"),
        "addr": after.get("addr") or before.get("addr"),
        "binary": after.get("binary") or before.get("binary"),
        "before_git": before.get("git_short"),
        "after_git": after.get("git_short"),
        "metrics_nir_delta": metric_delta(
            before.get("metrics_nir") or {}, after.get("metrics_nir") or {}
        ),
        "metrics_hir_delta": metric_delta(
            before.get("metrics_hir") or {}, after.get("metrics_hir") or {}
        ),
        "nir_changed": (before.get("code_nir") or "") != (after.get("code_nir") or ""),
        "hir_changed": (before.get("code_hir") or "") != (after.get("code_hir") or ""),
        "source": before.get("source") or after.get("source"),
        "source_path": before.get("source_path") or after.get("source_path"),
    }
    write_json(session_dir / "report.json", report)
    md = render_markdown(before, after, report)
    (session_dir / "report.md").write_text(md, encoding="utf-8")
    (session_dir / "diff_nir.patch").write_text(
        unified_diff(before.get("code_nir"), after.get("code_nir"), "nir.c"),
        encoding="utf-8",
    )
    (session_dir / "diff_hir.patch").write_text(
        unified_diff(before.get("code_hir"), after.get("code_hir"), "hir.c"),
        encoding="utf-8",
    )
    return report


def render_markdown(
    before: dict[str, Any], after: dict[str, Any], report: dict[str, Any]
) -> str:
    def fmt_metrics(delta: dict[str, Any], keys: list[str]) -> str:
        rows = ["| metric | before | after | delta |", "|---|---:|---:|---:|"]
        for key in keys:
            cell = delta.get(key) or {}
            rows.append(
                f"| `{key}` | {cell.get('before')} | {cell.get('after')} | {cell.get('delta')} |"
            )
        return "\n".join(rows)

    interesting = [
        "lines",
        "bytes",
        "goto_count",
        "label_count",
        "for_count",
        "while_count",
        "do_count",
        "if_count",
        "uvar_count",
    ]
    parts = [
        f"# Local decomp observe: {report.get('function_name') or '?'} @ {report.get('addr')}",
        "",
        f"- session: `{report.get('session')}`",
        f"- binary: `{report.get('binary')}`",
        f"- before git: `{report.get('before_git')}` → after git: `{report.get('after_git')}`",
        f"- nir_changed: **{report.get('nir_changed')}** · hir_changed: **{report.get('hir_changed')}**",
        f"- generated: {report.get('generated_at')}",
        "",
        "> Local observation only — not an official ranking / Pages result.",
        "",
    ]
    source = report.get("source")
    if source:
        parts += [
            "## 1. Source",
            "",
            f"_from_ `{report.get('source_path')}`",
            "",
            "```c",
            str(source).rstrip(),
            "```",
            "",
        ]
    else:
        parts += ["## 1. Source", "", "_(not provided — pass `--source` + `--source-symbol`)_", "",]

    parts += [
        "## 2. Before (baseline)",
        "",
        "### NIR",
        "",
        "```c",
        str(before.get("code_nir") or before.get("code") or "").rstrip(),
        "```",
        "",
        "### HIR",
        "",
        "```c",
        str(before.get("code_hir") or "").rstrip() or "_(none)_",
        "```",
        "",
        "## 3. After (remeasure)",
        "",
        "### NIR",
        "",
        "```c",
        str(after.get("code_nir") or after.get("code") or "").rstrip(),
        "```",
        "",
        "### HIR",
        "",
        "```c",
        str(after.get("code_hir") or "").rstrip() or "_(none)_",
        "```",
        "",
        "## Metrics (NIR)",
        "",
        fmt_metrics(report.get("metrics_nir_delta") or {}, interesting),
        "",
        "## Metrics (HIR)",
        "",
        fmt_metrics(report.get("metrics_hir_delta") or {}, interesting),
        "",
        "## Diff (NIR)",
        "",
        "```diff",
        unified_diff(before.get("code_nir"), after.get("code_nir"), "nir.c").rstrip()
        or "(no change)",
        "```",
        "",
    ]
    return "\n".join(parts) + "\n"


def print_show(session_dir: Path) -> None:
    report_md = session_dir / "report.md"
    if not report_md.is_file():
        if (session_dir / "before").is_dir() and (session_dir / "after").is_dir():
            build_report(session_dir)
        else:
            raise SystemExit(
                f"session incomplete: need before/ and after/ under {session_dir}"
            )
    sys.stdout.write(report_md.read_text(encoding="utf-8"))


def cmd_baseline(args: argparse.Namespace) -> int:
    cli = resolve_cli(Path(args.cli) if args.cli else None)
    binary = Path(args.binary).resolve()
    if not binary.is_file():
        raise SystemExit(f"binary not found: {binary}")
    addr = parse_addr(args.addr)
    out_root = Path(args.out_root).resolve()
    sid = args.session or session_id_for(binary, addr, args.label)
    session_dir = out_root / sid if not Path(sid).is_dir() else Path(sid)
    session_dir.mkdir(parents=True, exist_ok=True)

    source_text = None
    source_path = None
    if args.source:
        source_path = str(Path(args.source).resolve())
        if args.source_symbol:
            source_text = extract_source_symbol(Path(args.source), args.source_symbol)
            if source_text is None:
                print(
                    f"warning: symbol {args.source_symbol!r} not found in {args.source}",
                    file=sys.stderr,
                )
        else:
            source_text = Path(args.source).read_text(encoding="utf-8", errors="replace")

    row = run_decomp(cli, binary, addr, layer=args.layer, timeout_s=args.timeout)
    capture = capture_from_row(
        row,
        role="before",
        cli=cli,
        binary=binary,
        addr=addr,
        source_text=source_text,
        source_path=source_path,
        note=args.note,
    )
    write_capture_dir(session_dir, "before", capture)
    meta = {
        "session_id": session_dir.name,
        "session_dir": str(session_dir),
        "binary": str(binary),
        "addr": addr,
        "created_at": utc_now(),
        "cli": str(cli),
        "layer": args.layer,
        "source_path": source_path,
        "source_symbol": args.source_symbol,
    }
    write_json(session_dir / "meta.json", meta)
    print(f"baseline captured → {session_dir}")
    print(f"  function: {capture.get('function_name')} @ {addr}")
    print(f"  nir lines: {capture['metrics_nir']['lines']}  gotos: {capture['metrics_nir']['goto_count']}")
    print(f"next: rebuild CLI, then:")
    print(f"  python3 scripts/quality/local_decomp_observe.py after --session {session_dir.name}")
    print(f"  python3 scripts/quality/local_decomp_observe.py show --session {session_dir.name}")
    return 0


def cmd_after(args: argparse.Namespace) -> int:
    out_root = Path(args.out_root).resolve()
    session_dir = resolve_session(out_root, args.session)
    if not session_dir.is_dir():
        raise SystemExit(f"session not found: {args.session} (looked at {session_dir})")
    meta_path = session_dir / "meta.json"
    if not meta_path.is_file():
        raise SystemExit(f"missing meta.json in {session_dir}")
    meta = json.loads(meta_path.read_text(encoding="utf-8"))
    if not (session_dir / "before" / "capture.json").is_file():
        raise SystemExit(f"no baseline in session; run baseline/import-baseline first: {session_dir}")

    cli = resolve_cli(Path(args.cli) if args.cli else Path(meta["cli"]) if meta.get("cli") else None)
    binary = Path(args.binary).resolve() if args.binary else Path(meta["binary"])
    addr = parse_addr(args.addr) if args.addr else parse_addr(meta["addr"])
    layer = args.layer or meta.get("layer") or "both"

    source_text = None
    source_path = meta.get("source_path")
    if (session_dir / "source.c").is_file():
        source_text = (session_dir / "source.c").read_text(encoding="utf-8")
    elif source_path and meta.get("source_symbol") and Path(source_path).is_file():
        source_text = extract_source_symbol(Path(source_path), meta["source_symbol"])

    row = run_decomp(cli, binary, addr, layer=layer, timeout_s=args.timeout)
    capture = capture_from_row(
        row,
        role="after",
        cli=cli,
        binary=binary,
        addr=addr,
        source_text=source_text,
        source_path=source_path,
        note=args.note,
    )
    write_capture_dir(session_dir, "after", capture)
    report = build_report(session_dir)
    print(f"after captured → {session_dir}")
    print(f"  nir_changed={report['nir_changed']} hir_changed={report['hir_changed']}")
    mn = report["metrics_nir_delta"]
    for key in ("lines", "goto_count", "for_count", "do_count", "uvar_count"):
        cell = mn.get(key) or {}
        print(
            f"  nir.{key}: {cell.get('before')} → {cell.get('after')} (Δ {cell.get('delta')})"
        )
    print(f"report: {session_dir / 'report.md'}")
    print(f"show:   python3 scripts/quality/local_decomp_observe.py show --session {session_dir.name}")
    return 0


def cmd_import_baseline(args: argparse.Namespace) -> int:
    out_root = Path(args.out_root).resolve()
    sid = args.session or session_id_for(
        Path(args.binary) if args.binary else Path("manual"),
        parse_addr(args.addr) if args.addr else "0x0",
        args.label or "imported",
    )
    session_dir = out_root / sid if not Path(sid).is_dir() else Path(sid)
    session_dir.mkdir(parents=True, exist_ok=True)

    nir = Path(args.nir_file).read_text(encoding="utf-8") if args.nir_file else None
    hir = Path(args.hir_file).read_text(encoding="utf-8") if args.hir_file else None
    if args.code_file and not nir:
        nir = Path(args.code_file).read_text(encoding="utf-8")
    if not nir and not hir:
        raise SystemExit("import-baseline needs --nir-file and/or --hir-file (or --code-file)")

    source_text = None
    source_path = str(Path(args.source).resolve()) if args.source else None
    if args.source and args.source_symbol:
        source_text = extract_source_symbol(Path(args.source), args.source_symbol)
    elif args.source:
        source_text = Path(args.source).read_text(encoding="utf-8", errors="replace")

    row = {
        "name": args.function_name,
        "address": parse_addr(args.addr) if args.addr else None,
        "code_nir": nir,
        "code_hir": hir,
        "code": nir or hir,
        "layer": "both" if nir and hir else ("nir" if nir else "hir"),
        "engine_used": "imported",
    }
    capture = capture_from_row(
        row,
        role="before",
        cli=None,
        binary=Path(args.binary).resolve() if args.binary else None,
        addr=parse_addr(args.addr) if args.addr else None,
        source_text=source_text,
        source_path=source_path,
        note=args.note or "imported baseline (not live decomp)",
    )
    write_capture_dir(session_dir, "before", capture)
    meta = {
        "session_id": session_dir.name,
        "session_dir": str(session_dir),
        "binary": str(Path(args.binary).resolve()) if args.binary else None,
        "addr": parse_addr(args.addr) if args.addr else None,
        "created_at": utc_now(),
        "cli": str(resolve_cli(Path(args.cli) if args.cli else None)) if args.binary else None,
        "layer": "both",
        "source_path": source_path,
        "source_symbol": args.source_symbol,
        "imported": True,
    }
    write_json(session_dir / "meta.json", meta)
    print(f"imported baseline → {session_dir}")
    if args.binary and args.addr:
        print("next: capture live after with current CLI:")
        print(f"  python3 scripts/quality/local_decomp_observe.py after --session {session_dir.name}")
    return 0


def cmd_show(args: argparse.Namespace) -> int:
    out_root = Path(args.out_root).resolve()
    session_dir = resolve_session(out_root, args.session)
    if not session_dir.is_dir():
        raise SystemExit(f"session not found: {args.session}")
    print_show(session_dir)
    return 0


def cmd_list(args: argparse.Namespace) -> int:
    out_root = Path(args.out_root).resolve()
    if not out_root.is_dir():
        print(f"(no sessions under {out_root})")
        return 0
    sessions = sorted(p for p in out_root.iterdir() if p.is_dir())
    if not sessions:
        print(f"(no sessions under {out_root})")
        return 0
    for p in sessions:
        meta_path = p / "meta.json"
        has_before = (p / "before" / "capture.json").is_file()
        has_after = (p / "after" / "capture.json").is_file()
        status = (
            "complete"
            if has_before and has_after
            else "baseline-only"
            if has_before
            else "incomplete"
        )
        extra = ""
        if meta_path.is_file():
            meta = json.loads(meta_path.read_text(encoding="utf-8"))
            extra = f"  {meta.get('addr')}  {meta.get('binary')}"
        print(f"{p.name:40}  {status:13}{extra}")
    return 0


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        description="Local before/after decompilation observer (not official ranking).",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    p.add_argument(
        "--out-root",
        default=str(DEFAULT_OUT_ROOT),
        help=f"session root (default: {DEFAULT_OUT_ROOT})",
    )
    sub = p.add_subparsers(dest="cmd", required=True)

    b = sub.add_parser("baseline", help="Capture current decomp as before")
    b.add_argument("--binary", "-b", required=True)
    b.add_argument("--addr", "-a", required=True)
    b.add_argument("--cli", default=None, help="path to fission_cli")
    b.add_argument("--session", help="session id or directory name")
    b.add_argument("--label", help="friendly label used to build session id")
    b.add_argument("--source", help="optional original C source file")
    b.add_argument("--source-symbol", help="function name to extract from --source")
    b.add_argument("--layer", default="both", choices=["nir", "hir", "both"])
    b.add_argument("--timeout", type=float, default=120.0)
    b.add_argument("--note", default=None)
    b.set_defaults(func=cmd_baseline)

    a = sub.add_parser("after", help="Re-capture decomp as after and write report")
    a.add_argument("--session", "-s", required=True)
    a.add_argument("--binary", "-b", default=None)
    a.add_argument("--addr", default=None)
    a.add_argument("--cli", default=None)
    a.add_argument("--layer", default=None, choices=["nir", "hir", "both"])
    a.add_argument("--timeout", type=float, default=120.0)
    a.add_argument("--note", default=None)
    a.set_defaults(func=cmd_after)

    i = sub.add_parser(
        "import-baseline",
        help="Import a saved before snippet when live baseline was not captured",
    )
    i.add_argument("--session", help="session id")
    i.add_argument("--label", default=None)
    i.add_argument("--nir-file", default=None)
    i.add_argument("--hir-file", default=None)
    i.add_argument("--code-file", default=None, help="single-layer code if NIR/HIR unknown")
    i.add_argument("--binary", "-b", default=None, help="needed to run after")
    i.add_argument("--addr", "-a", default=None)
    i.add_argument("--cli", default=None)
    i.add_argument("--source", default=None)
    i.add_argument("--source-symbol", default=None)
    i.add_argument("--function-name", default=None)
    i.add_argument("--note", default=None)
    i.set_defaults(func=cmd_import_baseline)

    s = sub.add_parser("show", help="Print source + before + after report")
    s.add_argument("--session", "-s", required=True)
    s.set_defaults(func=cmd_show)

    l = sub.add_parser("list", help="List local observe sessions")
    l.set_defaults(func=cmd_list)

    return p


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    return int(args.func(args))


if __name__ == "__main__":
    raise SystemExit(main())
