# Fission CLI Reference

## Summary

`fission_cli` is the canonical headless product surface for Fission.

If you are evaluating Fission from the outside and want the shortest first-run path, start with [EVALUATION.md](./EVALUATION.md). This document remains the detailed command reference.

Use it when you want to:

- inspect a binary from the terminal
- list discovered functions
- disassemble by address
- decompile one function or a bounded batch of functions
- extract strings
- emit operator-grade inventories for automation and corpus work

The CLI is intentionally split into explicit subcommands:

- `info`
- `list`
- `disasm`
- `decomp`
- `strings`
- `inventory`

Legacy flat invocations still work for one transition period, but they are deprecated compatibility shims. New usage should always use the subcommand form.

---

## Build And Locate

Build the canonical binary:

```bash
cargo build -p fission-cli --release
```

The default output path is:

```bash
target/release/fission_cli
```

For local development:

```bash
cargo run -p fission-cli -- --help
```

---

## Command Model

### Human-facing commands

These are the commands most users should start with:

```bash
fission_cli info <binary>
fission_cli list <binary>
fission_cli disasm <binary> --addr <ADDR>
fission_cli decomp <binary> --addr <ADDR>
fission_cli strings <binary>
```

### Operator-oriented command

This command family is for automation, batch emitters, corpus curation, and offline reporting:

```bash
fission_cli inventory <SUBCOMMAND> ...
```

`inventory` is intentionally separated from the normal `decomp` path so the human-facing surface does not keep growing with batch-only flags.

---

## Common Patterns

### Show binary metadata

```bash
fission_cli info app.exe
```

Optional inventories:

```bash
fission_cli info app.exe --sections
fission_cli info app.exe --imports
fission_cli info app.exe --exports
fission_cli info app.exe --imports --json
```

Use `info` when you want quick metadata and binary inventory without starting a decompilation workflow.

### List discovered functions

```bash
fission_cli list app.exe
fission_cli list app.exe --json
```

Use `list` to discover candidate addresses before running targeted `disasm` or `decomp`.

### Disassemble by address

Instruction-window form:

```bash
fission_cli disasm app.exe --addr 0x140001000
fission_cli disasm app.exe --addr 0x140001000 --count 64
```

Full-function form:

```bash
fission_cli disasm app.exe --addr 0x140001000 --function
fission_cli disasm app.exe --addr 0x140001000 --function --json
```

Use `--function` when you want function boundaries instead of a fixed number of instructions.

### Decompile one function

```bash
fission_cli decomp app.exe --addr 0x140001000
```

Common variants:

```bash
fission_cli decomp app.exe --addr 0x140001000 --ghidra-compat
fission_cli decomp app.exe --addr 0x140001000 --json
fission_cli decomp app.exe --addr 0x140001000 --output out.c
fission_cli decomp app.exe --addr 0x140001000 --timeout-ms 1500
```

### Decompile a bounded batch

```bash
fission_cli decomp app.exe --all --limit 10
fission_cli decomp app.exe --all --limit 10 --json
```

`--all` exists for bounded batch-style local runs. It is not the operator-grade inventory surface.

### Extract strings

```bash
fission_cli strings app.exe
fission_cli strings app.exe --min-len 8
fission_cli strings app.exe --min-len 8 --json
```

---

## Decomp Command

`decomp` is the canonical human-facing decompilation entrypoint.

### Required target selection

Exactly one of these must be provided:

```bash
--addr <ADDR>
--all
```

Examples:

```bash
fission_cli decomp app.exe --addr 0x140001000
fission_cli decomp app.exe --all --limit 25
```

### Main options

#### `--profile <PROFILE>`

Selects the decompilation profile.

Current documented values:

- `balanced`
- `quality`
- `speed`
- `nir`

Compatibility note:

- `mlil-preview` remains a deprecated alias in compatibility paths

#### `--engine <ENGINE>`

Selects the decompilation engine.

Current documented values:

- `auto`
- `nir`
- `rust-sleigh`

Compatibility note:

- `mlil-preview` remains a deprecated alias
- `legacy` is hidden compatibility behavior, not the canonical public surface

#### `--compiler-id <ID>`

Overrides compiler ABI hints:

- `auto`
- `windows`
- `gcc`
- `clang`
- `default`

#### `--timeout-ms <MS>`

Per-function timeout in milliseconds.

