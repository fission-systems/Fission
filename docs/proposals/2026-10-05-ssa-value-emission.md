# Issue182: SSA value identity through materialization

## Accepted delivery decision

Candidate Z passes the agreed gates against main `493d93d2`: Union
**75 -> 76/250**, no lost perfect rows in any metric, and no lost recompiling
rows (133/166 retained). Mean GED decreases from 33.675 to 33.620833333333;
mean type and byte scores increase. The full regression report records every
changed scored row, external partial losses, bounded execution coverage,
determinism, artifact hashes and the 1,759 passing tests:
[measured regression report](../reports/2026-10-05-ssa-value-emission.md).

The sections below retain the pre-implementation proposal and chronological
candidate investigation. Their interim admission restrictions and failed
measurements are historical; the accepted scope is defined in the report.
Unsupported lifetime families still use the existing path, so #182 stays open.

## Measured anchor and owner

Base main493d93d2, immutable CLI SHA256708d9f430076ce70d03ad4d56e25b52f9c7b4e057a2a93e6fae04c729727a63d.
Fresh stripped observations are under the primary checkout's ignored
benchmark/artifacts/decbench/2026-10-05-ssa-lifetimes. All223 input hashes
match functions.json; one missing input was recovered by content hash from
an older kit (its anonymized filename was different).

| Program/configuration | Function / input | Measured defect |
| --- | --- | --- |
| findutils/O2 | process_all_startpoints, bin027 0x8070 | FILE* entry argc;3 later formal writes;8146 NIR bytes |
| sysvinit/O2-noinline | wall, bin012 0x2d50 | FILE* entry text;8 later formal writes;9900 NIR bytes |
| gnutls/O2-noinline | process_options, bin008 0x2ed0 | uint entry carrier receives later unrelated arithmetic;2 writes;13617 NIR bytes |

No whole-function behavior parity is available. Baseline full metrics are
75/250 Union, GED65/240 mean33.675, types18/228 mean.2657754047593307,
byte1/166 mean.2118900452260683, recompiles133/166. Fresh generation and
uncached scoring must reconfirm these before accepting a candidate.

First owner: builder definition lookup / materialization / join emission.
The definition lookup scans same-storage writes and dominance; scalar SSA
already identifies inputs with SsaUseSite/SsaAccessPiece. A definition dominating
a use is not necessarily the reaching value when another path modifies it.
Existing loop-name reservations also do not establish an emitted initialization;
see the rejected2026-10-04 loop proposal. Normalize entry-spill promotion can
subsequently rename a redefined local across its whole body.

## Invariant and implementation boundary

A read denotes a particular SSA value, not every value of its storage. Preserve
original operation identity and emitted CFG-node identity separately. Resolve
whole-storage, unguarded reads against existing SSA first; do not extrapolate
single-piece facts to incomplete/partial accesses or instruction-local skips.
Materialized definitions and their reads must share one typed value binding.
Keep formals for proven entry inputs; subsequent mutable phi carriers are
separate, with incoming copies on their actual predecessor paths. Cover rejection
keeps distinct names and copies; it never licenses a forced merge. Effectful
producers execute at their original site. Existing scalar SSA/call guards/ABI
providers remain the semantic source, no vendor dependency or output patch.

Extend current builder/value and merge owners; private typed emission planning
is justified where current scalar SSA facts have no emitted-binding/edge-copy
consumer. Reuse existing validate_shape and validate_scalar_ssa_with_context.
Unknown input identities, partial pieces, conditional definitions and unproven
clone correspondence are admission failures, not guessed formals. Entry-spill
promotion must decline whole-body renames with later redefinitions.

## Validation and delivery

- Synthetic def/use, predecessor override, nested phi, unseeded/default arms,
  loop0/1/multiple iterations, high pointer bits, width views, cover rejection,
  cyclic parallel copies, block clones, calls and memory ordering.
- Targeted then full nextest for affected crates; check pcode/decompiler;
  release CLI; fmt; clippy and existing boundary audit.
- Same actual NIR/HIR rows remeasured, executable slices explicitly labeled.
- Fresh250 generation twice, identical input/evaluator hashes, uncached all
  GED/type/byte scores. No lost perfect functions or compiled rows; GED mean
  must not increase and type/byte means must not decrease. Partial row losses
  individually inspected and published. Union must reach at least76/250.
