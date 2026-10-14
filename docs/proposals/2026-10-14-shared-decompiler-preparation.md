# Share immutable decompiler preparation and repeated query results

This is a mechanical performance change. It does not change lifting, CFG,
normalization, structuring, profiles, worker counts, or decompiler quality.

## Observed baseline

The pinned 2026-10-14 measurement uses validated source 87825864d, release
codegen, serial requested-function workers, three alternating process starts,
and the frozen qualification inputs. Linux Snappy O0 (six functions) takes
2.333 s median, Windows Snappy O2 (five) 1.259 s, and macOS Ninja O2 (five)
1.486 s. The corresponding Kuna 1.808 times are 0.190/0.112/0.333 s. These
are small-batch timings, not full-corpus or latest-release comparisons.

## Owner and invariant

`CompiledRuntimeRegistry::discover` clones all three immutable catalog vectors
on every call. Frontend selection separately clones the entire cached entry
catalog to retain one entry. Decode and callee preparation invoke those paths
repeatedly. Both catalogs already have process-lifetime initialization contracts;
this change shares that same immutable data instead of reconstructing ownership.

The public owned entry-list API stays owned. Frontend lookup can borrow the
cached catalog and clone only the selected entry. Registry accessors, ordering,
selection precedence, ambiguity/error behavior and failed-discovery retry remain
unchanged. No new process cache or production dependency is introduced.

The catalog already lives in a process-lifetime `OnceLock`; registry views borrow
static slices from it. An earlier `Arc` view experiment added unnecessary atomic
reference counting to an object whose lifetime cannot end. That representation
is not retained in the final candidate. This change is not attributed as the
cause of any timing variation without a controlled comparison.

This is an initial candidate, not proof that catalog allocation is the dominant
bottleneck. Symbolized profiling and before/after timing decide whether it is
worth keeping. The two NIR/HIR builds remain because their policies differ.

## Second repeated query: phi operand reachability

The symbolized Snappy baseline also samples `ssa_phi_has_operand_use` in the
builder. Two filters query every phi independently; each query scans all
operation inputs and recursively scans the entire edge-copy list. The result
depends only on the already-validated immutable `NirScalarSsa` snapshot.

For that exact predicate, seed values appearing in operation inputs, then walk
copy edges backwards until a fixed point. Membership is equivalent to the
existing forward reachability query, including live/dead cycles, diamonds,
parallel edges and disconnected components. Compute this set once for the two
existing filters; do not change observable-use handling, return ABI exceptions,
value identity, phi admission or edge-copy scheduling. Empty phi populations
need no precomputation. This replaces an existing query rather than adding a
recovery pass or semantic rule.

Before accepting this second candidate, compare the original predicate against
the worklist on every three-value directed graph and every set of operation-use
seeds, then run pcode nextest and the same real-output replay. No model is asked
for implementation advice, and no corpus identity enters production conditions.

## Whole-binary flow facts copied for every decode projection

**Rejected from this submission.** The prototype preserved all 250 function
outputs, but a large-input control pair took 250.183 s on baseline and 331.526 s
on the prototype. Concurrent machine work was observed and the cause was not
isolated. The projection borrow is excluded rather than accepting that unresolved
performance risk. The experiment and raw outputs remain in local artifacts and
the local `codex/decomp-preparation-experiments` branch.

The existing `control_flow_facts_for` cache stores an immutable snapshot in a
shared initialization slot, but returns a deep owned clone. Its two production
decode helpers immediately read that clone to produce a per-function context or
one extent. Native profiling samples that cloning path inside callee decoding.

Borrow the pinned cache slot while executing those read-only projections. Keep
the public owned-returning API unchanged and use the same cache key, eviction,
initialization, and projection code. The borrow must retain the initialization
slot across the callback and release the global cache mutex before computing or
calling the projection. This removes whole-program copies without a second
metadata map, a new runtime dependency, or altered address/CFG facts.

The prototype tests proved repeated projections see the same cached object, mutation of a
public owned result cannot alter the snapshot, and projected memory contexts are
identical to those obtained through the existing owned API. Whole-crate static
tests passed, and the same real-output comparison gate passed. Those results did
not establish a latency improvement.

## Validation gate

- Prove catalog sharing and independent owned-list mutation with focused tests.
- Run fission-sleigh nextest, decompiler/CLI checks, and fmt.
- Use the same release codegen and commands for alternating baseline/candidate
  runs. Compare every returned code layer, type/variable record, address map,
  error/fallback and evidence field after excluding explicit duration counters.
- Regenerate the frozen 250 stripped requests without decompilation caches.
  Require no missing/extra functions, no changed code/metadata/provenance, and
  deterministic repeats before reporting measured speed improvement.
- Preserve primary checkout changes and PR #203. Keep measurements under
  benchmark/artifacts/performance/2026-10-14 in the primary checkout.

No source/debug oracle is provided to either producer. No semantic or readability
gain is claimed, and no public ranking or evaluation policy is changed.
