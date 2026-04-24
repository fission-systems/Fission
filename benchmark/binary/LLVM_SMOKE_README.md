# LLVM Baremetal Smoke

This directory now contains freestanding LLVM-built object smoke binaries for
architecture variants that can be emitted directly by the local `clang`
toolchain without requiring external cross-linkers or platform SDK packaging.

Source template:

- `benchmark/binary/templates/llvm_smoke.c`

Builder:

- `benchmark/binary/build_llvm_baremetal_smoke.py`

Output convention:

- `benchmark/binary/<entry_id>/baremetal/small/source/c/llvm_smoke.c`
- `benchmark/binary/<entry_id>/baremetal/small/binary/c/llvm_smoke.o`

Current inventory summary:

- build summary:
  - `benchmark/binary/llvm_baremetal_build_summary.json`
- full-benchmark smoke corpus:
  - `benchmark/config/benchmark_corpus/llvm_baremetal_smoke_corpus.json`

This is intentionally a loader/disasm/info smoke surface first. It does not
claim runtime-ready decode/lift parity for every generated SLEIGH variant.

Architecture-wide readiness smoke can be run in parallel with:

```bash
python3 benchmark/full_benchmark/run_architecture_readiness_parallel.py \
  --manifest benchmark/config/benchmark_corpus/llvm_baremetal_smoke_corpus.json \
  --output-dir benchmark/artifacts/full_benchmark/architecture_readiness_latest
```

To include a delta against a previous aggregate report:

```bash
python3 benchmark/full_benchmark/run_architecture_readiness_parallel.py \
  --manifest benchmark/config/benchmark_corpus/llvm_baremetal_smoke_corpus.json \
  --output-dir benchmark/artifacts/full_benchmark/architecture_readiness_latest \
  --baseline-report benchmark/artifacts/full_benchmark/architecture_readiness_previous/architecture_readiness_aggregate.json
```

This lane is intentionally lower-bar than raw P-code parity. It answers:

- does the binary load
- does function inventory run
- can one small disasm window be emitted without panicking
