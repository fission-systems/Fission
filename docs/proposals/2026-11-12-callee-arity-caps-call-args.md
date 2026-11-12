# Proposal: a callee's proven parameter count caps the arguments at its call sites

Status: implemented; results below. Filled from
[`docs/templates/DECOMPILER_CHANGE_PROPOSAL.md`](../templates/DECOMPILER_CHANGE_PROPOSAL.md).

## 1. Baseline Row Anchor

Not one row: a repeating shape, measured on 33 x86-64 ELF binaries (one per
project, 18 at `-O0` and 15 at `-O2`, from the local DecBench binaries).

- Ground truth: `llvm-dwarfdump` on the unstripped binary. A subprogram counts
  when it has a name and a `low_pc`, is prototyped, and is not variadic. Its
  parameter count is its direct `DW_TAG_formal_parameter` children.
- Subject: `fission_cli decomp --all --json --no-db` on the **stripped** copy
  (`llvm-strip --strip-all`), so no debug information reaches the decompiler.
  Calls are matched by address (`sub_<hex>(`), never by name.
- Tool: `scripts/quality/call_arity_vs_dwarf.py`. Baseline build: `origin/main` at `c0c63b3ee`.

Result, 5,775 calls to named internal callees:

| | calls | exact | too few args | too many args |
|---|---|---|---|---|
| all | 5,775 | 63.1% | 14.4% | 22.5% |
| `-O0` | 3,901 | 60.9% | 12.2% | 26.9% |
| `-O2` | 1,874 | 67.6% | 19.1% | 13.4% |

The callees' *own* printed signatures are right far more often than the calls
to them: exact for 98.5% of `-O0` and 78.7% of `-O2` callees (2,493 signatures).
So the decompiler knows the parameter count and still prints calls that
disagree with it. The commonest wrong shapes are `2 -> 3` (452), `1 -> 2` (322)
and `3 -> 4` (239): one stale register passed as a further argument.

- Failure category: call-argument over-recovery.
- Smallest instance: a function the callee summary says takes 1 parameter is
  called as `f(local_30, xVar27)`; the second value is whatever the previous
  call left in the second argument register.

## 2. Owner Proof

- [x] Type/data recovery (callee summaries): `refine_nir_type_context_with_callee_effect_summaries`
  in `crates/fission-decompiler/src/facts/facts.rs`.

The callee summary is computed and correct (`FISSION_PREVIEW_DIAG=1`:
`target=sub_171e evidence=complete arity=1`). A guard then discards it for every
callee that has no resolved symbol, which is every callee of a stripped binary:

```rust
if !has_resolved_target_identity {
    prototype_summary.min_arity = 0;
    prototype_summary.locked_exact_arity = None;
}
```

With the lock gone, the existing normalize pass `prune_known_api_call_args_*`
(which truncates a call to `locked_exact_arity`) has nothing to act on, and the
call keeps the stale register. (A first attempt capped the arguments in the
builder; a unit test showed that was redundant with the pruning pass, so it was
dropped.) The guard's comment calls a stripped identity "not a declaration-level
proof"; the repository history is a single import commit, so no measurement for
it could be found.

## 3. Generality / Invariant Proof

```text
A register argument slot at or beyond a non-variadic callee's proven register
parameter count is not an argument of the call. The call must not carry it,
whatever value the caller left in that register.
```

The proof of the count is the callee's own body (the summary already carries it
as `locked_exact_arity`, set only when the callee is not variadic).

- [x] Not gated on one ISA or calling convention: the rule is on ABI argument
      *slots* from the register namer / cspec (`param_slots`), the same data the
      hole-filling rule uses.
- [x] No ISA-specific data outside cspec / the register namer.
- [x] The synthetic test states the shape (one-parameter callee, two staged
      argument registers) without a compiler tuple or function name.

Comparable coverage: 27 of 33 binaries contain at least three distinct callees
with a wrong call, at both `-O0` (1,051 over-argument calls) and `-O2` (251).

