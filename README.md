<div align="center">

<img src="https://raw.githubusercontent.com/sjkim1127/Fission/main/image/icon.png" alt="Fission" width="140" />

# Fission

**A Rust-native binary decompilation framework.**  
Precision lifting · Canonical IR · Structured pseudocode

[![CI](https://github.com/sjkim1127/Fission/actions/workflows/ci.yml/badge.svg)](https://github.com/sjkim1127/Fission/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/Rust-1.85%2B-orange.svg)](https://www.rust-lang.org/)
[![License: AGPL-3.0-or-later](https://img.shields.io/badge/license-AGPL--3.0--or--later-blue.svg)](https://www.gnu.org/licenses/agpl-3.0.html)

</div>

---

## What is Fission?

Fission is a high-performance, Rust-native reverse engineering and decompilation framework.  
It lifts binary machine code through a fully owned intermediate representation pipeline — from raw p-code to structured, human-readable pseudocode — with no runtime dependency on external decompiler engines.

**Core pipeline:**

```
Binary  →  Sleigh lift  →  NIR (normalized IR)  →  HIR (structured pseudocode)
```

| Layer | Role |
|---|---|
| **Sleigh** | Instruction semantics via `.sla` specs — no manual opcode tables |
| **NIR** | Semantically correct normalized IR; deterministic, auditable |
| **HIR** | Human-readable pseudocode; readability > mechanical equivalence |
| **CFG / Dominance** | Sound control-flow recovery using dom/postdom/SCC analysis |

---

## Cloning

`git clone` delivers **only Rust source code, CI, and docs** — no large assets.

```bash
git clone https://github.com/sjkim1127/Fission.git

# Pull runtime assets after clone (as needed)
git lfs pull --include="utils/sleigh-specs/**"   # ~25 MB — required for decompilation
git lfs pull --include="utils/signatures/**"     # ~482 MB — FID / signature matching
```

> [!NOTE]
> `utils/` (Sleigh specs, signature DBs, Ghidra data) is stored in Git LFS.
> CI jobs pull only what each step needs. Rust builds work without any LFS assets.

---

## Quick Start

### Requirements

- **Rust** 1.85+ (`rustup` recommended)
- **Git LFS** (`brew install git-lfs` / `apt install git-lfs`)

### Build

```bash
# Pull Sleigh specs (required for decompilation)
git lfs pull --include="utils/sleigh-specs/**"

# Build CLI
cargo build -p fission-cli --release

# Run
./target/release/fission_cli --help
```

### Basic Usage

```bash
# Inspect a binary
fission_cli info <binary>

# List functions
fission_cli list <binary>

# Decompile a function by address
fission_cli decomp <binary> --addr 0x1400010a0

# Decompile all functions to JSON
fission_cli decomp <binary> --all --json
```

---

## Repository Layout

```
Fission/
├── crates/
│   ├── fission-pcode/       # NIR/HIR, CFG, structuring, printer  ← core quality work
│   ├── fission-decompiler/  # Orchestration, Rust-Sleigh bridge
│   ├── fission-sleigh/      # Sleigh decode/lift runtime
│   ├── fission-static/      # Xrefs, discovery, patch, strings
│   ├── fission-loader/      # PE / ELF / Mach-O / TE / COFF parsing
│   ├── fission-automation/  # Quality lanes, nir-check
│   ├── fission-signatures/  # FID/signature lookup
│   ├── fission-cli/         # CLI surface
│   ├── fission-tui/         # Terminal UI (ratatui AI chat)
│   └── fission-dioxus/      # Desktop GUI (Dioxus)
├── utils/                   # Runtime assets — Git LFS (local + CI selective pull)
│   ├── sleigh-specs/        # Sleigh language + compiled specs
│   ├── signatures/          # FID / FIDB / type info / patterns
│   └── ghidra-data/         # Ghidra reference data
├── .github/
│   ├── workflows/           # CI/CD (ci.yml, ci-heavy.yml, cd.yml, release-tag.yml)
│   └── fixtures/            # Minimal C fixtures used by CI smoke/NIR jobs
└── docs/                    # Architecture, ADR, build guides
```

---

## Development

### Common Commands

```bash
# Check core IR crate
cargo check -p fission-pcode

# Run tests
cargo nextest run -p fission-pcode

# Run NIR quality lane
cargo run -p fission-automation -- nir-check --lane nir

# Full build validation
cargo build -p fission-cli --release
```

### Quality Workflow

For decompiler quality work, follow the **Decompiler Quality Loop** in [`AGENTS.md`](./AGENTS.md):

1. Anchor a concrete function row (binary, address, current behavior)
2. Diagnose the canonical owner (SLEIGH / NIR / structuring / printer)
3. Fix at the owner — not at the output layer
4. Run targeted tests → crate check → NIR lane
5. Compare before/after; confirm no regressions

### CI Pipeline

| Gate | What runs |
|---|---|
| `ci.yml` | Lint, security, unit tests (Linux / macOS / Windows), CLI smoke, NIR regression |
| `ci-heavy.yml` | Miri, coverage, MSRV, full benchmark validation |
| `cd.yml` | Release builds (triggered by `release-tag.yml` on green CI) |

---

## Architecture

See [`docs/architecture/ARCHITECTURE.md`](./docs/architecture/ARCHITECTURE.md) for the full design.

Key principles:
- **Fix at the canonical owner** — never patch semantic gaps in the printer or UI
- **Algorithmic correctness** — CFG, dominance, postdom, SCC, dataflow; no pattern hacks
- **Zero new runtime dependencies** by default
- **Deterministic output** — feeds reproducible snapshots, metrics, and CI

---

## Roadmap

| Priority | Focus |
|---|---|
| 🥇 | x86/x86-64 pseudocode quality — control flow, types, calling conventions |
| 🥈 | Type & data abstraction at NIR/HIR layer |
| 🥉 | Large function structuring with fixed-point analysis |
| 4 | SLEIGH lift correctness & regression prevention |
| 5 | FID / name recovery |
| 6 | ARM / MIPS / PPC / ELF / Mach-O breadth |

---

## License

AGPL-3.0-or-later — see [`LICENSE`](./LICENSE).
