# fission-automation

Canonical automation runner for Fission quality pipelines.

This crate is used to:

- run repeatable `nir-check` lanes over sentinel binaries
- emit machine-readable artifacts (`summary.json`, `decision_insights.json`, candidate lists)
- compare current runs against a baseline
- provide a **Go/Stop gate** for follow-up recovery work

Source-semantic benchmarking is owned by **`benchmark/source_semantic_benchmark/`** (Python). Call that directly — do not invoke it through this crate.

---

## Quick start

From repository root:

```bash
cargo run -p fission-automation -- nir-check --lane nir
```

By default, outputs are written under:

- `benchmark/artifacts/automation/<lane>-<profile>-<run_id>/`

and latest is mirrored to:

- `benchmark/artifacts/automation/latest/<lane>/`

---

## Important options

- `--lane <name>`
  - canonical lane is `nir`
  - `preview` is accepted but deprecated alias

- `--run-profile {fast|mid|full}`
  - `fast`: short feedback loop (smaller effective limits/timeouts)
  - `mid`: default balanced mode
  - `full`: broader validation window

- `--manifest <path>`
  - override the default lane manifest at `crates/fission-automation/config/sentinel_sets.toml`

- `--focus-top-mismatch <N>`
  - uses baseline candidates to focus run targets to binaries implicated by top `conditional_tail_exit_mismatch` rows
  - ideal for rapid iteration on conditional-tail recovery patches

- `--baseline <path/to/summary.json>`
  - enables baseline delta + row-level mismatch diff + go/stop gating

- `--functions-limit <N>` and `--timeout-ms <N>`
  - explicit overrides (still profile-adjusted)

- `--no-build` + `--fission-bin <path>`
  - skip building `fission_cli` and use an existing binary

- `--emit-legacy-preview-artifacts`
  - optional: duplicates candidate JSON under deprecated `preview_*` filenames (same content as `nir_*`).
  - default: only canonical `nir_*` files are written

---

## Key outputs

Main artifacts inside each run directory:

- `summary.json`
  - aggregate metrics + run metadata (`run_profile`, `target_count`, timing fields)
- `summary.md`
  - readable summary including baseline delta and decision insights
- `decision_insights.json`
  - mismatch subtype ranking
  - top mismatch rows (with per-row subtype split)
  - row-level baseline/current mismatch deltas
  - `go_stop_gate` decision
- `diagnosis.json`, `diagnosis.md`
  - diagnosis buckets and recommended next patch
- `nir_quality_candidates.json`
  - per-row candidate data used for deep triage (baseline focus mode reads this path)
- **Legacy (opt-in):** `preview_quality_candidates.json`, `preview_explicit_blocked_candidates.json`,
  `preview_explicit_aligned_candidate_report.json` — only when `--emit-legacy-preview-artifacts` is set

---

## Recommended iteration workflow

### 1) Fast loop (developer inner loop)

```bash
cargo run -p fission-automation -- nir-check \
  --lane nir \
  --run-profile fast \
  --focus-top-mismatch 5 \
  --no-build \
  --fission-bin ./target/debug/fission_cli \
  --baseline benchmark/artifacts/automation/latest/nir/summary.json
```

Use this to quickly validate whether the top mismatch rows move.

### 2) Mid loop (PR-quality check)

```bash
cargo run -p fission-automation -- nir-check \
  --lane nir \
  --run-profile mid \
  --no-build \
  --fission-bin ./target/debug/fission_cli \
  --baseline benchmark/artifacts/automation/latest/nir/summary.json
```

### 3) Full loop (periodic broader validation)

```bash
cargo run -p fission-automation -- nir-check --lane nir --run-profile full
```

---

## Interpreting the go/stop gate

`decision_insights.json` includes:

- `go_p5h3g_candidate`
  - mismatch improved under safe dominant subtype conditions
- `stop_hold_p5h3f`
  - no meaningful mismatch reduction or risk signal is insufficient
- `stop_no_baseline`
  - baseline comparison unavailable

Treat this as an operational guardrail, not an absolute truth. Always inspect top mismatch rows and subtype distribution together.

---

## Development notes

- Core modules:
  - `main.rs` + `cli.rs`: thin entry and Clap surface
  - `lanes/nir_check.rs`: NIR lane pipeline; `lanes/mod.rs`: manifest/target resolution
  - `artifacts.rs`: JSON/Markdown emission for a run directory
  - `gates.rs`: go/stop exit code and performance regression checks
  - `report/`: summary/delta/decision-insight construction and markdown rendering
  - `diagnosis.rs`: diagnosis buckets and recommended patch classification
  - `corpus.rs`: candidate aggregation and quality artifact assembly
  - `inventory.rs`: `fission_cli` inventory execution and loading
- Default automation config ships with this crate under `crates/fission-automation/config/`
  - `sentinel_sets.toml`: lane target definitions (paths inside are repo-root-relative)
  - `preview_explicit_source_inventory.json`: optional source inventory overlay (if present)
- Tests:
  - `cargo test -p fission-automation`