- Paired external local Docker dev6 and fixed unscored32 regression pool;
  no pool tuning or public ranking publication. Wrapper limitations explicit.
- No row-identifying production conditions, no external model/subagent advice.
  Ground truth stays evaluator-only. Consult vendor invariants without copying.
- Commit from clean delivery main, push review branch, CI then merge main only
  after all gates. Keep issue182 open until all remaining lifetime cases pass.

## Rejected candidates and refined admission

Fresh candidateC left Union75/250 and all metric values unchanged. CandidateF
changed29 NIR outputs but kept Union75/250 and lost no perfect functions.
GED mean33.6625 improved fractionally, while type mean.2657024711 and byte
mean.2107658685 regressed; candidateF is rejected for delivery. Its full gate
also exposed an entry-block phi lacking the implicit function-entry operand.
Such plans now decline atomically instead of emitting an uninitialized carrier.

Additional baseline anchors from the same immutable493d CLI:

- libexpat/O2-noinline/xcscmp, bin0200x3350: initial byte emitted to rax;
  the loop reads uVar1 before any initialization. Baseline1079 NIR bytes.
- coreutils/O2-noinline/main, bin0090x3920: initial load and loop carrier
  disagree. See rejected loop-entry proposal for the three-function diagnosis
  and the actual-vs-reserved definition trace.
- rsyslog/O0/parseRFCStructuredData, bin2050x29c5f: baseline7453 NIR bytes;
  byte predicates cross a CFG join; candidateF introduced explicit predicate
  carriers with a different statement/control surface. Its GED27→34 decline
  is recorded as a regression, not an improvement. Original493d PreHIR/trace
  were freshly captured; no whole-function execution parity is established.

Width refinement uses complete connected SSA storage families. Each wider
producer has one original-site binding; disjoint pieces are extracted from
that binding using unsigned casts, shifts and masks. Reads reassemble the
same pieces. A family is rejected if any member lacks a proven input or
producer, or requires an unimplemented edge move. Non-power-of-two byte
pieces use a containing integer plus an explicit physical-width mask.
Implicit ABI/return reads may consult an exact SSA state at that point; a
join without a unique incoming identity remains a rejection, not a nearest
storage-definition guess. Calls still require provider-backed preservation.

Conditional copies currently enter the new path only when existing
interference-checked congruence eliminates the actual move. Entry-block phis,
instruction-local guarded writes and unsupported copy placement keep the
old path. These limits are not represented as issue182 completion. Parallel
copy scheduling preserves cycles/fanout with an edge-local saved value.

## Conditional-edge refinement and rejected candidateL

CandidateL: Union75/250, no fresh-baseline perfect losses; GED mean33.8208333
regressed, type mean.2659503551, byte mean.2119764647, recompiles134/166.
External dev6 lost one bare-compilable row (4→3); frozen32 kept8 compilable.
L is rejected. The named-object carrier emitted generic field names against
a named-field layout. That layout still belongs to the ABI binding's type-hint
owner; an unproved layout transfer now rejects the entire connected family.
This fallback is a compatibility restriction, not a claimed type-recovery fix.

A non-coalesced phi copy on a two-way edge must use the branch decision before
any copy can overwrite its operands. The builder preflights a canonical final
CBranch and verifies its lowered targets against both actual CFG successors,
then records one preserved boolean snapshot used by both edge copies and the
terminator. Parallel copies run only in that decision's corresponding arm.
Unsupported branch shapes still reject the plan atomically. Tests cover an
unchanged/default ABI input path and a redefined wide-value path without
writing the formal. Load preservation belongs to every member of an emitted
congruence group, not only its first member.

## Entry definitions without phi

The measured input/redefinition defect is not limited to join blocks. A proven
ABI input with later operation definitions needs separate value bindings even
when its later uses have one reaching definition and heritage needs no phi.
Include those reused input storage families under the same complete-piece,
call-preservation, layout, guarded-definition and atomic-validation contracts.
This consumes existing input/value/use facts; it does not introduce a storage
name as a substitute identity or infer an unproved incoming upper byte.

CandidateM's recovered condition was preflighted before its block load had an
emitted binding, which cloned that load into the snapshot. M is rejected on
effect count before score acceptance. Conditional preflight must materialize
the original body first and use that resulting binding environment. A focused
load-count regression checks one read across the body and cached terminator.

