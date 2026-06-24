# Fission project map

**Last verified:** 2026-05-02

Single-page orientation for directories and crates. **Canonical ownership** for engineering tasks remains [`AGENTS.md`](../AGENTS.md) (“Where To Look” table). **System design** remains [`docs/architecture/ARCHITECTURE.md`](architecture/ARCHITECTURE.md).

## Workspace crates (`crates/`)

Declared in root [`Cargo.toml`](../Cargo.toml) `workspace.members`:

| Crate path | Role (summary) |
|------------|----------------|
| [`crates/fission-automation`](../crates/fission-automation) | Quality lanes (`nir-check`), reporting |
| [`crates/fission-core`](../crates/fission-core) | Shared core types/utilities |
| [`crates/fission-loader`](../crates/fission-loader) | Binary loading, sections, symbols, relocations, virtual types; structured **`loader::identity`** report (entropy/overlay/PE hints + evidence) |
| [`crates/fission-pcode`](../crates/fission-pcode) | Canonical IR, NIR/HIR, structuring, CFG analysis, printer |
| [`crates/fission-signatures`](../crates/fission-signatures) | Signature datasets / lookup |
| [`crates/fission-static`](../crates/fission-static) | Static facts, orchestration helpers, analysis services (`analysis`, `utils`) |
| [`crates/fission-dynamic`](../crates/fission-dynamic) | Dynamic analysis support |
| [`crates/fission-ttd`](../crates/fission-ttd) | Time-travel / trace-adjacent support |
| [`crates/fission-plugin`](../crates/fission-plugin) | Plugin contracts (`contracts`), manager/loader/hooks (`interactive_runtime`) |
| [`crates/fission-cli`](../crates/fission-cli) | CLI product (`fission_cli`) |
| [`crates/fission-decompiler`](../crates/fission-decompiler) | Decompilation orchestration, Rust-Sleigh bridge, routing/workers |
| [`crates/fission-sleigh`](../crates/fission-sleigh) | Sleigh decode/lift; CFG skeleton |
| [`crates/fission-tui`](../crates/fission-tui) | Terminal UI (ratatui-based AI chat interface) |
| `crates/fission-dioxus` | Pure Rust desktop GUI (Dioxus Desktop) — planned |

## Top-level directories

| Path | Purpose |
|------|---------|
| [`benchmark/`](../benchmark) | Corpus configs, curated binaries/fixtures, full_benchmark Python harness, automation artifacts layout |
| [`docs/`](../docs) | Versioned guides: architecture, CLI, evaluation, changelog, onboarding (this tree) |
| [`scripts/`](../scripts) | Benchmark/test helpers (`scripts/benchmark`, `scripts/test`, `scripts/corpus`, …) |
| [`utils/`](../utils) | Checked-in specs/data (see [`utils/MANIFEST.md`](../utils/MANIFEST.md)) |
| [`vendor/`](../vendor) | Third-party reference trees (see [`vendor/MANIFEST.md`](../vendor/MANIFEST.md), [`THIRD_PARTY.md`](../THIRD_PARTY.md)) |
| [`.github/workflows/`](../.github/workflows) | CI/CD workflows (`ci.yml`, `ci-heavy.yml`, `cd.yml`, `reusable-*.yml`) |

## Where to read next

- Contributor rules: [`AGENTS.md`](../AGENTS.md), [`CONTRIBUTING.md`](../CONTRIBUTING.md)
- Third-party provenance: [`THIRD_PARTY.md`](../THIRD_PARTY.md)
- Release expectations: [`docs/RELEASE.md`](RELEASE.md), [`docs/VERSIONING.md`](VERSIONING.md)
- Trend metrics (JSON contracts): [`docs/QUALITY_METRICS.md`](QUALITY_METRICS.md)
- Issue labels (taxonomy): [`docs/contributing/LABELS.md`](contributing/LABELS.md)