```bash
fission_cli decomp app.exe --addr 0x140001000 --timeout-ms 1500
```

Use `0` to mean no timeout where supported by the current execution path.

#### `--function-discovery-profile <PROFILE>`

Controls extra function discovery before execution:

- `conservative`
- `balanced`
- `aggressive`

Example:

```bash
fission_cli decomp app.exe --all --limit 20 --function-discovery-profile balanced
```

#### `--include-nonuser-functions`

By default, `decomp --all` filters imported functions and the zero-size `register_frame_ctor`
runtime wrapper so batch throughput reflects user-facing functions rather than CRT/runtime noise.

Use this flag to restore compatibility/forensics coverage of those non-user functions:

```bash
fission_cli decomp app.exe --all --include-nonuser-functions --json
```

### Output control

#### `--json`

Emits machine-readable JSON output instead of plain text.

Use this for automation and pipelines.

#### `--output <FILE>`

Writes output to a file instead of stdout.

```bash
fission_cli decomp app.exe --addr 0x140001000 --output out.c
```

#### `--verbose`

Emits extra progress and setup detail.

#### `--no-header`

Suppresses the generated function banner comment in the text output.

#### `--no-warnings`

Suppresses `WARNING` and `NOTICE` diagnostics in text output.

#### `--ghidra-compat`

Requests a more Ghidra-compatible output mode.

Use this when you want output closer to current benchmark and comparison surfaces.

#### `--benchmark`

Adds timing metadata in JSON output.

This is useful for benchmark/reporting workflows, but it does not turn `decomp` into the corpus benchmark runner.

---

## Info Command

`info` is the metadata and binary inventory surface.

### Base form

```bash
fission_cli info app.exe
```

### Section/import/export views

```bash
fission_cli info app.exe --sections
fission_cli info app.exe --imports
fission_cli info app.exe --exports
```

If none of those flags are provided, `info` falls back to the base metadata view.

### JSON mode

```bash
fission_cli info app.exe --sections --json
```

Use JSON mode when another tool needs to consume the inventory.

---

## List Command

`list` prints discovered functions for a binary.

### Examples

```bash
fission_cli list app.exe
fission_cli list app.exe --json
```

Typical workflow:

1. run `list`
2. pick an address
3. run `disasm --addr ...` or `decomp --addr ...`

---

## Disasm Command

`disasm` is the address-targeted disassembly surface.

### Required option

```bash
--addr <ADDR>
```

### Windowed instruction output

```bash
fission_cli disasm app.exe --addr 0x140001000
fission_cli disasm app.exe --addr 0x140001000 --count 64
```

### Full-function output

```bash
fission_cli disasm app.exe --addr 0x140001000 --function
```

### JSON mode

```bash
fission_cli disasm app.exe --addr 0x140001000 --function --json
```

---

## Strings Command

`strings` extracts printable strings from the binary image.

### Examples

```bash
fission_cli strings app.exe
fission_cli strings app.exe --min-len 8
fission_cli strings app.exe --min-len 8 --json
```

### Default threshold

If `--min-len` is not provided, the current default is `4`.

---

## Inventory Command

`inventory` is the operator-oriented surface. Use it when you need structured batch emitters, corpus-candidate views, or automation input artifacts.

It currently exposes:

- `function-facts`
- `preview-candidates`

### Why inventory is separate

`inventory` exists to keep these batch-only and reporting-oriented controls out of the normal `decomp` surface. If a flag exists only for batch emitters, it belongs here.

---

## Inventory Function Facts

This subcommand emits whole-binary function facts as JSONL plus an optional summary JSON.

### Basic example

```bash
fission_cli inventory function-facts app.exe \
  --output-jsonl rows.jsonl \
  --summary-json summary.json
```

By default, whole-binary inventory selection also filters imported functions and the zero-size
`register_frame_ctor` runtime wrapper. Use `--include-nonuser-functions` when you intentionally
want full compatibility/forensics coverage.

### Restrict to one address

```bash
fission_cli inventory function-facts app.exe \
  --addr 0x140001000 \
  --summary-json summary.json
```

### Address file input

```bash
fission_cli inventory function-facts app.exe \
  --addresses-file addrs.txt \
  --output-jsonl rows.jsonl \
  --summary-json summary.json
```

### Batch shaping options

