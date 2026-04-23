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

Outputs:

- `ghidra_raw_pcode.json`
- `fission_raw_pcode.json`
- `raw_pcode_parity_report.json`

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
- `ghidra_decode_error`
- `fission_decode_error`

The expected workflow is targeted:

1. reproduce a bad decompiler row
2. run this benchmark on the failing address
3. fix the raw p-code bucket first
4. only then rerun the full decompiler benchmark
