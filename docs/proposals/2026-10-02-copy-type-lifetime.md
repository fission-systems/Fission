# Decompiler Change Proposal: COPY type lifetime

## 1. Measured baseline anchors

Base main `8945427766d51dcdfd58ecebc53dca3e91c7705d`; production-identical
immutable baseline CLI SHA256
`00549ea9563b2ee4b67dcda30db6545e3febd7f715fd6b50abbd289d15443b43`.
Accepted fresh standalone DecBench baseline: Union 73/250, GED 65/240,
types 16/228, byte 1/158, compiles 124/158. Local results only.

| Program / optimization / function | Binary / address | GED | Types | Byte |
| --- | --- | ---: | ---: | ---: |
| iproute2 rtmon / O0 / main | bin_050.elf / 0x6de7 | 17 | .5 | .181189 |
| openssh ssh-add / O2-noinline / revoked_certs_generate | bin_187.elf / 0x3dd20 | 40 | 1/9 | .254174 |
| mirai / O2 / ensure_single_instance | bin_013.elf / 0x8e30 | 11 | 0 | .237805 |

These are static decompilations; no benchmark binaries are executed and no
executable semantic oracle/case count is available. Pass-level diagnostic
snapshots capture before/after binding types and actual definitions. The
instrumentation is temporary and removed from production.

The main anchor's two 32-bit flag locals become Ptr(Unknown) after copying an
entry scalar from a carrier later reused for a pointer-returning call. Each flag
also has a typed integer-one definition. The second anchor's 32-bit status local
has ten definitions, including the scalar error value 4294967294, but acquires
Ptr(Unknown) through a carrier with 43 definitions. The third anchor's 16-bit
family field is explicitly assigned integer two and copied from a reused carrier,
yet is promoted to a pointer. The shape repeats across three programs at O0,
O2 and O2-noinline. Broader diagnostic scan is investigation, not a score claim.

## 2. Owner proof

First observed type corruption: existing use-driven type inference, specifically
`collect_assignment_copy_constraints`' forward pointer constraint for plain
`dst = src`, and operation-edge `TypeFlowSolver`'s forward COPY transfer.

```text
entry: carrier = scalar; flag = carrier; ... flag = Int32(1);
later: carrier = pointer-returning call;
use-driven pass: flag Int32 -> Ptr(Unknown)

status = scalar-return carrier; ... status = Int32(-2);
carrier is reused for unrelated pointer/scalar results;
use-driven pass: status Int32 -> Ptr(Unknown)
```

The legacy collector comments already assign pointer COPY equality to TypeFlow,
but its forward branch still appends a Ptr constraint from the global source
binding. TypeFlow guards the destination against multiple/self definitions for
forward transfer, and guards both endpoints only for reverse transfer. Its
source fact can therefore describe a later value instead of this copy's input.
Ghidra's TypeOpCopy/ActionInferTypes propagate between operation Varnodes (value
identities), not the lifetime-wide hardware/storage name. No reference code is
copied or linked.

## 3. Invariant and shared ownership

A plain COPY alone cannot establish equality of types for names representing
several different values. Bidirectional binding-wide COPY transfer requires
single, non-self-referential definition evidence for both endpoints. Explicit
casts and memory access constraints remain operation-local evidence. Extend
existing TypeFlow's definition/self-reference proof and make the legacy collector
delegate pointer COPY propagation to that owner, as its existing contract says.
No new pass/helper/dependency, ISA/register/compiler/row guard or telemetry.

This preserves scalar flags/status/fields from unrelated later pointer results;
that justification does not refer to the metric. A full versioned reaching-value
solver is outside this narrow correction. Conservative skipped copies may leave
unknown types or fewer inferred pointers; explicit access and cast facts still
operate. Do not suppress correct values or tune to the fixed regression pool.

## 4. Coverage and risks

Synthetic arbitrary-name invariants: source redefinition, destination branch
redefinition, self-referential source, stable single-definition pointer alias,
explicit pointer cast, locked pointer input. Existing alias-chain/pointer-chase
and backward-refinement cases must continue passing. Snapshot order/output
must remain deterministic. Scalar legacy constraints are unchanged.

## 5. Validation matrix

1. Targeted TypeFlow/use-driven tests, full normalize+pcode nextest, cargo checks
   normalize/pcode/decompiler, workspace cargo fmt, native release CLI.
2. Fresh standalone DecBench output for all 250 identities, no DB/cache/resume;
   native-variable metadata through existing standalone adapter; fresh GED,
   type matching and Linux GNU recompilation/byte matching. Compare perfect row
   sets and partial scores, inspect all changed output units.
3. Mandatory external local Docker using the locally built Linux CLI: same frozen
   unscored 32-row pool (regression-only), dev smoke and static Linux anchors.
   Existing pool lacks behavior oracle and has a known 7900-character truncation
   heuristic; disclose its effects, not a blanket semantic pass.
