# fission-dir Area Guide

Generated: 2026-07-22
Scope: `crates/fission-dir/`

## Overview

DIR/HIR differential verification: proves `fission-pcode`'s structuring
stage doesn't silently change a function's behavior while restructuring its
control flow (the class of bug an AArch64 if/else-collapse slipped through
undetected, found only by manual Ghidra diffing before this crate existed).

**DIR is not a new IR.** It's the same `Vec<HirStmt>`/`HirExpr` grammar as
HIR (`fission_midend_core::ir::{Dir, Hir}`, thin newtypes over the identical
`Vec<HirStmt>`), captured immediately before vs. immediately after
structuring runs. Same type, same variable namespace -- one interpreter,
fed the same concrete arguments, evaluates both and diffs the results, with
no ABI/register modeling or real machine execution needed. See `src/lib.rs`'s
module doc for the full rationale and roadmap.

## Structure

```text
fission-dir/
├── src/
│   ├── interp.rs   # Tree-walking HirStmt/HirExpr interpreter (Phase 1: no memory/general-call model)
│   ├── diff.rs      # DIR-vs-HIR differential harness + sample generation
│   └── lib.rs       # Module-level roadmap doc (Phase 1 done, Phase 2 not started)
├── tests/
│   └── real_binary_end_to_end.rs   # Real compiled binary through the real production pipeline
└── testdata/
    ├── src_max2.c   # Source for the real end-to-end fixture
    └── max2.elf      # Compiled fixture (zig cc, -O0, static x86-64)
```

## Where To Look

| Task | Location | Notes |
|---|---|---|
| Add an interpreter-supported construct | `interp.rs`'s `eval_expr`/`exec_stmt` | Bail loudly (`Err`) for anything unmodeled -- never guess a default value |
| Recognize a new pure intrinsic (`__foo`) | `interp.rs`'s `eval_builtin_call` | Only add names already on `fission_pcode`'s own `is_pure_intrinsic_call` list -- a fixed whitelist, not general `Call` support |
| Change how samples are generated | `diff.rs`'s `default_samples` | Boundary-heavy concrete sampling; Phase 2 replaces this with solver-backed proof |
| The DIR/HIR capture hook itself | `fission-pcode`'s `midend/orchestrate.rs` (`take_last_dir_snapshot`/`take_last_hir_function_snapshot`) | Not owned by this crate -- purely observational thread-local side channels, same pattern as `take_last_layered_pseudocode` |
| Add a new real-binary fixture | `testdata/`, mirror `tests/real_binary_end_to_end.rs` | Prefer a small, purpose-built binary (`zig cc -target x86_64-linux-musl -O0 -static`) over hunting for a pure function inside an existing stripped testdata ELF |

## Conventions

- No memory model in Phase 1: `Load`/`Store`/`Deref`/`Index`/`FieldAccess`/
  `AggregateCopy`/general `Call` all bail with `Err`, never a default or
  guessed value. A function needing one of these is reported `Unsupported`,
  not silently marked equivalent.
- Both DIR and HIR erroring on the same sample is inconclusive (shared
  unmodeled construct or UB), not a divergence -- see `diff_dir_hir`'s doc
  comment. Only report a `Divergence` when exactly one side fails, or both
  succeed with different results.
- Keep `Dir`/`Hir` as distinct newtypes at every public API boundary in
  this crate -- don't collapse them back to bare `Vec<HirStmt>`; that
  reopens the swapped-argument footgun the newtype exists to prevent.
- Prefer validating a new interpreter feature against a real compiled
  fixture over a hand-built `PcodeFunction`/`HirStmt` -- hand-built
  fixtures repeatedly missed real gaps this crate's own history found only
  by running against `max2.elf` (`__sborrow`-style flag intrinsics,
  undeclared return-scaffold locals).
- Comments and doc comments in this crate are English-only, matching the
  rest of the workspace, even when the concept originated in a non-English
  conversation (e.g. "DIR" itself).

## Anti-Patterns

- Do not silently default an unmodeled construct to `0`/`Ok` -- every
  unsupported `HirExpr`/`HirStmt` variant must `bail!`.
- Do not add general interprocedural `Call` support to `interp.rs` -- Phase 1
  is deliberately scoped to pure/arithmetic functions; a real `Call` model
  belongs in Phase 2 alongside the emulator/solver work, not smuggled in as
  "just one more builtin."
- Do not report `Equivalent` from zero comparable samples -- `diff_dir_hir`
  returns `Unsupported` when `checked == 0` specifically to prevent this.
- Do not fix a structuring bug this crate finds *in this crate* -- a real
  structuring fix belongs in `fission-pcode`'s structuring owner and goes
  through ADR 0006's proposal-gate process; this crate only detects and
  reports.

## Validation

```bash
cargo test -p fission-dir
cargo build --workspace
python3 scripts/quality/golden_corpus_check.py check   # confirms the orchestrate.rs hooks stay zero-behavior-change
```

## Roadmap

Phase 1 (done): DIR-vs-HIR differential interpretation by concrete
sampling, validated against both hand-built fixtures and one real compiled
binary. Phase 2 (not started; `fission-emulator`/`fission-solver` deps
already wired into `Cargo.toml`): a symbolic mirror of `interp` for
solver-backed equivalence proof instead of sampling, and real-execution
fidelity checking for DIR itself via the emulator -- catches
builder/normalize lifting bugs, which Phase 1 structurally cannot see
since DIR and HIR are both downstream of that stage. See `src/lib.rs`'s
module doc for the full rationale.
