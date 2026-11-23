# Fission project map

Single-page orientation for directories and crates. **Canonical ownership** for engineering tasks remains [`AGENTS.md`](https://github.com/fission-systems/Fission/blob/main/AGENTS.md) (“Where To Look” table). **System design** remains [`docs/architecture/ARCHITECTURE.md`](architecture/ARCHITECTURE.md).

## Workspace crates (`crates/`)

Declared in root [`Cargo.toml`](https://github.com/fission-systems/Fission/blob/main/Cargo.toml) `workspace.members`:

| Crate path | Role (summary) |
|------------|----------------|
| [`crates/fission-core`](https://github.com/fission-systems/Fission/tree/main/crates/fission-core) | Shared core types/utilities |
| [`crates/fission-loader`](https://github.com/fission-systems/Fission/tree/main/crates/fission-loader) | Binary loading, sections, symbols, relocations, virtual types; structured **`loader::identity`** report (entropy/overlay/PE hints + evidence) |
| [`crates/fission-analysis-protocol`](https://github.com/fission-systems/Fission/tree/main/crates/fission-analysis-protocol) | Result shapes `fission_cli` prints as JSON (`list`, `callgraph`, `disasm`) |
| [`crates/fission-analysis-db`](https://github.com/fission-systems/Fission/tree/main/crates/fission-analysis-db) | Immutable typed program metadata snapshots |
| [`crates/fission-project`](https://github.com/fission-systems/Fission/tree/main/crates/fission-project) | Persistent analyst decisions, binary identity binding, and edit history for `db` |
| [`crates/fission-pcode-ir`](https://github.com/fission-systems/Fission/tree/main/crates/fission-pcode-ir) | p-code IR, p-code optimizer, block-level CFG facts (leaf; no workspace dependencies) |
| [`crates/fission-cspec`](https://github.com/fission-systems/Fission/tree/main/crates/fission-cspec) | cspec / ldefs / pspec parsing, register model (`RegisterNamer`), entry parameter-arity inference (ADR 0012 amendment) |
| [`crates/fission-pcode`](https://github.com/fission-systems/Fission/tree/main/crates/fission-pcode) | NIR/HIR, structuring, CFG analysis, printer; re-exports `fission-pcode-ir` under the historical paths |
| `crates/fission-midend-core` | Midend shared substrate (**owns** IR / action_pipeline / wave_stats / labels; ADR 0012) |
| `crates/fission-midend-prehir` | Mutable flat PreHIR AST used by static normalize and structuring; not the DIR product (ADR 0014) |
| `crates/fission-midend-normalize` | Normalize owner (**owns** normalize pipeline source; ADR 0012) |
| `crates/fission-midend-structuring` | Structuring free-function owners + pcode PreviewBuilder residual (ADR 0012) |
| `crates/fission-dir` | Experimental P-code-rooted DIR product, native pure-region reconstruction, emulator/solver observation verification, and transition diagnostics (ADR 0014) |
| [`crates/fission-signatures`](https://github.com/fission-systems/Fission/tree/main/crates/fission-signatures) | Signature datasets / lookup |
| [`crates/fission-static`](https://github.com/fission-systems/Fission/tree/main/crates/fission-static) | Static facts, orchestration helpers, analysis services (`analysis`, `utils`) |
| [`crates/fission-dynamic`](https://github.com/fission-systems/Fission/tree/main/crates/fission-dynamic) | Dynamic analysis support |
| [`crates/fission-emulator`](https://github.com/fission-systems/Fission/tree/main/crates/fission-emulator) | P-code execution engine and OS high-level emulation; backs the execution differential |
| [`crates/fission-solver`](https://github.com/fission-systems/Fission/tree/main/crates/fission-solver) | Native SMT/SAT stack (AST, AIG, CNF, SAT, theory) used by symbolic and concolic paths |
| [`crates/fission-ttd`](https://github.com/fission-systems/Fission/tree/main/crates/fission-ttd) | Time-travel / trace-adjacent support |
| `crates/fission-causal` | Experimental CSIC (RFC #1101 Track B): one-variable counterfactual interventions on emulator replays; no production dependents |
| [`crates/fission-plugin`](https://github.com/fission-systems/Fission/tree/main/crates/fission-plugin) | Plugin contracts (`contracts`), manager/loader/hooks (`interactive_runtime`) |
| [`crates/fission-cli`](https://github.com/fission-systems/Fission/tree/main/crates/fission-cli) | CLI product (`fission_cli`) |
| [`crates/fission-script`](https://github.com/fission-systems/Fission/tree/main/crates/fission-script) | Rust-native embedded scripting (Rhai) for read-only binary automation |
| [`crates/fission-decompiler`](https://github.com/fission-systems/Fission/tree/main/crates/fission-decompiler) | Decompilation orchestration, Rust-Sleigh bridge, routing/workers |
| [`crates/fission-sleigh`](https://github.com/fission-systems/Fission/tree/main/crates/fission-sleigh) | Sleigh decode/lift; CFG skeleton |

## Top-level directories

| Path | Purpose |
|------|---------|
| [`benchmark/`](https://github.com/fission-systems/Fission/tree/main/benchmark) | Corpus configs and patch-validation pool (see [`BENCHMARK_DECBENCH.md`](BENCHMARK_DECBENCH.md)) |
| [`docs/`](index.md) | Versioned guides: architecture, CLI, evaluation, changelog, onboarding (this tree) |
| [`scripts/`](https://github.com/fission-systems/Fission/tree/main/scripts) | Benchmark/test helpers (`scripts/quality`, `scripts/ci`, `scripts/release`, `scripts/test`) |
| [`utils/`](https://github.com/fission-systems/Fission/tree/main/utils) | Checked-in specs/data (see [`utils/MANIFEST.md`](https://github.com/fission-systems/Fission/blob/main/utils/MANIFEST.md)) |
| `vendor/` (local) | Optional reference trees described in [`THIRD_PARTY.md`](https://github.com/fission-systems/Fission/blob/main/THIRD_PARTY.md) |
| [`.github/workflows/`](https://github.com/fission-systems/Fission/tree/main/.github/workflows) | CI/CD: `ci.yml` (L0), `ci-heavy.yml` (L1), `release-e2e.yml` (L2), `release-tag.yml`, `cd.yml` (L3), `reusable-*.yml` |

## Where to read next

- Contributor rules: [`AGENTS.md`](https://github.com/fission-systems/Fission/blob/main/AGENTS.md), [`CONTRIBUTING.md`](https://github.com/fission-systems/Fission/blob/main/CONTRIBUTING.md)
- Third-party provenance: [`THIRD_PARTY.md`](https://github.com/fission-systems/Fission/blob/main/THIRD_PARTY.md)
- Release gates (conservative L0–L3): [`docs/CI_RELEASE_GATES.md`](CI_RELEASE_GATES.md)
- Release expectations: [`docs/RELEASE.md`](RELEASE.md), [`docs/VERSIONING.md`](VERSIONING.md)
- Trend metrics (JSON contracts): [`docs/QUALITY_METRICS.md`](QUALITY_METRICS.md)
- Issue labels (taxonomy): [`docs/contributing/LABELS.md`](contributing/LABELS.md)
