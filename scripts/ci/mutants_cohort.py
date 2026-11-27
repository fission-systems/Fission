#!/usr/bin/env python3
"""Immutable-cohort mutation testing scheduler, reconciler, and diagnostic reporter.

Replaces moving-window weekly shard rotation (Issue #1251) with immutable
cohorts pinned to explicit (base_sha, head_sha, diff_sha256, tool_version,
nextest_version, test_profile, sharding_method, shard_count).

Design principles:
- Report-only: missed mutants are diagnostic leads, never a blanket fail gate.
- No silent completion over moving populations: each shard outcome reconciles
  strictly against its immutable cohort's mutant IDs; unattempted mutants stay
  explicitly `not_run` and expired/evicted cohorts never claim completion.
- Bounded backlog: active cohorts are capped (`MAX_ACTIVE_COHORTS`) and aged out
  (`MAX_COHORT_AGE_DAYS`) strictly within `MANIFEST_RETENTION_DAYS` (90 days),
  so compact cohort manifests outlive every cohort they track.
- Honest accounting: `timeout`, `unviable`, `not_run`, and `infra_error` are
  never counted as `caught` (killed).
"""

from __future__ import annotations

import argparse
from datetime import datetime, timedelta, timezone
import hashlib
import json
import math
from pathlib import Path
import sys
from typing import Any

SCHEMA_VERSION = 1
POLICY_MODE = "immutable_cohort_bounded_backlog"
CARGO_MUTANTS_VERSION = "27.1.0"
CARGO_NEXTEST_VERSION = "0.9.136"
SHARDING_METHOD = "round-robin"
SHARDS_PER_RUN = 4
MAX_SHARDS_PER_COHORT = 48
TARGET_MUTANTS_PER_SHARD = 120
MAX_ACTIVE_COHORTS = 3
MAX_COHORT_AGE_DAYS = 42
MANIFEST_RETENTION_DAYS = 90
MAX_SHARD_RETRIES = 2

ACTIVE_COHORT_STATUSES = frozenset({"pending", "in_progress"})
TERMINAL_MUTANT_STATUSES = frozenset({"caught", "missed", "timeout", "unviable"})
RUNNABLE_SHARD_STATUSES = frozenset({"pending", "failed_retryable"})

DEFAULT_TEST_PROFILE: dict[str, Any] = {
    "test_tool": "nextest",
    "features": [],
    "timeout_multiplier": 3,
    "jobs": 2,
    "path_filters": ["crates/**/*.rs", ":!crates/**/tests/**"],
}


def utc_now() -> datetime:
    return datetime.now(timezone.utc)


