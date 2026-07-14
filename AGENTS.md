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
│   ├── fission-analysis-db/  # Typed immutable program metadata snapshots
│   ├── fission-signatures/   # FID/signature data and lookup
│   ├── fission-cli/          # CLI surface
│   ├── fission-tui/          # Terminal UI (ratatui-based AI chat)
│   └── fission-dioxus/       # Pure Rust desktop GUI (Dioxus)
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
- `crates/fission-loader/AGENTS.md`

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
| Canonical program metadata view | `crates/fission-analysis-db/` | Deterministic IDs and provenance for memory blocks, functions, symbols, and relocations; read-only downstream contract |
| Binary loaders (PE, ELF, Mach-O, TE, COFF, etc.) | `crates/fission-loader/src/loader/` | Format-specific byte parsing, section mapping, relocation, and symbol resolution |
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
8. **ISA-agnostic semantic rules** ([`docs/adr/0009-isa-agnostic-semantic-rules.md`](docs/adr/0009-isa-agnostic-semantic-rules.md)): optimize measurement on x86/x86-64, but implement register/loop/join/return/cmov-class logic as shared CFG and ABI-*slot* invariants. Put ISA differences in cspec, register namer, calling-convention tables, and SLEIGH — not as copy-pasted control-structure cores gated on `X86_32` / mnemonic / EBP offset alone.
9. **Program metadata ownership** ([`docs/adr/0010-typed-program-metadata-substrate.md`](docs/adr/0010-typed-program-metadata-substrate.md)): loader facts flow into the immutable `fission-analysis-db` snapshot. Do not add new parallel program-fact maps to `fission-pcode`, CLI, or UI layers.

## Anti-Patterns

- Do not patch semantic gaps only in printer/UI output.
- Do not add one-off binary-specific shortcuts without invariant-based guards.
- Do not duplicate the same metric definition across pcode and automation.
- Do not treat `fission-cli` or `fission-dioxus` as semantic repair layers.
- Do not treat benchmark/reporting scripts as semantic repair layers.
- Do not bypass `PathConfig`, `PATHS`, `resource_roots`, or related helpers by embedding `/Users/sjkim1127/Fission/utils` directly in implementation logic.
- Do not link against, shell out to, bind to, or otherwise depend on `/Users/sjkim1127/Fission/vendor` code in production paths.
- Do not claim success from one targeted test if crate-level regression remains.
- Do not grow parallel x86-32 / x64 / ARM copies of the same materialize, loop-carried, join, or short-circuit rule; restate once as a common invariant and supply ISA data only through models (cspec/CC/SLEIGH).

## Build / Test Commands

```bash
# CLI
cargo build -p fission-cli --release

# Common decompiler validation
cargo nextest run -p fission-pcode
cargo check -p fission-pcode
cargo check -p fission-decompiler
cargo check -p fission-automation

# Quality lane
cargo run -p fission-automation -- nir-check --lane nir

# Canonical benchmark runner
python3 benchmark/source_semantic_benchmark/run_source_semantic_benchmark.py --help
```

## Workflow Bias

- Use `cargo nextest run` as the default local Rust test runner. Use `cargo test` only when checking doctests, harness-specific behavior, or when nextest is unavailable.
- For NIR/structuring changes: targeted nextest filter → `cargo nextest run -p fission-pcode` → `cargo check -p fission-pcode`.
- For orchestration / Rust-Sleigh glue: also `cargo check -p fission-decompiler` (and CLI/TUI surfaces as needed).
- For resource path / bundle resolution changes: `cargo nextest run -p fission-core` and smoke `fission_cli resources status`.
- If telemetry/reporting changes: also run `cargo check -p fission-automation`.
- If benchmark/reporting changes: validate under `benchmark/source_semantic_benchmark/` and keep artifacts under `benchmark/artifacts/`.
- Use `.github/workflows/ci.yml` and `ci-heavy.yml` as CI source of truth.
- Release gate policy: [`docs/CI_RELEASE_GATES.md`](docs/CI_RELEASE_GATES.md) (L0 Fast / L1 Heavy / L2 Release E2E / L3 CD).
- Ship Git **release tags** via `.github/workflows/release-tag.yml` (`Release Tag (CI green)`): tags a commit only after **ci.yml push** + **ci-heavy** success on that SHA and **release-e2e** green; then `cd.yml` builds assets. Do not auto-tag every `main` push.

