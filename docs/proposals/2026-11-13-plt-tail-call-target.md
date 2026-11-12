# A `jmp` to a PLT stub is a tail call, not an opaque indirect branch

Filled from `docs/templates/DECOMPILER_CHANGE_PROPOSAL.md`.

## 1. Baseline

`return puts(s);` compiled at `-O2` ends in `jmp puts@plt`. Fission printed
`__fission_branchind(puts);` plus a spurious `unsigned long long puts;` global:
no call, no arguments, no `return`.

Measured on the 33 local x86-64 ELFs used for the earlier call-arity work (3,860
functions, `-O0` and `-O2`), counting functions whose output contains
`__fission_branchind(<identifier>);`:

| | functions | with the pseudo-branch |
|---|---|---|
| `-O0` | 1,981 | 18 |
| `-O2` | 1,879 | 213 |
| all | 3,860 | 231 |

At `-O2` that is 11.3% of functions.

## 2. Owner

Builder, terminator lowering: `recover_known_external_tail_call_expr`
(`builder/control/terminator.rs`). It names the jump target from
`type_context.call_target_refs` only. An ordinary `call` names its target through
`resolve_call_target_address` (`call_target_refs`, then `iat_target_refs`, then
`call_targets`, with `ambiguous_call_targets` honoured). An ELF PLT stub is named by
the latter maps, so the call path found it and the tail-jump path did not.

## 3. Invariant

A direct `jmp` to an address outside the function that the type context names as a
call target leaves the function permanently through that callee; it is a tail call
whichever map holds the name. An address the context marks ambiguous is still left
alone.

No ISA-specific rule: the lookup is shared with the call path, and the PLT stub
decoding stays in the loader.

## 4. Validation and results

- Targeted tests (`bootstrap_x86.rs`): a stub named only in `call_targets` becomes
  `return puts();`; an ambiguous one does not. Restoring the old lookup fails the
  first (positive control).
- `fission-pcode`, `fission-decompiler`, `fission-cli`: 1,440 tests pass; fmt and the
  CI clippy line pass.
- Same 33 binaries, base vs fixed release builds, caches disabled: functions with the
  pseudo-branch 231 -> 51 (`-O2` 213 -> 34, `-O0` 18 -> 17); 186 outputs changed, 185
  of them `-O2`. A sample of changed outputs shows real tail calls
  (`return memcpy(...)`, `return fseeko(fp)`, `__printf_chk(1, ...)` with its
  arguments), and `goto`s to the stub's block collapse into `return`.
- DecBench sample set (same scorer for both builds): GED perfect 68/233 before and
  after, types perfect 16/222 before and after, total distance 7568 -> 7567. Three
  rows moved: -8, +3, +4. In the two worse rows the new output is the more faithful
  one (a `goto` to a block that ends in `__fission_branchind(pthread_mutex_unlock)`
  becomes `return pthread_mutex_unlock(param_1);`, and temporaries that only fed the
  jump disappear); the structure score compares node counts and prefers the larger
  output there.

## 5. What this does not claim

No ranking or structure-score improvement: the score is flat (net -1 over 233 rows).
The claim is the printed program: a call and its callee name are no longer replaced
by an opaque branch. Tail-call argument recovery is unchanged and still imperfect
(`__printf_chk(1, rax, rax)` in one sampled row repeats a register); that is a
separate defect that was previously hidden because the call was not printed at all.
34 `-O2` functions still show the pseudo-branch; they are not examined here.
