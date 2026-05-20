# Fission Agent Guide

Generated: 2026-04-21
Scope: repository root

## Overview

Fission is a Rust-first reverse-engineering/decompilation workspace. Ghidra-native lifting feeds Rust-owned NIR/HIR normalization, structuring, rendering, and automation quality lanes.

**Repository layout (directories + workspace crates):** see [`docs/PROJECT_MAP.md`](docs/PROJECT_MAP.md). Use this file plus the tree below when navigating; avoid duplicating full crate lists in multiple docs.

## Structure

```text
Fission/
├── benchmark/
│   ├── config/              # Benchmark corpus + automation manifests
│   ├── source_semantic_benchmark/ # Canonical source-vs-Fission semantic benchmark
│   ├── full_benchmark/      # Benchmark runner, support modules, rendering
│   ├── artifacts/           # Benchmark outputs (automation/, source_semantic_benchmark/, full_benchmark/)
│   └── binary/              # Curated benchmark binaries and fixtures
├── crates/
│   ├── fission-pcode/        # Canonical IR, NIR/HIR, structuring, CFG, printer
│   ├── fission-decompiler/   # Orchestration + Rust-Sleigh bridge (re-exports IR crate)
│   ├── fission-sleigh/       # Sleigh decode/lift runtime
│   ├── fission-static/       # Static facts, native preparation, analysis services
│   ├── fission-automation/   # Quality lanes, deltas, go/stop signals, artifacts
│   ├── fission-loader/       # Binary parsing, symbols, sections, strings
│   ├── fission-signatures/   # FID/signature data and lookup
│   ├── fission-cli/          # CLI surface
│   └── fission-tauri/        # Desktop surface
├── utils/                    # Checked-in signatures, type info, benchmark support data
├── vendor/                   # Ghidra, RetDec, other reference code
├── scripts/benchmark/        # Benchmark setup / history helpers
├── scripts/test/             # Smoke / fuzz / automation helpers
└── .github/workflows/        # CI/CD source of truth
```

## Child AGENTS

- `crates/fission-pcode/src/nir/AGENTS.md`
- `crates/fission-pcode/src/nir/structuring/AGENTS.md`
- `crates/fission-automation/AGENTS.md`
- `crates/fission-cli/AGENTS.md`

Read the nearest child file before editing those areas.

## Where To Look

| Task | Location | Notes |
|---|---|---|
| NIR structuring / canonicalization | `crates/fission-pcode/src/nir/structuring/` | Core algorithmic decompiler work lives here |
| NIR telemetry contract | `crates/fission-pcode/src/nir/types.rs` | `NirBuildStats` is canonical |
| Decompilation orchestration / Rust-Sleigh | `crates/fission-decompiler/` | Routing, workers, type-context assembly; consumes `fission-pcode` + `fission-static` facts |
| Quality lanes / automation summaries | `crates/fission-automation/` | `nir-check`, reports; must stay aligned with `NirBuildStats` |
| Automation summaries / deltas (implementation) | `crates/fission-automation/src/report/` | Markdown/JSON pipeline; must stay aligned with `NirBuildStats` |
| Source semantic benchmark / corpus reports | `benchmark/source_semantic_benchmark/` | Canonical source-vs-Fission semantic quality surface; Ghidra is not used as the oracle |
| Ghidra reference benchmark | `benchmark/full_benchmark/` | Reference/comparison lane only; keep reporting/gating additive |
| Benchmark manifests / automation manifests | `benchmark/config/` | Corpus manifests and sentinel sets live here now |
| CLI one-shot parsing / command ownership | `crates/fission-cli/src/cli/` | Keep subcommand UX and legacy shims separate from semantics |
| Runtime resource paths (signatures, DiE, FID, patterns, typeinfo) | `crates/fission-core/src/core/path_config.rs`, `resource_roots.rs` | `PATHS` / `PathConfig::detect`; overrides: CLI `--resource-root`, `FISSION_RESOURCE_ROOT`; operator docs: `docs/CLI.md` § *Runtime resource bundle* |
| Checked-in utility resources | `/Users/sjkim1127/Fission/utils` | Prefer existing resource/path config and utility loaders over hardcoded paths; use this tree when reusable signatures, type info, benchmark support data, or other checked-in resources already cover the need |
| Loader identity / binary provenance hints | `crates/fission-loader/src/loader/identity/` | Evidence-backed `BinaryIdentityReport` on `LoadedBinary`; not an IR/decompiler repair layer |
| Static facts and binary-derived analysis services | `crates/fission-static/src/analysis/` | Xrefs, discovery, patches, strings; fact extraction — not decompiler orchestration |
| Decomp-facing facts / native prep surface | `crates/fission-static/src/analysis/decomp/` | `FactStore` and related helpers consumed by `fission-decompiler` |
| Reference algorithms | `/Users/sjkim1127/Fission/vendor`, especially `/Users/sjkim1127/Fission/vendor/ghidra/` and `/Users/sjkim1127/Fission/vendor/retdec-5.0/` | Reference these often for invariants and behavior, but do not add runtime/build dependencies, bindings, or copied implementation shortcuts |

