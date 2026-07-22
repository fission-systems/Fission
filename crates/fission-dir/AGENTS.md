# fission-dir Area Guide

Generated: 2026-07-22
Scope: `crates/fission-dir/`

## Overview

DIR/HIR differential verification: proves `fission-pcode`'s structuring
stage doesn't silently change a function's behavior while restructuring its
control flow (the class of bug an AArch64 if/else-collapse slipped through
undetected, found only by manual Ghidra diffing before this crate existed).

**DIR is a genuinely independent IR**, not `HIR` reused under a different
name. `fission-pcode`'s builder emits `DirStmt`/`DirExpr`
(`fission_midend_core::ir`) directly from p-code -- the flattened,
goto/label-based form -- and normalize/structuring's own internal passes
read and incrementally rewrite that same `Dir*` grammar across many
sub-passes. Structuring performs one real, explicit
`DirFunction -> HirFunction` conversion
(`fission_midend_core::ir::dir_stmts_to_hir_stmts`, called from
`fission-pcode`'s `midend/orchestrate.rs` right after
`run_structuring_pipeline` + `eliminate_redundant_var_assigns` finish) once
its CFG-to-AST rewrite is done. `HirStmt`/`HirExpr` are a separately
defined type (not a macro-generated mirror of `DirStmt`/`DirExpr`, even
though the two start out shape-identical) -- see
`fission_midend_core::ir::hir`'s module doc for why independence, not
sharing, is the point: it's what makes an accidental DIR/HIR argument swap
in this crate a compile error instead of two same-shaped types silently
type-checking while being logically backwards, and it leaves room for DIR
and HIR to diverge as their respective needs diverge (DIR is a pipeline-
internal, execution-facing representation; HIR carries surface-presentation
concerns DIR never needs).

This crate consumes `DirFunction`/`HirFunction` on equal footing: both are
real outputs `fission-pcode`'s production pipeline produces (via
`fission_pcode::take_last_dir_snapshot`/`take_last_hir_function_snapshot`),
not something this crate privately lowers from someone else's type. One
interpreter, fed the same concrete arguments, evaluates a `DirStmt` tree and
a `HirStmt` tree and diffs the results -- see `src/lib.rs`'s module doc for
the full rationale and roadmap.

## Structure

```text
fission-dir/
├── src/
│   ├── interp.rs   # define_interp! macro -> dir::interpret + hir::interpret (Phase 1: no memory/general-call model)
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
| Add an interpreter-supported construct | `interp.rs`'s `define_interp!` macro body (`eval_expr`/`exec_stmt`) | One edit updates both `dir::interpret` and `hir::interpret` -- they cannot drift, by construction. Bail loudly (`Err`) for anything unmodeled -- never guess a default value |
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
- Keep `DirStmt`/`HirStmt` (and `DirFunction`/`HirFunction`) as genuinely
  separate types at every API boundary in this crate -- don't collapse them
  onto a shared type or a type alias; that reopens the swapped-argument
  footgun independence exists to prevent, and forecloses DIR/HIR diverging
  later.
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
  unsupported `DirExpr`/`DirStmt`/`HirExpr`/`HirStmt` variant must `bail!`.
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
