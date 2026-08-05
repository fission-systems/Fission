# Decompiler Change Proposal: Rotated-Loop Latch Orphaning and Guard-Clause Fallthrough Deletion

Date: 2026-08-06

## 1. Context

Follow-up to `2026-08-05-orphan-goto-continue-sibling-label-visibility.md`'s
"known-separate, NOT fixed" section: `kv_lookup`/`find_pair_value` (both
`-O2`) had a third, independent bug on top of the two already fixed that
session -- `xVar10` (representing `len`/`param_2`) read uninitialized:

```c
uVar0 = 0;
goto block_14000158d;
xVar10 = param_2;       // dead: the goto above always skips this
if (xVar10 != uVar0) {   // xVar10 read here, uninitialized
```

Chasing this down turned up two separate, real bugs, both upstream of the
`normalize`-stage code that proposal originally (wrongly) suspected -- one
in `fission-midend-structuring`'s loop-body lowering, one in its goto-
elimination cleanup. Both are common shapes in real `-O1+` GCC/clang output
(loop rotation and early-exit guard clauses), so both had a corpus-wide
blast radius well beyond `kv_lookup`.

## 2. Bug 1: rotated loops silently drop their own latch

`kv_lookup`'s real disassembly:

```
140001575: xor EAX,EAX          ; uVar0 = 0
140001577: jmp 14000158d        ; jump directly into the loop head
140001580: add RAX,0x1          ; latch (block idx 2): uVar0++
...
14000158b: jz 1400015a0
14000158d: cmp dword[RCX],R8D   ; head (block idx 3): *param_1 vs param_3
140001590: jnz 140001580         ; back-edge to the latch
```

GCC's loop-rotation codegen places the back-edge test *before* the loop
header in the instruction stream, so the latch (block 2) has a *lower*
block index than its own head (block 3), even though it executes *after*
the head on every real iteration.

