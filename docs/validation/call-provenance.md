# Fission call provenance: measured local prototype

Source baseline: `f714ffdf656fbc25993a4e6ffb038bb851ac6ba5`. Isolated worktree; unrelated checkout changes were preserved.

This is mechanical producer/evaluator transport, not a semantic-quality or ranking claim. Prepared as a draft contribution; remote CI must validate the submitted commit.

## Same-source frozen-input results

| Platform | Functions | Native call-PC records | Unique source/native call links | Calls with a unique native target entry |
|---|---:|---:|---:|---:|
| linux | 67 | 159 | 155 | 14 |
| windows | 67 | 145 | 143 | 127 |
| macos | 66 | 178 | 175 | 82 |

Totals: 200/200 nonempty outputs; call-PC records 482; unique instruction links 473; unique-target call links 0 → 223. These are call occurrences, not distinct callee counts.

All 200 functions completely decoded. Native bytes agree between oracle and stripped executable regions. Independently checked 523 ARM64 BL / x64 E8 target encodings. All 482 NIR and 503 HIR call records point at actual call instruction starts. No invalid PC was accepted.

10 source-call claims remain ambiguous and receive no link; 182 AST call expressions lack supported mapping (including synthetic or complex cases). A unique call PC does not prove an indirect target. Stubs/unqualified target extents remain unresolved.

## Preserved contracts

NIR/HIR text, variable facts excluding the new optional shape, old definition mappings, fallback population and the legacy-facing adapter projection (excluding dedicated call/shape metadata) are unchanged on all 200 functions. Legacy C type-address gates are unchanged. Call-only rows are transported separately in adapter metadata, because the old C metric treats any ordinary line-PC row as potential variable evidence. Optional shape metadata is excluded from default/full-object scoring.

Two final candidate captures agree on all declared deterministic fields for all 200 functions. FINAL-SOURCE.json is unchanged across final capture/audit; timing statistics are not asserted deterministic.

Partial type context: 2118 variable shapes and 13 field entries (possibly repeated descriptions) are transported from existing NIR inference. Aggregates explicitly retain layout_complete=false. This is not new field recovery, complete-object truth, or instruction-to-field verification.

## Implementation

Real CALL/CALLIND origin is retained through PreHIR/HIR, conversion and reconstruction. Synthetic helpers and tail-transfer summaries have no origin. Both semantic equality and legacy Debug keys exclude origin, preserving optimizer identity. The renderer traces only simple statement lines containing a single call. Scope stays separate from variable definitions; clone multiplicity is retained for consumer rejection.

FISSION.patch contains the isolated producer change. DECBENCH-FOLLOWUP.patch contains two tracked transport owners plus the delta from the previous native-facts helper/doc snapshot. The prior native-facts COMPLETE.patch and base DecBench HEAD are prerequisites for reproducing that experimental checkout. source/ retains exact changed owner files.

## Validation and limits

Release CLI build, all-targets compile checks for fission-pcode/fission-decompiler/fission-cli, workspace fmt, owner-boundary scan, scoped Black/Ruff and mypy for the two Python package owners passed. Existing test literals were mechanically updated for additive fields; no new tests were added and no test suites were executed. This is not merge-ready validation.

No GED/type/byte scoring replay, recompilation, native target execution, external semantic benchmark, fresh four-tool ranking or full 45-image replay was performed in this cycle. The 200 inputs belong to one project at O0 across three platforms. No general C++ completeness claim follows.

Next boundaries: call origins in complex headers/initializers and multiple-call expressions need precise occurrence spans; object memory effects need independent instruction-to-base/offset provenance before field-access facts can earn credit.

## Reproduction inputs and retained evidence

Dataset: [frozen C++ qualification revision](https://huggingface.co/datasets/sjkim1127/decbench-cpp-qualification/tree/07dd95dbe6214b8a78fc1e4a55961ab515986489).
The compact evidence in `call-provenance-200.json` preserves input hashes, exact
requested addresses, output hashes and native-call PCs for each image. Its source
addresses are diagnostic evidence only and are not used by production rules.
The exact same stripped bytes and address lists were supplied to both builds.
Full raw captures, commands, patches, binary hashes and independent byte-audit
scripts remain in the local `benchmark/artifacts/fission-call-provenance/2026-10-24/`
bundle. They are not included in this patch as a new runtime dependency.

Build commands used for each immutable CLI/worker set:

```sh
cargo build -p fission-cli --release
cargo check -p fission-pcode -p fission-decompiler -p fission-cli --all-targets --release
cargo fmt --all --check
scripts/check/owner_boundaries.sh
```

The producer invocation for each stripped image was:

```sh
fission_cli --resource-root RESOURCE_ROOT decomp STRIPPED_INPUT --json --no-db \
  --layer nir --no-header --no-warnings --timeout-ms 60000 \
  --addresses-file FROZEN_ADDRESSES
```

The byte audit is an experimental DecBench consumer of the optional producer
contract. It is not a dependency of Fission or part of default DecBench scoring.
