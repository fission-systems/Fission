# Redefined ABI merge values: issue #182, bounded first step

## 1. Measured baseline row anchors

Base: main `2e0052a38de7c347c5c0896406e45cab13beaaa0` (PR186).
Fresh stripped-input baseline executable: `/private/tmp/fission-copy-storage-candidate-cli`,
SHA256 `abe35ec79f354b6f86a7cc1a0f600107609cc6c0787a42c2f763157632946c3e`.
Inputs: standalone DecBench sample-set kit, not a Fission-specific evaluator.

| Program / optimization / function | Binary / address | GED | Type | Byte |
| --- | --- | ---: | ---: | ---: |
| findutils / O2 / process_all_startpoints | bin_027.elf / 0x8070 | 212 | .133333 | .237687 |
| sysvinit / O2-noinline / wall | bin_012.elf / 0x2d50 | 86 | .105263 | 0 |
| gnutls / O2-noinline / process_options | bin_008.elf / 0x2ed0 | 503 | .090909 | .218527 |

No whole-function behavior pass count is available; do not imply one.
Baseline artifacts/traces are in
`benchmark/artifacts/decbench/2026-10-04-entry-lifetime` (ignored measurement data).
Reproduction: run the immutable executable with `decomp BINARY --addr ADDRESS
--no-db --json --prehir --debug-decomp`. For owner tracing, additionally set
`FISSION_PREVIEW_DIAG=1 FISSION_PREVIEW_DIAG_ADDR=ADDRESS`.
Initial global baseline: Union75/250; recompiles133/166; fixed91 Union23/91.

## 2. Owner proof

- [x] Builder/materialize: `ensure_explicit_merge_binding_for_block` selects a
  formal parameter solely because the output occupies an ABI slot.
- [ ] SLEIGH, normalize, structuring, printer, benchmark repair.

Existing emit traces show the first incorrect identity selection:

```text
find: merge_block=0x842f output offset=0x38 size=8 selected_binding=param_1
wall: merge_block=0x3013 output offset=0x38 size=8 selected_binding=param_1
wall: merge_block=0x308c output offset=0x38 size=4 selected_binding=param_1
psktool: merge_block=0x331e output offset=0x38 size=4 selected_binding=param_1
```

The formal is an incoming scalar in find, yet final output declares FILE* and
later assigns streams to it. These are different values of the same storage.
The issue also contains an executable pointer slice demonstrating why narrowing
a shared scalar/pointer carrier before splitting is unsafe.

## 3. Generality and invariant

When every incoming CFG edge has an unconditional definition covering the
entire merged storage, the join value is initialized independently of the
function-entry value. Its merge carrier must not be the ABI formal merely
because their physical storage overlaps. Existing materialization still emits
the predecessor assignments or join select. No copies/effects move.

Use the original heritage CFG, including edges pruned by structuring. A
predecessor may merely forward a value defined earlier: backward must-analysis
walks every such path, stopping at complete unconditional definitions, rejecting
entry paths without a seed, partial/guarded writes and call clobbers. Cycles
alone are not initialization evidence. The existing redefinition/storage and
instruction-local conditional-definition facts remain the owner of each stop.

Decline separation if a predecessor is missing, has only a partial definition,
or its last definition is instruction-locally conditional. In those cases an
entry/default value may still be required, and the existing behavior remains.
This is deliberately a bounded first step: arbitrary lifetime splitting,
entry-owned loop state and same-block conditional writes remain in issue182.

- [x] CFG, full storage coverage and conditional-definition facts; no ISA gate.
- [x] ABI-specific slot lookup remains in the existing provider.
- [x] Tests state diamonds/skip paths and storage widths, not corpus identity.
- Comparable patterns: the three measured functions above, across three
  programs and O2 / O2-noinline.

## 4. Risks and ownership

Extend the existing merge-binding owner; use its predecessor/redefinition and
same-block conditional-write helpers. No new pass, dependency or telemetry.
Do not reuse an existing formal-named merge from another block when the
current join proves a separate value. Preserve entry-owned merges when proof
is incomplete. Risks: narrower/wider overlapping carrier names, return
recovery, pointer type propagation, saved entry copies and effects.

## 5. Validation matrix

- Targeted builder tests: fully-defined diamond gets an independent carrier;
  missing/partial/conditional edge keeps initialized entry state; same carrier
  reused consistently at subsequent fully-defined joins; repeated lookup stable.
- `cargo nextest run -p fission-pcode`; normalize crate tests if affected.
- `cargo check -p fission-pcode -p fission-decompiler`; release CLI; fmt/clippy.
- Uncached fresh250 generation and unchanged standalone type/GED/byte scorers,
  compared against PR186 overlays, with old-perfect and recompilation losses
  individually inspected. No denominator changes or evaluator repairs.
- Mandatory external Docker: paired dev6 and fixed unscored32, used only as
  regression go/stop; wrapper gaps remain explicit.
- Native/Linux anchor agreement; capture-stub slice checks must preserve
  `0x123456789abcdef0`, including default and guarded paths. A slice is not
  whole-function semantic parity.
- Quality language only after real before/after evidence; unchanged Union is
  reported unchanged. Public ranking is not updated by local measurements.

## 6. AI firewall

