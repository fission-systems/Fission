# Preserve value provenance across multiple CFG entries (#182)

## Pre-implementation gate

Base main: `20b55dce483daa45a0f374beb11754c01e4d268a` (PR187).
This proposal was recorded before semantic production changes. Two transient
logging probes do not alter output and will be removed before measurement.

## 1. Measured anchors

All250 scored functions were lifted with the accepted immutable PR187 CLI,
with `raw-pcode --addr ... --json --no-db`. Four have multiple roots; two
have overlapping root-reachable regions. An additional108 unscored functions
from the same two stripped binaries were lifted once for repetition evidence.
No source/DWARF or benchmark identity enters production.

| Binary / function | Address | Blocks / roots | Invalid dominance pairs | Baseline GED / types / byte |
| --- | --- | --- | --- | --- |
| coreutils/O2-noinline/touch/main (bin009) | 0x3920 | 53 / 0,48 | 39 | 187 / .1818182 / 0 (not compilable) |
| bzip2/O2/bzip2/fallbackSort (bin048) | 0xf510 | 104 / 0,103 | 464 | 351 / .1020408 / .0476437 |
| bzip2/O2/bzip2/sub_2500 (unscored) | 0x2500 | 140 / 0,134,137 | 238 | not scored; CLI baseline retained |

A pair is invalid when the claimed dominated block can be reached from
entry0 while excluding the claimed dominator. This is a direct graph
counterexample, independent of ranking. The scored anchors have8284/69470
NIR characters and18/67 gotos; whole-function runtime parity is unmeasured.
The other two multi-root scored functions have disjoint components and are
negative controls. This repeats in three functions, two programs, O2 and
O2-noinline configurations.

Artifacts: primary workspace
`benchmark/artifacts/decbench/2026-10-04-multiroot-dominance/`, plus raw
baseline investigation in `/private/tmp/fission-dominance-roots-baseline/`.

## 2. Owner proof

- [x] Shared CFG dominance (`fission-midend-structuring::cfg_analysis::DomTree`).
- Builder lookup is the first semantic consumer of the incorrect fact.

`DomTree::analyze` computes a local dominator set per root and calls
`dominators.extend(local)`. Shared descendants are overwritten by the last
root's path, losing paths from earlier roots. In the first anchor, root48
(0x3ebe) is disconnected from entry0 but reaches block14 (0x3b75). The old
map claims48 dominates14 although0 reaches14 without48.

Raw p-code at14 compares a current four-byte value with preserved input
storage. Temporary logging shows the latter resolves to a Copy in block48,
which forwards the primary-return surface. The printed predicate then compares
the wrong value and normalization deletes a branch. The shared fact is wrong
before type recovery, normalization, structuring or printing.

## 3. General invariant

A real block dominates a node only if it is on every path from every graph
entry to that node. For a node reachable from multiple roots, its dominators
are the intersection of the per-root dominator sets. An additional root for
a closed disconnected SCC must obey the same intersection rule if its
reachable region joins a previously analyzed region.

The repair extends the current shared fact owner; it does not add a value,
loop, predicate, presentation or architecture-specific pass. Root discovery
and single-root behavior stay as defined by the existing owner. This is
analogous to one virtual root connected to every discovered entry; Fission's
ImmDomTree and Ghidra Graph::calcForwardDominator use that invariant. Vendor
was consulted only for the invariant; no copied implementation/dependency.

Why the output is better without mentioning a metric: a comparison must read
its preserved input, not an unrelated definition on a path that need not have
executed. Rejecting false dominance prevents that provenance substitution.

- [x] No ISA/CC/function/address/compiler guards.
- [x] No new owner-to-owner dependency, pass or telemetry.
- [x] Shared CFG substrate repair, not printer/predicate workaround.

## 4. Risk

All consumers of DomTree see the corrected fact: reaching definitions,
stack-address recovery, region admission and loop analysis may become more
conservative. Existing perfect functions, call/memory effects, bounded output
and deterministic NIR/HIR/metadata are acceptance bars. Type/GED/byte deltas
and compilation losses must be reported; no score increase is assumed.

## 5. Validation matrix

- Four generic owner tests: overlapping entry paths, secondary loop entry,
  disconnected closed SCC, exhaustive4096 four-node directed graphs against
  an independent path-exclusion oracle. First show failure on the old code.
- Targeted then full nextest for fission-midend-structuring and fission-pcode;
  normalize regression, pcode/decompiler checks, clippy, workspace formatting.
- Fresh stripped all250 generation, uncached DecBench GED/type/byte with
  unchanged source/adapter/scoring inputs; compare accepted PR187 overlays.
- Remeasure the three real anchors, inspect NIR/HIR and original comparison
  operands; execute bounded output-slice cases including unequal values.
