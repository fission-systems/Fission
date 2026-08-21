# The scorer moved under us

## 1. Baseline Row Anchor

`vendor/decbench` sat at `325046fd` (2026-08-05) while upstream advanced 18
commits, four of them inside `decbench/metrics/`. Every structural and type
measurement this cycle was taken against the older scorer, and two of the four
changes alter what our numbers mean.

Updated to `d9f4f8a`, preserving the local Fission backend
(`decbench/decompilers/raw/fission_raw.py` plus its registration). Upstream had
edited the same registration list to add `glaurung_raw`; both are kept.

## 2. What changed in the metrics

**GED, `#57` -- large isomorphic CFGs.** `GED_MAX_NODES` went from **60 to
200**, and an isomorphism check now short-circuits to a distance of zero when
the graphs match regardless of size.

This invalidates a premise used throughout this cycle. "Above 60 nodes the
score is `|Δnodes| + |Δedges|` rather than a real edit distance" was true of
`325046fd` and is no longer true: the approximation now begins at 200, so most
functions that previously took the cheap path now get real VJ-GED, and a large
function we reproduce exactly now scores perfect instead of approximately
perfect.

Both changes favour an accurate decompiler. The reported mean of 318.3 was
measured under the old rule; the same output would score differently now, and
the local reproduction of it -- our node+edge total matching 109% of the
leaderboard's GED sum -- no longer holds as a check.

**Types, `#65` -- pointee normalization narrowed**, and `_PLACEHOLDER_TYPES`
added. Width-only spellings (`undefined`, `undefined8`, `_QWORD`) now count as
positive evidence that the type was *not* recovered, and the pointee map no
longer lets `undefined8 *` satisfy `size_t *`.

**This one lands on us.** We emit bare `undefined` 42 times across 34 of the
250 scored functions (14%), and bare is worse than Ghidra's `undefined8`: it
states no width at all. The recompilation fixup maps `undefined -> unsigned
char`, so an eight-byte variable is compiled as one byte, which perturbs
codegen and therefore byte-match as well.

Its owner is not the printer. `print_type` renders `NirType::Unknown` as
`"undefined"` because that variant carries no width -- the width is already
gone by the time the type reaches rendering.

**ARM, `#61` -- Thumb-state bit masked in symbol addresses.** Upstream reached
the same conclusion, the same day, as `4bf334475` here: `raw_address & ~1 if
is_arm else raw_address`, gated on `EM_ARM`, with the same reasoning that "ARM
`STT_FUNC` symbol values carry the Thumb-state marker in bit zero". Two
independent derivations of one rule is good evidence the rule is right.

## 3. What this costs the measurements already taken

Directionally safe, numerically stale:

- The structure work stands. Removing an unreachable statement removes a node
  under any scorer, and the before/after was measured with one harness on both
  sides.
- The type census stands as a description of *our own output* against DWARF --
  width inflation, pointers as integers -- because it compares what we emit to
  ground truth rather than depending on scoring subtleties.

Genuinely stale:

- Any statement about where the GED approximation begins.
- Absolute type percentages. `fission-benchmark/runner/type_match.py` is a fork
  1,222 diff lines from upstream, so this cycle's 34%/17% perfect figures came
  from an implementation that does not score us. The gap should be closed or
  the divergence documented before those numbers are quoted again.

## 4. The report drafted for the maintainer, withdrawn

A report was written arguing that `_get_location` discards DWARF register
locations. It does -- upstream's current code keeps only `DW_OP_fbreg` and its
location-list branch still requires `len(ops) == 1` -- but the docstring shows
it is deliberate:

> Register-resident variables (the common case at -O2) yield `([], True)`;
> fully optimized-out variables yield `([], False)`.

So it is a design choice with a known consequence, not an oversight, and a
report framed as "you are throwing this away" is wrong in tone and in premise.
Anything sent should instead be a proposal to carry the register number and add
a matching pass, with the measurement that 79% of `-O2` ground-truth variables
are unreachable without it. Not sent.

## 5. Standing rule this produces

**Check the scorer's HEAD before quoting a number from it.** Four metric
changes accumulated in sixteen days, two of them altering what a score means.
A measurement is only as current as the ruler.
