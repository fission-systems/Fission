# Pointer-use evidence for immutable entry inputs

Recorded before production edits; main76b877141, accepted Union74/250.

## 1. Measured baseline

Fresh native all250/223 stripped binaries match accepted NIR/HIR/native variable
metadata exactly (baseline-parity.json); no decomp cache/database. Baseline metric
inputs therefore match the accepted uncached overlays. Artifacts in primary
benchmark/artifacts/decbench/2026-10-03-pointer-cast-input.

| Program / optimization | Function / kit / address | Type / GED / byte | NIR lines |
| --- | --- | --- | --- |
| grep / O0 | print_line_tail / bin_185.elf / 0x7f87 | .8 / recorded in overlays / recorded in overlays | 82 |
| coreutils / O2-noinline | copy_reg / bin_039.elf / 0x8680 | .117647058824 / 91 / .147478591817 | 1009 |
| openssh-portable / O2-noinline | argv_split / bin_193.elf / 0x39a80 | .181818181818 / recorded in overlays / recorded in overlays | 239 |

Three functions, three programs, mixed optimization. No whole-source executable
cases are available; semantic status is unmeasured, not pass. Printed lines and
exact score tuples will be retained in baseline-anchors.json before implementation.
Native variable diagnostics identify an immutable word input used through an
explicit pointer cast but still declared integer. Examples:

- grep: local_18 = (uchar *)(param_1); input is unchanged; resulting cursor is
  used as pointer/comparison/address and returned. This row lacks one type.
- cp: rax = (char *)(param_8); input remains unchanged, also copied to local_1e8.
  Source-side evaluator says bool*, so byte-pointer inference may remain imperfect.
- ssh: r15 = (unsigned long long *)(param_3); *r15 = 0; input also passed to
  another helper. Underlying source type is char***; no exact recovery is assumed.

The cp/shred descriptor counterexample has uint32 input cast to a pointer64;
width mismatch forbids promoting it. Reused ABI slots from #182 remain excluded.
Source/DWARF is evaluator-only, pinned HF revision7af6c5e19289b6a357404708e5373bc7bf6fcacd.

## 2. Owner proof

[x] Type/data recovery: existing TypeFlow operation-edge solver.

collect_assignment_edges records Cast{output,ty} but discards the direct source
of the cast. It refines the destination only. Generic COPY equality cannot cross
this expression; legacy use inference intentionally declines known integer
source bindings. Thus explicit pointer value evidence never reaches an unchanged
input, despite full-width transport. Printer and builder are not the repair.

Extend the existing edge representation with source-local pointer evidence;
do not introduce a pass, rename variables, split lifetimes, initialize carriers
or rewrite statements. The candidate is limited to untouched entry bindings.

## 3. Invariant and independent justification

An unlocked, unexposed input with no body definition, no scalar-use evidence,
no semantic integer metatype and only a pointer-width storage guess may be
refined by a direct same-width pointer cast of that exact input. The cast's
explicit type belongs to that use, not to later definitions of its destination.
Conflicting pointee casts must decline input refinement; pointer facts must
converge deterministically. No propagation through nested integer conversion,
width change, binary expression or arbitrary reused local.

Why better: the declaration describes the unchanged address value transported
into an explicitly typed pointer, preserving representation and evaluation.
No missing phi assignments can be created because assignments/control flow
are unchanged. Exact source spelling is not inferable from every pointer use;
measure actual type/effect results instead of assuming a ranking gain.

[x] No ISA/compiler/name/address guards; ABI pointer width from existing function
model; no vendor dependencies/copying. Reuse existing use-role extraction for
scalar counterevidence rather than a duplicate AST role classifier.

## 4. Ownership/risk

Existing owner: type_flow.rs. Existing type-use-role collector in use_type_infer.rs
owns scalar vs address usage; expose that fact locally if needed. No new program
metadata maps, public telemetry, dependency or cross-layer edge. Raw integer
addresses can intentionally become pointers, so locked/semantic integers and
scalar roles must remain untouched. Multi-def/loop/entry-carrier reuse excluded.
No new pass; extend the existing fixed-point solver and its invariant tests.

## 5. Validation