The no-phi entry extension declines an ABI input that is also the provider's
primary return slot: return recovery contains implicit reads with a separate
contract, so actual operation operands alone do not prove full correspondence.
The broader test gate exposed that omission; the guard is role based and does
not name an ISA or register encoding. Those cases retain the established path.

The isolated builder now lowers all original bodies and terminators before
applying the plan, so a missing RHS or failed terminator rejects it without
changing the live builder's bindings. Empty plans exit before that preflight.

CandidateN exposed a second observational boundary: full-path preflight
published legacy parameter/return state when it lowered every block directly
on the trial that would be committed. N is rejected before score acceptance.
The full-path probe now uses its own cloned host and discarded register-origin
channel. Only SSA planning and necessary original-site conditional bodies are
committed. A regression verifies that an unselected ABI read in the probe
cannot publish an unrelated formal before the actual lowering reaches it.
Conditional snapshot identity survives ordinary terminator-cache invalidation.

CandidateO is rejected: fresh Union75/250 with no perfect-function loss,
GED mean33.7958333333 (baseline33.675), type mean.2676117055,
byte mean.2091150661 (baseline.2118900452), recompiles132/166.
Previously compiling argv_split, xcscmp and strcmp_until stopped compiling;
discover_class and check_ea_in_inode newly compiled. Native repeated generation
was deterministic for all250 functions' NIR/HIR/PreHIR and variable metadata.
External dev6 and fixed32 retained4 and8 bare-compiling functions respectively,
but their wrapper limits do not establish whole-function execution parity.
The benign extracted tar carrier body passed8/8 bounded stub cases; this
does not supersede the full-score and recompilation rejection.

## Observable phi transport refinement

A diagnostic-only native build observed phi outputs with no path through
phi copies to an actual operation operand: xcscmp49/54 (123 incoming copies),
wall129/147 (298), and parseRFCStructuredData217/217 (566). The first two
are O2-noinline; the third is O0. These are *operand-unobserved*, not a proof
that ABI return recovery cannot read them. Preserve every primary-return-slot
phi conservatively. Call admission continues to require preserved slots.

Invariant: a transport that can reach neither an explicit operand nor an
implicit admitted ABI read must not invent an entry-value use or emitted
binding. Determine reachability through the existing phi-copy graph, retaining
all producer operations/effects and all used phi operands. A closed unused
phi cycle is not an observable read. Filter only those phi bindings/copies;
do not change heritage, interference covers, metric definitions or effects.
This is justified by value-use correspondence and the absence of a consumer,
not by emitting fewer graph nodes. Add live-vs-dead cyclic-phi tests and
implicit-return retention before remeasuring; no quality claim is made yet.

## Layout compatibility required by separate pointer carriers

Actual xcscmp (libexpat/O2-noinline), strcmp_until (cronie/O2), and the
Bash/O2-noinline function at bin0960x9660 walk a byte pointer by one byte
and load the same element width at offsets0/1. The first two acquired
synthetic one/two-byte structures after separate SSA pointer-carrier emission;
the third already has that misleading shape on493d. Exact NIR and input
hashes are in array-layout-anchors.json. CandidateO's first two fail the
unchanged recompiler's member access checks. This is a type-owner compatibility
defect; the evaluator is not changed.

Owner proof: aggregate_fields::can_upgrade_binding_to_aggregate treats a
Ptr(Int8/Int16) as automatic record evidence while already requiring mixed
access widths for Ptr(Int32/Int64). Constant offsets alone cannot distinguish
a homogeneous scalar array from a record. Extend that existing homogeneous
scalar-pointee rule to all integer widths. Unknown pointees and known aggregate
layouts retain their existing policies; heterogeneous-width record evidence
continues to refine an inferred scalar pointee. This preserves the element
model of a recovered byte cursor instead of asserting an unsupported object
identity. It does not migrate all type inference to SSA IDs or add a pass.

Validate byte/halfword homogeneous arrays, mixed byte/pointer records and
trusted named byte-field structures; then remeasure all250 stripped functions,
compile retention, effects and the external paired dev/pool gates. No quality
gain or issue182 closure follows from synthetic coverage alone.

The exact first identity leak is narrower than all type inference:
DefinitionDependencyMap retains `cursor -> formal` from an initial copy even
after another definition advances cursor. `collect_identity_provenance_vars`
adds nothing for the advancement, but also does not revoke the earlier edge.
typed_facts then rebases subsequent cursor fields onto the immutable formal.
The three anchored functions all show an initial pointer copy and later
one-byte cursor update. The scalar-width guard alone cannot remove an already
inferred shape while that identity edge survives.