## Core Rules

1. Fix behavior at the canonical owner, not downstream UI/surface layers.
2. Prefer algorithmic CFG / dom / postdom / SCC facts over lexical or binary-specific shortcuts.
3. Use typed contracts; do not invent parallel telemetry payloads outside `NirBuildStats`.
4. Keep behavior deterministic when outputs feed snapshots, metrics, or automation comparisons.
5. Large refactors are acceptable when they reduce long-term complexity and tighten ownership.
6. Do not hardcode repository-local resource paths in code; when `/Users/sjkim1127/Fission/utils` has reusable signatures, type info, benchmark support data, or other checked-in resources, route access through existing `PathConfig`, `PATHS`, `resource_roots`, or utility loaders instead of embedding absolute paths.
7. Treat `/Users/sjkim1127/Fission/vendor` as a reference corpus only: consult it often for algorithms, invariants, and expected behavior, but keep Fission-owned Rust implementations dependency-free from that tree.

## Anti-Patterns

- Do not patch semantic gaps only in printer/UI output.
- Do not add one-off binary-specific shortcuts without invariant-based guards.
- Do not duplicate the same metric definition across pcode and automation.
- Do not treat `fission-cli` or `fission-tauri` as semantic repair layers.
- Do not treat benchmark/reporting scripts as semantic repair layers.
- Do not bypass `PathConfig`, `PATHS`, `resource_roots`, or related helpers by embedding `/Users/sjkim1127/Fission/utils` directly in implementation logic.
- Do not link against, shell out to, bind to, or otherwise depend on `/Users/sjkim1127/Fission/vendor` code in production paths.
- Do not claim success from one targeted test if crate-level regression remains.

## Build / Test Commands

```bash
# CLI
cargo build -p fission-cli --release

# Common decompiler validation
cargo test -p fission-pcode
cargo check -p fission-pcode
cargo check -p fission-decompiler
cargo check -p fission-automation

# Quality lane
cargo run -p fission-automation -- nir-check --lane nir

# Canonical benchmark runner
python3 benchmark/source_semantic_benchmark/run_source_semantic_benchmark.py --help
```

## Workflow Bias

- For NIR/structuring changes: targeted tests → `cargo test -p fission-pcode` → `cargo check -p fission-pcode`.
- For orchestration / Rust-Sleigh glue: also `cargo check -p fission-decompiler` (and CLI/Tauri surfaces as needed).
- For resource path / bundle resolution changes: `cargo test -p fission-core` and smoke `fission_cli resources status`.
- If telemetry/reporting changes: also run `cargo check -p fission-automation`.
- If benchmark/reporting changes: validate under `benchmark/source_semantic_benchmark/` and keep artifacts under `benchmark/artifacts/`.
- Use `.github/workflows/ci.yml` and `ci-heavy.yml` as CI source of truth.
- Ship Git **release tags** via `.github/workflows/release-tag.yml` (`Release Tag (CI green)`): it only tags a commit after `ci.yml` has a successful **push** run for that SHA, then `cd.yml` builds assets.

## Decompiler Quality Loop

Use this loop for source-semantic or pseudocode-quality work, especially when a concrete row/function motivated the change.

1. **Anchor the row:** record the source file, binary, address, function name, current behavior status, case pass count, semantic/static scores, and the top missing/extra features.
2. **Find the owner:** prove whether the bug belongs to SLEIGH/raw p-code, NIR materialization, type recovery, structuring, cleanup, printer, or benchmark/automation. Fix behavior at that owner.
3. **Add focused coverage:** add or update the smallest targeted Rust/Python test that captures the invariant. Synthetic tests are necessary but not sufficient for decompiler-quality claims.
4. **Make the scoped change:** keep production changes invariant-based, not function/address/sample-specific. Do not add runtime/build dependencies on `vendor/` reference tools.
5. **Run local checks:** run the targeted test first, then the relevant crate checks/builds from the Build/Test section. If a known unrelated test is already failing, call it out explicitly.
6. **Run the focused benchmark:** rerun the exact source-semantic row with no stale decompilation or behavior cache when measuring a semantic fix. Compare behavior status, case progress, stdout/stderr, line/byte size, and static feature gaps.
7. **Check regressions:** after a focused improvement, run the broader smoke manifest or automation lane. Existing pass rows must not regress, and weighted semantic/static scores should not drop without an explicit tradeoff.
8. **Report both bars:** distinguish “mechanically changed” from “quality improved.” A merged test-only or telemetry-only change is not a semantic fix unless the row-level oracle moves.

## Regression-Prevention Workflow Prompt