`crates/fission-midend-structuring/src/loops.rs::lower_loop_body_subgraph`
processes a loop's body blocks in address-sorted order starting from
`start_pos` (the head's position) and scans **forward only**
(`while pos < sorted_body.len() { ...; pos += 1 }`). For `body_set = {2, 3}`
with `start_idx = 3`, `sorted_body = [2, 3]` and `start_pos = 1` -- the loop
starts at position 1, processes block 3, then `pos` becomes 2 (`>=
len() == 2`) and exits. Block 2, sitting at position 0 (*before*
`start_pos`), is never visited by this subgraph lowering at all.

Downstream, this orphaned latch got picked up as a disconnected residual by
the outer SESE driver (`sese_driver.rs`), producing a dangling
`goto block_140001580;` to a label the loop itself never defines, and --
separately -- a duplicate/speculative re-lowering of the same latch content
that's where `xVar10 = param_2` first appeared, attached to the wrong
control-flow edge.

**Fix**: rotate `sorted_body` so the scan starts at the head and *wraps
around* to any lower-indexed body block instead of dropping it:

```rust
sorted_body.rotate_left(start_pos);
let start_pos = 0;
```

Every downstream `.position()`/`tombstone_range` call in the function
already operates on `sorted_body`'s array order (not raw index order), so
this is a pure reordering with no other call-site changes needed.

## 3. Bug 2: guard-clause promotion can delete a live fallthrough return

With bug 1 fixed, `kv_lookup`'s latch correctly folds into the loop as
`break`/`continue`, but the function *still* lost its trailing
`return -1;` after the loop -- a **separate**, pre-existing bug.

`crates/fission-midend-structuring/src/cleanup/goto.rs::guard_clause_promotion`
rewrites `if (cond) { goto L } code; L: return val;` into
`if (cond) { return val } code;` (dropping `L` and its tail entirely) once
it's satisfied via `ref_counts[L] == 1` that nothing else textually
references `L`. But `ref_counts` is built purely from `Goto`/`Label` text --
it cannot see a loop's own `break`, which reaches "whatever comes right
after the loop" with no goto or label naming it at all. For `kv_lookup`,
`code` was `uVar0 = 0; while (1) { ...; if (cond) break; else continue; }`
-- a loop whose `break` falls through to exactly the position the rule was
about to delete. The promotion fired, and `return -1;` vanished, leaving a
`break` with nothing after it (a fall-off-the-end path with an undefined
return value).

**Fix**: added `stmts_may_fall_through` (and a break-reachability helper
`loop_body_has_reachable_break`, correctly *not* descending into nested
loops/switches, since a break there belongs to that inner construct) and
gated the promotion on `code` being provably unable to fall through past
its own end:

```rust
let code = &stmts[(i + 1)..label_pos];
if !stmts_may_fall_through(code) && is_promotable_guard_tail(&tail) { ... }
```

Conservative by construction (errs toward "may fall through" -- i.e.
declines the optimization -- for any shape not proven terminal), matching
this session's established pattern for exactly this class of ambiguity: a
missed cosmetic optimization is far cheaper than a silently deleted return
path.

## 4. A companion fix surfaced along the way: nested-label goto/if collapse

Bug 1's fix alone still left a redundant, uninitialized-looking guard:
`goto L; xVar10=param_2; if (xVar10 != uVar0) { while (1) { L: ... } } `
-- `single_pred_label_inline_flat` (`fission-midend-normalize`) already
handles `Goto(L); <dead-zone>; Label(L)` when `L` is a *top-level* element,
but here `L` sits two levels down (`If.then_body -> While.body[0]`), so its
flat scan never found it. Added
`hoist_goto_target_from_guarded_infinite_loop`: when the flat search fails,
look for `If { then_body: [While { cond: nonzero const, body }], else_body:
[] }` (or the arms swapped) whose loop body starts with `Label(L)`; if the
dead zone between the goto and the if has no external references (reusing
the existing `collect_defined_labels`-based check), replace the whole
guarded-if with the bare `While` it wraps. Also removed an unconditional
`eprintln!("[DEBUG-INLINE] ...")` left in `single_pred_label_inline_flat`
from earlier debugging -- unrelated to this bug, but clearly leftover
cruft that shouldn't ship.

## 5. Verification

- `cargo nextest run --workspace --no-fail-fast`: 2309/2316, same 7
  pre-existing unrelated `fission-emulator` baseline failures established
  earlier this session -- no regressions.
- `kv_lookup`/`find_pair_value` (`--layer hir`): both now render as a clean
  `if (!param_2) { return -1; } ... while (1) { ...; break; ...} return
  -1;`, with the loop-carried pointer correctly renamed `ptr` by this
  session's earlier semantic-naming pass. Compiled the exact rendered HIR
  standalone and ran it against a real lookup table: all four cases
  (found at each position, not-found, and empty-table) return the correct
  value and terminate cleanly.
- Corpus-wide `git`-stash A/B (`--layer hir`, all 72 dev-corpus binaries):
  every diff traced back to either (a) cosmetic pointer-name reshuffling
  (the semantic-naming pass's `ptr`/`p`/`addr` pool assignment shifting
  order because of genuinely different upstream structure -- same variable
  set, different labels) or (b) more instances of the *same* two bug
  classes being fixed elsewhere in the corpus: e.g. `factorial` in
  `math_clang_O2.exe` gained a previously-missing trailing `return rax;`
  (and its inferred return type corrected from `uint` to `longlong` now
  that the real return path is visible to type inference), and
  `bubble_sort` in the same binary had a `break;` correctly expanded back
  into `goto block_1400016e0; ... block_1400016e0: <the actual midpoint
  recomputation>;` that was previously being silently collapsed to a bare
  break. `goto`-count deltas per file were checked directly: every file
  where bug 2's new safety check could plausibly have blocked a
  previously-valid promotion showed **zero** goto-count increase except
  one (`math_clang_O2.exe`, +1), which is exactly the `bubble_sort` case
  above -- a correctness fix, not a false decline.
- No file in the corpus sweep showed a decompiler panic or crash across any
  compiler/optimization-level combination (gcc/clang, O0-O3/Os, x86/m32).

## 6. Known pre-existing, unrelated issue noticed in passing

`bubble_sort` in `math_clang_O2.exe` references `param_1` directly in its
body (`slot_0_4 = (uint *)param_1`) despite the function's own signature
using `addr` for that parameter -- confirmed present identically in the
pre-fix build too (3 occurrences), so unrelated to this round's changes.
Not investigated further; flagged for a future pass.
