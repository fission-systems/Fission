# Stage Parity Benchmark (MVP)

This benchmark isolates mid-pipeline parity by running Fission per-function
preview decompilation and extracting stage-specific metrics from
`preview_build_stats` plus code metrics.

Stages:

- A. P-code -> NIR build
- B. NIR/HIR normalize
- C. Structuring proof

The goal is not IR structural equivalence with Ghidra, but feature parity:

- call target resolved
- arity surfaced
- type surface stability
- structuring completeness

## Outputs

Each row writes:

- `stage_parity_report.json`

The aggregate file is:

- `aggregate_stage_parity_report.json`

## Example run

```bash
python3 benchmark/stage_parity_benchmark/run_stage_parity.py \
  --manifest benchmark/stage_parity_benchmark/manifests/sample_rows.json \
  --output-dir benchmark/artifacts/stage_parity_benchmark/sample
```

## Manifest shape

Manifests accept both flat `rows[]` and grouped `binaries[]`, matching the
raw p-code benchmark shape.

```json
{
  "defaults": {
    "language": "x86:LE:64:default",
    "compiler": "windows"
  },
  "binaries": [
    {
      "id": "test-functions-x64",
      "path": "benchmark/binary/x86-64/window/small/binary/c/test_functions.exe",
      "rows": [
        {
          "name": "call-target-case",
          "addr": "0x140001160",
          "feature_group": "call_recovery",
          "feature": "direct_import_and_thunk",
          "owner": "nir_builder_call_recovery",
          "notes": "Ghidra resolves imported callee and arity"
        }
      ]
    }
  ]
}
```
