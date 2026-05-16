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
| Reference algorithms | `vendor/`, especially `vendor/ghidra/` and `vendor/retdec-5.0/` | Reference these often for invariants and behavior, but do not add runtime/build dependencies, bindings, or copied implementation shortcuts |

## Core Rules

1. Fix behavior at the canonical owner, not downstream UI/surface layers.
2. Prefer algorithmic CFG / dom / postdom / SCC facts over lexical or binary-specific shortcuts.
3. Use typed contracts; do not invent parallel telemetry payloads outside `NirBuildStats`.
4. Keep behavior deterministic when outputs feed snapshots, metrics, or automation comparisons.
5. Large refactors are acceptable when they reduce long-term complexity and tighten ownership.
6. Do not hardcode repository-local resource paths in code; route reusable resources through existing `utils`-backed path discovery/configuration when applicable.
7. Treat `vendor` as a reference corpus only: use it to understand algorithms and invariants, but keep Fission-owned Rust implementations dependency-free from that tree.

## Anti-Patterns

- Do not patch semantic gaps only in printer/UI output.
- Do not add one-off binary-specific shortcuts without invariant-based guards.
- Do not duplicate the same metric definition across pcode and automation.
- Do not treat `fission-cli` or `fission-tauri` as semantic repair layers.
- Do not treat benchmark/reporting scripts as semantic repair layers.
- Do not bypass `PathConfig`, `PATHS`, `resource_roots`, or related helpers by embedding `/Users/sjkim1127/Fission/utils` directly in implementation logic.
- Do not link against, shell out to, bind to, or otherwise depend on `vendor/` code in production paths.
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

## References

- `docs/architecture/ARCHITECTURE.md`
- `docs/adr/` — architectural decisions (ADR index lives alongside numbered entries)
- `docs/build/BUILD.md`
- `README.md`
- `.github/workflows/ci.yml`
- `.github/workflows/ci-heavy.yml`
- `.github/workflows/release-tag.yml`
