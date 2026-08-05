# Decompiler Change Proposal: Orphan-Goto-to-Continue Rewrite Used Too-Narrow Label Visibility

Date: 2026-08-05

## 1. Context

Follow-up to `2026-08-05-loop-carried-pointer-advance-load-address-conflict.md`.
`kv_lookup`/`find_pair_value` (both `-O2`) shared a bug distinct from the
loop-carried pointer-advance fix: their decompiled output correctly advanced
the array pointer and index (`param_1 += 2; uVar0++;`), but that increment
code was **unreachable**:

```c
while (1) {
block_14000158d:
    uVar16 = *param_1;
    if (uVar16 != param_3) {
        continue;          // WRONG -- there is no enclosing loop this
                            // legitimately continues; it silently skips
                            // straight back to block_14000158d without
                            // ever running the code below
    }
    return uVar0;
}
}
uVar0++;                    // unreachable: nothing but `continue` above
param_1 += 2;                // ever targeted this via goto, and continue
if (xVar10 != uVar0) {       // doesn't reach it
    goto block_14000158d;
}
return -1;
```

Extracted standalone and run under `timeout 3`: hangs (exit 124),
reproducing the benchmark's `timeout` classification independent of the
pointer-advance bug (which affects a different function, `list_sum`).

## 2. Root cause

`cleanup/loops_conds.rs::rewrite_orphan_loop_gotos_to_continue` handles a
real, narrower problem: sometimes an earlier pass leaves behind a `goto L`
inside a loop where `L`'s own label statement got dropped somewhere else in
normalize (documented as "common after match-path return rewrites leave a
continue-edge label unreferenced"). When that happens, rewriting the
dangling goto to `continue` is usually the right call -- but the pass
decided "the label is genuinely gone" by checking `!defined.contains(label)`
against a `defined` set built by `collect_defined_labels(stmts)` -- computed
from **whatever `stmts` slice this specific pass invocation was given**, not
the whole function.

`rewrite_orphan_loop_gotos_to_continue` is called once per nesting level
from `cleanup_stmt_list_with_options_and_preserved`, which recurses into
`If`'s `then_body`/`else_body` **separately** (each as its own `stmts`).
For `kv_lookup`'s shape -- `If(xVar10 != uVar0) { then: [While(true){...}]
} else { Label(block_140001580); ...increment... }` -- the recursive call
processing `then_body` (containing the `While`, and the `goto
block_140001580` inside it) computes `defined` from `then_body` alone.
`block_140001580`'s actual `Label` lives in the sibling `else_body`, a
completely separate recursive call -- invisible to this one. So the goto
looked "orphaned" purely because its label is a sibling's, not because it's
genuinely gone, and got rewritten to `continue` -- silently discarding
everything at that label (here, the loop's own increment step).

## 3. Fix

Threaded a whole-function `defined_labels: Option<&HashSet<String>>`
parameter through `cleanup_stmt_list_with_options_and_preserved` (mirroring
the pre-existing `global_refs` parameter, which solves the identical
"need whole-function context, but this function only sees a nested slice"
problem for referenced-label tracking -- same pattern, same fix shape).
Computed once at the true top level (`depth == 0`, from the *entire*
`func.body` via `collect_defined_labels`, the recursive whole-body label
collector added earlier this session for the nested-label-segment-drain
fix) and passed down unchanged through every recursive call.
`rewrite_orphan_loop_gotos_to_continue` now takes this as a parameter
instead of computing its own (too-narrow) `defined` set, and does nothing
at all if it isn't available (`None`) -- conservative by construction:
without whole-function visibility, it cannot tell "genuinely orphaned" from
"sibling's label", so it skips rather than guesses.

## 4. Verification

- `cargo nextest run -p fission-midend-normalize -p fission-pcode`:
  1259/1259 passed (existing test for this pass updated to pass an
  explicit whole-tree `defined` set, preserving its original orphan-goto
  intent).
- `cargo nextest run --workspace --no-fail-fast`: 2301/2308, same 7
  pre-existing unrelated `fission-emulator` baseline failures.
- `kv_lookup @ advanced_patterns_gcc_O2.exe`: the dangling `continue`
  became `goto block_140001580;`, and `block_140001580:` is now emitted
  and reachable, with the increment code intact.
- git-stash A/B on six real corpus binaries, decompiling every function in
  each: every diff is previously-dropped code now correctly restored, and
  the impact is **much broader than the two originally-targeted functions**
  -- `advanced_patterns_gcc_O2.exe` has a ~150-line diff restoring an
  entire missing PE base-relocation handler (type 1/2/4/8/16 relocation
  branches, each with its own `mark_section_writable`/`memcpy` call, all
  previously silently dropped as "orphan continue" targets), and
  `math_gcc-m32_O2.exe` restores a binary-search midpoint recomputation
  (`edx = (ecx - edx >> 1) + edx + 1;`) that was completely missing. No
  diff removes or alters anything else -- purely code that used to be
  silently deleted now correctly appearing.

## 5. Known-separate, NOT fixed by this change: `xVar10` read-before-write

While verifying `kv_lookup` end-to-end (compiling the fixed output and
running it against a real 3-entry table), found a **third**, independent,
pre-existing bug in the same function -- confirmed present in the very
first pre-normalize PreHIR snapshot captured while investigating this fix,
so it predates and is unrelated to both this change and the pointer-advance
fix:

```c
uVar0 = 0;
goto block_14000158d;
xVar10 = param_2;      // dead: the goto above always skips this
if (xVar10 != uVar0) {  // xVar10 is read here uninitialized
```

`xVar10` (representing `len`) is assigned in a position the unconditional
`goto` immediately above always skips -- so every read of `xVar10` sees
whatever garbage was already on the stack, not `param_2`. Confirmed with a
printf-instrumented standalone build: `xVar10` prints as an uninitialized
garbage value (observed `8328223784` in one run), and the loop runs for
tens of thousands of iterations past the real 3-entry table before
eventually reading unmapped memory and segfaulting -- functionally another
`timeout`-class failure (the loop bound never matches a huge garbage value
within any reasonable time), just from a different mechanism than the
`continue` bug this change fixes.

This looks like a statement-*ordering* bug in materialize/structuring (the
assignment feeding a value needed at the loop's first real use ends up
positioned after an unconditional jump that skips it, rather than before
it) -- a different mechanism than either bug fixed so far this session
(loop-carried naming, orphan-goto-to-continue label visibility). Not
investigated further this round; left as the next concrete lead for
whoever picks up `kv_lookup`/`find_pair_value` next.