Offline simulation on the same rows (cap each call at its callee's printed
arity): exact 63.1% -> 84.5% overall; `-O0` 60.9% -> 87.7% with no call made
worse; `-O2` 67.6% -> 77.9% with 44 calls made worse and 238 fixed. The 44 are
callees whose body does not read a parameter the source declares (unused
parameters under optimisation); that is the known limit of a count taken from
reads, and it is why the rule is stated for a *proven* count, not a guessed one.

## 4. Risk And Ownership Check

- Existing owner: the callee summary in `facts.rs` plus the normalize pass that
  prunes a call to `locked_exact_arity`; both exist and are unchanged.
- Shared substrate: [x] Type constraint / calling-convention fact (the callee
  summary).
- Why extending it is enough: the fact and the pruning already exist; a guard
  was discarding the fact for stripped callees.
- No new pass, helper or metric; no new owner dependency.
- Interaction: a dropped argument may leave its defining statement dead; later
  dead-code passes own that.
- Known cases that must not change: variadic callees (`locked_exact_arity` is
  `None`), unresolved or indirect callees, callees with no summary, calls that
  already agree with the callee.
- Telemetry: none new; the existing call-signature counters are untouched.

## 5. Validation And Results

Change: drop the guard above; make the lock a proof by setting it only when the
callee's decode was complete (a truncated decode can miss a later read of a
further argument register, so its count is a lower bound).

- Targeted tests (`crates/fission-cli/tests/callee_arity_cli.rs`, fixture
  `x64_stale_arg_register.{c,elf}`, stripped): a one-parameter callee called
  after another call that set the next argument register is called with one
  argument; a two-parameter callee keeps both. Positive control: restoring only
  the original guard makes the first fail.
- Crates: `fission-cli` 195/195; `fission-decompiler`, `fission-pcode`,
  `fission-midend-normalize` pass (three interactive-emulator CLI tests failed
  once when four crates ran in parallel and passed alone and in the full
  `fission-cli` run). CI clippy line and `cargo fmt --check` pass.
- Same 33 binaries, same 5,775 calls, release build, call for call:

  | | before | after |
  |---|---|---|
  | exact | 63.1% | 80.6% |
  | too many args | 22.5% | 4.8% |
  | too few args | 14.4% | 14.6% |
  | `-O0` exact | 60.9% | 83.4% (0 calls made worse) |
  | `-O2` exact | 67.6% | 74.8% (11 calls made worse) |

  1,024 calls went from too many to exact. The 11 that got worse are all calls
  to one callee whose body does not read its second declared parameter: the
  known limit of a count taken from reads.
- DecBench sample-set gate, caches disabled, same build pair: HIR GED perfect
  68/233 before and after; types perfect 16/222 before and after; total distance
  7565 -> 7568. One row of 233 changed (+3). Its output lost a stale argument and
  six assignments to a temporary that existed only to feed it; the callee takes
  one parameter, so the new output is the correct one. The structure score
  cannot see argument counts and prefers the larger wrong output because that
  output is closer in node count to the source, which is why the claim rests on
  the DWARF comparison and not on this gate.

## What this does not claim

No ranking, no readability claim and no structure-score gain. The measured gain
is call-argument correctness against DWARF on this corpus. The remaining 4.8%
too-many and 14.6% too-few calls are not addressed here.

## 6. AI Review / Prompt Firewall

- The idea came from a user-supplied reading of a third-party decompiler's
  published design (a callee's recovered parameter list constraining its
  callers). It is restated here as an owner-native invariant and was then
  measured on Fission itself. No code, identifier or file layout was copied, and
  no model was given row identities.
- Ghidra/other decompilers: reference for the idea only; no output-style target.
- Unseen or synthetic evidence: synthetic invariant test (above); the patch
  validation pool is a go/stop check only and is not a tuning target.

## 7. Review Notes

- No hardcoded binary, function or address guard: confirmed.
- No quality claim from tests or dashboards: the claim rests on the DWARF
  comparison above and its remeasurement.