4. No official latest/Pages publication. Rank comparison uses a frozen rival
   snapshot and is an explicitly local projection.

## 6. AI firewall and acceptance

No external/cross-model proposal advice. Pass inspection is local; implementation
is generic value-lifetime evidence. Real row identities occur only in this
proposal/evidence, never production guards. Synthetic invariants provide an
independent signal; unscored validation pool is go/stop evidence only.
Actual score improvement remains unproven until fresh remeasurement. Do not
claim completion of the Kuna target unless Union exceeds 88/250.

## 7. First candidate measurement and operation-local follow-up

The first candidate measured Union 73/250 (no perfect gains/losses), GED 65/240,
types 16/228, byte 1/158. Type mean .26007486 -> .26184993; byte mean
.20499280 -> .20633632; compiles 124 -> 125/158. Six partial type gains and two
partial losses require disclosure. The mirai intermediate corruption is observed,
but its final output does not change; it is not a measured final repair.
External paired dev: six identical existing compile errors. Frozen unscored
32-row pool: seven changed outputs, identical status sets and four existing
adapter errors. These are limited regression checks, not semantic correctness.

Before the follow-up implementation, an operation-level diagnostic scan found
this repeated loss of typed values:

| Program / optimization / function | Anchor | Saved value / current final type | GED / type / byte baseline |
| --- | --- | --- | --- |
| iproute2 / O0 / rtmon main | bin_050.elf / 0x6de7 | fopen64 Ptr(Unknown) -> local_48 / long long | 17 / .8 / .181189 |
| tar / O2 / print_stats | bin_045.elf / 0xbf60 | dcgettext Ptr(Int8) -> xVar75 / unsigned long long | 29 / .2 / .213793 |
| findutils / O2 / process_all_startpoints | bin_027.elf / 0x8070 | dcgettext Ptr(Int8) -> xVar329 / unsigned long long | 212 / .133333 / .257778 |

Each destination has exactly one definition; each call-result carrier is reused
elsewhere. Each COPY is immediately after the pointer-typed call assignment in
the same statement list, with no intervening statement or label. The final
saved binding remains scalar despite the pointer type already present on the
operation. TypeFlow currently sees only global source-name facts and rejects
such copies with the necessary multiple-definition guard. Callsite typing owns
call contracts already, but its transitive copy helper also requires stable
names and cannot express this operation-local relation. Extend existing
TypeFlow COPY-edge collection; do not add a callsite/printer repair or pass.

Invariant: a COPY immediately following assignment of a semantically typed
pointer call result receives that operation's pointer type, independent of
other values represented by the carrier name. Apply only to a local with one
non-self-referential definition, no declaration initializer and no address
escape; exclude entry parameters. Preserve locked types and width compatibility
via the existing solver. The saved pointer then has the type of its actual
value, rather than the type of an unrelated carrier lifetime. No metric is
needed to justify this correction.

A new operation-local COPY edge is needed because a name-wide COPY cannot
represent this evidence, while the existing unrestricted CAST edge would miss
the destination lifetime proof. Restrict evidence to already pointer-typed Call
expressions: integer call types can be storage placeholders. No lookahead across
labels, branches, blocks, loops, intervening writes/calls or joins. Inner
statement lists are considered independently. Copy chains without a local
operation proof remain conservative; full reaching-definition SSA is deferred.

Focused coverage: pointer result followed by scalar carrier overwrite; earlier
scalar value followed by pointer call; label/branch/unknown/intervening-call
barriers; multiple destination definitions; self-copy; parameter, initialized,
address-taken, wrong-width and locked destinations. Full crate and all-250
fresh scores plus external paired regression checks must be repeated for this
combined candidate. Frozen unscored pool remains go/stop only, never tuning.

## 8. Follow-up owner correction before implementation

Combined candidate `7eb262d2d44542243a206bf8f3d9a7cf0cc400ead2ba48a9782611d9a18cd4d0`
(native CLI `87bfdb94ded048fc78aa21a4c4fc4cbf0bb01aab283849ac14dd15e7c5e66715`)
regenerated 250/250 functions. Actual rtmon NIR still declares the saved FILE
value as unsigned long long, while HIR declares void*. A both-layer pass-level
snapshot proves one unexposed, uninitialized local definition immediately after
the typed fopen64 call in both modes. Width is compatible and the carrier is
reused. So the adjacency proof is present; loss is downstream in the same owner.

`restore_scalar_only_pointer_locals` correctly tries to undo guessed pointers,
but `collect_binding_use_roles_stmt` treats every plain COPY as a non-pointer
value definition because `expr_type(Var)` is Unknown. A null check (`!saved`),
or a scalar legacy alias constraint, then qualifies this proven pointer value
for restoration to a machine-width integer. The same already measured typed
pointer-result COPY shape repeats in the three anchors in section 7; the final
scalar type is not authoritative evidence that its COPY produced a scalar.

