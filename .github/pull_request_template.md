## Summary

<!-- What does this PR change and why? Link issues (e.g. `Closes #123`). -->

## Issue / Stabilization Context

- Linked issue(s), primary phase, priority and owner:
- Base revision / reproduction (or preventive-work rationale):
- Probe (defect before, gone after) and acceptance test (fails on base, passes on head; path):
- Violated invariant / expected behavior:
- Dependencies, risk, and out-of-scope changes:

<!-- See docs/ISSUE_PR_WORKFLOW.md. Apply the corresponding labels after
reviewing the actual change. Use Refs for partial/umbrella work; Closes only
when all acceptance criteria are verified. -->

## Change Type

- [ ] Decompiler semantics / IR / structuring / types
- [ ] SLEIGH lifting
- [ ] Loader / binary format support
- [ ] Static facts / native preparation
- [ ] Decompilation orchestration / Rust-Sleigh
- [ ] Verification (`fission-dir`) / solver
- [ ] Emulator / dynamic analysis / TTD
- [ ] CLI / JSON contract
- [ ] Benchmark / measurement scripts
- [ ] Documentation only
- [ ] Build / CI / release infrastructure

## Semantic Ownership

<!-- Check every area you materially changed; reviewers use this for routing. -->

- [ ] **NIR / structuring / pcode** (`crates/fission-pcode/`, `crates/fission-midend-*`; `AGENTS.md` "NIR structuring")
- [ ] **SLEIGH** (`crates/fission-sleigh/`, `crates/fission-cspec/`)
- [ ] **Static facts / native prep** (`crates/fission-static/`)
- [ ] **Decompilation orchestration / Rust-Sleigh** (`crates/fission-decompiler/`)
- [ ] **Loader / binary parsing / metadata** (`crates/fission-loader/`, `crates/fission-analysis-db/`, `crates/fission-signatures/`)
- [ ] **Verification / solver** (`crates/fission-dir/`, `crates/fission-solver/`)
- [ ] **Emulator / dynamic / TTD** (`crates/fission-emulator/`, `crates/fission-dynamic/`, `crates/fission-ttd/`)
- [ ] **CLI / JSON contract** (`crates/fission-cli/`)
- [ ] **Benchmark scripts** (`benchmark/config/`, `scripts/quality/`; see [`docs/BENCHMARK_FRESHBENCH.md`](../docs/BENCHMARK_FRESHBENCH.md))
- [ ] **Docs / process only** (no semantic code paths)
- [ ] **CI / release plumbing** (`.github/`, `docs/RELEASE.md`)

## Validation

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings -A clippy::style -A clippy::complexity` (CI's exact line)
- [ ] `cargo nextest run -p <crates touched>`
- [ ] Each new test fails with the fix disabled (positive control)
- [ ] Correctness evidence, if this fixes wrong output (see below)
- [ ] FreshBench measurement, if decompiler output can change (see below)
- [ ] Not run — reason:

<details>
<summary>Commands / key output</summary>

```bash

```

</details>

## Correctness Evidence

<!--
Required when this PR claims wrong output became right. A benchmark measures
similarity to the source, not correctness (docs/adr/0018, docs/adr/0019). Cite what checks
meaning: native replay (docs/VERIFY_REPLAY.md), `fission_cli verify --tier
ground-truth` (divergences before -> after), the architecture manual, DWARF/PDB,
call sites counted by instruction address, and two or three raw before/after
hunks. If the FreshBench score got worse, say what the output now prints that
costs it. Use `n/a` otherwise.
-->

## Benchmark Impact

<!--
Required when decompiler output can change. Quality claims cite a full
FreshBench M1 run (`freshbench sweep run`, docs/BENCHMARK_FRESHBENCH.md); a
`--builds` / `--only` slice is the quick pre-check, not the evidence. Compare
output hashes before scores (`freshbench sweep diff`): rows that move with
identical text are scorer noise. Report distance next to the perfect count:
the count alone has ranked a regression as a win. Use `n/a` for changes that
cannot change output.
-->

```text
slice pre-check (--builds / --only <...>):
  GED perfect <before> -> <after>, mean distance <before> -> <after>

full M1 run (sweep report, per size band where it moved):
  GED perfect (rows at 0): <before> -> <after>
  GED mean distance:       <before> -> <after>
  Types:                   <before> -> <after>
  failures:                <before> -> <after>
  sweep diff:              <functions whose output changed>
```

## Telemetry and Reporting

- [ ] Changes to decompiler semantics or inventory output: **confirmed** [`NirBuildStats`](../crates/fission-midend-core/src/ir/build_stats.rs) consumers stay aligned, or this PR updates them together with tests.
- [ ] No parallel ad-hoc telemetry JSON outside the established contracts ([`docs/QUALITY_METRICS.md`](../docs/QUALITY_METRICS.md)).

## ADR / Design Traceability

- [ ] Architectural trade-offs are covered by an existing [`docs/adr/`](../docs/adr/) decision, **or** this PR adds/updates an ADR.
- ADR link(s): <!-- e.g. `docs/adr/0001-cli-first-external-surface.md` -->

## Risk Notes

> [!IMPORTANT]
> If this changes semantic recovery, structuring, loader classification, or benchmark gates, include the exact reproduction command and artifact path.

## Checklist

- [ ] Documentation updated or not needed
- [ ] New/changed behavior is covered by tests, benchmark rows, or an explicit reason
- [ ] Generated artifacts, local corpora, and downloaded samples are not committed
