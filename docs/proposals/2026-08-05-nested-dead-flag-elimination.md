# Decompiler Change Proposal: Recurse Dead-Flag Elimination Into Nested Bodies

Date: 2026-08-05

## 1. Context

Continuing the Fission-vs-Ghidra gap investigation (see
`2026-08-05-nested-label-segment-drain-fix.md`) into the benchmark's
`compile_error` bucket. `manipulate_bitfields` was the other function
recurring across multiple compiler variants (`memory_layouts_gcc_O0.exe`,
`memory_layouts_gcc-m32_O0.exe`) in that bucket. Unlike `dot_product_stride`,
its generated code compiled fine syntactically -- the real problem was
quality: raw x86 condition-code state (`of`/`sf`/`zf`/`pf`) leaking directly
into the decompiled output as declared local `bool` variables with dozens of
dead-store assignments, instead of being eliminated as the compiler-internal
noise it is.

## 2. Root cause: two existing dead-flag passes both miss nested bodies

`crates/fission-midend-normalize/src/recovery/flag_recovery.rs` already has
well-tested machinery for this exact problem:

1. `dead_flag_definition_sites` / `remove_dead_sites`: a precise, CFG-based
   (`Goto`/`Label`-edge-aware) backward liveness analysis. Its own doc
   comment says it "finds pure **top-level** flag definitions" -- it only
   inspects `stmts`' own direct elements for `PreHirStmt::Assign` matches.
   Any nested compound statement (`Block`/`If`/`While`/etc.) is folded into
   this analysis only as one opaque unit via `LivenessTransfer::for_stmt`'s
   whole-subtree summary (documented as deliberately "avoiding inventing a
   second structured-CFG engine").
2. `remove_globally_unused_flags`: a fallback that *does* recurse into every
   nested body, but only removes a flag definition when its name has **zero
   uses anywhere in the entire function** (`DefUseMap`-based global count).

`manipulate_bitfields`'s whole body is one top-level `Block` (confirmed via
a temporary env-gated trace dump of the pre-pass statement shape,
`FISSION_DFC_TRACE`, added and removed for this investigation) --
`func.body = [Block([...])`, `Return(...)]`. Every flag assignment lives one
level inside that `Block`, so pass 1 never even looks at them (`Block` isn't
a `PreHirStmt::Assign`, so `flag_definition` never matches it -- only
`flag_uses`, via `LivenessTransfer`, correctly sees inside it for the "is it
used" question, just not for "mark this specific assignment dead").

Pass 2 *does* reach inside the `Block`, but its all-or-nothing global check
is defeated by a genuine but narrow idiom later in the same function: a
variable-shift-count flag-preservation pattern,
`sf = !xVar76 & sf | xVar76 & xVar67 < 0;` (and the `zf`/`pf` equivalents) --
this is a real *read* of the flag's own prior value (from a case where the
shift count is 0 and the original flags should be kept). That single
self-referential use anywhere in the function means `uses.get("sf") > 0`
globally, so pass 2 refuses to remove **any** `sf = ...;` definition in the
whole function -- including the dozens of others that are unconditionally
overwritten (or never reach that one read at all) and are genuinely, locally
dead.

## 3. Fix

Added a third mechanism, `dead_flag_sites_recursive` /
`dead_flag_site_in_stmt` (`flag_recovery.rs`), that composes precise
backward liveness across nesting levels by properly threading a `live_out`
set through `Block`/`If`/`Switch` (these don't affect control flow the way
loops or raw `Goto`/`Label` do, so live-out threads through them exactly
like ordinary sequential statements): `Block`'s live-out is whatever's live
immediately after it in its parent list; `If`'s live-in is the union of both
branches' live-in (they share the same post-If live-out) plus the
condition's own uses; `Switch` composes the same way across its arms.

`While`/`For`/`DoWhile` bodies, and any list containing a `Goto`/`Label`
anywhere within reach of the recursion, stay opaque (fold in
`LivenessTransfer`'s whole-subtree summary, mark nothing inside dead) --
deliberately narrower in scope than a full structured-CFG liveness engine,
matching the existing passes' own precedent of a safe, bounded approximation
over a bigger, riskier "prove it correct for every construct" rebuild. A
raw `Goto` can jump past whatever this backward walk assumes is needed after
a given point (which only accounts for normal fallthrough), so a flag
definition right before one could look dead to a naive per-statement scan
while still being needed at the jump target -- bailing the whole list to
the conservative summary when either appears keeps this purely additive:
it can only find *more* dead sites than the existing two passes, never an
unsafe one.

`remove_dead_flag_assigns` now runs this as a second pass after the existing
precise top-level one, using `remove_dead_sites_recursive` (mirrors
`remove_dead_sites` but recurses into `Block`/`If`/`Switch` to actually
delete what the analysis found at whatever depth it lives).

## 4. Verification

- New unit test
  `dead_flag_cleanup_prunes_locally_dead_definition_inside_wrapping_block_despite_global_use`:
  reproduces the minimal shape (a `Block` with an immediately-overwritten
  dead `zf` definition followed by a live one, wrapped exactly like a real
  function body) and asserts the dead one is pruned even though `zf` has a
  nonzero global use count. Fails without the fix (both survive), passes
  with it.
- `cargo nextest run -p fission-midend-normalize`: 290/290 passed.
- `cargo nextest run --workspace --no-fail-fast`: 2300/2307 passed, same 7
  pre-existing unrelated `fission-emulator` baseline failures.
- Real repro, `manipulate_bitfields @ 0x140001530` in
  `memory_layouts_gcc_O0.exe`: before, `pf` declared and assigned 9 times,
  never read anywhere -- entirely dead; after, `pf` doesn't even appear in
  the output (every assignment pruned). `of`/`sf`/`zf` dropped from ~25
  total assignments down to the 3 that feed the one genuine
  flag-preservation read. Extracted the after-fix function to a standalone
  `.c` file and confirmed it still compiles (`gcc -c -w -O0`).
- Before/after `git stash` A/B on six real corpus binaries spanning every
  compiler/opt combination touched (`control_flow_gcc-m32_O0`,
  `memory_layouts_clang_O0`, `advanced_patterns_gcc_O2`,
  `math_gcc-m32_O2`, `memory_layouts_gcc_O0`, `crypto_gcc_O2`), decompiling
  every function in each (`fission_cli decomp --all`): every diff line is
  either a removed dead flag store, or a downstream cascading improvement
  the removal enabled in an *already-existing* pass (e.g. `if (98 <
  iVar19)` collapsing to `if (98 < param_1 - 1)` once the now-pruned `cf =
  iVar19 < 98;` stopped keeping `iVar19` from being inlined; `eax = (uint)
  (ulonglong)(&DOS_HEADER);` folding to `eax = 23117;` once dead `zf = 1;
  zf = xVar16 - 23117 == 0;` noise around it was gone). No unexpected or
  unsafe-looking change anywhere in any of the six diffs.

## 5. Scope note

This intentionally does not attempt precise dead-flag elimination inside
loop bodies or across raw `Goto`/`Label` edges nested more than one level
deep -- those stay exactly as conservative as they were before this change.
A future pass could extend `While`/`For`/`DoWhile` handling with a real
fixed-point iteration (mirroring how `dead_flag_definition_sites` already
does this for the flat top-level CFG case), but the bounded version here
already resolves the concretely observed regression
(`manipulate_bitfields`) and the vast majority of straight-line,
`If`-structured flag noise in the corpus without taking on that added
complexity and risk in the same change.
