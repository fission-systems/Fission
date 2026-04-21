# Fission External Evaluation Guide

## Summary

This guide is the canonical external evaluation path for Fission.

It is written for teams evaluating Fission in a headless workflow from the CLI, without depending on the desktop UI or internal contributor documentation.

Today, the best-supported evaluation surface is:

- `fission_cli`
- Windows x64 binaries
- one-function decompilation and JSON/text output inspection

Do not assume, yet:

- a polished stable library API
- Python bindings as a primary supported product surface
- production-grade parity across every architecture and file format

The current recommendation is:

1. validate the CLI first
2. inspect one or two real decompilation outputs
3. use the operator-oriented inventory path if you need machine-readable artifacts
4. run the benchmark only after basic CLI success

For the full CLI reference, see [CLI.md](./CLI.md).

## What Fission Is Today

Fission is a Rust-native decompilation pipeline built around Sleigh-based instruction semantics, Rust-owned p-code/NIR/HIR normalization, graph-based structuring, and pseudocode rendering.

The most mature product surface today is the CLI. Rust crate integration is possible, but the external product story is still CLI-first rather than library-first.

## Capability Boundaries

### Stable enough to evaluate now

- headless CLI usage
- binary metadata inspection
- function discovery
- targeted disassembly
- targeted decompilation
- machine-readable JSON output
- operator-grade function inventory emit

### Still evolving

- final pseudocode readability on harder functions
- stable public library API boundaries
- Python binding story
- broad architecture parity claims beyond the current strongest Windows x64 path

## Evaluation Sample Set

The recommended first-run sample surface in this checkout is:

- [benchmark/binary/x86-64/window/small/](../benchmark/binary/x86-64/window/small/)

This directory is checked into the repository and is the preferred external evaluation set for this guide.

Recommended starter binaries:

- `binary/c/test_functions.exe`
  - basic control flow
  - simple arithmetic
  - clean first `info/list/decomp` run
- `binary/c/structs_and_pointers.exe`
  - struct and pointer surfacing
- `binary/c/bitops_and_control_flow.exe`
  - bit operations and branch-heavy logic
- `binary/c/function_pointers_and_strings.exe`
  - function pointers, strings, and indirect-style patterns
- `binary/c/math_operations.exe`
  - arithmetic-heavy patterns
- `binary/c/array_operations.exe`
  - array indexing, loops, and sorting-style logic

If the compiled binaries are not present in your checkout, build them first:

```bash
cd benchmark/binary/x86-64/window/small
./build.sh c
```

If you want the full multi-language set:

```bash
cd benchmark/binary/x86-64/window/small
./build.sh all
```

The example commands below assume this binary exists:

```text
benchmark/binary/x86-64/window/small/binary/c/test_functions.exe
```

## 30-Minute Evaluation Path

This is the recommended first pass.

### 1. Build the CLI

```bash
cargo build -p fission-cli --release
```

Canonical binary:

```text
target/release/fission_cli
```

### 2. Inspect binary metadata

```bash
./target/release/fission_cli info benchmark/binary/x86-64/window/small/binary/c/test_functions.exe
```

What to inspect:

- binary format and architecture
- entry point
- discovered function count
- whether the binary loads cleanly without special setup

Example payload: [info-test_functions.txt](./examples/cli/info-test_functions.txt)

### 3. List discovered functions in JSON

```bash
./target/release/fission_cli list benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --json
```

What to inspect:

- address formatting
- function naming shape
- whether JSON is easy to feed into downstream tooling

Recommended first function:

- `0x140001450` (`add`)

Example payload: [list-test_functions.json](./examples/cli/list-test_functions.json)

### 4. Disassemble one function

```bash
./target/release/fission_cli disasm benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --addr 0x140001450 --function
```

What to inspect:

- function boundary discovery
- whether instruction output is usable for manual validation before decompilation

### 5. Decompile one function in text form

```bash
./target/release/fission_cli decomp benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --addr 0x140001450
```

What to inspect:

- output shape
- local variable surfacing
- readability of the generated pseudocode
- whether this is close enough to your downstream processing needs

Example payload: [decomp-add.txt](./examples/cli/decomp-add.txt)

### 6. Decompile the same function in JSON form

```bash
./target/release/fission_cli decomp benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --addr 0x140001450 --json
```

What to inspect:

- machine-readable code payload
- engine identity
- fallback flags
- preview statistics shape

Example payload: [decomp-add.json](./examples/cli/decomp-add.json)