No external or cross-model implementation advice; no subagents. Ground truth
is evaluator-only and is not consulted by production. Synthetic invariants
and the fixed unscored regression pool supplement the motivating rows.

## 7. Review

No row/function/address guards, no output-only semantics, no score-fitting
structure transformations. Semantic justification without a metric: a later
initialized value must not overwrite the independent incoming value's binding
and attach its pointer constraints to that incoming scalar.

## Results

The initial direct-predecessor-only candidate changed17/250 outputs, retained
Union75/250 and recompiles133/166, but type mean fell .265468346 → .265326863.
It did not change the three motivating anchors and is rejected for delivery.
The follow-up extends the same must-initialize invariant through forwarding
blocks; it is not a new row-specific exception or a relaxation for partial,
guarded, unseeded or call-clobbered paths.

### Accepted final measurement

The final builder uses the original-CFG backward must-analysis and filters
formal names before choosing an existing merge carrier. Carrier selection is
ordered by block/name instead of hash-map iteration. The latter fixes six
output/metadata differences observed between the intermediate and final builds;
two fresh final250 runs agree on NIR, HIR and variable metadata for every row.

| Metric | PR186 baseline | Final candidate |
| --- | ---: | ---: |
| Union | 75/250 | 75/250; no gained/lost perfect rows |
| GED perfect / mean distance | 65/240 / 33.8041667 | 65/240 / 33.7166667 |
| Type perfect / mean score | 18/228 / .265468346 | 18/228 / .265685895 |
| Byte perfect / mean score | 1/166 / .211909950 | 1/166 / .211910634 |
| Recompiles | 133/166 | 133/166; no gains/losses |
| Fixed91 Union / recompiles | 23/91 / 67/91 | 23/91 / 67/91 |

All250 requests completed on223 stripped binaries without errors;28 NIR outputs
changed. Standalone DecBench ingestion/type/GED/byte scoring ran fresh with
`DECBENCH_NO_CACHE=1`; no evaluator or adapter changes. Source-oracle inputs
remained hash-identical. The frozen-rival projection remains rank8 overall,
tied5 on fixed91; these are local measurements, not current public rankings.
Exceeding the frozen Kuna88/250 still requires14 additional perfect functions.

Row-level type deltas (fractional match scores, not perfect-function counts):

| Row | Before | After |
| --- | ---: | ---: |
| gnutls/O2-noinline/psktool/process_options | .0909091 | .1818182 |
| coreutils/O2-noinline/cp/copy_reg | .1176471 | .1470588 |
| openssh-portable/O2-noinline/ssh-keygen/ssh_rsa_verify | .1153846 | .0769231 |
| zlib/O2-noinline/minigzip/inflate_table | .2258065 | .1935484 |

These two type-score regressions are explicit tradeoffs. GED and byte scores
also move both ways on individual rows; report the complete overlays rather
than selecting gains. Lower mean GED does not prove whole-function correctness
or superior presentation. No Union/ranking improvement is claimed.

The motivating psktool output now declares its entry argument `uint` and forwards
the later pointer through a separate value to `execvp`; it no longer writes that
pointer into the narrow formal. Actual output tails were compiled and executed
with an execvp capture stub:12 cases per build/platform (six pointer sentinels,
default/selected modes),48/48 total. Both baseline and candidate preserve all64
bits, argv forwarding and exactly one call. This checks that separating/narrowing
the formal does not reintroduce issue182's pointer truncation; it is a call-slice
regression check, not execution/parity of the complete program. The original
find/wall carrier conflation and the touch lifetime defect remain unresolved.

Final validation:

- Six targeted invariant tests and1,639 crate tests passed; one existing skip.
  Pcode/decompiler checks, pcode clippy, workspace fmt and boundary audit passed.
- Paired local external Docker dev6 and fixed unscored32: identical outputs,
  fail categories, contracts and bare-compilation counts (4/6 and8/32). Existing
  six dev wrapper compile failures,28 missing pool wrappers and four adapter
  failures prevent any new whole-function behavior-pass claim. Pool was used
  only for regression go/stop, never tuning or ranking.
- Six Linux/native anchors: five exact NIR/HIR matches; one existing platform
  temporary-name difference is bijectively equivalent with identical types,
  expressions, branch/call/memory order. That difference exists on the baseline
  too. No broad cross-platform determinism claim.
- Named-call lexical inventory is unchanged on all250 outputs; lexical counts
  do not establish path-sensitive effect parity.

Artifacts: `benchmark/artifacts/decbench/2026-10-04-entry-lifetime/final/` in the
primary workspace; retain scorer tools, source.patch, logs, full overlays,
determinism results, call inventory, pointer-slice source/results and Linux
outputs. Issue182 remains open for saved-input/conditional/loop lifetimes.

Measured provenance before commit:

```text
base main: 2e0052a38de7c347c5c0896406e45cab13beaaa0
crates patch SHA256: 3ddee6b403050a345d201648b169622e10edbe269e6acb9ff39f4c76303b785d
native CLI SHA256: 2b11428dc3a89bae291c49974ccd8a1508c22e62b172af4141bb5f61b3284048
Linux bundle source fingerprint: ff7d2b3744f6897dedde5d132408d0d14966514b81f74cdfad996b6691506cda
fixed91 mask SHA256: 883255f7b14081af9347c6c510b6e36d48a30887b0b28155e9100ab5300cfae3
```
