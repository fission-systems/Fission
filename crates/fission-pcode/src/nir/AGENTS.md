# NIR Area Guide

Generated: 2026-03-27
Scope: `crates/fission-pcode/src/nir/`

## Overview

This tree owns Rust-side decompiler semantics after p-code lifting: builder state, normalization, structuring, rendering, and the canonical NIR telemetry contract.

Architecturally, this tree is a substrate plus owner-layer stack, not a single
place to keep adding semantic code. Substrate modules hold IR/HIR types,
telemetry, action-pipeline framework, and shared CFG/def-use/type/alias facts.
Owner layers are builder/materialize, normalize, type/data recovery,
structuring, and render/printer.

## Structure

```text
nir/
├── builder/        # Preview/NIR lowering from p-code (see builder/AGENTS.md)
├── normalize/      # HIR normalization passes (see normalize/AGENTS.md)
├── structuring/    # CFG-driven reconstruction to higher-level HIR
├── tests/          # Synthetic NIR/structuring integration tests
├── mod.rs          # PreviewBuilder state + top-level pipeline
├── types.rs        # Structured IR types (Hir*) + NirBuildStats
└── (print lives in crate-root `src/render/`)
```

## Where To Look

| Task | Location | Notes |
|---|---|---|
| Action pipeline framework | `action_pipeline/` | Pass/ActionGroup/Pipeline; see [`docs/architecture/DECOMPILER_ACTIONS.md`](../../../docs/architecture/DECOMPILER_ACTIONS.md) |
| Normalize group registry | `normalize/pipeline/groups.rs` | Ghidra-ordered ActionGroups; production driver via `run_normalize_pipeline` |
| Canonical stage functions | `normalize/pipeline/stages.rs` | Shared pass sequence for each ActionGroup (1:1 parity) |
| Pipeline helpers | `normalize/pipeline/run.rs` | `run_pass_logged`, cleanup families, admission summaries |
| Telemetry fields | `types.rs` | `NirBuildStats` is canonical |
| PreviewBuilder state | `mod.rs`, `builder/mod.rs` | Keep builder state/projection aligned |
| Structuring rules | `structuring/` | Read child AGENT there first |
| Output formatting | crate-root `src/render/` (`printer.rs`, `hir_presentation.rs`) | Printer + HIR presentation (moved out of `nir/`); see [`render/AGENTS.md`](../render/AGENTS.md), [ADR 0011](../../../../docs/adr/0011-hir-presentation-contract.md) |
| Synthetic regression tests | `tests/` | Prefer adding targeted NIR tests here |
| Normalize pass layout | `normalize/AGENTS.md` | Directory map for `arith/`, `pipeline/`, etc. |

## Conventions

- Add new quality counters in `types.rs` first, then wire through builder snapshot/projection.
- Prefer typed helpers and deterministic ordering; many tests depend on stable output.
- Keep semantics in NIR/structuring layers, not static postprocess or UI surfaces.
- Treat AI suggestions, benchmark rows, Ghidra diffs, and validation-pool signals as evidence only. Production changes must enter this tree as owner-native invariants over p-code semantics, CFG facts, def-use, types, calling convention, or alias facts.
- Before adding a new pass/helper, ask whether the invariant belongs in shared analysis. Repeated special cases should become dataflow, def-use, type-constraint, calling-convention, CFG, or alias facts instead of another narrow rule.
- **ISA-agnostic semantic rules** ([`docs/adr/0009-isa-agnostic-semantic-rules.md`](../../../../docs/adr/0009-isa-agnostic-semantic-rules.md)): keep x86/x86-64 as the measurement focus, but write materialize/structuring/normalize *meaning* in terms of register families, loop-carried updates, join live-in, same-block forward skips, and ABI slots. Decode x86 encodings into those facts; do not fork a parallel m32 control-structure policy. Concrete OK/DEBT/ENV inventory: [`docs/audits/2026-07-10-isa-semantic-debt-inventory.md`](../../../../docs/audits/2026-07-10-isa-semantic-debt-inventory.md).

