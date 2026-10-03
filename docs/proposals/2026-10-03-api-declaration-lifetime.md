# API declarations belong to a stable argument value

Recorded before production edits. Base main 2726ddd753, 2026-10-03.

## 1. Measured baseline anchors

Fresh accepted stripped-input outputs and uncached standalone DecBench scores:
`benchmark/artifacts/decbench/2026-10-03-generic-api-pointer` (primary checkout).
This cycle records baseline-anchors.json and full anchored output in
`benchmark/artifacts/decbench/2026-10-03-api-declaration-lifetime`.

| Program / source / optimization | Function / binary / address | Type / GED / byte | NIR lines / gotos | Defect |
| --- | --- | --- | --- | --- |
| findutils / find.c / O2 | process_all_startpoints / bin_027.elf / 0x8070 | .133333333 / 212 / .262806236 | 244 / 10 | argc input param_1 declared FILE*, later assigned 1 and stream values |
| sysvinit / wall.c / O2-noinline | wall / bin_012.elf / 0x2d50 | .105263158 / 89 / 0 | 360 / 19 | text input param_1 declared FILE*, eight subsequent assignments include descriptor and stream values |
| gzip / gzip.c / O0 | get_method / bin_219.elf / 0x72f1 | .076923077 / 45 / .129527991 | 730 / 16 | reused rax declared FILE*, includes byte/scalar/string values and spreads declaration to copies |

Three functions, three programs, mixed optimization. HF ground truth source
bodies are evaluation-only, verified at revision
7af6c5e19289b6a357404708e5373bc7bf6fcacd. Entry arg correspondence in the
first two anchors uses arg_index, not type. No executable semantic cases are
available for this static slice. Union baseline 74/250; byte baseline uses the
fresh paired GNU-tools 166-function lane, not earlier compiler environments.

## 2. Owner proof

Existing normalize/type owner: call_target_surface::apply_api_surface_type_transitively.
It writes surface_type_name on the immediate argument before checking
binding_is_safe_for_backward_refine. That check protects only further copies.
The declaration locks unrelated definitions of a reused binding. Parameters
also have an implicit entry definition: even one body assignment means a later
call need not constrain the entry value. Raw lifting and printer do not own
these callsite-derived facts.

## 3. General invariant and non-metric justification

An exact API argument declaration can refine a whole binding only when its
value is stable: no multiple definitions or self-referential updates, and an
entry parameter must have no body assignment. Check this before each binding
update, including the first argument and backwards copy sources. Untouched
parameters and unique-definition local copies retain specific API declarations.

Why better: using a register for a file stream later must not declare the
earlier input integer or text pointer as a stream. No function, address, ISA,
compiler or dataset identity enters the condition. No vendor implementation is
copied and no runtime ground truth is available.

## 4. Scope and risks

Extend the existing helper with the existing shared definition/self-reference
facts; no new pass/helper/metric/dependency/telemetry. The entry-parameter check
adds the implicit definition to this owner's existing proof without changing
the shared solver contract. This cycle concerns exact surface declarations;
full SSA lifetime splitting and alias-aware memory modeling are separate work.
Some legitimate homogeneous reassignments may lose a useful surface name;
inspect complete output and recompilation changes. Do not change effects,
evaluation order, call targets, arity, or existing explicit surfaces.