Tests: untouched input, mutable destination, written input including one write,
scalar uses, locked/semantic input, escaped address, width mismatch, conflicting
casts, pointer32/pointer64, stable convergence and nested statement lists.
Targeted nextest, full pcode/normalize nextest, checks, fmt-all/check, scoped
Clippy and boundary audit; native/Linux release builds.
Fresh all250 outputs + unchanged adapter + uncached standalone GED/type/byte
with paired GNU GCC14.2; inspect changed code and perfect row gains/losses.
Mandatory paired external local Docker dev6 + fixed unscored32, no caches/resume;
existing compile/no-wrapper/adapter failures are explicit limitations, not passes.
Anchored output slice checks high pointer bits, effect count and return/address
representation; no original malware execution. Synthetic test supplements focused
rows; pool is regression-only and never used to tune or rank the candidate.

## 6. AI firewall

No external advice or subagents. Row observation generalized to storage/type and
immutable-input facts at the existing owner. Vendor TypeOp/propagateType inspection
is conceptual reference only; explicit conversion boundaries require the extra
admission proof above and are not universal COPY equalities.

## 7. Phase 1 rejection and corrected owner proof (before phase 2)

Phase 1's explicit Cast source edge passed four invariant tests and 1,628
crate tests, but fresh all250 NIR/HIR/variable metadata changed zero rows.
The initial printed-cast interpretation above was wrong: the printer inserted
that cast; the actual owner sees COPY. Phase 1 will be removed, not shipped.

Diagnostic PreHIR traces show the value-specific relation:

- grep O0 print_line_tail / bin_185.elf / 0x7f87: local_18=param_1;
  local_20=param_2; rax=local_20; unsigned local_18<rax. param_2 is uchar* at
  that operation. local_18 is later updated and rax reused as integer, so the
  binding-wide COPY equality correctly declines propagation. Baseline82lines,
  Type.8, GED6, byte.16292134831460675; behavior unmeasured.
- grep O2-noinline print_line_tail / bin_208.elf / 0x88b0: unsigned
  param_2<=param_1 with param_2 uchar* at that operation; subsequent byte loads
  use param_2-1. Baseline63lines and param_1 rendered long long. This is an
  unscored function observed on the stripped full binary; no rank contribution
  is assigned. Symbol addresses from original ELF are evaluator selection only.
- openssh-portable O2-noinline argv_split / bin_193.elf / 0x39a80:
  r15=param_3; later *r15=0 before r15=0 and scalar reuse. Baseline239lines,
  Type.181818181818, GED56, byte.222482435597; behavior unmeasured.

These are three real functions across two programs and mixed optimization.
Unscored baseline outputs are in /private/tmp/fission-pointer-cursor-observe;
scored anchors are retained in baseline-anchors.json. copy_reg is a negative
anchor: param_8's rax copy is overwritten without any address use, so its later
char* binding declaration is not evidence about param_8. inflate_table is a
negative mutable input anchor; original input body definitions exclude it.

Phase 2 extends the same TypeFlow owner with operation-local entry-use evidence.
Track exact COPY provenance and operation types in straight-line regions, seeded
only by entry types and actual expressions, never by reused local declarations.
An unsigned ordered comparison to a known pointer or typed memory address use
can constrain the unchanged full-width entry origin. Redefinition kills COPY
provenance; labels/gotos/joins/loops discard uncertain local facts. Explicit
integer conversion, scalar-only operators, escaping entry addresses, semantic
integer metatypes, narrow widths, locked types and conflicting pointee evidence
exclude refinement. Do not propagate LOAD values as address origins, and do not
infer nested conversions or pointer arithmetic origins in this bounded change.

Justification independent of score: input declarations describe address values
actually used before destination reuse, while dead copies and unrelated later
pointer values cannot contaminate them. Statements, effects, carrier assignments
and widths remain unchanged. This does not repair #182's missing out-of-SSA
assignments and does not infer unknown struct names or assume exact source types.

A private collector may live adjacent to type_flow.rs because the existing
unordered edge list has discarded value-lifetime ordering; the existing solver
consumes its facts. This is no new pass, no program metadata owner, no telemetry
channel and no printer repair. Tests must cover fresh/copy pointer comparisons,
address use before/after redefinition, branch/label/loop barriers, load-result
separation, casts/locks/metatypes/escape/width/conflicts, and 32/64-bit convergence.
Validation and acceptance remain the full matrix in section5, with all250 fresh
scores and mandatory paired external Docker. No quality claim before measurement.