- Local Linux bundle plus paired external Docker dev6 and the fixed unscored
  regression pool (go/stop only, never tune/rank from pool outcomes).
- Boundary audit; retain any failure/tradeoff and all artifacts.

## 6. AI firewall / review

No other model was asked for implementation advice. Investigation is local.
Synthetic tests state only graph invariants, no corpus identities. Scored or
unscored observations are not runtime parity or official/public ranking.

## 7. Results

### Completed native measurement

- Four targeted owner tests fail3/pass1 before the repair and pass4 afterward.
  The exhaustive oracle covers all4096 four-node directed graphs.
- Full nextest:1935 passed,1 existing skip across structuring/pcode/normalize.
  pcode/decompiler checks, strict clippy, formatting and owner-boundary audit pass.
- Two fresh stripped all250 native generations:250 outputs,0 failures; NIR,
  HIR, variables, assembly and addresses are identical on repeat.
- Exactly one scored output changes: the first anchor restores the independent
  current-value/input comparison, file-processing call and loop. NIR8284→9328,
  HIR7464→8505 characters; gotos remain18. The second anchor output is unchanged.
- Executed unchanged equality-branch slices:5/10→10/10 on both native macOS
  and Linux x64. Only branch routing is executed, not file processing or the
  entire function. The oracle is the raw four-byte operand equality.

| Fresh unchanged DecBench contract | Accepted PR187 | Candidate |
| --- | --- | --- |
| Union | 75/250 | 75/250 |
| GED perfect / measured | 65/240 | 65/240 |
| GED mean distance | 33.7166667 | 33.675 |
| Type perfect / measured | 18/228 | 18/228 |
| Type mean | .2656858953 | .2656858953 |
| Byte perfect / measured | 1/166 | 1/166 |
| Byte mean | .2119106337 | .2119106337 |
| Recompiles | 133/166 | 133/166 |
| Fixed91 Union / recompiles | 23/91;67/91 | 23/91;67/91 |

The first anchor GED decreases187→177; all other metric values are unchanged.
Accepted perfect functions lost:0. The frozen-rival local projection remains
8th overall and tied5th on the fixed91. Overtaking Kuna88/250 still requires14
additional perfect functions. These are not official/public ranking results.
All three metrics were rerun only after complete250-row ingestion; an earlier
run started before preparation completed and was rejected. The restarted byte
checkpoint directory is fresh; metric caches are disabled. Source oracle and
manifest hashes match accepted inputs.

### Limits and external regression result

The restored loop still reads an uninitialized synthetic carrier in its first
guard (`uVar44`); its entry/live-in connection is a separate remaining defect.
This patch proves the shared dominance invariant and the measured equality
slice. It does not establish whole-function semantic parity or a Union gain.
Issue182 remains open for additional lifetime/loop initialization work.

Linux anchored outputs have temporary-name differences from macOS in two
functions. The unchanged negative control has the same Linux output before
and after this patch; cross-platform whole-output equality is not claimed.
Native repeated-generation determinism is measured independently.

External local Docker dev6 and fixed unscored32 were compared against a fresh
clean-main `20b55dce4` Linux build made with the same toolchain/resources.
All38 outputs and fail categories are identical; bare compilation remains4/6
and8/32. Six dev wrapper compile failures,28 missing pool wrappers and four
adapter failures remain, so this is regression evidence with limited behavior
coverage, not a semantic-pass claim. No checkpoint rows were recovered.

The earlier accepted Linux bundle differed on one pool output: two calls lost
their descriptor arguments. The new clean-main baseline already exhibits that
same output as the candidate, proving the delta is not caused by this patch.
The older artifact discrepancy remains unexplained; it is retained, not erased
or counted as a quality gain. No implementation was tuned against the pool.
Final go/stop uses the fresh clean-main build comparison.

The third, unscored bzip2 anchor was rerun with the immutable native candidate.
Its final NIR/HIR output is unchanged; this is repeated raw-graph owner evidence,
not an additional output-quality gain.

Measured provenance before commit:

```text
base main: 20b55dce483daa45a0f374beb11754c01e4d268a
crates patch SHA256: 23b26e1b5a4f977ba6f5ba439a0aa1850d0b50e769653bbd42c1510832e68a45
native CLI SHA256: dbbdf1d2ec469d810d6b04abc8372f83f4a1381f4992388f70aa98707d06a6d8
Linux candidate source fingerprint: b6e95bd6c281998132a0428cc5e4e2e1f5ee9f812334613fc6b8881845227e49
Linux clean-main baseline fingerprint: 3e39041a9e4ef11a39c549f412f551c2840967fdc1502c1455a2f2fd72ecf5bd
fixed91 mask SHA256: 883255f7b14081af9347c6c510b6e36d48a30887b0b28155e9100ab5300cfae3
```


