# A mask over a value read on entry is not redundant

Date: 2026-11-19. Status: **implemented and measured** -- see section 5.
Filled from [`docs/templates/DECOMPILER_CHANGE_PROPOSAL.md`](../templates/DECOMPILER_CHANGE_PROPOSAL.md).

## 1. Baseline Row Anchor

Found by `fission_cli verify --tier ground-truth` (PR #927) and then read
against the disassembly. gzip `-O0`, `0x5acc` (the `stat` bookkeeping after
`fstat`):

```text
mov  eax, dword ptr [0xde198]     ; st_mode
and  eax, 0xf000
cmp  eax, 0x8000                  ; S_ISREG
jnz  ...
```

printed before this change as

```c
if (tmp_de198 == 32768) {
```

The mask is gone: a regular file with any permission bit set fails the test
the program makes. The same shape, three more of many:

| function | config | printed | binary |
|---|---|---|---|
| gzip `0x6a00` | `-O2` | `tmp_dc078 != 32768` | `(st_mode & 0xf000) != 0x8000` |
| gzip `0xb700` | `-O2` | `!(ushort)uVar5` | `!(uVar5 & 0x1000)` |
| bash `0x82770` | `-O2` | `if ((uchar)g) { g = g; ...` | `if (g & 1) { g &= ~1; ...` |

Why the output is wrong, without a metric: it tests or stores a different
value than the program does.

## 2. Owner Proof

- [x] Normalize: `global_opt/nz_mask.rs` (`compute_nz_masks`).

The builder's PreHIR is right (`uVar1 = tmp_de198; uVar1 = uVar1 & 61440;
... uVar1 == 32768`). A per-pass probe of the normalize pipeline put the loss
in `nz_mask_simplification`: `compute_nz_masks` starts every local at "no
bits" and adds the bits its assignments can store. `tmp_de198` -- a global the
builder declares among the locals -- is never assigned in the function, so it
was taken to be always zero, `uVar1 & 0xf000` to have no bits outside the
mask, and the `&` was deleted.

The same "starts at zero" holds for a register live into the function (`rdx`
read after a call that returns in `rdx:rax`) and for a local whose address is
taken. All three hold any value of their type, as a parameter does; Ghidra's
non-zero mask gives a function input every bit for the same reason.

Fixing it exposed a second defect it had hidden. `variable_merge` coalesces a
copy `u = g` by renaming `u` onto `g`; once the mask survived, `u = g;
u &= 0x51;` became `g &= 0x51;`, a store to a global the program never makes.
The merge treats `g` as a local because nothing in normalize said otherwise.

## 3. Generality / Invariant Proof

- [x] Production condition is not gated on a calling convention or ISA.
- [x] No ISA data involved.
- [x] Synthetic tests state the dataflow shape without a compiler.

Invariant 1: a variable's value set includes its value on entry unless every
path from entry to every read passes an assignment to it; a variable whose
address is taken can change where no assignment shows it. New
`analysis/entry_values.rs` computes the names that fail this -- a forward
"definitely assigned" pass over structured PreHIR, labels solved to a fixed
point over whole passes (a backward `goto` joins its label), loops re-walked
until the back edge adds nothing. `compute_nz_masks` gives those names their
type's full mask.

Invariant 2: a name that denotes file-scope storage is observable outside the
function and can change in a callee. Presentation already decides which names
those are (symbol names, and locals spelled as a mapped address); the rule is
now one function, `render::file_scope_storage_names`, and orchestrate hands
its answer to normalize in `GlobalSymbolContext::storage_names`.
`variable_merge` makes them copy-merge barriers and `nz_mask` gives them the
full mask.

## 4. Risk And Ownership Check

- `compute_nz_masks` also feeds `subvar_flow` and `idioms/subflow`; both now
  see fewer "provably narrow" values. That is the direction of soundness.
- A local read before assignment on a path the program never takes is now
  full-width, so some masks that were removed correctly stay. Those print as
  an extra `& mask`, not as a wrong value.
- No new pass. One new analysis module, one new context field.

## 5. Validation Matrix

- [x] Targeted invariant tests: `analysis::entry_values::tests` (5),
  `nz_mask::tests::a_mask_over_a_value_read_on_entry_is_kept`,
  `variable_merge::tests::a_copy_of_a_global_is_not_merged_onto_the_global`.
  Positive controls: disabling the entry-value seed fails the mask test;
  removing the merge barrier fails the merge test, which also carries an
  in-test control (no storage fact → the rename happens).
- [x] Crate-level gate: `cargo nextest run -p fission-midend-normalize
  -p fission-pcode -p fission-decompiler`.
- [ ] Focused rows / sample: see "Measured" below.

### Measured

All functions of the 16 sample binaries (six `-O0`, ten `-O2`; 15,567
functions), `decomp --all` with this branch against main, then read:

| | functions |
|---|---|
| output changed | 925 |
| a mask that main deleted is printed again | 288 |
| fewer assignments to a file-scope name | 122 |
| more assignments to a file-scope name | 10 |
| fell back to disassembly (batch time budget; both builds decompile it alone in 8 s) | 1 now, 2 fewer |

Read against the disassembly:

- *fewer global assignments* -- bash `_rl_timeout_init` (`0xed830`): main
  merged `rax` onto the global `tmp_14e3f0` and printed three stores the
  program never makes (`tmp_14e3f0 = gettimeofday(..)`); now `rax` stays a
  register.
- *more global assignments* -- dpkg `0x1ec20` and bash `0x49f6a`: a
  nesting counter incremented on entry and decremented on exit. Main printed
  `return g - 1;` with no store; the store is now printed.
- the rest of the changed functions mostly gain a named copy of a global
  (`iVar9 = tmp_dd870;`) that the merge used to fold away. That fold was
  not sound across a call, so the copy stays.

DecBench sample-set gate (`FISSION_BENCHMARK_NO_CACHE=1`, run from a
checkout with the eval kit), main vs. this branch: **perfect 67/233, total
distance 7,530 for both; all 233 rows identical.** A mask is not a CFG node,
so the score cannot see this change; it is a correctness change, not a
quality claim.

### Exposed, not fixed here

`0x49f6a` shows a pre-existing builder defect that the merge used to hide.
The builder forwards the store on entry (`[g] = eax`) to the load at the
exit (`mov eax, [g]` after six calls), so the decrement now stores
`entry_value - 1` instead of re-reading `g`. Global accesses are `ram`-space
varnodes and the reaching-definition lookup does not treat a call as a
possible write to them (Ghidra gives calls an INDIRECT effect on
address-tied storage). That is the builder's owner and a separate change.

## 6. AI Review / Prompt Firewall

No external model was consulted.

## 7. Review Notes

Not a quality claim unless the measured section says the scored rows moved;
the change is justified by correctness of the printed test, which the
benchmark's graph distance cannot see (a mask is not a node).
