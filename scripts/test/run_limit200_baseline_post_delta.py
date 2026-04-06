#!/usr/bin/env python3
"""Run putty/everything decomp batches and emit summary/delta artifacts.

This script standardizes the local workflow used in recent NIR rounds:
- run rust-sleigh batch decomp (--decomp-all --decomp-limit N)
- snapshot /tmp unsupported inventory files per run
- generate summary json/md
- generate putty unmapped cluster json/md
- generate baseline-vs-current delta json/md
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
from collections import Counter, defaultdict
from dataclasses import dataclass
from pathlib import Path
from typing import Any


TMP_UNSUPPORTED_GLOB = "fission_preview_*_unsupported.json"
TMP_PREVIEW_LOG_GLOB = "fission_preview_*.log"
MARKER = "__fission_indirect_cf_unsupported()"


@dataclass
class RunConfig:
    name: str
    binary_path: Path
    baseline_summary_path: Path | None


def resolve_baseline_summary(
    repo_root: Path,
    name: str,
    limit: int,
    explicit_path: Path,
) -> Path | None:
    candidate = (repo_root / explicit_path).resolve()
    if candidate.exists():
        return candidate

    fallbacks = [
        repo_root / f"artifacts/local/{name}_limit{limit}_summary_rebuilt.json",
        repo_root / f"artifacts/local/{name}_limit{limit}_summary_after_term.json",
        repo_root / f"artifacts/local/{name}_limit{limit}_summary_after_passthrough.json",
    ]
    for path in fallbacks:
        if path.exists():
            return path.resolve()
    return None


def read_json(path: Path) -> Any:
    return json.loads(path.read_text())


def write_json(path: Path, payload: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2) + "\n")


def write_markdown(path: Path, lines: list[str]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines) + "\n")


def clean_tmp_preview_files(tmp_dir: Path) -> None:
    for pattern in (TMP_UNSUPPORTED_GLOB, TMP_PREVIEW_LOG_GLOB):
        for fp in tmp_dir.glob(pattern):
            if fp.is_file():
                fp.unlink()


def run_decomp_batch(
    cli_bin: Path,
    binary_path: Path,
    limit: int,
    out_json: Path,
    out_log: Path,
    debug_env: bool,
) -> None:
    out_json.parent.mkdir(parents=True, exist_ok=True)
    out_log.parent.mkdir(parents=True, exist_ok=True)

    env = os.environ.copy()
    if debug_env:
        env["FISSION_PREVIEW_DEBUG"] = "1"
        env["FISSION_PREVIEW_DIAG"] = "1"

    cmd = [
        str(cli_bin),
        str(binary_path),
        "--engine",
        "rust-sleigh",
        "--decomp-all",
        "--decomp-limit",
        str(limit),
        "--json",
        "-o",
        str(out_json),
    ]

    with out_log.open("w") as log_file:
        subprocess.run(cmd, check=True, env=env, stdout=log_file, stderr=subprocess.STDOUT)


def snapshot_inventory(tmp_dir: Path, out_dir: Path) -> int:
    if out_dir.exists():
        shutil.rmtree(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    count = 0
    for fp in tmp_dir.glob(TMP_UNSUPPORTED_GLOB):
        if fp.is_file():
            shutil.copy2(fp, out_dir / fp.name)
            count += 1
    return count


def summarize_run(post_json_path: Path, run_log_path: Path, inventory_dir: Path) -> dict[str, Any]:
    rows = read_json(post_json_path)
    engine_counts = Counter(row.get("engine_used") for row in rows)
    fell_back = sum(1 for row in rows if row.get("fell_back"))

    marker_entries = 0
    marker_occurrences = 0
    marker_addrs: set[str] = set()
    for row in rows:
        code = row.get("code") or ""
        occ = code.count(MARKER)
        if occ:
            marker_entries += 1
            marker_occurrences += occ
            marker_addrs.add((row.get("address") or "").lower())

    inventory_files = sorted(inventory_dir.glob(TMP_UNSUPPORTED_GLOB))
    inventory_stage_counts: Counter[str] = Counter()
    inventory_function_files = 0
    inventory_block_starts: set[int] = set()
    inventory_addrs: set[str] = set()

    for fp in inventory_files:
        m = re.match(r"fission_preview_([0-9a-fA-F]+)_unsupported\.json$", fp.name)
        if not m:
            continue
        inventory_addrs.add(f"0x{m.group(1).lower()}")
        events = read_json(fp)
        if events:
            inventory_function_files += 1
        for event in events:
            stage = event.get("stage")
            if stage:
                inventory_stage_counts[stage] += 1
            block_start = event.get("block_start")
            if isinstance(block_start, int):
                inventory_block_starts.add(block_start)

    log_text = run_log_path.read_text(errors="ignore")
    cfg_warn_counts = {
        "cbranch_true_target_unmapped": len(re.findall(r"\bcbranch_true_target_unmapped\b", log_text)),
        "branch_target_unmapped": len(re.findall(r"\bbranch_target_unmapped\b", log_text)),
        "control_block_no_successors": len(re.findall(r"\bcontrol_block_no_successors\b", log_text)),
    }

    return {
        "rows": len(rows),
        "fell_back": fell_back,
        "engine_counts": dict(engine_counts),
        "marker_entries": marker_entries,
        "marker_occurrences": marker_occurrences,
        "inventory_files": len(inventory_files),
        "inventory_stage_counts": dict(inventory_stage_counts),
        "inventory_function_files": inventory_function_files,
        "inventory_block_starts": len(inventory_block_starts),
        "marker_without_inventory_file": len(marker_addrs - inventory_addrs),
        "inventory_file_without_marker": len(inventory_addrs - marker_addrs),
        "cfg_warn_counts": cfg_warn_counts,
    }


def summarize_to_markdown(title: str, summary: dict[str, Any]) -> list[str]:
    lines = [f"# {title}", ""]
    for key in (
        "rows",
        "fell_back",
        "marker_entries",
        "marker_occurrences",
        "inventory_files",
        "inventory_function_files",
        "inventory_block_starts",
        "marker_without_inventory_file",
        "inventory_file_without_marker",
    ):
        lines.append(f"- {key}: {summary.get(key)}")

    lines.extend(["", "## engine_counts"])
    for key, value in sorted(summary.get("engine_counts", {}).items()):
        lines.append(f"- {key}: {value}")

    lines.extend(["", "## inventory_stage_counts"])
    for key, value in sorted(
        summary.get("inventory_stage_counts", {}).items(), key=lambda item: (-item[1], item[0])
    ):
        lines.append(f"- {key}: {value}")

    lines.extend(["", "## cfg_warn_counts"])
    for key, value in summary.get("cfg_warn_counts", {}).items():
        lines.append(f"- {key}: {value}")

    return lines


def make_delta_payload(baseline: dict[str, Any], current: dict[str, Any]) -> dict[str, Any]:
    scalar_keys = (
        "fell_back",
        "marker_entries",
        "marker_occurrences",
        "inventory_files",
        "inventory_function_files",
        "inventory_block_starts",
        "marker_without_inventory_file",
        "inventory_file_without_marker",
    )
    delta = {key: current.get(key, 0) - baseline.get(key, 0) for key in scalar_keys}

    baseline_stage = Counter(baseline.get("inventory_stage_counts", {}))
    current_stage = Counter(current.get("inventory_stage_counts", {}))
    delta_stage = {
        key: current_stage[key] - baseline_stage[key]
        for key in sorted(set(baseline_stage) | set(current_stage))
        if current_stage[key] - baseline_stage[key] != 0
    }

    baseline_cfg = Counter(baseline.get("cfg_warn_counts", {}))
    current_cfg = Counter(current.get("cfg_warn_counts", {}))
    delta_cfg = {
        key: current_cfg[key] - baseline_cfg[key]
        for key in sorted(set(baseline_cfg) | set(current_cfg))
        if current_cfg[key] - baseline_cfg[key] != 0
    }

    return {
        "delta": delta,
        "delta_inventory_stage_counts": delta_stage,
        "delta_cfg_warn_counts": delta_cfg,
    }


def delta_to_markdown(title: str, payload: dict[str, Any]) -> list[str]:
    lines = [f"# {title}", "", "## delta"]
    for key, value in payload.get("delta", {}).items():
        lines.append(f"- {key}: {value:+d}")

    lines.extend(["", "## delta_inventory_stage_counts"])
    if payload.get("delta_inventory_stage_counts"):
        for key, value in payload["delta_inventory_stage_counts"].items():
            lines.append(f"- {key}: {value:+d}")
    else:
        lines.append("- (no change)")

    lines.extend(["", "## delta_cfg_warn_counts"])
    if payload.get("delta_cfg_warn_counts"):
        for key, value in payload["delta_cfg_warn_counts"].items():
            lines.append(f"- {key}: {value:+d}")
    else:
        lines.append("- (no change)")

    return lines


def build_putty_unmapped_cluster(run_log_path: Path) -> dict[str, Any]:
    lines = run_log_path.read_text(errors="ignore").splitlines()
    start_re = re.compile(r"\[CFG\-DIAG\]\s+start\s+entry=(0x[0-9a-fA-F]+)")
    event_re = re.compile(
        r"\[CFG\-DIAG\]\s+(branch_target_unmapped|cbranch_true_target_unmapped|control_block_no_successors).*?target=(0x[0-9a-fA-F]+)"
    )

    current_fn: str | None = None
    fn_events: dict[str, list[tuple[str, str]]] = defaultdict(list)
    global_targets: Counter[str] = Counter()

    for line in lines:
        start_match = start_re.search(line)
        if start_match:
            current_fn = start_match.group(1).lower()
            continue

        event_match = event_re.search(line)
        if current_fn and event_match:
            kind = event_match.group(1)
            target = event_match.group(2).lower()
            fn_events[current_fn].append((kind, target))
            global_targets[target] += 1

    top_functions: list[dict[str, Any]] = []
    total_unmapped_events = 0
    for fn, events in fn_events.items():
        if not events:
            continue
        total_unmapped_events += len(events)
        kind_counts = Counter(kind for kind, _ in events)
        target_counts = Counter(target for _, target in events)
        top_functions.append(
            {
                "function": fn,
                "total_unmapped": len(events),
                "cbranch_true_target_unmapped": kind_counts.get("cbranch_true_target_unmapped", 0),
                "branch_target_unmapped": kind_counts.get("branch_target_unmapped", 0),
                "control_block_no_successors": kind_counts.get("control_block_no_successors", 0),
                "top_targets": [
                    {"target": target, "count": count}
                    for target, count in target_counts.most_common(5)
                ],
            }
        )

    top_functions.sort(key=lambda row: (-row["total_unmapped"], row["function"]))

    return {
        "source_log": str(run_log_path),
        "functions_with_unmapped": len([1 for events in fn_events.values() if events]),
        "total_unmapped_events": total_unmapped_events,
        "top_targets_global": [
            {"target": target, "count": count} for target, count in global_targets.most_common(20)
        ],
        "top_functions": top_functions[:20],
    }


def putty_cluster_to_markdown(cluster: dict[str, Any]) -> list[str]:
    lines = ["# putty unmapped cluster", ""]
    lines.append(f"- source_log: {cluster['source_log']}")
    lines.append(f"- functions_with_unmapped: {cluster['functions_with_unmapped']}")
    lines.append(f"- total_unmapped_events: {cluster['total_unmapped_events']}")

    lines.extend(["", "## top_targets_global"])
    for row in cluster["top_targets_global"]:
        lines.append(f"- {row['target']}: {row['count']}")

    lines.extend(["", "## top_functions"])
    for row in cluster["top_functions"]:
        lines.append(
            "- "
            f"{row['function']}: total={row['total_unmapped']} "
            f"cbranch={row['cbranch_true_target_unmapped']} "
            f"branch={row['branch_target_unmapped']} "
            f"no_succ={row['control_block_no_successors']}"
        )

    return lines


def run_single_target(
    repo_root: Path,
    tmp_dir: Path,
    cli_bin: Path,
    limit: int,
    tag: str,
    out_dir: Path,
    config: RunConfig,
    debug_env: bool,
) -> None:
    post_json = out_dir / f"{config.name}_limit{limit}_post_{tag}.json"
    run_log = out_dir / f"{config.name}_limit{limit}_post_{tag}.run.log"
    inventory_dir = out_dir / f"{config.name}_inventory_{tag}"

    clean_tmp_preview_files(tmp_dir)
    run_decomp_batch(cli_bin, config.binary_path, limit, post_json, run_log, debug_env)
    inventory_count = snapshot_inventory(tmp_dir, inventory_dir)

    summary = summarize_run(post_json, run_log, inventory_dir)
    summary_json = out_dir / f"{config.name}_limit{limit}_summary_{tag}.json"
    summary_md = out_dir / f"{config.name}_limit{limit}_summary_{tag}.md"
    write_json(summary_json, summary)
    write_markdown(summary_md, summarize_to_markdown(f"{config.name} limit{limit} summary {tag}", summary))

    if config.name == "putty":
        cluster = build_putty_unmapped_cluster(run_log)
        cluster["source_log"] = str(run_log.relative_to(repo_root))
        cluster_json = out_dir / f"putty_unmapped_cluster_{tag}.json"
        cluster_md = out_dir / f"putty_unmapped_cluster_{tag}.md"
        write_json(cluster_json, cluster)
        write_markdown(cluster_md, putty_cluster_to_markdown(cluster))

    if config.baseline_summary_path and config.baseline_summary_path.exists():
        baseline = read_json(config.baseline_summary_path)
        delta_payload = make_delta_payload(baseline, summary)
        delta_payload["baseline"] = str(config.baseline_summary_path.relative_to(repo_root))
        delta_payload["current"] = str(summary_json.relative_to(repo_root))

        delta_json = out_dir / f"{config.name}_limit{limit}_delta_{tag}.json"
        delta_md = out_dir / f"{config.name}_limit{limit}_delta_{tag}.md"
        write_json(delta_json, delta_payload)
        write_markdown(
            delta_md,
            delta_to_markdown(f"{config.name} limit{limit} delta {tag}", delta_payload),
        )

    print(
        f"[{config.name}] rows={summary['rows']} inventory_files={inventory_count} "
        f"marker_entries={summary['marker_entries']} fell_back={summary['fell_back']}"
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run putty/everything baseline-post-delta measurement in one command."
    )
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--cli-bin", type=Path, default=Path("target/debug/fission_cli"))
    parser.add_argument("--putty-bin", type=Path, default=Path("samples/windows/x64/putty.exe"))
    parser.add_argument(
        "--everything-bin", type=Path, default=Path("samples/windows/x64/everything.exe")
    )
    parser.add_argument("--out-dir", type=Path, default=Path("artifacts/local"))
    parser.add_argument("--limit", type=int, default=200)
    parser.add_argument("--tag", type=str, default="auto")
    parser.add_argument(
        "--baseline-putty-summary",
        type=Path,
        default=Path("artifacts/local/putty_limit200_summary_rebuilt.json"),
    )
    parser.add_argument(
        "--baseline-everything-summary",
        type=Path,
        default=Path("artifacts/local/everything_limit200_summary_rebuilt.json"),
    )
    parser.add_argument(
        "--no-debug-env",
        action="store_true",
        help="Do not set FISSION_PREVIEW_DEBUG/FISSION_PREVIEW_DIAG during run",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()

    repo_root = args.repo_root.resolve()
    os.chdir(repo_root)

    cli_bin = (repo_root / args.cli_bin).resolve()
    putty_bin = (repo_root / args.putty_bin).resolve()
    everything_bin = (repo_root / args.everything_bin).resolve()
    out_dir = (repo_root / args.out_dir).resolve()

    if not cli_bin.exists():
        raise FileNotFoundError(f"missing cli binary: {cli_bin}")
    if not putty_bin.exists():
        raise FileNotFoundError(f"missing putty binary: {putty_bin}")
    if not everything_bin.exists():
        raise FileNotFoundError(f"missing everything binary: {everything_bin}")

    tmp_dir = Path("/tmp")
    tag = args.tag
    debug_env = not args.no_debug_env

    baseline_putty = resolve_baseline_summary(
        repo_root,
        "putty",
        args.limit,
        args.baseline_putty_summary,
    )
    baseline_everything = resolve_baseline_summary(
        repo_root,
        "everything",
        args.limit,
        args.baseline_everything_summary,
    )

    targets = [
        RunConfig(
            name="putty",
            binary_path=putty_bin,
            baseline_summary_path=baseline_putty,
        ),
        RunConfig(
            name="everything",
            binary_path=everything_bin,
            baseline_summary_path=baseline_everything,
        ),
    ]

    for target in targets:
        run_single_target(
            repo_root=repo_root,
            tmp_dir=tmp_dir,
            cli_bin=cli_bin,
            limit=args.limit,
            tag=tag,
            out_dir=out_dir,
            config=target,
            debug_env=debug_env,
        )

    print("done: generated post/summary/delta artifacts for putty and everything")


if __name__ == "__main__":
    main()
