# Raw P-code Parity Benchmark

This benchmark compares Fission's raw SLEIGH p-code against Ghidra's raw
instruction p-code. It intentionally uses `Instruction.getPcode()` through
PyGhidra, not Ghidra decompiler `HighFunction` p-code.

## Why this exists

Decompiler similarity mixes too many layers:

- decode
- constructor selection
- operand binding
- raw p-code emission
- NIR/HIR normalization
- structuring
- printing

This benchmark isolates the SLEIGH runtime layer.

It now also records end-to-end wall-clock and throughput metrics for the raw
P-code extraction path on both sides:

- Ghidra oracle invocation
- Fission raw probe invocation

These are benchmark harness timings, not isolated decode-only microbenchmarks.

## One-window run

```bash
python3 benchmark/raw_p_code_benchmark/run_raw_pcode_parity.py \
  --binary benchmark/binary/x86-64/window/small/binary/c/test_functions.exe \
  --addr 0x140001450 \
  --count 8 \
  --language x86:LE:64:default \
  --compiler windows \
  --output-dir benchmark/artifacts/raw_p_code_benchmark/test-functions-add
```

## Canonical row gate

```bash
python3 benchmark/raw_p_code_benchmark/run_raw_pcode_parity.py \
  --manifest benchmark/raw_p_code_benchmark/canonical_rows.json \
  --output-dir benchmark/artifacts/raw_p_code_benchmark/canonical-latest
```

The x86-64 canonical exact-parity gate is stricter and should be used before
promoting SLEIGH runtime changes:

```bash
python3 benchmark/raw_p_code_benchmark/run_raw_pcode_parity.py \
  --manifest benchmark/raw_p_code_benchmark/canonical_rows.json \
  --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC \
  --fission-release \
  --require-perfect-canonical \
  --expected-full-match 44 \
  --output-dir benchmark/artifacts/raw_p_code_benchmark/perfect-gate-latest
```

This gate fails the command unless comparable semantic rows are exact:

- `average_similarity_score = 1.0`
- `average_parity_ratio = 1.0`
- `compat_emitter_used = 0`
- `fake_placeholder_op = 0`
- `invalid_pcode_shape = 0`
- successful rows use only decoded `.sla ConstructTpl` (`sla_construct_tpl`)

Rows classified as `both_decode_error_or_padding` stay visible in bucket totals,
but are excluded from the semantic similarity denominator.

Target one feature slice from the same manifest:

```bash
python3 benchmark/raw_p_code_benchmark/run_raw_pcode_parity.py \
  --manifest benchmark/raw_p_code_benchmark/canonical_rows.json \
  --feature relative_call \
  --output-dir benchmark/artifacts/raw_p_code_benchmark/relative-call-latest
```

The aggregate output is:

- `aggregate_raw_pcode_parity_report.json`

Each manifest row also writes the normal per-window files under a row-named
subdirectory.

The aggregate manifest report now also includes:

- `feature_totals`
- `group_totals`
- `performance_summary`

Outputs:

- `ghidra_raw_pcode.json`
- `fission_raw_pcode.json`
- `raw_pcode_parity_report.json`

Each source JSON includes:

- `timing.wall_clock_sec`
- `timing.instruction_count`
- `timing.pcode_op_count`
- `timing.instructions_per_sec`
- `timing.pcode_ops_per_sec`

Each parity report also includes:

- `performance.ghidra`
- `performance.fission`
- `performance.delta`

## Ghidra version pinning

The Ghidra oracle path is now pinned explicitly instead of relying on whichever
installation `pyghidra` happens to discover first.

Resolution order:

1. `--ghidra-dir`
2. `GHIDRA_INSTALL_DIR`
3. repo defaults:
   - `vendor/ghidra/ghidra-Ghidra_12.0.4_build`
   - `ghidra-Ghidra_12.0.4_build`

Only launchable packaged installs are accepted. A source/build checkout that
does not contain `support/`, `Utility.jar`, and `PyGhidra.jar` is rejected
instead of being used implicitly as an ambiguous oracle.

Example:

```bash
python3 benchmark/raw_p_code_benchmark/run_raw_pcode_parity.py \
  --manifest benchmark/raw_p_code_benchmark/canonical_rows.json \
  --ghidra-dir vendor/ghidra/ghidra-Ghidra_12.0.4_build \
  --output-dir benchmark/artifacts/raw_p_code_benchmark/canonical-12_0_4
```

## Buckets

The comparator currently reports:

- `full_match`
- `decode_no_match`
- `length_mismatch`
- `mnemonic_mismatch`
- `pcode_op_count_mismatch`
- `pcode_opcode_mismatch`
- `pcode_arity_mismatch`
- `varnode_size_mismatch`
- `varnode_space_mismatch`
- `input_varnode_mismatch`
- `output_varnode_mismatch`
- `temp_space_mismatch`
- `label_target_mismatch`
- `ghidra_decode_error`
- `fission_decode_error`
- `compat_emitter_used`

## Manifest intent

`canonical_rows.json` keeps two surfaces at once:

- legacy coarse rows for continuity with earlier waves
- feature-isolated rows for owner-local debugging

Each row can declare:

- `feature_group`
- `feature`
- `owner`
- `notes`

The expected workflow is no longer just “run all rows.” It is:

1. identify the failing owner family
2. run `--feature` or `--group` against the manifest
3. fix the raw p-code bucket first
4. then rerun the full canonical manifest
5. only then rerun the full decompiler benchmark

The goal is breadth with separation, not one giant startup row that mixes every
problem family together.

## Architecture-parallel smoke

There is also an architecture-scoped parallel smoke runner for LLVM baremetal
objects:

```bash
python3 benchmark/raw_p_code_benchmark/run_architecture_parallel.py \
  --manifest benchmark/raw_p_code_benchmark/llvm_arch_smoke_rows.json \
  --ghidra-dir /Users/sjkim1127/Downloads/ghidra_12.0.4_PUBLIC \
  --output-dir benchmark/artifacts/raw_p_code_benchmark/architecture_parallel_latest \
  --fission-release
```

This runner is intentionally separate from the canonical x86-64 parity manifest.
It is meant to answer:

- which architecture rows execute today
- which rows fail closed as compile-only or unsupported
- what the per-architecture raw p-code speed looks like