Kuna's recent changes motivate investigating value lifetime and ABI evidence:
[alias preservation #832](https://github.com/Noelo-Lab/kuna/pull/832),
[load guards #882](https://github.com/Noelo-Lab/kuna/pull/882), and
[forwarded call arguments #875](https://github.com/Noelo-Lab/kuna/pull/875).
These are reference directions; their upstream impact figures are not Fission
measurements and the fixes are not transplanted.

## 5. Validation matrix

- Synthetic boundary coverage: reused direct local, self-referential local,
  overwritten parameter with one body definition, stable local copy and
  untouched parameter. Existing callsite tests must remain green.
- Targeted nextest, then full normalize+pcode suites; checks including
  decompiler, fmt --all/check and scoped Clippy.
- Immutable native release CLI; all250 fresh stripped outputs, native
  provenance, cache-disabled GED/type/byte with unchanged metric sources.
  Inspect anchored NIR/HIR and complete changed outputs; preserve perfect rows.
- Linux release CLI, mandatory external local Docker dev6 plus existing fixed
  unscored32 pool against accepted baseline with caches/resume disabled.
  Existing wrapper/adapter/no-wrapper failures are limitations, not passes.
- Report partial regressions, compile deltas and local rank projection against
  the frozen rival snapshot. Never publish local results as official latest.

## 6. AI firewall and review

No external model or subagent implementation advice. Local investigation only.
Synthetic tests plus the unchanged unscored patch pool guard generality; pool
is not a tuning target. No benchmark metric or source-oracle changes. Production
guards remain owner-native definition/use facts. Quality is claimed only after
before/after real measurement; test green alone is insufficient.

## 7. Rejected first experiment and revised scope (before revision)

Blanket direct-binding guards were measured and rejected: Union 74->73,
type perfect 17->16, mean type .264112288->.258498962, and restore's GED
27->36. Preserve that experiment under 2026-10-03-api-declaration-lifetime;
nothing was committed. Typed nullable locals in close_files and restore have
an initial zero plus their meaningful pointer definition; a simple count is
insufficient to classify these as heterogeneous local values. Removing scalar
typedef surfaces also changed a downstream conditional. Do not ship those
changes or weaken the guard through a row/type-name exception.

Revised scope: protect the implicit incoming value of an entry parameter.
Any body assignment to that parameter means a later API use cannot establish
its entry declaration. Apply this before updating the parameter, whether it
is the immediate argument or reached through a stable local copy. Keep the
existing local refinement behavior and copy guards. Local lifetime splitting
or proof across all reaching definitions remains future work; do not claim
the reused-local part of the defect is fixed.

Three focused entry-value anchors before revision: process_all_startpoints
(findutils O2), wall (sysvinit O2-noinline), and getpasswd (sysvinit
O2-noinline, sources/sulogin.c, bin_209.elf 0x3b40). The third source argument
is console*, but its input is declared FILE*: record-field reads use the input
and three later param_1 assignments load stderr for calls. Current type .05,
GED49, byte0, NIR219 lines/9 gotos. These are three distinct functions across
two programs and mixed optimization, not duplicate copies of wall.

The revised guard uses the existing body-definition counts and parameter list;
no new traversal, pass, helper or dependency. Tests cover unchanged parameters,
one body assignment (including zero), and stable local aliases that must not
carry a later declaration into an overwritten input. Remeasure all250 from
the accepted 74/250 baseline with unchanged metric sources and fresh output,
plus another external Docker pair. Preserve first-experiment evidence.

## 8. Entry snapshot proof (before third experiment)

The entry-write-count-only experiment retains Union74 but decreases mean type
.264112288->.262297144. It is also rejected and uncommitted. Evidence lives in
2026-10-03-api-entry-value. Two gains (find argc and psktool input) are offset by
losses in ssh-keysign/log_verbose_add (O2, .5->0) and sshd/ssh_agent_sign
(O2-noinline, .5->.428571429). They save incoming values in rbp, r12 and xVar14
before using their ABI carriers for other calls. The saved local still holds
the input; later API evidence on that immutable local legitimately constrains
the input declaration. wall similarly saves its input in local_248 before
reuse. These shapes span openssh/sysvinit and mixed optimization; the three
original overwritten-entry anchors remain the motivating defect.

Refine the existing API owner with an immutable entry-copy certificate:
traverse the unconditional structured spine from entry, track entry origins
through plain or machine-word integer copies, and record local copies only
when they have one definition, no initializer and no address escape. On
structured branches/loops/switches, invalidate every potentially written name
using existing shared definition counts, then continue along the spine; do
not invent proofs from inside conditional/loop bodies. Stop creating proofs
at labels or explicit transfers. Previously recorded immutable copies remain
valid. A surviving original value can be saved after an unrelated branch.

When the API argument is such a certified immutable copy, carry its exact
declaration to the certified entry parameter even if that parameter's ABI
carrier was later overwritten. Other overwritten parameters keep the guard.
This preserves a value, not a name or a chosen typedef, and uses no row/ISA
identities. Do not remove all local declaration propagation. No change to
scalar/return lattice inference, call arity, effects, or metrics.

The flat callsite tuple and whole-function copy_sources cannot distinguish
an incoming value from a later reuse of the same binding name. Format literal
tracking is a different fact and does not supply entry provenance. A small
owner-local certificate helper is justified; reuse the existing bit-preserving
copy recognizer, definition-count traversal and address-taken facts. No new
pass, telemetry or cross-owner dependency. Synthetic tests must reject copies
after overwrite, overwritten/address-escaped locals, narrowing casts, loop
assignments and label crossings, and preserve the saved input despite later
carrier writes. Rerun all250 and external Docker against accepted baseline74;
the unscored pool remains a go/stop signal, never a tuning source.

## 9. Third experiment measured outcome

Evidence: 2026-10-03-api-entry-snapshots in the primary checkout's ignored
benchmark/artifacts/decbench tree. Final immutable native CLI SHA256
29834d4450bd08f7f2f053f6d9edd60ae9881a1f21d14c35813c3f8ff17de612.
The exact production patch fingerprint is recorded in run.json.
No metric, adapter, oracle or input-dataset changes.

- Fresh250 outputs across223 stripped binaries; zero generation/ingest errors.
- Type mean .264112288->.264490127; perfect17/228 unchanged. Two gains:
  find/process_all_startpoints .133333->.2; psktool/process_options
  .090909->.181818. One partial loss remains: ssh_agent_sign .5->.428571.
  Its alg char* declaration is still lost; key's old char* was also incorrect
  (ground truth sshkey*). The certificate preserves saved values, not every
  interprocedural pointee inference. Do not claim all recovered types correct.
- All250 GED values/source hashes unchanged; perfect65/240, mean33.629167.
- Byte perfect1/166 and compilation132/166 unchanged. Mean
  .203560186->.203552053: find decreases .262806->.26, psktool increases
  .197561->.199017. This small byte decrease is a measured tradeoff, not a win.
- Union74/250 unchanged, zero perfect rows lost. Rank8 local projection
  against frozen rivals, not a live official rerun. Kuna88 requires89:15 more
  perfect functions remain. A type-mean increase is not a ranking increase.
- 46 targeted tests, full normalize+pcode1629 passes/one skip; checks, scoped
  Clippy, fmt and boundary audit passed. Native/Linux release builds passed.
- External Docker dev6 and fixed unscored32: identical outputs/statuses,
  bare compilation4/6 and8/32 unchanged; existing wrapper and adapter/no-wrapper
  limitations remain. No executable semantic equivalence claim. Linux anchors
  agree after bijective temporary renaming; two host-specific names differ.

The static type average rose, but this third experiment is rejected too.
The execution probe below proves a value-preservation regression. This result
does not qualify as a decompiler quality improvement. Full SSA/ABI carrier splitting is the next owner-level work:
an input and the later call-preparation value should not share a C binding.
Kuna's call/return and alias-preservation history supports prioritizing value
lifetimes and ABI evidence ahead of additional output formatting.


## 10. Rejected before merge: width loss in a reused carrier

No production change from any of these three experiments is shipped. The
third experiment passed all static gates above but failed the additional
value-preservation check. Accepted production remains main2726ddd753:
Union74/250, type17/228 with mean .264112288, GED65/240, byte1/166,
compilation132/166. The candidate .264490127 type mean is not the accepted
baseline. No ranking gain was achieved.

Measured anchor: gnutls/O2-noinline/psktool/process_options, bin_008.elf,
0x2ed0. Its incoming argc is later reused to carry a pager filename pointer.
The immutable copy certificate recovers the incoming scalar declaration but
does not separate storage for the later pointer. Baseline declares param_1
char*, candidate declares it uint. Both emit the same real tail:

```c
param_1 = !zf ? rax : "more";
__argv = 16 + rsp;
execvp((unsigned long long)param_1, __argv);
```

Executed an extracted output slice with rax set to the 64-bit sentinel
0x123456789abcdef0, zf=false, and execvp replaced by a capture stub. Only the
parameter declaration differs between baseline/candidate slices. Compiled
with native clang -O2, then with the benchmark GNU musl GCC14.2.0 and
-std=gnu17 -Wno-error=int-conversion -Wno-error=incompatible-pointer-types.
The statically linked x86-64 probes were executed in a network-disabled
linux/amd64 Docker container; both compiler/platform pairs produced the same
respective preserved/truncated results. No executable corpus binary or real execvp was
run. This is an anchored slice check, not whole-function equivalence.

| Slice | Declaration | Captured call value |
| --- | --- | --- |
| Accepted baseline | char* param_1 | 123456789abcdef0 |
| Rejected candidate | uint param_1 | 000000009abcdef0 |

The candidate drops the high 32 bits. Recompilation success cannot detect this
loss: the benchmark fixup accepts pointer/integer conversions. Byte distance
can improve despite incorrect values. Mean type recovery cannot override the
NIR/effect-value preservation contract.

Reproduction sources, compile logs, LLVM IR, exact extracted tail and output
hashes are retained under the third experiment's width-probe directory.
The real output SHA256s are:

- Baseline: 930c03c8f2c5a39c8c2f439bf161ace5f972ee732b2217ab37e62d95f9ec90a2
- Candidate: 55dadc74715c39355f884ff57ad025fece5e75a4c67a656b9f40c3e22d991269

The intended production diff is archived as source.patch before restoring
both modified type-owner files to their exact accepted Git contents. Preserve
all failed measurements; do not keep a known truncation change for a score.

## 11. Next implementation direction from owner evidence and Kuna history

Tracked as [Fission issue182](https://github.com/fission-systems/Fission/issues/182).

First priority: represent the immutable incoming ABI value separately from
mutable register/merge carriers. The width and effect order of each later
value must be preserved before incoming type declarations can narrow.

Existing owner hotspots, requiring raw-p-code/SSA trace confirmation:

- Builder ensure_explicit_merge_binding_for_block chooses a parameter name
  from ABI slot identity for a merge carrier. Slot identity alone does not
  prove that later definitions are the incoming source value.
- Builder same_block_cmov_entry_register_binding_name_at checks prior writes
  within the block. Incoming-value reuse additionally needs reaching-definition
  evidence across predecessors; a predecessor's write is not an entry value.
- Normalize entry_param_promotion renames an entry-spill local throughout the
  whole body. A mutable spill requires incoming-value evidence and separate
  storage, rather than global source-parameter identity.
- Callsite declarations constrain argument values. A whole-name surface
  declaration cannot repair heterogeneous definitions.

Do not add another callsite guard or printer substitution. Reuse the existing
scalar SSA, storage-piece, CFG dominance and out-of-SSA copy owners to prove
which value reaches each read. A split must keep the skip/default arm, joins,
loop-carried values, saved entry copies, pointer width, partial-register
extensions and address escapes. ABI differences belong in provider models.
Require a new pre-implementation proposal with three real repeated anchors
across two programs and mixed optimization, focused executable width/effect
checks, crate gates, all250 uncached metrics and the external unscored pool.
Do not tune on that pool or publish local scores as official rankings.

Kuna priorities supported by inspected primary changes:

1. Call/return value lifetimes and forwarded incoming arguments (#875, #877,
   #879, #889): distinguish value provenance from physical register identity.
2. Alias-aware memory effects (#832, #882, #893): preserve stores before
   pointer reads and address-passed storage; avoid stale constant replacement.
3. Aggregate/field and ABI split-width recovery (#863, #828, #864): preserve
   width/extension rules before refining source-facing types.

Kuna's #875 explicitly relies on declared parameter/return evidence and admits
ambiguities. Fission's scored stripped lane has no runtime source/DWARF, so
those declarations are not an available production oracle. Transfer the
invariants, not the implementation or its reported upstream measurements.