- `--functions-limit <N>`
- `--include-nonuser-functions`
- `--chunk-size <N>`
- `--resume-from <FILE>`
- `--quiet-batch-errors`

### Decomp-related execution controls

- `--compiler-id <ID>`
- `--profile <PROFILE>`
- `--timeout-ms <MS>`
- `--function-discovery-profile <PROFILE>`

Use these only when inventory emission needs to align with a specific analysis configuration.

---

## Inventory Preview Candidates

This subcommand emits preview candidate inventory rows or performs preview candidate batch scans.

### Inventory view

```bash
fission_cli inventory preview-candidates app.exe --inventory
```

### Batch view

```bash
fission_cli inventory preview-candidates app.exe \
  --batch \
  --output-jsonl rows.jsonl \
  --summary-json summary.json
```

### Common selectors

- `--addr <ADDR>`
- `--preview-candidate-limit <N>`
- `--include-nonuser-functions`
- `--addresses-file <FILE>`
- `--functions-limit <N>`
- `--chunk-size <N>`
- `--resume-from <FILE>`

### Execution tuning

- `--compiler-id <ID>`
- `--profile <PROFILE>`
- `--timeout-ms <MS>`
- `--function-discovery-profile <PROFILE>`
- `--quiet-batch-errors`

This command is for corpus curation and candidate analysis, not normal first-pass decompilation.

---

## JSON And Automation Notes

### When to prefer JSON

Prefer `--json` when:

- another tool will parse the output
- you are collecting stable machine-readable results
- you are building wrappers or automation around `fission_cli`

Prefer plain text when:

- you are reading one result directly in the terminal
- you want the default human-facing pseudocode or inventory output

### Inventory vs decomp JSON

- `decomp --json` is for decompilation results
- `inventory ...` is for structured whole-binary or batch-emitter output

Do not treat `inventory` as a cosmetic alias for `decomp --all`. It owns a different workflow.

---

## Legacy Compatibility

Legacy flat syntax is still accepted for one transition period.

Examples of deprecated forms:

```bash
fission_cli app.exe --info
fission_cli app.exe --funcs
fission_cli app.exe --asm 0x140001000
fission_cli app.exe --decomp 0x140001000
```

When used, the CLI emits a deprecation warning and internally normalizes into the canonical subcommand execution path.

Canonical replacements:

```bash
fission_cli info app.exe
fission_cli list app.exe
fission_cli disasm app.exe --addr 0x140001000
fission_cli decomp app.exe --addr 0x140001000
```

New scripts, docs, and operator workflows should not introduce new uses of the flat syntax.

---

## Recommended Workflows

### Human inspection workflow

```bash
fission_cli info app.exe
fission_cli list app.exe --json
fission_cli disasm app.exe --addr 0x140001000
fission_cli decomp app.exe --addr 0x140001000 --ghidra-compat
```

### Small local batch workflow

```bash
fission_cli decomp app.exe --all --limit 10 --json
```

### Operator inventory workflow

```bash
fission_cli inventory function-facts app.exe \
  --output-jsonl rows.jsonl \
  --summary-json summary.json
```

### Benchmark-adjacent decomp workflow

For full corpus benchmark and comparative reporting, use the canonical benchmark runner:

```bash
python3 benchmark/full_benchmark/full_decomp_benchmark.py --help
```

`fission_cli --benchmark` is a decomp-output timing flag. It is not the benchmark suite driver.

---

## Boundary Rules

- `fission_cli` owns command parsing, UX grouping, and output routing.
- It does not own semantic repair for the decompiler.
- Batch/operator emitters belong under `inventory`, not under `decomp`.
- Benchmark/reporting scripts are the canonical corpus validation surface, not a place to patch CLI semantics.

If a proposed CLI change starts to alter decompiler semantics, it belongs in the decompiler crates instead.

---

## Validation

Minimum validation for CLI surface changes:

```bash
cargo test -p fission-cli
cargo check -p fission-cli
cargo build -p fission-cli
```

Useful manual checks:

```bash
cargo run -p fission-cli -- --help
cargo run -p fission-cli -- info --help
cargo run -p fission-cli -- list --help
cargo run -p fission-cli -- disasm --help
cargo run -p fission-cli -- decomp --help
cargo run -p fission-cli -- strings --help
cargo run -p fission-cli -- inventory --help
```

If you change compatibility behavior, also verify at least one legacy flat invocation still routes correctly and emits a deprecation warning.