Use this prompt as the standing operating model for decompiler-quality cycles:

```text
Current priority order:

1. x86 / x86-64 decompilation correctness and readable pseudocode quality.
   SLEIGH lift coverage is now good enough that day-to-day quality work should focus on source-semantic correctness and human-readable pseudocode for x86/x86-64 sample binaries.
   The goal is not only mechanically correct p-code/NIR, but final output that reads like useful C pseudocode.

   Focus areas:
   - control-flow recovery
   - if / else / switch / loop / break / continue structuring
   - pointer, array, struct, and field-access expressions
   - calling convention, parameter, and local-variable recovery
   - removal of unnecessary temporaries
   - C-friendly recovery of pointer arithmetic into array/index/field forms
   - return-value, accumulator, and loop-induction-variable cleanup
   - function-level pseudocode readability compared with Ghidra

2. Type and data abstraction.
   Improve struct, pointer, array, field access, calling convention, parameter, and local recovery at the NIR/HIR semantic layer, not by output-only substitution.

3. Large and hard function structuring.
   Improve small sample functions first, then extend to complex x86/x86-64 functions using CFG, dominance, post-dominance, SCC, dataflow, and fixed-point analysis.

4. Maintain SLEIGH lift correctness and prevent regressions.
   Do not add manual mappings in the SLEIGH engine. Keep `.sla` ConstructTpl execution as the success source, and do not grow legacy token cursor, BoundOperand fallback, or compatibility-classifier debt.
   When SLEIGH changes are necessary, validate row-level raw p-code parity first, then the canonical gate, then benchmarks.

5. FID/name recovery.
   Gradually improve packed `.fidb`, exact hash inputs, and program seeker coverage relative to Ghidra Function ID / signature / symbol ecosystems.

6. Architecture and file-format breadth.
   Expand to ARM, MIPS, PPC, ELF, and Mach-O advanced cases only after x86/x86-64 quality is strong enough.

Required principles:

1. Improve sample binary quality before real-world binary breadth.
2. Treat Ghidra as a cleanroom reference for algorithms, invariants, and edge cases.
3. Default to zero new production dependencies.
4. Prefer CFG, dominance, dataflow, fixed-point, and constraint-based reasoning over brittle pattern matching or temporary heuristics.
5. Avoid overfitting to one ISA/compiler, while keeping x86/x86-64 as the current optimization target.
6. Consider Rust libraries only when a confirmed long-term bottleneck cannot be solved internally. Do not add C++ bindings.
7. Prefer long-term maintainability and generalizable architecture over short-term output patches.
8. Make proposals and implementations valid across multiple future quality cycles, with explicit observability and verification.
9. Do not use estimates as evidence. Base claims on measured, reproducible data.
10. The final success criterion is actual improvement in `benchmark/source_semantic_benchmark` semantic correctness and pseudocode quality.

Resource rules:

- `/Users/sjkim1127/Fission/utils` contains reusable Fission resources, type information, signatures, and benchmark support data.
- Prefer existing resource loaders, `PathConfig`, `PATHS`, and resource-root mechanisms over hardcoded paths or duplicate implementations.
- Use `utils` only when it supports a maintainable semantic-layer design, not as a workaround.
- `/Users/sjkim1127/Fission/vendor` and especially `/Users/sjkim1127/Fission/vendor/ghidra/ghidra-Ghidra_12.0.4_build` are reference-only sources.
- Do not add runtime/build dependencies on vendor code, do not copy implementations, and do not add C++ bindings.

Regression-prevention workflow:

1. Start from a concrete source-semantic row or small sample function.
2. Record baseline behavior, case pass count, semantic/static score, stdout/stderr, line/byte size, and top feature gaps.
3. Diagnose the canonical owner before editing: SLEIGH/raw p-code, NIR materialization, type/data recovery, structuring, cleanup, printer, benchmark, or automation.
4. Make the smallest invariant-based production change at that owner.
5. Add targeted coverage for the invariant before or with the fix.
6. Run the targeted test, relevant crate checks/tests, and release CLI build when benchmark validation needs it.
7. Run the focused source-semantic benchmark with stale decompilation and behavior caches disabled for semantic changes.
8. Compare against the baseline and inspect candidate artifacts, not just aggregate scores.
9. Run smoke or automation regression checks after focused improvement.
10. Report whether quality improved, stayed unchanged, or regressed. Separate test/telemetry changes from semantic fixes.
11. Commit and push intermittently only from `main`, and stage only intended hunks in a dirty worktree.
```

## References

- `docs/architecture/ARCHITECTURE.md`
- `docs/adr/` — architectural decisions (ADR index lives alongside numbered entries)
- `docs/build/BUILD.md`
- `README.md`
- `.github/workflows/ci.yml`
- `.github/workflows/ci-heavy.yml`
- `.github/workflows/release-tag.yml`