## 8. Accepted phase 2 measurements

Fresh immutable CLI45ace13148efd7453d4d05d5ae2a617a8291f6a6a3c124fe857ab44a05dfab2f,
main76b877141 plus crates patch09e32aa1618b50b61e4aa4fd89af8776bcb904bddb0906d6f965729e42a59634.
Standalone DecBench unchanged adapter/matcher, HF revision pinned as above,
DECBENCH_NO_CACHE=1; exact compiled inputs linked and verified; GNU GCC14.2 +
paired binutils, published source CFG hashes verified. Production uses stripped
bytes + public requested addresses only. Source/DWARF remains evaluator-only.

| Lane | Accepted baseline | Candidate |
| --- | --- | --- |
| Union all250 | 74/250 (29.6%) | 75/250 (30.0%) |
| Type perfect / measured | 17/228 | 18/228 |
| Type mean | .2641122882797613 | .26498948126221744 |
| GED perfect / measured | 65/240 | 65/240 |
| GED mean | 33.62916666666667 | unchanged |
| Byte perfect / measured | 1/166 | 1/166 |
| Byte mean | .20356018608863982 | unchanged |
| Recompiles | 132/166 | 132/166 |
| Fixed public intersection91 Union | 22/91 | 23/91 |
| Fixed91 Type perfect / measured | 7/85 | 8/85 |
| Fixed91 recompiles | 66/91 | 66/91 |

Only two of250 NIR/HIR/metadata outputs changed: grep's first input from word
integer to uchar*, ssh's third from word integer to word pointer. Assignments,
control flow and effects are unchanged; redundant rendered COPY casts disappear.
The only score delta is grep print_line_tail Type.8→1.0. Exactly one new Union
function, zero lost. ssh's exact char*** spelling remains unrecovered; its score
stays unchanged. The unscored optimized grep observation remains unchanged in
final output, so this change does not claim to repair that full pipeline case.
copy_reg dead-copy and ssh_rsa_verify carrier counterexamples remain unchanged.

5 focused invariant tests and 1,629 relevant normalize/pcode tests passed,
1 existing skip; pcode/decompiler checks, scoped Clippy -D warnings, fmt-all/check
and boundary audit0 findings passed. Native and Linux release builds succeeded.
Four Linux anchors matched native NIR/HIR exactly, including both unchanged
counterexamples. Observed COPY/store slices on native and Linux64 retain real
allocated addresses above32bits and the exact word store; these are bounded
slice checks, not full-function semantic parity. Full-function behavior remains
unmeasured for the scored anchors.

Mandatory external paired Docker dev6 + fixed unscored32: zero output, status,
bare-compile or checkpoint-recovery deltas. Dev keeps six existing wrapper
compile failures (bare-compile4/6); pool keeps28 no-wrapper and4 adapter failures
(bare-compile8/32). No full-function semantic passes are asserted from that lane.
Pool is regression-only, never used to tune the candidate or contribute rank.
Candidate Linux bundle fingerprintc40d31ab8d35b317d872b01414ba748d25581bf541cfcb957818a59aca42a403.

Local rank projection only against frozen public2026-09-23 rival scores:
all250 remains8th; fixed91 is tied5th (23/91). Official DecBench is not submitted
or updated. Kuna's88/250 still requires14 additional new perfect functions to
surpass, assuming no loss. AI targets on fixed91 remain48/91 for Codex47/91.
#182 stays open: no ABI entry/carrier splitting or out-of-SSA assignment repair.

Evidence retained under benchmark/artifacts/decbench/2026-10-03-pointer-cast-input:
copy-validation-summary.json, copy-all-overlays.json, copy-intersection91.json,
copy-output-deltas.json, copy-run.json, copy-candidate.patch, focused/full logs,
copy-external-comparison.json, Linux anchors and observed slice sources/results.
Only phase2 Rust code and this proposal are delivery changes; phase1 Cast-source
prototype and diagnostic instrumentation were removed before measurement.