Extend the existing identity dependency owner with a conservative barrier for
any definition that does not preserve an exact pointer value (arithmetic,
load, call, constant or a select with such a branch). Once a binding has that
barrier, its whole-binding identity map cannot forward an earlier alias.
General address/type contributor graphs remain additive; they do not claim
exact object identity. This is an interim conservative lifetime contract,
not a conversion of all type inference to value IDs. Cover both definition
orders, cyclic copies, select barriers and unchanged exact-copy chains.

CandidateP (observable phi transport) and Q (integer-array guard) are both
rejected for delivery. Each kept Union75/250 with no perfect losses, GED
mean33.8833333333 and byte mean.2105126565; compiles133/166 still lost
xcscmp and strcmp_until while gaining discover_class and check_ea_in_inode.
P type mean.2676179533; Q.2678173153. Q changed three ARM NIR outputs
but did not resolve the measured x86 cursor/formal identity leak. Their
measurements motivate the existing identity dependency barrier above;
neither is evidence of completion.

CandidateR still forwards the second xcscmp cursor's fields onto its formal.
Its PreHIR has `rsi = formal`, then `xVar40 = rsi + 1`,
`xVar13 = xVar40`, and `rsi = xVar13`. Both definitions of rsi
are copies, but their source values differ. A whole-binding identity graph
cannot union these definitions and assert exact object identity for both.
The same first-copy/later-cursor-update pattern is already anchored in the
three mixed-optimization functions above. Compare the source set of every
identity-preserving definition; different sets invalidate whole-binding
identity. A transparent self-copy adds no definition evidence. A select
preserves exact identity only when both alternatives have the same sources.
Address contributor graphs remain unchanged. This conservative rule must
be tested in both traversal orders and on cyclic copies before remeasurement.
The existing structured-loop regression demonstrates that syntactically
different sources can be the same value: head -> cursor -> next -> cursor.
Retain copy SCCs with one terminal value source, and reject copy closures
with conflicting terminal sources. Traverse the complete graph after all
definitions are collected; do not depend on statement traversal order.
CandidateR also still has an uninitialized legacy loop binding in xcscmp;
this type refinement alone cannot validate that output or complete #182.

CandidateR reaches Union76/250 without perfect losses, type mean.2718710102
and byte mean.2124294144. GED mean33.8833333333 still violates the baseline
33.675 limit, and xcscmp still loses recompilability. R is not accepted.

## Emitted binding initialization proof

Lowering success is not a definition/read correspondence proof. xcscmp's
loop reads an emitted local before its first definition; the previously
discarded loop candidate has the same class of mismatch. Formal/redefinition
anchors above also require default and zero-iteration carrier initialization.
Before committing a plan, run a must-definition fixed point on the original
CFG's lowered bodies and actual edge copies. Seed only declared formals and
explicitly initialized emitted temporaries. An edge copy If uses the recorded
decision for that edge; other conditionals contribute only definitions present
on both arms. Check every emitted temporary read against the resulting state,
including terminator reads. Unsupported local control shapes decline the
plan. This extends the existing isolated lowering probe at the builder owner,
and keeps its legacy state private. It does not initialize missing values with
zero or bypass interference/dominance checks. Test loop entry/backedge, skipped
definition, edge-only initialization and previous-definition reads. This proof
is conservative and does not claim to repair unsupported legacy output.

## Signedness-sensitive storage extraction

The complete benign strcmp_until source/recovered bodies agree on only
161/256 ASCII/null/delimiter cases for candidateS at both host O0 and O2.
Negative differences become positive bytes: -1 -> 255, -97 -> 159.
The SSA producer already emits an unsigned full-width cast before extracting
the upper piece. The cleanup owner removes an apparently redundant cast
while a binding is unsigned; a later type refinement makes the binding
signed. Logical shift has also been normalized to unsigned division, whose
bare C operands then execute signed truncating division. Measured emitted
signed-divisor extraction forms also occur in bin046, bin107 and bin193;
these observations are expression anchors, not runtime claims for them.
Preserve explicit integer operand views across Div/Mod/Shr/Sar in the
existing redundant-cast cleanup, using the same boundary contract already
used for comparisons. Do not repair printed text or modify the evaluator.
Test cast preservation before and after a binding signedness refinement;
then rerun the actual benign source/recovered function and all score gates.

