<div align="center">

<img src="https://raw.githubusercontent.com/fission-systems/Fission/main/image/fission-logo.svg" alt="Fission - reverse engineering workspace" width="760" />

[![CI](https://github.com/fission-systems/Fission/actions/workflows/ci.yml/badge.svg)](https://github.com/fission-systems/Fission/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/Rust-1.85%2B-orange.svg)](https://www.rust-lang.org/)
[![License: AGPL-3.0-or-later](https://img.shields.io/badge/license-AGPL--3.0--or--later-blue.svg)](https://www.gnu.org/licenses/agpl-3.0.html)

</div>

---

# Fission

Fission is a Rust-native reverse-engineering and decompilation workspace. It
loads binaries, lifts instruction semantics through Ghidra-style Sleigh
specifications, and owns everything after that — its own IR, structuring,
type recovery, rendering, and quality gates — in Rust.

The goal is not to decode instructions. It is to produce output that is
mechanically traceable, semantically defensible, and readable enough to be
worth reading.

## Where it stands

Measured on [DecBench](https://decbench.com)'s sample set: 224 binaries,
250 functions, compared per file. Not aggregate totals — every claim below
survived a per-function diff.

| | v0.1.9 | v0.2.1 |
|---|---|---|
| Explicit `goto`s (NIR) | 2,089 | **621** (−70.3%) |
| Explicit `goto`s (HIR) | 1,983 | **621** (−68.7%) |
| Functions decompiled | 250/250 | 250/250 |
| Binaries | 224/224 | 224/224 |

On the 204 functions Fission, Ghidra, and angr all decompile:

| Decompiler | `goto`s |
|---|---|
| angr 9.2 | 420 |
| **Fission** | **597** |
| Ghidra 12.0 | 691 |

Control flow is one axis, and not the one that decides whether output is
*correct*. Fission also runs an execution differential: evaluate the
decompiled body, run the same machine code under `fission-emulator`, and
compare. It catches semantics-preserving claims that are not — including a
deliberately injected "negate every `if` condition" sabotage.

See [`docs/changelog/v0.2.0.md`](docs/changelog/v0.2.0.md) for how the
structuring numbers were reached, including the approaches that were
measured and rejected.

## Quick start

Requires Rust 1.85+ and [`cargo-nextest`](https://nexte.st/).

```bash
git clone https://github.com/fission-systems/Fission.git
cd Fission
```

Real decompilation needs Sleigh specifications and signature data. Both are
in `utils/`, which is committed — the clone already has them, so there is
nothing to download. (Only `utils/source/`, the inputs the packed `.fpk`
tables are built from, stays out of git.)

```bash
cargo build -p fission-cli --release
./target/release/fission_cli --help
```

For local iteration prefer `--profile quick-release`: `[profile.release]`
uses fat LTO and `codegen-units = 1`, which serializes linking and dominates
rebuild time. `quick-release` drops both but keeps `opt-level = 3` —
measured ~2.9x faster on a one-crate rebuild (44s → 15s) with byte-identical
output on the regression set. Use plain `--release` for anything feeding a
benchmark or a perf measurement.

```bash
fission_cli info    <binary>          # format, architecture, provenance
fission_cli list    <binary>          # discovered functions
fission_cli decomp  <binary> --addr 0x1400010a0
fission_cli decomp  <binary> --all --json
```

Full command reference: [`docs/CLI.md`](docs/CLI.md).

## How it works

```text
Binary bytes
  → fission-loader          format, sections, symbols, imports
  → fission-static          facts and provenance
  → fission-sleigh          decode and raw p-code lift
  → fission-pcode  NIR      canonical semantics
  → fission-pcode  HIR      human-readable derivation
  → structuring, cleanup, rendering
  → fission-decompiler      result contracts
  → CLI, TUI, GUI, automation
```

Two output layers with different contracts:

| Layer | Contract |
|---|---|
| **NIR** | Semantically identical to the machine code. Correctness and parity come first; it is not prettified by losing behaviour. |
| **HIR** | Readable pseudocode derived from correct semantics. May drop temporaries when that improves readability without hiding what happened. |

### Structuring

The interesting part. Fission implements all three published approaches
against one substrate and lets them compete per function:

| Approach | Reference | Module |
|---|---|---|
| Rules over a live graph | Ghidra `CollapseStructure` | `collapse_structure.rs` |
| Schema match-and-fold | angr Phoenix | `collapse_driver.rs` |
| Reaching conditions | DREAM | `reaching_driver.rs` |

The substrate is `CollapseGraph`: a CFG that **shrinks** as regions fold, so
each match sees the already-simplified shape. Every reference implementation
folds a live graph; Fission used to analyse a static one with side tables,
and that was the root architectural gap.

Drivers do not pre-empt each other. Each *offers* a candidate and
`structuring_quality` decides, comparing raw, normalized, and full
post-layout output — a candidate that looks worse before cleanup routinely
wins after it. `goto` count must strictly drop; destroying a recovered
`switch` or leaving empty `if` shells is a hard veto; nesting depth, guard
formula size, and statement count are budgets scaled by how many jumps the
candidate actually removed.

## Where to look next

| | |
|---|---|
| [`AGENTS.md`](AGENTS.md) | Working rules, quality loop, anti-patterns. Read this before changing decompiler behaviour. |
| [`docs/PROJECT_MAP.md`](docs/PROJECT_MAP.md) | Crate-by-crate ownership map |
| [`docs/CLI.md`](docs/CLI.md) | Command reference |
| [`docs/QUALITY_METRICS.md`](docs/QUALITY_METRICS.md) | What is measured and how |
| [`docs/EVALUATION.md`](docs/EVALUATION.md) | Benchmark lanes and evidence rules |
| [`docs/architecture/DYNAMIC_ANALYSIS.md`](docs/architecture/DYNAMIC_ANALYSIS.md) | Emulator, TTD, symbolic execution, taint, concolic exploration |
| [`docs/architecture/`](docs/architecture/) | Pipeline architecture, diagrams, Ghidra parity audit |
| [`docs/adr/`](docs/adr/) | Architecture decision records |
| [`docs/changelog/`](docs/changelog/) | Release notes, newest first |
| [`docs/proposals/`](docs/proposals/) | Designs in flight, including rejected ones and why |
| [`docs/contributing/FIELD_GUIDE_AND_PLAYBOOKS.md`](docs/contributing/FIELD_GUIDE_AND_PLAYBOOKS.md) | Per-area playbooks, review question bank, handoff template |
| [`docs/contributing/TROUBLESHOOTING.md`](docs/contributing/TROUBLESHOOTING.md) | Common local failures, and the glossary |

Ownership is strict: a semantic problem in the final pseudocode gets fixed
where the behaviour is owned, never in the renderer. Vendor trees under
`vendor/` are references for reading only — no production path may depend on
them at build or runtime.

## Testing

```bash
cargo nextest run --workspace          # full suite
cargo nextest run -p fission-pcode     # one crate
cargo build --workspace --all-targets  # compile everything
```

`fission-dir` carries the execution differential; run it after any change
that claims to preserve semantics. Aggregate metrics must not hide
row-level regressions — a changed pseudocode file is not automatically an
improvement, and a passing synthetic test is necessary but not sufficient
for a quality claim. The reasoning is in
[`AGENTS.md`](AGENTS.md#decompiler-quality-loop).

## Security

Fission analyses untrusted binaries. Sample handling rules are in
[`docs/MALWARE_SAMPLE_POLICY.md`](docs/MALWARE_SAMPLE_POLICY.md). Report
vulnerabilities through GitHub Security Advisories rather than a public
issue.

## License

AGPL-3.0-or-later. See [`LICENSE`](LICENSE).