## Anti-Patterns

- Do not add alternate telemetry payloads outside `NirBuildStats`.
- Do not fix structuring bugs only in `printer.rs`.
- Do not skip large-sample validation when changing rejection/acceptance logic.
- Do not prevent regressions with downstream workarounds or sample-specific heuristics (address guards, hard-coded function names, etc.). Fix the root cause at the canonical owner (builder, normalize, or structuring) using invariant-based algorithms (CFG, dominance, def-use chains, type-system rules). A builder-level fix that stops the wrong binding from being created is better than a normalize pass that tries to clean it up later.
- Do not implement an AI-proposed rule until it has been translated into an invariant owned by an existing builder/materialize/normalize/structuring/type-data component. Prompt output is not an owner.
- Do not add owner-to-owner dependencies when a fact can be moved down into substrate. Existing cross-layer references are migration debt, not precedent.
- Do not reintroduce deleted narrow idiom passes (`security_cookie`, `xor_swap`, `string_copy`, `recurrence`, `call_artifact`, `bitstream`, `likely_trash`) without a Ghidra Rule/Action reference.
- Do not add parallel dead-code or bitmask transform layers; use the consolidated owners documented in [`docs/architecture/DECOMPILER_ACTIONS.md`](../../../docs/architecture/DECOMPILER_ACTIONS.md).
- Do not gate join/loop/cmov-class semantics on `CallingConvention::X86_32` alone, or maintain separate x64/ARM copies of the same rule; put convention differences in namer/cspec/CC tables.

## Pre-Implementation Gate

Before adding semantic-layer production code, fill out [`docs/templates/DECOMPILER_CHANGE_PROPOSAL.md`](../../../../docs/templates/DECOMPILER_CHANGE_PROPOSAL.md) and apply [`ADR 0006`](../../../../docs/adr/0006-decompiler-quality-change-gate.md). The proposal must capture the row anchor, owner proof, invariant proof, and validation matrix.

Extend the existing builder/materialize/normalize/structuring/type-data owner by default. Add a new pass, helper, or metric only after proving that no current owner covers the invariant. Targeted tests are required, but success also needs the crate-level gate, focused row rerun, and smoke/automation regression evidence.

## Regression Prevention

Every semantic fix must pass **both** the targeted unit test and the broader gate. Do not claim success from one targeted test if crate-level regression remains.

1. **Anchor the row:** Before fixing, record source file, binary, address, function name, current behavior status, semantic/static scores, and top missing features.
2. **Add builder-level synthetic regression tests:** When a real binary sample reveals a bug, add the smallest invariant-based test that captures it (e.g., `loop_carried_gpr32_update_with_prior_wide_def_does_not_rebind_param`).
3. **Gate on crate-level tests:** Run `cargo nextest run -p fission-pcode`. If a pre-existing failure blocks the gate, mark it `#[ignore]` with a ticket or maintain an allowlist so new regressions are never masked.
4. **Gate on smoke benchmark rows:** Add the fixed sample to the smoke manifest under `benchmark/config/`. Re-run the exact row with no stale decompilation cache, diff behavior status, case progress, and scores.
5. **Check for cross-sample regressions:** After a focused improvement, run the broader smoke manifest or automation lane. Existing pass rows must not regress.

### Pre-merge checklist

- [ ] Targeted unit test passes (`cargo nextest run --filter ...`)
- [ ] Crate-level unit tests pass (pre-existing failures explicitly called out)
- [ ] Smoke benchmark row for the fixed sample shows improvement with no stale cache
- [ ] No regression in other smoke rows (diff artifacts)
- [ ] `cargo check -p fission-pcode` and `cargo build -p fission-cli --release` are clean (modulo existing warnings)

## Validation

```bash
cargo nextest run -p fission-pcode
cargo check -p fission-pcode
cargo build -p fission-cli --release
cargo run -p fission-automation -- nir-check --lane nir --no-build --fission-bin ./target/debug/fission_cli --functions-limit 200
```