### 7. Stop here or continue

If the first six steps succeed, you already have enough signal to judge:

- setup friction
- CLI maturity
- JSON automation usability
- initial decompilation readability

Move to benchmark evaluation only after this basic CLI path works cleanly.

## Deeper Evaluation Path

If the first pass looks promising, continue with a slightly deeper headless workflow.

### Operator-grade function inventory

Use the operator path when you want whole-binary machine-readable artifacts rather than one-function text output:

```bash
./target/release/fission_cli inventory function-facts \
  benchmark/binary/x86-64/window/small/binary/c/test_functions.exe \
  --functions-limit 1 \
  --output-jsonl /tmp/fission_rows.jsonl \
  --summary-json /tmp/fission_summary.json
```

What this demonstrates:

- batch-oriented artifact emission
- JSONL row output
- summary JSON output
- preview/build statistics for automation and corpus work

Example summary payload: [inventory-function-facts-summary.json](./examples/cli/inventory-function-facts-summary.json)

Use `inventory` when you want structured artifacts for automation, not when you want a quick human-facing one-function decompilation result.

## Benchmark Evaluation

Benchmarking is stage 2, not the first external evaluation step.

Run it only after:

- the CLI builds successfully
- `info`, `list`, and `decomp` work on at least one sample binary
- you have inspected one or two live outputs manually

Canonical benchmark entrypoint:

```bash
python3 benchmark/full_benchmark/full_decomp_benchmark.py --help
```

One canonical corpus command:

```bash
python3 benchmark/full_benchmark/full_decomp_benchmark.py \
  --corpus-manifest benchmark/config/benchmark_corpus/smoke_corpus.json \
  --ghidra-dir /path/to/ghidra
```

What it compares:

- Fission whole-binary decompilation output
- Ghidra reference output
- similarity, coverage, owner metrics, and shape-drift proxies

First artifact to read:

- `benchmark_compact_summary.json`

Why this artifact first:

- it is the compact AI/operator-facing summary
- it is much smaller than the full benchmark summary
- it surfaces headline quality metrics, owner drift, shape drift, x86/x64 split, and promotion blockers without requiring a deep artifact dive

For the full benchmark workflow, see [benchmark/full_benchmark/README.md](../benchmark/full_benchmark/README.md).

## Text Output vs JSON Output

Use plain text when:

- a human is reading one function interactively
- you want quick qualitative feedback on readability

Use JSON when:

- you want a stable machine-readable payload
- you plan to feed results into downstream automation
- you care about engine/fallback metadata and preview statistics

Use `inventory` when:

- you want batch emitters
- you need JSONL and summary artifacts
- you are evaluating corpus-scale or operator-grade workflows

## Library And Sleigh Positioning

### Does Fission rely on Sleigh?

Yes.

Fission relies on Sleigh for instruction semantics and the knowledge encoded in `.slaspec` files.

### Does the canonical path depend on Ghidra's decompiler runtime?

No.

The lift path is Sleigh-based, but the post-lift pipeline is Rust-native:

```text
Sleigh spec -> Fission lift to p-code -> Rust-native NIR/HIR -> structuring/rendering
```

### Can teams use it as a library?

Yes, at the Rust crate level.

That said, the most mature supported product surface today is still the CLI, not a stable public library API contract.

### Is Python binding a primary supported surface today?

No.

Treat Python binding as a future or experimental direction unless it is explicitly documented as supported in a later release.

## Known Limitations

- The CLI is the strongest external interface today; library ergonomics are less polished.
- Windows x64 is the strongest evaluation/parity surface right now.
- Harder functions may still show readable-structure gaps or rough pseudocode.
- Some machine-readable flows are intentionally operator-oriented and artifact-based rather than simple stdout JSON.
- Example payload files in `docs/examples/cli/` are curated payload excerpts. They preserve current field names but omit many low-signal fields for readability. Your live run may include additional preamble lines such as local config notices.

## What A Successful External Evaluation Looks Like

An evaluator should be able to answer all of these without maintainer help:

- can I build the CLI cleanly?
- can I inspect binary metadata?
- can I list functions in machine-readable form?
- can I decompile one function from a checked-in sample binary?
- can I get a JSON payload for downstream processing?
- can I emit operator-grade summary artifacts?
- do I understand where Sleigh ends and Rust-native ownership begins?
- do I understand what is stable today versus still evolving?

If the answer is yes to those questions, the external evaluation pack is doing its job.
