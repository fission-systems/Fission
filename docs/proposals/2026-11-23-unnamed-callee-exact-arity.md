# Decompiler Change Proposal: an unnamed internal callee's proven arity reaches its callers

## 1. Baseline Row Anchor

- Binary: DecBench `O2-noinline/coreutils/cp` and `O2-noinline/coreutils/df` (stripped), `O2-noinline/coreutils/chcon`
- Function: `dir_name` (`cp` `0xe730`, `df` `0x7a80`), `me_for_dev` (`df` `0x5110`)
- Corpus row or benchmark command: `~/fission-benchmark/decbench-full/sweep.py run --config full --workers 2`, baseline run `main-1f10b8c78`
- Current output summary: `void sub_e730(void) { rax = sub_e790(); ... }` -- source `dir_name(char const *file) { char *result = mdir_name(file); ... }`, compiled as `call mdir_name` with `rdi` untouched. The callee `sub_e790(unsigned long long param_1)` is decompiled with its parameter.
- Failure category: call-argument recovery -- a pass-through argument is dropped, so the caller also loses its own parameter.
- Observations: per-variable decomposition of the full-dataset baseline: ground-truth arguments with no recovered parameter ("fn:arg") are the largest single sole loss class at `-O2` (15.5k variables; 3.5k functions would be type-perfect with only this fixed), and in 3.1k `-O2` functions the output declares `(void)` while the body calls on.

## 2. Owner Proof

- [x] Builder/materialize: `fission-pcode` `builder/calls/call_recovery.rs`, `exact_call_integer_register_slots`.

Evidence (`FISSION_PREVIEW_DIAG=1`):

```text
[CALLEE-TYPE-DIAG] target=sub_e790 evidence=complete arity=1 ...
```

The facts layer (`refine_nir_type_context_with_callee_effect_summaries`)
summarises the unnamed callee under `sub_<address>` with a locked exact
arity, but `exact_call_integer_register_slots` names a `Call` only through
`resolve_lifted_direct_call_target`, which returns `None` for any target
without a `call_target_refs` / `call_targets` entry. The summary is never
looked up (a debug print in the lookup fired for `malloc`/`memcpy` only).

## 3. Generality / Invariant Proof

```text
A direct call outside the function whose target has no symbol is printed,
and summarised by facts, as `sub_<address>`. Look its prototype summary up
under that key. A locked exact arity is proof (from the callee's complete
decode) that the callee reads those argument registers; the existing
entry-owned / dominating-definition recovery supplies the values.
```

- [x] Not gated on an ISA or calling convention; argument slots come from the existing ABI tables.
- [x] Synthetic test states the shape only (one call, no register write before it).

## 4. Risk And Ownership Check

- Existing owner extended: the name lookup in `exact_call_integer_register_slots`; no new pass.
- Substrate: calling-convention fact (callee arity).
- Interaction: adds arguments only where a locked arity proves them and only from values that already reach the call (`register_holds_entry_value_at_call`, `check_ancestor_realistic`).
- Out of scope: internal tail jumps (`jmp sub_x`) are not summarised by facts and are lowered by a different path (#1039 touches it).

## 5. Validation Matrix

- [x] Targeted test: `cargo nextest run -p fission-pcode exact_arity_of_an_unnamed` (fails with the fallback renamed away).
- [x] Crate gate: `cargo nextest run -p fission-pcode`.
- [x] Full-dataset sweep before/after (see PR body).

## 6. AI Review / Prompt Firewall

- [x] No other model consulted.

## 7. Review Notes

- [x] No hardcoded binary/function/address/corpus guards.
- [x] No metric-only edits.
- [x] Extends the existing owner.
