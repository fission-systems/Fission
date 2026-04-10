# Fission Automation Guide

Generated: 2026-04-09  
Scope: `crates/fission-automation/`

## Overview

This crate runs quality lanes against `fission_cli`, aggregates inventory/diagnosis into JSON/Markdown, computes deltas vs baselines, and emits go/stop signals. Telemetry in summaries must stay aligned with `fission_pcode::NirBuildStats` (canonical definition: `crates/fission-pcode/src/nir/types.rs`).

## Layout

```text
src/
├── main.rs           # `nir-check` CLI
├── lanes.rs          # sentinel manifest + lane target resolution
├── inventory.rs      # subprocess inventory emit
├── diagnosis.rs      # diagnosis buckets / next-patch hints
├── corpus.rs         # corpus artifacts + totals
├── model.rs          # row/summary types
└── report/           # snapshots, deltas, insights, render, baseline I/O
```

## `nir-check` flags (high level)

| Flag | Role |
|------|------|
| `--lane` | Sentinel lane name (e.g. `nir`; `preview` aliases to `nir`) |
| `--release` / `--no-build` | How `fission_cli` is resolved under `target/` |
| `--fission-bin` | Explicit `fission_cli` path |
| `--manifest` | Override `config/sentinel_sets.toml` |
| `--baseline` | Baseline `summary.json` for deltas + perf gate (optional) |
| `--update-latest` / `--no-update-latest` | Copy run into `artifacts/fission-automation/latest/<lane>/` |
| `--run-profile` | `fast` / `mid` / `full` — adjusts limits/timeouts |
| `--functions-limit`, `--timeout-ms` | Per-target overrides |
| `--dry-run` | Print targets and paths; no subprocess |
| `--fail-on-stop` | Exit non-zero unless `go_stop_gate.decision` starts with `go_` |
| `--jobs` | Parallel inventory runs (default `1`) |

## CI

Heavy workflow builds `fission-cli` (release), runs crate tests, then a **fast** `nir-check` without baseline (perf regression gate skipped) and without updating `latest/`. Artifacts upload: `artifacts/fission-automation/`.

## Validation

```bash
cargo test -p fission-automation
cargo run -p fission-automation -- nir-check --lane nir --dry-run
```

## Conventions

- Do not duplicate `NirBuildStats` field semantics; extend counters in `fission-pcode` and let automation roll up via `BinarySnapshot` / aggregates.
- Contract tests in `report/snapshot.rs` guard JSON shape for embedded `nir_build_stats_totals`.
