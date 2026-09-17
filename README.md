<div align="center">

<img src="https://raw.githubusercontent.com/fission-systems/Fission/main/image/fission-logo.svg" alt="Fission - reverse engineering workspace" width="760" />

[![CI](https://github.com/fission-systems/Fission/actions/workflows/ci.yml/badge.svg)](https://github.com/fission-systems/Fission/actions/workflows/ci.yml)
[![Rust](https://img.shields.io/badge/Rust-1.85%2B-orange.svg)](https://www.rust-lang.org/)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](https://www.apache.org/licenses/LICENSE-2.0)

</div>

---

# Fission

A Rust-native reverse-engineering workspace. It loads binaries, lifts
instruction semantics through Ghidra-style Sleigh specifications, and owns
everything after that — its own IR, structuring, type recovery, and rendering.

The long-term target is restoration rather than readability: taking a compiled
binary back to a project that builds and runs again, not a listing to read.
That target is **not reached**. Decompiling one binary's 68 functions and
compiling them as a single unit currently stops on 20 errors, every one of them
a missing or duplicated *declaration* rather than a wrong statement.

Because the goal is a tree that rebuilds, correctness is checked by execution
rather than by eye: `fission-dir` evaluates the decompiled body, runs the same
machine code under `fission-emulator`, and compares. What it cannot prove it
reports as an assumption instead of hiding.

## Quick start

Requires Rust 1.85+ and [`cargo-nextest`](https://nexte.st/).

```bash
git clone https://github.com/fission-systems/Fission.git
cd Fission
cargo build -p fission-cli --release
./target/release/fission_cli --help
```

Sleigh specifications and signature data live in `utils/`, which is committed —
the clone already has them, nothing to download. (Only `utils/source/`, the
inputs the packed `.fpk` tables are built from, stays out of git.)

```bash
fission_cli info    <binary>          # format, architecture, provenance
fission_cli list    <binary>          # discovered functions
fission_cli disasm  <binary> --addr 0x1400
fission_cli decomp  <binary> --addr 0x1400010a0
fission_cli decomp  <binary> --all --json
fission_cli xrefs   <binary> --to 0x140002000
```

For local iteration prefer `--profile quick-release`. `[profile.release]` uses
fat LTO and `codegen-units = 1`, which serializes linking and dominates rebuild
time; `quick-release` drops both but keeps `opt-level = 3` — measured ~2.9x
faster on a one-crate rebuild (44s → 15s) with byte-identical output on the
regression set. Use plain `--release` for anything feeding a benchmark.

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
  → CLI, TUI, GUI
```

Two output layers with different contracts:

| Layer | Contract |
|---|---|
| **NIR** | Semantically identical to the machine code. Correctness and parity come first; it is not prettified by losing behaviour. |
| **HIR** | Readable pseudocode derived from correct semantics. May drop temporaries when that improves readability without hiding what happened. |

### Structuring

Fission implements all three published approaches against one substrate and
lets them compete per function:

| Approach | Reference | Module |
|---|---|---|
| Rules over a live graph | Ghidra `CollapseStructure` | `collapse_structure.rs` |
| Schema match-and-fold | angr Phoenix | `collapse_driver.rs` |
| Reaching conditions | DREAM | `reaching_driver.rs` |

The substrate is `CollapseGraph`: a CFG that **shrinks** as regions fold, so
each match sees the already-simplified shape. Every reference implementation
folds a live graph; Fission used to analyse a static one with side tables, and
that was the root architectural gap.

Drivers do not pre-empt each other. Each *offers* a candidate and
`structuring_quality` decides, comparing raw, normalized, and full post-layout
output — a candidate that looks worse before cleanup routinely wins after it.

That admission rule is known to be optimizing the wrong thing. Forcing all six
drivers onto the near-miss functions yields zero additional exact matches, and
`goto` count does not track the structural distance benchmarks score. Finding a
signal that does — computable without the source CFG — is open work.

## Evaluation

Fission is scored on [DecBench](https://decbench.com), which measures structure
(graph edit distance against the source CFG), types (against DWARF), and
byte_match (recompile and diff). All three count only *exact* matches, so a
near miss scores the same as a miss.

Two cautions about reading those numbers, both learned here:

- **A benchmark that scores CFG shape and declared types cannot see whether the
  emitted C does what the binary does.** Several real defect classes were found
  by diffing recompiled assembly or by execution differential, and none of them
  moved a score by more than a row — dropped writes to globals, return
  addresses passed as trailing arguments, argument registers read across an
  intervening call.
- **`goto` density is not a quality metric.** Earlier releases reported it as a
  headline number. The structure metric is purely topological and `goto` count
  is not part of it; optimizing structural accuracy instead raised the count.

Current numbers, how each was reached, and the approaches measured and rejected
are in [`docs/changelog/`](docs/changelog/) and
[`docs/EVALUATION.md`](docs/EVALUATION.md).

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

`fission-dir` carries the execution differential; run it after any change that
claims to preserve semantics. Aggregate metrics must not hide row-level
regressions — a changed pseudocode file is not automatically an improvement,
and a passing synthetic test is necessary but not sufficient for a quality
claim. The reasoning is in [`AGENTS.md`](AGENTS.md#decompiler-quality-loop).

## Security

Fission analyses untrusted binaries. Sample handling rules are in
[`docs/MALWARE_SAMPLE_POLICY.md`](docs/MALWARE_SAMPLE_POLICY.md). Report
vulnerabilities through GitHub Security Advisories rather than a public issue.

## License

Apache-2.0. See [`LICENSE`](LICENSE) for the terms and [`NOTICE`](NOTICE) for
third-party attributions.