def format_utc(dt: datetime) -> str:
    return dt.astimezone(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def parse_utc(value: str) -> datetime:
    normalized = value[:-1] + "+00:00" if value.endswith("Z") else value
    dt = datetime.fromisoformat(normalized)
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return dt.astimezone(timezone.utc)


def empty_state(now: datetime | None = None) -> dict[str, Any]:
    ts = format_utc(now or utc_now())
    return {
        "schema_version": SCHEMA_VERSION,
        "policy_mode": POLICY_MODE,
        "updated_at": ts,
        "cohorts": [],
    }


def _canonical_json(obj: Any) -> str:
    return json.dumps(obj, sort_keys=True, separators=(",", ":"), ensure_ascii=True)


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def mutant_id_for_entry(entry: dict[str, Any]) -> str:
    """Derive a deterministic, unique mutant ID from a cargo-mutants JSON entry."""
    span = entry.get("span") or {}
    start = span.get("start") or {}
    end = span.get("end") or {}
    payload = {
        "package": entry.get("package", ""),
        "file": entry.get("file", ""),
        "start_line": int(start.get("line", entry.get("line", 0)) or 0),
        "start_col": int(start.get("column", entry.get("column", 0)) or 0),
        "end_line": int(end.get("line", 0) or 0),
        "end_col": int(end.get("column", 0) or 0),
        "genre": entry.get("genre", ""),
        "replacement": entry.get("replacement", ""),
    }
    return hashlib.sha256(_canonical_json(payload).encode("utf-8")).hexdigest()[:16]


def choose_shard_count(
    mutant_count: int,
    max_shards: int = MAX_SHARDS_PER_COHORT,
    target_per_shard: int = TARGET_MUTANTS_PER_SHARD,
) -> int:
    if mutant_count <= 0:
        return 0
    return min(max_shards, max(1, math.ceil(mutant_count / max(1, target_per_shard))))


def assign_shard(
    index: int,
    shard_count: int,
    *,
    sharding_method: str = SHARDING_METHOD,
    total_mutants: int | None = None,
) -> int:
    if shard_count <= 0:
        raise ValueError("shard_count must be positive")
    if sharding_method == "round-robin":
        return index % shard_count
    if sharding_method == "slice":
        if total_mutants is None or total_mutants <= 0:
            raise ValueError("total_mutants must be positive for slice sharding")
        return min(shard_count - 1, (index * shard_count) // total_mutants)
    raise ValueError(f"unsupported sharding_method: {sharding_method}")


def compute_cohort_id(
    base_sha: str,
    head_sha: str,
    diff_sha256: str,
    tool_version: str,
    nextest_version: str,
    test_profile: dict[str, Any],
    sharding_method: str,
    shard_count: int,
) -> str:
    payload = {
        "base_sha": base_sha,
        "head_sha": head_sha,
        "diff_sha256": diff_sha256,
        "tool_version": tool_version,
        "nextest_version": nextest_version,
        "test_profile": test_profile,
        "sharding_method": sharding_method,
        "shard_count": shard_count,
    }
    return hashlib.sha256(_canonical_json(payload).encode("utf-8")).hexdigest()[:16]


def build_cohort(
    *,
    base_sha: str,
    head_sha: str,
    diff_sha256: str,
    mutant_entries: list[dict[str, Any]],
    created_at: datetime,
    tool_version: str = CARGO_MUTANTS_VERSION,
    nextest_version: str = CARGO_NEXTEST_VERSION,
    test_profile: dict[str, Any] | None = None,
    sharding_method: str = SHARDING_METHOD,
    max_shards: int = MAX_SHARDS_PER_COHORT,
    target_per_shard: int = TARGET_MUTANTS_PER_SHARD,
    max_cohort_age_days: int = MAX_COHORT_AGE_DAYS,
    shard_count_override: int | None = None,
) -> dict[str, Any]:
    profile = dict(DEFAULT_TEST_PROFILE if test_profile is None else test_profile)

    deduped_raw: list[tuple[str, dict[str, Any]]] = []
    seen_ids: set[str] = set()
    for entry in mutant_entries:
        mid = mutant_id_for_entry(entry)
        if mid in seen_ids:
            continue
        seen_ids.add(mid)
        deduped_raw.append((mid, entry))

    total = len(deduped_raw)
    if shard_count_override is not None:
        shard_count = 0 if total == 0 else max(1, min(max_shards, shard_count_override))
    else:
        shard_count = choose_shard_count(
            total, max_shards=max_shards, target_per_shard=target_per_shard
        )

    cohort_id = compute_cohort_id(
        base_sha=base_sha,
        head_sha=head_sha,
        diff_sha256=diff_sha256,
        tool_version=tool_version,
        nextest_version=nextest_version,
        test_profile=profile,
        sharding_method=sharding_method,
        shard_count=shard_count,
    )

    mutants: list[dict[str, Any]] = []
    shards: dict[str, dict[str, Any]] = {
        str(s): {
            "shard": s,
            "status": "pending",
            "attempts": 0,
            "assigned_run_id": None,
            "last_run_id": None,
            "last_updated_at": None,
            "last_error": None,
            "mutant_ids": [],
        }
        for s in range(shard_count)
    }
    outcomes: dict[str, dict[str, Any]] = {}

    for idx, (mid, entry) in enumerate(deduped_raw):
        shard_idx = assign_shard(
            idx,
            shard_count,
            sharding_method=sharding_method,
            total_mutants=total,
        )
        span_start = (entry.get("span") or {}).get("start") or {}
        mutants.append(
            {
                "index": idx,
                "mutant_id": mid,
                "shard": shard_idx,
                "package": entry.get("package", ""),
                "file": entry.get("file", ""),
                "line": int(span_start.get("line", entry.get("line", 0)) or 0),
                "column": int(span_start.get("column", entry.get("column", 0)) or 0),
                "genre": entry.get("genre", ""),
                "replacement": entry.get("replacement", ""),
                "name": entry.get("name", ""),
            }
        )
        shards[str(shard_idx)]["mutant_ids"].append(mid)
        outcomes[mid] = {
            "status": "not_run",
            "shard": shard_idx,
            "run_id": None,
            "attempts": 0,
            "detail": None,
        }

    expires_at = created_at + timedelta(days=max_cohort_age_days)
    is_empty = total == 0
    return {
        "cohort_id": cohort_id,
        "created_at": format_utc(created_at),
        "expires_at": format_utc(expires_at),
        "base_sha": base_sha,
        "head_sha": head_sha,
        "diff_sha256": diff_sha256,
        "tool_version": tool_version,
        "nextest_version": nextest_version,
        "test_profile": profile,
        "sharding_method": sharding_method,
        "shard_count": shard_count,
        "empty_diff": is_empty,
        "status": "completed" if is_empty else "pending",
        "mutants": mutants,
        "shards": shards,
        "outcomes": outcomes,
    }


def _mark_orphaned_shard_mutants_infra_error(
    cohort: dict[str, Any], shard_record: dict[str, Any], detail: str, run_id: str | None
) -> None:
    for mid in shard_record["mutant_ids"]:
        out = cohort["outcomes"][mid]
        if out["status"] == "not_run":
            out["status"] = "infra_error"
            out["run_id"] = run_id
            out["detail"] = detail


def _recompute_cohort_status(cohort: dict[str, Any]) -> None:
    if cohort["status"] in {"expired", "evicted"}:
        return
    if cohort.get("empty_diff") and cohort["shard_count"] == 0:
        cohort["status"] = "completed"
        return

    shard_records = list(cohort["shards"].values())
    all_completed = all(s["status"] == "completed" for s in shard_records)
    has_unsettled_mutants = any(
        o["status"] in {"not_run", "infra_error"} for o in cohort["outcomes"].values()
    )
    if all_completed and not has_unsettled_mutants:
        cohort["status"] = "completed"
        return

    if any(s["status"] in {"pending", "failed_retryable", "in_progress"} for s in shard_records):
        if any(
            s["attempts"] > 0 or s["status"] != "pending" for s in shard_records
        ):
            cohort["status"] = "in_progress"
        else:
            cohort["status"] = "pending"
        return

    # All shards have reached a non-runnable state without completing every mutant.
    cohort["status"] = "incomplete_failed"


def plan_tick(
    state: dict[str, Any],
    *,
    now: datetime,
    run_id: str,
    new_cohort: dict[str, Any] | None = None,
    shards_per_run: int = SHARDS_PER_RUN,
    max_active_cohorts: int = MAX_ACTIVE_COHORTS,
    max_shard_retries: int = MAX_SHARD_RETRIES,
    manifest_retention_days: int = MANIFEST_RETENTION_DAYS,
) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    """Advance cohort lifecycle, register optional new cohort, and assign runnable shards."""
    updated = json.loads(json.dumps(state))
    now_iso = format_utc(now)
    retention_cutoff = now - timedelta(days=manifest_retention_days)

    # 1. Prune only terminal manifests older than manifest_retention_days.
    kept_cohorts: list[dict[str, Any]] = []
    for cohort in updated.get("cohorts", []):
        created = parse_utc(cohort["created_at"])
        if cohort["status"] not in ACTIVE_COHORT_STATUSES and created < retention_cutoff:
            continue
        kept_cohorts.append(cohort)
    updated["cohorts"] = kept_cohorts

    # 2. Recover orphaned `in_progress` shards left by canceled/crashed earlier runs.
    for cohort in updated["cohorts"]:
        if cohort["status"] not in ACTIVE_COHORT_STATUSES:
            continue
        for shard_rec in cohort["shards"].values():
            if shard_rec["status"] == "in_progress" and shard_rec.get("assigned_run_id") != run_id:
                shard_rec["last_run_id"] = shard_rec.get("assigned_run_id")
                shard_rec["assigned_run_id"] = None
                shard_rec["last_updated_at"] = now_iso
                shard_rec["last_error"] = "orphaned_in_progress_run_never_reconciled"
                if shard_rec["attempts"] < max_shard_retries:
                    shard_rec["status"] = "failed_retryable"
                else:
                    shard_rec["status"] = "failed_permanent"
                    _mark_orphaned_shard_mutants_infra_error(
                        cohort,
                        shard_rec,
                        "shard retries exhausted after orphaned run",
                        shard_rec["last_run_id"],
                    )
        _recompute_cohort_status(cohort)

    # 3. Expire stale active cohorts past max_cohort_age_days.
    for cohort in updated["cohorts"]:
        if cohort["status"] not in ACTIVE_COHORT_STATUSES:
            continue
        if now >= parse_utc(cohort["expires_at"]):
            cohort["status"] = "expired"
            for shard_rec in cohort["shards"].values():
                if shard_rec["status"] in {"pending", "failed_retryable", "in_progress"}:
                    shard_rec["status"] = "expired"
                    shard_rec["assigned_run_id"] = None
                    shard_rec["last_updated_at"] = now_iso
                    shard_rec["last_error"] = "cohort_expired"

    # 4. Register new immutable cohort if provided and deduplicated by cohort_id.
    if new_cohort is not None:
        existing_ids = {c["cohort_id"] for c in updated["cohorts"]}
        if new_cohort["cohort_id"] not in existing_ids:
            updated["cohorts"].append(json.loads(json.dumps(new_cohort)))

        # Bound active backlog by evicting oldest active cohorts first.
        active = [c for c in updated["cohorts"] if c["status"] in ACTIVE_COHORT_STATUSES]
        active.sort(key=lambda c: parse_utc(c["created_at"]))
        excess = max(0, len(active) - max_active_cohorts)
        for victim in active[:excess]:
            victim["status"] = "evicted"
            for shard_rec in victim["shards"].values():
                if shard_rec["status"] in {"pending", "failed_retryable", "in_progress"}:
                    shard_rec["status"] = "evicted"
                    shard_rec["assigned_run_id"] = None
                    shard_rec["last_updated_at"] = now_iso
                    shard_rec["last_error"] = "backlog_capacity_evicted"

    # 5. Schedule up to shards_per_run runnable shards, FIFO oldest active cohort first.
    active_sorted = sorted(
        (c for c in updated["cohorts"] if c["status"] in ACTIVE_COHORT_STATUSES),
        key=lambda c: (parse_utc(c["created_at"]), c["cohort_id"]),
    )

    assignments: list[dict[str, Any]] = []
    for cohort in active_sorted:
        if len(assignments) >= shards_per_run:
            break
        shard_keys = sorted(cohort["shards"].keys(), key=int)
        for key in shard_keys:
            if len(assignments) >= shards_per_run:
                break
            shard_rec = cohort["shards"][key]
            if shard_rec["status"] not in RUNNABLE_SHARD_STATUSES:
                continue
            shard_rec["status"] = "in_progress"
            shard_rec["attempts"] += 1
            shard_rec["assigned_run_id"] = run_id
            shard_rec["last_updated_at"] = now_iso
            assignments.append(
                {
                    "cohort_id": cohort["cohort_id"],
                    "base_sha": cohort["base_sha"],
                    "head_sha": cohort["head_sha"],
                    "diff_sha256": cohort["diff_sha256"],
                    "shard": shard_rec["shard"],
                    "shard_count": cohort["shard_count"],
                    "sharding_method": cohort["sharding_method"],
                    "tool_version": cohort["tool_version"],
                    "nextest_version": cohort["nextest_version"],
                    "attempt": shard_rec["attempts"],
                }
            )
            if cohort["status"] == "pending":
                cohort["status"] = "in_progress"

    updated["updated_at"] = now_iso
    return updated, assignments


def _find_mutants_out_file(out_dir: Path, filename: str) -> Path | None:
    direct = out_dir / filename
    if direct.is_file():
        return direct
    nested = out_dir / "mutants.out" / filename
    if nested.is_file():
        return nested
    return None


def _map_cargo_mutants_summary(summary: Any) -> tuple[str, str]:
    if summary == "CaughtMutant":
        return "caught", "CaughtMutant"
    if summary == "MissedMutant":
        return "missed", "MissedMutant"
    if summary == "Timeout":
        return "timeout", "Timeout"
    if summary == "Unviable":
        return "unviable", "Unviable"
    if isinstance(summary, dict):
        key = next(iter(summary.keys()), "Unknown")
        if key == "CaughtMutant":
            return "caught", key
        if key == "MissedMutant":
            return "missed", key
        if key == "Timeout":
            return "timeout", key
        if key == "Unviable":
            return "unviable", key
        return "infra_error", str(key)
    return "infra_error", str(summary)


def parse_cargo_mutants_outcomes(
    out_dir: Path, cohort: dict[str, Any], shard_idx: int
) -> tuple[dict[str, dict[str, Any]], bool]:
    """Parse cargo-mutants outcomes.json (or text logs) for a shard.

    Returns (observed_by_mutant_id, baseline_failed).
    """
    observed: dict[str, dict[str, Any]] = {}
    baseline_failed = False

    outcomes_path = _find_mutants_out_file(out_dir, "outcomes.json")
    if outcomes_path is not None:
        raw = json.loads(outcomes_path.read_text(encoding="utf-8"))
        entries = raw.get("outcomes", []) if isinstance(raw, dict) else []
        for item in entries:
            scenario = item.get("scenario")
            summary = item.get("summary")
            if scenario == "Baseline":
                if summary != "Success":
                    baseline_failed = True
                continue
            if isinstance(scenario, dict) and "Mutant" in scenario:
                mid = mutant_id_for_entry(scenario["Mutant"])
                mapped, detail = _map_cargo_mutants_summary(summary)
                observed[mid] = {"status": mapped, "detail": detail}
        return observed, baseline_failed

    # Fallback: match txt files against known mutant names in this shard.
    shard_ids = set(cohort["shards"][str(shard_idx)]["mutant_ids"])
    name_to_ids: dict[str, list[str]] = {}
    for m in cohort["mutants"]:
        if m["mutant_id"] in shard_ids and m.get("name"):
            name_to_ids.setdefault(m["name"], []).append(m["mutant_id"])

    file_map = [
        ("caught.txt", "caught", "caught.txt"),
        ("missed.txt", "missed", "missed.txt"),
        ("timeout.txt", "timeout", "timeout.txt"),
        ("unviable.txt", "unviable", "unviable.txt"),
    ]
    for fname, status_val, detail in file_map:
        path = _find_mutants_out_file(out_dir, fname)
        if path is None:
            continue
        for line in path.read_text(encoding="utf-8").splitlines():
            key = line.strip()
            if not key:
                continue
            for mid in name_to_ids.get(key, []):
                observed[mid] = {"status": status_val, "detail": detail}

    return observed, baseline_failed


def reconcile_shard(
    cohort: dict[str, Any],
    shard_idx: int,
    run_id: str,
    *,
    job_status: str,
    exit_code: int | None,
    observed_outcomes: dict[str, dict[str, Any]],
    baseline_failed: bool,
    now: datetime,
    max_shard_retries: int = MAX_SHARD_RETRIES,
) -> None:
    """Reconcile one shard execution into `cohort` without crossing cohort boundaries."""
    shard_key = str(shard_idx)
    if shard_key not in cohort["shards"]:
        return
    shard_rec = cohort["shards"][shard_key]
    shard_mutant_ids = set(shard_rec["mutant_ids"])
    now_iso = format_utc(now)

    # Apply valid terminal outcomes belonging ONLY to this shard of this cohort.
    for mid, obs in observed_outcomes.items():
        if mid not in shard_mutant_ids:
            continue
        obs_status = obs.get("status", "infra_error")
        out_rec = cohort["outcomes"][mid]
        out_rec["attempts"] = shard_rec["attempts"]
        if obs_status in TERMINAL_MUTANT_STATUSES:
            out_rec["status"] = obs_status
            out_rec["run_id"] = run_id
            out_rec["detail"] = obs.get("detail")
        elif out_rec["status"] == "not_run":
            out_rec["run_id"] = run_id
            out_rec["detail"] = obs.get("detail")

    missing_mutants = [
        mid
        for mid in shard_rec["mutant_ids"]
        if cohort["outcomes"][mid]["status"] not in TERMINAL_MUTANT_STATUSES
    ]

    clean_exit = exit_code in (0, 2, 3)
    clean_job = job_status == "success"
    shard_ok = clean_job and clean_exit and (not baseline_failed) and len(missing_mutants) == 0

    shard_rec["last_run_id"] = run_id
    shard_rec["assigned_run_id"] = None
    shard_rec["last_updated_at"] = now_iso

    if shard_ok:
        shard_rec["status"] = "completed"
        shard_rec["last_error"] = None
    else:
        reasons: list[str] = []
        if not clean_job:
            reasons.append(f"job_status={job_status}")
        if not clean_exit:
            reasons.append(f"exit_code={exit_code}")
        if baseline_failed:
            reasons.append("baseline_failed")
        if missing_mutants:
            reasons.append(f"unattempted_mutants={len(missing_mutants)}")
        err_msg = ", ".join(reasons) or "unknown_shard_failure"
        shard_rec["last_error"] = err_msg
        if shard_rec["attempts"] < max_shard_retries:
            shard_rec["status"] = "failed_retryable"
        else:
            shard_rec["status"] = "failed_permanent"
            _mark_orphaned_shard_mutants_infra_error(cohort, shard_rec, err_msg, run_id)

    _recompute_cohort_status(cohort)


def summarize_cohort(cohort: dict[str, Any], *, now: datetime) -> dict[str, Any]:
    counts = {
        "caught": 0,
        "missed": 0,
        "timeout": 0,
        "unviable": 0,
        "not_run": 0,
        "infra_error": 0,
    }
    for out in cohort["outcomes"].values():
        st = out["status"]
        if st in counts:
            counts[st] += 1
        else:
            counts["infra_error"] += 1

    total_mutants = len(cohort["outcomes"])
    eligible_mutants = max(0, total_mutants - counts["unviable"])
    attempted_mutants = counts["caught"] + counts["missed"] + counts["timeout"]

    attempted_ratio = 1.0 if eligible_mutants == 0 else attempted_mutants / eligible_mutants
    killed_ratio = 1.0 if eligible_mutants == 0 else counts["caught"] / eligible_mutants

    shards = list(cohort["shards"].values())
    shards_completed = sum(1 for s in shards if s["status"] == "completed")
    shards_pending = sum(
        1 for s in shards if s["status"] in {"pending", "failed_retryable", "in_progress"}
    )
    shards_failed = sum(1 for s in shards if s["status"] == "failed_permanent")

    created = parse_utc(cohort["created_at"])
    age_days = max(0.0, (now - created).total_seconds() / 86400.0)
    completion_allowed = (
        cohort["status"] == "completed"
        and counts["not_run"] == 0
        and counts["infra_error"] == 0
    )

    return {
        "cohort_id": cohort["cohort_id"],
        "status": cohort["status"],
        "base_sha": cohort["base_sha"],
        "head_sha": cohort["head_sha"],
        "diff_sha256": cohort["diff_sha256"],
        "created_at": cohort["created_at"],
        "expires_at": cohort["expires_at"],
        "backlog_age_days": round(age_days, 2),
        "tool_version": cohort["tool_version"],
        "nextest_version": cohort["nextest_version"],
        "sharding_method": cohort["sharding_method"],
        "shards_total": cohort["shard_count"],
        "shards_completed": shards_completed,
        "shards_pending": shards_pending,
        "shards_failed": shards_failed,
        "total_mutants": total_mutants,
        "eligible_mutants": eligible_mutants,
        "attempted_mutants": attempted_mutants,
        "caught": counts["caught"],
        "missed": counts["missed"],
        "timeout": counts["timeout"],
        "unviable": counts["unviable"],
        "not_run": counts["not_run"],
        "infra_error": counts["infra_error"],
        "attempted_coverage_ratio": round(attempted_ratio, 4),
        "killed_ratio_of_eligible": round(killed_ratio, 4),
        "completion_claim_allowed": completion_allowed,
    }


def summarize_state(
    state: dict[str, Any], *, now: datetime, run_id: str
) -> dict[str, Any]:
    cohorts = [summarize_cohort(c, now=now) for c in state.get("cohorts", [])]
    active = [c for c in cohorts if c["status"] in ACTIVE_COHORT_STATUSES]
    oldest_active_age = max((c["backlog_age_days"] for c in active), default=0.0)

    return {
        "schema_version": SCHEMA_VERSION,
        "policy_mode": POLICY_MODE,
        "report_only": True,
        "advisory": True,
        "generated_at": format_utc(now),
        "run_id": run_id,
        "tool_versions": {
            "cargo_mutants": CARGO_MUTANTS_VERSION,
            "cargo_nextest": CARGO_NEXTEST_VERSION,
        },
        "limits": {
            "shards_per_run": SHARDS_PER_RUN,
            "max_shards_per_cohort": MAX_SHARDS_PER_COHORT,
            "target_mutants_per_shard": TARGET_MUTANTS_PER_SHARD,
            "max_active_cohorts": MAX_ACTIVE_COHORTS,
            "max_cohort_age_days": MAX_COHORT_AGE_DAYS,
            "manifest_retention_days": MANIFEST_RETENTION_DAYS,
            "max_shard_retries": MAX_SHARD_RETRIES,
        },
        "backlog_summary": {
            "total_cohorts": len(cohorts),
            "active_cohorts": len(active),
            "oldest_active_age_days": round(oldest_active_age, 2),
            "completed_cohorts": sum(1 for c in cohorts if c["status"] == "completed"),
            "expired_cohorts": sum(1 for c in cohorts if c["status"] == "expired"),
            "evicted_cohorts": sum(1 for c in cohorts if c["status"] == "evicted"),
            "incomplete_failed_cohorts": sum(
                1 for c in cohorts if c["status"] == "incomplete_failed"
            ),
        },
        "cohorts": cohorts,
    }


def format_markdown_summary(summary: dict[str, Any]) -> str:
    b = summary["backlog_summary"]
    lines = [
        "## Immutable Mutation Cohort Report (Report-Only)",
        "",
        f"- **Policy**: `{summary['policy_mode']}` (`cargo-mutants {summary['tool_versions']['cargo_mutants']}`, `cargo-nextest {summary['tool_versions']['cargo_nextest']}`)",
        f"- **Backlog**: {b['active_cohorts']} active (oldest {b['oldest_active_age_days']}d), "
        f"{b['completed_cohorts']} completed, {b['expired_cohorts']} expired, "
        f"{b['evicted_cohorts']} evicted, {b['incomplete_failed_cohorts']} incomplete/failed",
        "",
        "| Cohort | Status | Shards (done/total) | Age (d) | Caught | Missed | Timeout | Unviable | Not Run | Infra Err | Attempted % | Complete? |",
        "|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|",
    ]
    for c in summary["cohorts"]:
        lines.append(
            f"| `{c['cohort_id']}` (`{c['head_sha'][:7]}`) "
            f"| `{c['status']}` "
            f"| {c['shards_completed']}/{c['shards_total']} "
            f"| {c['backlog_age_days']:.1f} "
            f"| {c['caught']} "
            f"| {c['missed']} "
            f"| {c['timeout']} "
            f"| {c['unviable']} "
            f"| {c['not_run']} "
            f"| {c['infra_error']} "
            f"| {c['attempted_coverage_ratio'] * 100:.1f}% "
            f"| {'yes' if c['completion_claim_allowed'] else 'no'} |"
        )
    if not summary["cohorts"]:
        lines.append("| _(none)_ | - | 0/0 | 0.0 | 0 | 0 | 0 | 0 | 0 | 0 | 100.0% | - |")
    return "\n".join(lines) + "\n"


def reconcile_run(
    state: dict[str, Any],
    *,
    assignments: list[dict[str, Any]],
    results_dir: Path,
    run_id: str,
    now: datetime,
    max_shard_retries: int = MAX_SHARD_RETRIES,
) -> dict[str, Any]:
    """Reconcile all assignments from a workflow run into the cohort state."""
    updated = json.loads(json.dumps(state))
    cohort_by_id = {c["cohort_id"]: c for c in updated.get("cohorts", [])}

    for item in assignments:
        cid = item["cohort_id"]
        shard_idx = int(item["shard"])
        cohort = cohort_by_id.get(cid)
        if cohort is None:
            continue

        shard_dir = results_dir / f"{cid}-{shard_idx}"
        if not shard_dir.is_dir():
            alt = results_dir / f"mutants-shard-{cid}-{shard_idx}"
            if alt.is_dir():
                shard_dir = alt

        meta_path = shard_dir / "shard-result.json"
        if meta_path.is_file():
            meta = json.loads(meta_path.read_text(encoding="utf-8"))
            job_status = str(meta.get("job_status", "failure"))
            exit_code = meta.get("exit_code")
            if exit_code is not None:
                exit_code = int(exit_code)
            diff_sha = meta.get("diff_sha256")
            if diff_sha and diff_sha != cohort["diff_sha256"]:
                job_status = "diff_sha_mismatch"
                observed, baseline_failed = {}, False
            else:
                observed, baseline_failed = parse_cargo_mutants_outcomes(
                    shard_dir, cohort, shard_idx
                )
        else:
            job_status = "missing_result"
            exit_code = None
            observed, baseline_failed = {}, False

        reconcile_shard(
            cohort,
            shard_idx,
            run_id,
            job_status=job_status,
            exit_code=exit_code,
            observed_outcomes=observed,
            baseline_failed=baseline_failed,
            now=now,
            max_shard_retries=max_shard_retries,
        )

    updated["updated_at"] = format_utc(now)
    return updated


def _load_state(path: Path | None, now: datetime) -> dict[str, Any]:
    if path is not None and path.is_file():
        return json.loads(path.read_text(encoding="utf-8"))
    return empty_state(now)


def _cmd_plan(args: argparse.Namespace) -> int:
    now = parse_utc(args.now) if args.now else utc_now()
    state = _load_state(Path(args.state_in) if args.state_in else None, now)

    new_cohort: dict[str, Any] | None = None
    if args.mutants_json and args.diff_file and args.base_sha and args.head_sha:
        diff_bytes = Path(args.diff_file).read_bytes()
        entries = json.loads(Path(args.mutants_json).read_text(encoding="utf-8"))
        if entries:
            new_cohort = build_cohort(
                base_sha=args.base_sha,
                head_sha=args.head_sha,
                diff_sha256=sha256_hex(diff_bytes),
                mutant_entries=entries,
                created_at=now,
                tool_version=args.tool_version,
                nextest_version=args.nextest_version,
            )

    next_state, assignments = plan_tick(
        state,
        now=now,
        run_id=args.run_id,
        new_cohort=new_cohort,
        shards_per_run=args.shards_per_run,
        max_active_cohorts=args.max_active_cohorts,
        max_shard_retries=args.max_shard_retries,
    )

    state_out = Path(args.state_out)
    state_out.parent.mkdir(parents=True, exist_ok=True)
    state_out.write_text(json.dumps(next_state, indent=2) + "\n", encoding="utf-8")

    assign_out = Path(args.assignments_out)
    assign_out.parent.mkdir(parents=True, exist_ok=True)
    assign_out.write_text(json.dumps(assignments, indent=2) + "\n", encoding="utf-8")

    if args.github_output:
        with open(args.github_output, "a", encoding="utf-8") as fh:
            fh.write(f"assignments={json.dumps(assignments, separators=(',', ':'))}\n")
            fh.write(f"has_assignments={'true' if assignments else 'false'}\n")

    return 0


def _cmd_reconcile(args: argparse.Namespace) -> int:
    now = parse_utc(args.now) if args.now else utc_now()
    state = json.loads(Path(args.state_in).read_text(encoding="utf-8"))
    assignments = json.loads(Path(args.assignments_in).read_text(encoding="utf-8"))

    next_state = reconcile_run(
        state,
        assignments=assignments,
        results_dir=Path(args.results_dir),
        run_id=args.run_id,
        now=now,
        max_shard_retries=args.max_shard_retries,
    )
    summary = summarize_state(next_state, now=now, run_id=args.run_id)
    markdown = format_markdown_summary(summary)

    Path(args.state_out).parent.mkdir(parents=True, exist_ok=True)
    Path(args.state_out).write_text(json.dumps(next_state, indent=2) + "\n", encoding="utf-8")

    Path(args.summary_out).parent.mkdir(parents=True, exist_ok=True)
    Path(args.summary_out).write_text(json.dumps(summary, indent=2) + "\n", encoding="utf-8")

    if args.markdown_out:
        Path(args.markdown_out).parent.mkdir(parents=True, exist_ok=True)
        Path(args.markdown_out).write_text(markdown, encoding="utf-8")

    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Immutable-cohort mutation scheduler")
    sub = parser.add_subparsers(dest="cmd", required=True)

    p_plan = sub.add_parser("plan", help="Advance backlog and select shard assignments")
    p_plan.add_argument("--state-in")
    p_plan.add_argument("--state-out", required=True)
    p_plan.add_argument("--assignments-out", required=True)
    p_plan.add_argument("--diff-file")
    p_plan.add_argument("--mutants-json")
    p_plan.add_argument("--base-sha")
    p_plan.add_argument("--head-sha")
    p_plan.add_argument("--tool-version", default=CARGO_MUTANTS_VERSION)
    p_plan.add_argument("--nextest-version", default=CARGO_NEXTEST_VERSION)
    p_plan.add_argument("--run-id", required=True)
    p_plan.add_argument("--now")
    p_plan.add_argument("--shards-per-run", type=int, default=SHARDS_PER_RUN)
    p_plan.add_argument("--max-active-cohorts", type=int, default=MAX_ACTIVE_COHORTS)
    p_plan.add_argument("--max-shard-retries", type=int, default=MAX_SHARD_RETRIES)
    p_plan.add_argument("--github-output")

    p_rec = sub.add_parser("reconcile", help="Reconcile shard outcomes into cohort state")
    p_rec.add_argument("--state-in", required=True)
    p_rec.add_argument("--assignments-in", required=True)
    p_rec.add_argument("--results-dir", required=True)
    p_rec.add_argument("--state-out", required=True)
    p_rec.add_argument("--summary-out", required=True)
    p_rec.add_argument("--markdown-out")
    p_rec.add_argument("--run-id", required=True)
    p_rec.add_argument("--now")
    p_rec.add_argument("--max-shard-retries", type=int, default=MAX_SHARD_RETRIES)

    args = parser.parse_args(argv)
    if args.cmd == "plan":
        return _cmd_plan(args)
    if args.cmd == "reconcile":
        return _cmd_reconcile(args)
    return 2


if __name__ == "__main__":
    sys.exit(main())
