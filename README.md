# Fission

![Fission logo](./image/logo.png)

[![CI](https://github.com/sjkim1127/Fission/actions/workflows/ci.yml/badge.svg)](https://github.com/sjkim1127/Fission/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/Rust-1.85%2B-orange.svg)](https://www.rust-lang.org/)
[![License: AGPL-3.0-or-later](https://img.shields.io/badge/license-AGPL--3.0--or--later-blue.svg)](https://www.gnu.org/licenses/agpl-3.0.html)

**Fission** is a high-performance, Rust-native reverse-engineering and decompilation framework designed for precision binary analysis at scale.

## Overview

Fission represents a fundamental rearchitecture of decompilation workflows, placing Rust at the core of:

- **Instruction Semantics**: Precision lift via Sleigh, with semantics-preserving IR normalization
- **Canonical Intermediate Representation**: NIR/HIR layers ensuring deterministic, auditable transformations
- **Control-Flow Recovery**: Graph-based structuring with algorithmic soundness, not heuristics
- **Pseudocode Rendering**: Type-aware, context-sensitive output generation

Fission pursues **independent decompilation excellence** with Ghidra available as a benchmarking and validation reference.

### Key Principles

- **Correctness-first**: Unsafe decompilation (even with high precision) fails closed to fallback modes
- **Deterministic**: All output feeds reproducible snapshots, metrics, and CI validation
- **Auditable**: Every transformation step is tracked, logged, and verifiable
- **Modular**: Each layer (lift → IR → structure → render) owns its contract independently

License: AGPL-3.0-or-later. Contributions welcome under the CLA in [`CLA.md`](./CLA.md).

---

## System Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Pseudocode Rendering                     │
│          (Type-aware formatting, symbol resolution)         │
└─────────────────────────────────────────────────────────────┘
                              ↑
┌─────────────────────────────────────────────────────────────┐
│              Structured IR (NIR/HIR Layers)                 │
│      (Control-flow recovery, loop/region detection)         │
└─────────────────────────────────────────────────────────────┘
                              ↑
┌─────────────────────────────────────────────────────────────┐
│        Canonical IR (P-Code Normalization & Semantics)      │
│     (SSA form, value numbering, dataflow analysis)          │
└─────────────────────────────────────────────────────────────┘
                              ↑
┌─────────────────────────────────────────────────────────────┐
│   Instruction Semantics & Lifting (Sleigh-based)            │
│         (Precise CFG skeleton, lift contracts)              │
└─────────────────────────────────────────────────────────────┘
```

### Core Components

| Component | Role | Ownership |
|-----------|------|-----------|
| **fission-sleigh** | Instruction decode, lift semantics, CFG skeleton | Sleigh layer |
| **fission-pcode** | Canonical IR, NIR/HIR, structuring, pseudocode printer | IR/Structure layers |
| **fission-static** | Static facts, native helpers, analysis services | Analysis layer |
| **fission-decompiler-core** | Orchestration, routing, postprocess pipeline | Workflow layer |
| **fission-loader** | Binary format parsing, symbols, sections, strings | Binary layer |
| **fission-signatures** | Function signatures, type signatures, identifier data | Data layer |
| **fission-automation** | Quality lanes, regression testing, telemetry reporting | Quality layer |
| **fission-cli** | Headless CLI (one-shot subcommands), Rhai `script`, operator `inventory` | Product layer |
| **fission-tauri** | Desktop GUI, interactive analysis, visualization | Product layer |

---

## Documentation Hub

Fission maintains comprehensive, role-based documentation:

### For Researchers & Architects
- [`docs/PROJECT_MAP.md`](./docs/PROJECT_MAP.md) — One-page repo layout (crates, `benchmark/`, `utils/`, `vendor/`)
- [`docs/architecture/ARCHITECTURE.md`](./docs/architecture/ARCHITECTURE.md) — Detailed system design and invariants
- [`AGENTS.md`](./AGENTS.md) — Contributor workflows and conventions
- [`benchmark/full_benchmark/README.md`](./benchmark/full_benchmark/README.md) — Canonical decompilation benchmark workflow

### For Operators & Users
- [Wiki Home](https://github.com/sjkim1127/Fission/wiki) — Tutorials, guides, FAQ
- [`wiki/DOCUMENTATION_HUB.md`](./wiki/DOCUMENTATION_HUB.md) — Wiki vs repository doc split (browse in-tree); mirrors [Documentation Hub](https://github.com/sjkim1127/Fission/wiki/DOCUMENTATION_HUB) on GitHub Wiki
- [Getting Started](https://github.com/sjkim1127/Fission/wiki/Getting-Started) — Installation and first steps
- [User Guides](https://github.com/sjkim1127/Fission/wiki/User-Guides) — Workflow documentation
- [`docs/onboarding/FIRST_30_MINUTES.md`](./docs/onboarding/FIRST_30_MINUTES.md) — Contributor-oriented first-session checklist (repository docs)
- [`docs/EVALUATION.md`](./docs/EVALUATION.md) — External headless evaluation path for CLI-first users
- [`docs/CLI.md`](./docs/CLI.md) — Detailed `fission_cli` reference and operator workflow guide

### Release & Changelog
- [`docs/VERSIONING.md`](./docs/VERSIONING.md) — SemVer rules and tagging (`v*.*.*`)
- [`docs/RELEASE.md`](./docs/RELEASE.md) — Maintainer checklist aligned with CD builds
- [`THIRD_PARTY.md`](./THIRD_PARTY.md) — Vendored upstream provenance (CLA § third-party)
- [`SECURITY.md`](./SECURITY.md) — Coordinated disclosure and sample-handling expectations
- [`docs/changelog/Legacy/CHANGELOG.md`](./docs/changelog/Legacy/CHANGELOG.md) — Historical release log
- [`docs/changelog/Legacy/`](./docs/changelog/Legacy/) — Archived dated development logs (`YYYYMMDD_Changelog.md`) and legacy rollup

---

## Current Capabilities

### Decompilation Paths

| Path | Status | Coverage | Notes |
|------|--------|----------|-------|
| **NIR (Rust-native)** | Primary | PE x64, ARM64 | Canonical Rust architecture path |

### Supported Binary Formats

- **PE** (Windows x86, x64, ARM64) — Full support
- **ELF** (Linux x86, x64, ARM, ARM64) — Core support
- **Mach-O** (macOS x64, ARM64) — Experimental

### Project Maturity Status

**Solid & Production-Ready:**
- ✅ Headless CLI (`fission_cli`: subcommands, JSON/automation paths, `inventory`, Rhai `script`)
- ✅ Rust-native decompilation pipeline
- ✅ Quality assurance and regression testing
- ✅ Automated benchmarking against Ghidra
- ✅ Deterministic, reproducible output

**In Active Development:**
- 🔄 Large function readability and precision
- 🔄 Advanced data abstraction and memory modeling
- 🔄 Rich type inference and name recovery
- 🔄 Desktop UI polish and end-user experience
- 🔄 Additional architecture targets (MIPS, PPC, etc.)

**Technology Notes:**
PE x64 has the strongest direct NIR coverage. Other architectures and formats exist as development targets and should not be treated as equivalent production-quality claims.

---

## Repository Layout

### Core Decompiler Modules

| Crate | Responsibility | Key Artifacts |
|-------|-----------------|----------------|
| [`crates/fission-sleigh`](./crates/fission-sleigh) | Instruction decode, semantics lift, CFG skeleton | Sleigh bindings, lift contracts |
| [`crates/fission-pcode`](./crates/fission-pcode) | Canonical IR, NIR/HIR layers, structuring, printing | P-Code IR, graph reduction, pseudocode output |
| [`crates/fission-static`](./crates/fission-static) | Static fact generation, prepare helpers, analysis | Dominance, SCC, value analysis |
| [`crates/fission-decompiler-core`](./crates/fission-decompiler-core) | Orchestration, routing, postprocess pipeline | End-to-end workflow |

### Supporting Modules

| Crate | Responsibility |
|-------|-----------------|
| [`crates/fission-loader`](./crates/fission-loader) | Binary loading, symbol extraction, section parsing |
| [`crates/fission-signatures`](./crates/fission-signatures) | Function/type signatures, identifier resolution |
| [`crates/fission-core`](./crates/fission-core) | Core data structures |
| [`crates/fission-dynamic`](./crates/fission-dynamic) | Dynamic analysis capabilities |

### Product Surfaces

| Crate | Purpose |
|-------|---------|
| [`crates/fission-cli`](./crates/fission-cli) | Headless one-shot CLI and operator workflows |
| [`crates/fission-tauri`](./crates/fission-tauri) | Cross-platform desktop GUI |
| [`crates/fission-automation`](./crates/fission-automation) | Quality lanes, test automation, CI/CD integration |

---

## Quick Start

### Prerequisites

- **Rust** 1.85+ ([install](https://www.rust-lang.org/tools/install))
- **Cargo** (bundled with Rust)
- C++ compiler (for some dependencies)

### Build the CLI

```bash
git clone https://github.com/sjkim1127/Fission.git
cd Fission
cargo build -p fission-cli --release
```

The compiled binary is available at: `target/release/fission_cli`

### Basic Usage

```bash
# Display binary information
./target/release/fission_cli info <binary>

# Decompile a single function at address
./target/release/fission_cli decomp <binary> --addr <address>

# List discovered functions
./target/release/fission_cli list <binary> --json

# Batch decompilation with limits
./target/release/fission_cli decomp <binary> --all --limit 100

# Operator-facing inventory
./target/release/fission_cli inventory function-facts <binary> --json
```

Legacy flat invocations still work for one transition period, but canonical
usage is now subcommand-based.

For the full command model, subcommand ownership, operator inventory workflows,
JSON guidance, and legacy compatibility rules, see
[`docs/CLI.md`](./docs/CLI.md).

If you are evaluating Fission externally and want the shortest CLI-first path,
use [`docs/EVALUATION.md`](./docs/EVALUATION.md). That guide is opinionated,
Windows x64-first, and includes checked-in sample binaries plus example output
payloads.

Library-level use is possible at the Rust crate level, but the CLI is the
current primary documented product surface.

If you want comparative evaluation rather than a first manual CLI pass, use the
canonical benchmark workflow in
[`benchmark/full_benchmark/README.md`](./benchmark/full_benchmark/README.md).

### Run the Desktop GUI

The desktop application lives in [`crates/fission-tauri`](./crates/fission-tauri)
and uses Tauri + Vite for the UI shell.

```bash
# Install GUI frontend dependencies once
cd crates/fission-tauri
npm install

# Launch the desktop GUI in development mode
npm run tauri -- dev
```

For a production desktop build:

```bash
cd crates/fission-tauri
npm run tauri -- build
```

### Run Quality Assurance

Execute the main quality lane for regression testing:

```bash
cargo run -p fission-automation -- nir-check --lane nir
```

### Build All Products

```bash
# Release build (optimized)
cargo build --release

# Desktop GUI shell
cd crates/fission-tauri
npm run tauri -- build

# Full test suite
cargo test --all
```

---

## Engineering Status

### Production-Ready Components ✅

- **Decompilation Pipeline**: Full Rust-native NIR/HIR path with deterministic output
- **Command-Line Interface**: One-shot subcommands with JSON/inventory surfaces and optional Rhai `script` (no interactive REPL or TUI in `fission-cli`)
- **Quality Assurance**: Integrated regression testing and automated benchmarking
- **Binary Support**: PE x64 (primary), ELF x64/ARM64, Mach-O (experimental)
- **Telemetry**: Built-in metrics, statistics, and CI/CD reporting

### Active Development Areas 🔄

| Area | Target | Timeline |
|------|--------|----------|
| **Large Function Handling** | >10K instruction functions | Q2 2026 |
| **Data Abstraction** | Field/type-aware modeling | Q2 2026 |
| **Name Recovery** | Symbol and identifier inference | Q3 2026 |
| **UI/UX Polish** | Desktop workflow optimization | Q3 2026 |
| **Additional Targets** | MIPS, PPC, additional architectures | Q4 2026 |

### Known Limitations

- Large functions (>10K instructions) may produce simplified output
- Advanced data abstraction patterns in progress
- Limited cross-architecture coverage (PE x64 is primary target)
- Desktop UI is functional but undergoing refinement

---

## Advanced Usage

### Benchmark Against Ghidra

For comparative quality analysis:

```bash
python3 benchmark/full_benchmark/full_decomp_benchmark.py \
  <binary> \
  --fission-bin target/release/fission_cli \
  --ghidra-dir vendor/ghidra/ghidra_11.4.2_PUBLIC \
  --output-dir benchmark/artifacts/full_benchmark/<run-name> \
  --limit 50
```

Canonical benchmark config and artifacts now live under:

- [`benchmark/config/benchmark_corpus/`](./benchmark/config/benchmark_corpus/)
- [`benchmark/artifacts/full_benchmark/`](./benchmark/artifacts/full_benchmark/)
- [`benchmark/artifacts/automation/`](./benchmark/artifacts/automation/)

Use `benchmark_compact_summary.json` for first-pass machine review and the
verbose JSON/Markdown artifacts for deep debugging.

### Inspect Quality Reports

Automated quality metrics are stored in:

```
benchmark/artifacts/automation/          # Fast-lane test results
benchmark/artifacts/full_benchmark/      # Detailed benchmark runs
```

### Extended Architecture

For detailed system design, read [`docs/architecture/ARCHITECTURE.md`](./docs/architecture/ARCHITECTURE.md)

---

## User Interface

### Desktop Application

The Fission desktop GUI provides an integrated analysis environment:

**Main Workspace**
![Fission main screen](./image/main_screen.jpeg)

**Decompilation View**
![Fission decompile view](./image/decompile.jpeg)

Features:
- Interactive function browser with call graphs
- Real-time decompilation with syntax highlighting
- Symbol resolution and type inference
- Batch analysis and report generation
- Cross-reference navigation

---

## Contributing

Fission welcomes contributions from the reverse-engineering and decompilation communities.

### Getting Started

1. Review [`CONTRIBUTING.md`](./CONTRIBUTING.md) for guidelines
2. Sign the Contributor License Agreement ([`CLA.md`](./CLA.md))
3. Check [`AGENTS.md`](./AGENTS.md) for code organization and conventions
4. Open an issue to discuss your proposed changes

### Contribution Areas

- **Instruction Semantics**: Accuracy improvements for Sleigh lifts
- **IR Transformations**: New optimizations and normalization passes
- **Structuring Algorithms**: Control-flow recovery improvements
- **Binary Format Support**: Additional architectures and formats
- **Testing & Benchmarking**: Quality metrics and regression detection
- **Documentation**: Tutorials, guides, and architectural documentation

---

## Community & Support

### Communication

- **Issues & Discussions**: [GitHub Issues](https://github.com/sjkim1127/Fission/issues)
- **Discord Community**: [Join our server](https://discord.gg/dgzqGwBpcE)
- **Social Media**: [LinkedIn](https://www.linkedin.com/in/sung-joo-kim-718a93303/)

### Learning Resources

- [Reverse-Engineering Workflows Wiki](https://github.com/sjkim1127/Fission/wiki/Reverse-Engineering-Workflows)
- [Contributor Onboarding](https://github.com/sjkim1127/Fission/wiki/Contributor-Onboarding)
- [Troubleshooting Guide](https://github.com/sjkim1127/Fission/wiki/Troubleshooting)
- [FAQ](https://github.com/sjkim1127/Fission/wiki/FAQ)

---

## Vision & Long-Term Direction

Fission is architected for **project-level software restoration** — not just decompilation.

### Current Focus (2026)
✅ High-precision decompilation for PE x64  
✅ Deterministic, auditable analysis pipelines  
✅ Measurable quality metrics and benchmarking  

### Medium-Term (2026-2027)
🔄 Expanded architecture support  
🔄 Advanced data abstraction and memory modeling  
🔄 Integrated static/dynamic analysis workflows  
🔄 Semantic-aware type recovery  

### Long-Term Vision (2027+)
🎯 Project-level program comprehension  
🎯 Cross-function fact accumulation  
🎯 AI-assisted analysis on verified artifacts  
🎯 Protocol-facing and behavioral analysis integration  
🎯 Commercial-grade analysis platform  

### Design Philosophy

Rather than building a thin UI over existing decompilers, Fission pursues **independent decompilation excellence** with:

- **Algorithmic Soundness**: Graph-based, mathematically rigorous transformations
- **Auditability**: Every decision is verifiable and reproducible
- **Modularity**: Clean separation of concerns across layers
- **Quality Focus**: Metrics and regression detection as first-class citizens
- **Long-term Maintenance**: Sustainable, understandable codebase

---

## License & Citation

```
SPDX-License-Identifier: AGPL-3.0-or-later
```

**License**: GNU Affero General Public License v3.0 or later  
See [`LICENSE`](./LICENSE) for full text

### Citation

If you use Fission in academic work, please cite:

```bibtex
@software{fission2024,
  title={Fission: A Rust-Native Decompilation Framework},
  author={Kim, Sung Joo},
  year={2024},
  url={https://github.com/sjkim1127/Fission}
}
```

---

## Acknowledgments

Fission builds upon decades of decompilation research and engineering. Special acknowledgment to:

- **Ghidra** — Reference architecture, semantic lifting, benchmarking
- **RetDec** — Decompilation techniques and IR design
- **Radare2** — Analysis ecosystem and tooling inspiration
- **LLVM** — Compiler infrastructure and optimization patterns
- The reverse-engineering research community