## Decompiler Quality Loop

Use this loop for source-semantic or pseudocode-quality work, especially when a concrete row/function motivated the change.

### Mandatory external benchmark (fission-benchmark docker)

Quality claims must run the **external** benchmark against a **local** Fission
build, not only `fission_cli` one-shots or unit tests:

- Compose base: `/Users/sjkim1127/fission-benchmark/docker-compose.yml`
- Local overlay: `/Users/sjkim1127/fission-benchmark/docker-compose.local.yml`
- Full procedure: [`docs/BENCHMARK_DOCKER.md`](docs/BENCHMARK_DOCKER.md)

```bash
cd /Users/sjkim1127/fission-benchmark
export FISSION_ROOT=/Users/sjkim1127/Fission
./scripts/prepare_local_fission.sh
docker compose -f docker-compose.yml -f docker-compose.local.yml \
  --profile local up -d --build fission
# Then: python runner/runner.py --corpus dev --decompilers fission ...
```

Never promote local docker / local runner results to official latest or Pages.

## AI Overfit Firewall

AI-assisted decompiler quality work must not leak benchmark identity into the
proposal prompt. When asking another model for review or implementation ideas,
describe only the structural failure pattern, owner evidence, invariant
candidates, forbidden shortcuts, and validation matrix. Redact function names,
addresses, binary paths, corpus row ids, and compiler tuples unless the prompt is
strictly local and will not be used for implementation advice.

Use [`docs/templates/AI_DECOMPILER_REVIEW_PROMPT.md`](docs/templates/AI_DECOMPILER_REVIEW_PROMPT.md)
for external or cross-model review prompts. The template exists to keep prompts
about invariants, not benchmark rows. Ghidra may be cited as a cleanroom
correctness/reference oracle, but do not ask models to reproduce Ghidra-specific
output style or known readability artifacts such as side effects hidden inside
conditions.

Do not repeatedly tune against dev/holdout corpus results. A semantic quality
claim needs either an unseen patch-validation-pool signal or a synthetic
invariant test in addition to the focused motivating row. Patch validation pool
results are go/stop regression evidence only; they must not be promoted into
dashboard rankings or used as a new tuning target.

This firewall is an architecture contract before it is a CI rule. Static scans
can audit for leaks, but the required gate is that any AI suggestion or benchmark
observation must be restated as an owner-native invariant in the canonical
semantic layer before production code changes.

### Pre-implementation gate

Before adding production code for builder, materialize, normalize, structuring, or type/data recovery fixes, fill out [`docs/templates/DECOMPILER_CHANGE_PROPOSAL.md`](docs/templates/DECOMPILER_CHANGE_PROPOSAL.md) as required by [`docs/adr/0006-decompiler-quality-change-gate.md`](docs/adr/0006-decompiler-quality-change-gate.md). The proposal must show row anchor, owner proof, invariant proof, and validation matrix before implementation starts.

Default to extending the existing owner/pass. Add a new pass, helper, or metric only when the proposal shows that no existing owner covers the invariant. Do not claim success from a targeted test alone; crate-level tests, focused row rerun, and smoke/automation regression checks are part of the quality claim.

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
   **Optimization target is not rule language:** motivate on m32/x64 rows, implement CFG/register/ABI-slot invariants that another ISA can reuse (ADR 0009).
6. Consider Rust libraries only when a confirmed long-term bottleneck cannot be solved internally. Do not add C++ bindings.
7. Prefer long-term maintainability and generalizable architecture over short-term output patches.
8. Make proposals and implementations valid across multiple future quality cycles, with explicit observability and verification.
9. Do not use estimates as evidence. Base claims on measured, reproducible data.
10. The final success criterion is actual improvement in `benchmark/source_semantic_benchmark` semantic correctness and pseudocode quality.
11. Prefer extending shared helpers (loop-carried update, same-block forward branch, return-join live-in, unconditional-copy merge) over new end-of-pipeline passes or ISA-local cleanups.

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

## NIR vs HIR Policy
1. **NIR (Normalized Intermediate Representation)**: Must be 100% semantically identical to the source. Correctness and semantic parity are the absolute highest priorities.
2. **HIR (High-level Intermediate Representation)**: Acts as human-readable pseudocode. It does NOT need to be 100% semantically identical, and can prioritize readability and C-friendly structures over strict, mechanical semantic equivalence.