The shared operation-local definition proof must also inform existing use-role
classification. Expose the existing TypeFlow COPY-value proof as a transient
set of eligible local names; do not reconstruct a second adjacency/lifetime
policy, add a pass, or change rendering/oracle. Use only names whose destination
width is compatible and whose binding is unlocked. Because every admitted local
has exactly one non-self definition, classify its definition as pointer-valued,
so null checks and global carrier scalar constraints cannot erase it. Scalar
restoration stays active for names without this value proof. Add use-driven
coverage for null checks on saved pointers and multiple-definition negatives.
Repeat complete fresh metrics and external gates after this owner correction.

Final review also found the legacy pointer-base fallback still accepts a plain
COPY when its source binding is Unknown. Being Unknown is not a lifetime proof;
a multiply defined unknown carrier is just as unsafe to refine backward. Close
that remaining bypass for plain Var RHS and let TypeFlow supply both directions
of pointer equality. Pointer arithmetic/casts/address expressions retain their
existing evidence path. This completes the COPY ownership invariant in section
3, with an arbitrary-name multi-definition unknown-source regression test;
no additional quality claim is based on that synthetic case.

Focused type correspondence after section 8 confirms the rtmon anchor remains
.8: the FILE value is now void* (concrete API surface is not yet propagated),
and a separate stack aggregate is unmatched. Null-check correctness is repaired
but this row is not type-perfect. Do not infer perfect status from declarations
or from the intermediate use-driven pass. Full scores decide acceptance.

## 9. Acceptance evidence and limits

Final production patch SHA256:
`a28db1a7c8e824154b1e5dd5d56f61e1721491ef5690e5b49525b9e17bcb3641`.
Native CLI SHA256:
`ac5da815906c7bf64852480bc38c91cd9fc6e6a6c8e48bb0aa2742128f86b669`.
Linux local bundle fingerprint:
`1d83729301dc83a5edf6e0a9c0fb3a6813bedbaa96f0d0bd55517e5bdc1b2ad2`.
The final fresh generation covered 223 binaries / 250 functions, with no errors.
All 250 NIR + native-variable metric input payloads are identical to the fully
remeasured section-8 candidate; an additional complete cache-disabled metric
run is retained in the final evidence. No official result was published.

| Metric | Accepted baseline | Candidate |
| --- | ---: | ---: |
| Union | 73/250 | 73/250 |
| GED perfect | 65/240 | 65/240 |
| Type perfect | 16/228 | 16/228 |
| Byte perfect | 1/158 | 1/158 |
| Compiles | 124/158 | 124/158 |
| Mean type score | .260074861 | .261849930 |
| Mean byte score | .204992801 | .204733461 |

Six partial type gains and two losses; no perfect gains/losses. The two partial
losses are copy_reg (.147059 -> .117647) and describe_change (.545455 -> .454545).
Removing lifetime-unsafe propagation can lose a coincidentally correct global
binding type; the admitted local typed-value rule does not yet reconstruct all
reaching values. Explicit tradeoff: modest type mean gain and restored pointer
null-check typing, with a small byte mean decrease and no Union improvement.
Do not call this a ranking win or a whole-program semantic-equivalence result.
Kuna remains 88/250 in the frozen rival snapshot, so 16 additional perfect
functions are still needed to exceed it. Local projected rank remains 8.

Focused tests: 12 passed. Full normalize+pcode: 1623 passed, one existing skip.
Normalize/pcode/decompiler checks, workspace fmt/diff checks and native/Linux
release builds passed. The initial earlier-candidate full suite reported one
leaky process; a detailed rerun and subsequent full runs did not reproduce it.

Final external candidate versus fresh immutable baseline: dev six unchanged
wrapper compile errors (bare compile 4/6 on both); fixed unscored pool 32 rows,
six changed outputs, unchanged four adapter errors and 28 no-wrapper statuses
(bare compile 8/32 on both). Zero checkpoint rows recovered, caches disabled.
Pool hash `ab3dbf2aba55f39bf0b436d1c531d2126ee4749d4e494626985de5f9a7f3b725`,
eight programs, zero scored-sample overlap, all O0. These limited regression
checks supply no executable semantic pass. Pool results were not tuning input.
The baseline bundle keeps its original 7080acbf6-dirty build label; its pinned
production patch is the accepted PR #179 code now in main, not a clean build of
that old HEAD. Final candidate keeps its own 894542776-dirty build label.

Evidence is local under `benchmark/artifacts/decbench/2026-10-02-copy-type-lifetime/`
(in the primary checkout): all fresh scores/overlays, input identity proof,
compressed pass-level snapshots, proposal, source patch, test/build logs,
external before/after checks, native package, static Linux anchor diffs and
GitHub delivery status. Fission-only repository submission; no DecBench public
submission, Pages/latest promotion, runtime vendor dependency, or oracle swap.
