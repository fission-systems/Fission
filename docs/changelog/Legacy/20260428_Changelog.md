# Changelog: fission-sleigh Structural Refactor Boundary

**Date:** 2026-04-28
**Scope:** `fission-sleigh` compiler/runtime structure, common SLEIGH spine ownership, raw P-code regression guards

## Summary

Refactored `fission-sleigh` toward clearer internal owner boundaries without changing the raw P-code semantics contract. The goal of this wave was not new architecture coverage or generated artifact refresh; it was to reduce responsibility concentration in compiler/runtime entry files and prepare a future split into compiler/runtime/SLA crates.

The strict raw P-code policy remains unchanged:

- no approximate P-code success path
- no compatibility emitter counted as successful raw P-code
- no fake placeholder op
- no invalid P-code shape entering downstream IR
- successful raw P-code rows remain `SpecDerived`

## Implementation Notes

### Compiler Facade Split

- `compiler/mod.rs` now acts more like a public facade and compatibility export layer.
- Added `compiler/discovery.rs` for spec roots, generated paths, manifest lookup, entry-spec conversion, and Ghidra install discovery.
- Added `compiler/policy.rs` for runtime status, executable candidate checks, compatibility aliases, and canonical processor mapping.
- Existing `crate::compiler::*` public exports were preserved.

### Runtime Owner Split

- Moved raw P-code function lifting and CFG block construction out of `runtime/mod.rs` into `runtime/function.rs`.
- Kept public `build_cfg_blocks` re-export stable.
- Preserved runtime shared types and decode contracts in `runtime/mod.rs`.
- Follow-up runtime-centric split moved frontend construction, decode window handling,
  function lifting orchestration, and diagnostics into `runtime/frontend.rs`,
  `runtime/decode.rs`, `runtime/lift.rs`, and `runtime/diagnostics.rs`.
- `runtime/mod.rs` remains the public facade and shared contract owner.

### Compiled-table Spine Boundary

- Converted the compiled-table executor from a large include-style namespace into explicit owner modules:
  - `context`
  - `strategy`
  - `selection`
  - `walker`
  - `handles`
  - `display`
  - `template_eval`
  - `legacy_token_policy`
- Native/common candidate dispatch is now routed through a `RuntimeDecodeStrategy`
  owner instead of passing raw `Option<NativeBackend>` through selection and walker
  layers.
- Fixed/exported handle materialization was split into `handles`, leaving
  `template_eval` focused on checked ConstructTpl execution and primitive emission.
- Kept `legacy_token_policy` explicitly named as compatibility debt rather than a canonical architecture provider.
- Preserved the public decode/lift entrypoints and fail-closed behavior.

### Documentation

- Updated `crates/fission-sleigh/AGENTS.md` with the new module layout.
- Documented future crate split dependency rules:
  - compiler layers must not import runtime types
  - runtime may consume compiler facade/IR types but not compiler orchestration
  - SLA decoder may know IR template types only
  - generated/native backend loading remains runtime ownership

## Validation

Required checks run:

```text
cargo check -p fission-sleigh
cargo test -p fission-sleigh generated_runtime_decodes_reg32_lea_without_decode_no_match_or_compatibility_lift -- --test-threads=1
cargo test -p fission-sleigh generated_runtime_decodes_startup_rip_relative_load_without_compatibility_lift -- --test-threads=1
cargo test -p fission-sleigh generated_runtime_decodes_rip_relative_mov32_without_decode_no_match -- --test-threads=1
cargo build --release -p fission-cli
python3 -m py_compile benchmark/raw_p_code_benchmark/*.py
```

Targeted tests:

```text
generated_runtime_decodes_reg32_lea_without_decode_no_match_or_compatibility_lift: ok
generated_runtime_decodes_startup_rip_relative_load_without_compatibility_lift: ok
generated_runtime_decodes_rip_relative_mov32_without_decode_no_match: ok
```

Raw P-code feature gates:

```text
python3 benchmark/raw_p_code_benchmark/run_raw_pcode_parity.py --manifest benchmark/raw_p_code_benchmark/canonical_rows.json --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC --fission-release --feature lea --output-dir benchmark/artifacts/raw_p_code_benchmark/runtime_arch_refactor_lea
python3 benchmark/raw_p_code_benchmark/run_raw_pcode_parity.py --manifest benchmark/raw_p_code_benchmark/canonical_rows.json --ghidra-dir vendor/ghidra/ghidra_12.0.4_PUBLIC --fission-release --feature rip_relative_load --output-dir benchmark/artifacts/raw_p_code_benchmark/runtime_arch_refactor_rip
```

Raw P-code results:

| Feature | Rows | Result | `compat_emitter_used` | `fake_placeholder_op` | `invalid_pcode_shape` |
|---|---:|---|---:|---:|---:|
| `lea` | 2 | `full_match = 2` | 0 | 0 | 0 |
| `rip_relative_load` | 1 | `full_match = 1` | 0 | 0 | 0 |

Template source totals:

```text
lea:               SpecDerived = 2
rip_relative_load: SpecDerived = 1
```

Performance evidence from the feature gates:

```text
lea:
  Fission/Ghidra wall-clock speedup: 0.6735557615045383x
  average parity ratio: 1.0
  average similarity score: 1.0

rip_relative_load:
  Fission/Ghidra wall-clock speedup: 3.20893447720115x
  average parity ratio: 1.0
  average similarity score: 1.0
```

## Commit Scope Notes

- Generated artifacts under `crates/fission-sleigh/generated` were not intentionally staged for this wave.
- Ghidra project DB artifacts under `benchmark/binary/*_ghidra` were not staged.
- Benchmark output artifacts under `benchmark/artifacts/raw_p_code_benchmark/runtime_arch_refactor_*` were generated validation evidence and are not part of the intended commit payload.

## Remaining Work

- `compiler/mod.rs` still contains some build/report orchestration. The next cleanup can move that into `compiler/build` and `compiler/report` once the current facade boundary is stable.
- Runtime facade files can be made fully private/adapted for a future crate split once
  the `frontend` / `decode` / `lift` / `diagnostics` boundaries stay stable.
- `legacy_token_policy` remains transitional debt. It should shrink as display/template/walker ownership becomes fully spec-derived.