CandidateU's operand cast preservation alone leaves the executed failure.
The earlier unsigned power-of-two division owner allows an unknown operand
and emits a bare Var with unsigned result metadata. A variable has no type in
the expression contract; C selects division semantics from its eventual
declaration. On logical Shr -> Div conversion, retain an explicit unsigned
storage-width cast for unknown operands. The redundant-cast boundary must
then preserve it across binding refinements. This extends that existing
arithmetic owner; it does not add a late signedness repair or printer rule.
# Follow-up owner evidence: extraction operand views (candidate W)

The diagnostic build of candidate V emitted identical NIR, HIR, PreHIR and
variable metadata to V while recording the existing normalization pass bodies.
The observer was removed after inspection. Its per-pass evidence is recorded in
`benchmark/artifacts/decbench/2026-10-05-ssa-lifetimes/extraction-pass-observations.json`.
The unsigned 64-bit cast feeding division by 256 survives statement cleanup and
SCCP, then first disappears in `subflow_pruning_early`. Later binding refinement
replaces the unsigned producer with a signed carrier. A cast that was redundant
against the earlier type map is therefore necessary to preserve the extraction's
interpretation. V still matches only 161/256 complete benign function executions
at both host O0 and O2, despite passing the aggregate numeric gates.

Extend the existing subflow owner, before remeasuring W: preserve an explicit
integer operand view at division, remainder and shift boundaries. Standalone
identity casts can still be removed. Constant folding and width/signedness-safe
double-cast folding remain available. This invariant does not depend on a
function, address, ISA or score: later carrier type refinement must not change
an already explicit integer operation's signedness or width. Focused coverage
must exercise unsigned and signed operands, type refinement, and ordinary cast
elimination. Complete recovered-function execution and every original gate
remain required before accepting W.

W resolves the complete benign function's 256/256 host O0 and O2 executions,
retains every previously compiling scored row (133/166), and retains GED
perfect65/240. Its byte mean .2117157300 is below baseline .2118900452,
so W is rejected. External dev6/fixed32 retain4/8 bare-compiles; the unscored
pool includes one structural distance increase (0 ->13), recorded only as
regression evidence and not used to choose an implementation condition.

Next invariant at the existing SSA piece-emission owner: an extracted byte
window needs only its highest requested bit, not every bit of its producer.
For an unsigned right shift followed by a piece-width truncation, first
view the source through the smallest supported unsigned integer containing
the requested window. Discarding higher bits cannot affect that window.
For example, bits8..31 of a64-bit producer are exactly bits8..31 of its
unsigned32-bit view; bits32..63 still require64 bits. Endianness determines
the existing window shift, and the same rule then applies. Keep original-site
materialization and the existing final piece mask. Test all physical windows
against direct bit extraction, including all-ones and high-bit sentinels,
before fresh execution and all score/regression gates. This is a producer
width/window identity rule; no score, address, function or pool condition is
part of production admission.

X retains the256/256 and8/8 executed cases, but its byte mean remains
.2117157300, so it is not accepted. The existing complete-producer read path
requires an exactly equal Varnode width. A narrower read of the same proven
producer falls through to splitting/recombining its pieces even when every
piece still has that one operation identity. Extend that existing path to
contained storage views: prove the producer covers the requested bytes,
retain its original-site snapshot, and extract the requested unsigned view
with the existing endian-aware shift and physical-width mask. Do not merge
phi lifetimes or follow ambiguous copy aliases. Same-producer equality is
still required for every piece. Test narrower lower and upper views against
the snapshot, including unrelated partial-write rejection. This removes a
redundant reconstruction of an already materialized value without changing
its evaluations or edge transport.

Final scope review: withdraw the prototype's general unknown-operand
Shr-to-Div cast insertion. An expression result type alone is not evidence
of an untyped operand's storage width after later binding refinement. The
verified SSA extraction paths already supply explicit views from actual
storage pieces, and preserving those views at subflow/cleanup boundaries
resolves the executed defect. Restore the arithmetic owner and its prior
expectation unchanged; do not broaden this change into a guess about legacy
unknown operands. Candidate Z must rerun all gates and keep256/256 execution
before delivery. Any future unsigned division repair needs independent input
width evidence at its owner, not a result-type-based assumption.
