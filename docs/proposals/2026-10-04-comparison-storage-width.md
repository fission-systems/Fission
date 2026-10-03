# Comparison reads preserve storage width and signedness

## 1. Measured baseline anchors

Accepted main a2ef7011394abb4816df0ac2e943b0431dcc603e, immutable native CLI
45ace13148efd7453d4d05d5ae2a617a8291f6a6a3c124fe857ab44a05dfab2f.
Evidence: benchmark/artifacts/decbench/2026-10-04-value-lifetime in the primary
checkout. Scores use the unchanged standalone DecBench evaluator and adapter.

| Project / configuration / function | Kit / address | GED | Type | Byte | Defect |
| --- | --- | --- | --- | --- | --- |
| gzip / O2-noinline / fill_inbuf | bin_015.elf / 0xd830 | 4 | .5 | .2238806 | 32-bit -1 comparison reads a 64-bit carrier |
| bzip2 / O2-noinline / bsPutBit | bin_164.elf / 0x1b10 | 0 | .3333333 | 0, not compilable | putc result compared to -1 as a 64-bit carrier |
| sysvinit / O0 / check_init_fifo | bin_203.elf / 0x90ae | 37 | 0 | 0, not compilable | signed 32-bit stat/open tests read a pointer carrier |

These are three functions from three programs and mixed optimization levels.
Baseline whole sample: Union 75/250; fixed intersection 23/91. Semantic cases
are not available for these full functions; no full-function parity claim.
Additional defects (lost accumulator updates, pointer-carrier lifetime, unknown
prototypes and missing output assignments) remain outside this scoped change.

## 2. Owner proof

- [x] Builder expression/comparison lowering and COPY read conversions.
- [x] Existing normalize assignment/comparison cast cleanup.
- [ ] Raw SLEIGH, structuring, printer, evaluator.

Raw p-code preserves operand sizes. gzip cmp at 0xd858 stages a size-4 register,
then IntSub against size-4 -1. Current NIR emits `if (rax == -1)` with
`long long rax`. bzip2 has the same size-4 subtraction at 0x1b4c after putc;
NIR compares long long rax. sysvinit has size-4 IntSLess against zero, but NIR
emits `if (rax < 0)` with uint* rax. lower_compare_operands and flag tested
value recovery lower varnodes into bare bindings and discard that read width.
The existing unsigned-operand coercion deliberately skips all variables.

## 3. Invariant and independent justification

A p-code comparison observes its operand's byte width, not every bit of the
binding that currently carries those bytes. Its signed/unsigned opcode sets
the interpretation of the observed bit pattern. Preserve a narrow read with
an explicit use-site integer cast when a known binding is wider or a pointer;
preserve signed order with a signed view. Do not narrow or split the binding.
This prevents a zero-extended 32-bit error value from missing an error branch,
and prevents signed tests from becoming pointer or unsigned comparisons.

- [x] Rule is storage size + operand signedness, with no ISA/name/address guards.
- [x] ABI differences remain in the register namer and SLEIGH.
- [x] Synthetic tests cover high bits, signed/unsigned order, and equal-width
  and full-width pointer/carrier reads that must not be narrowed.

Reference: Ghidra RuleZextSless / RuleSubZext document preservation of the
original bit-vector domain around comparisons; reference only, no copied code
or runtime dependency.

## 4. Scope and risk

Extend existing builder comparison coercion, ordinary integer comparison
lowering and recovered flag predicates. No new pass or dependencies/telemetry.
Known bindings use the builder's existing params/temps/locals type facts.
Unknown bindings are not guessed. Equality constants and negative sentinels
must retain the same low-width bit pattern. Cast removal/type inference may
interact with these views: inspect final NIR/HIR and all affected outputs.
Full-width pointer reads, API inputs, stores and returns must retain high bits.
The previously rejected entry/carrier split is not reintroduced.

## 5. Validation matrix (before implementation)

1. Add targeted owner-native tests for 64-bit carriers read through 32-bit
   equality, signed and unsigned order; full-width read and equal-width cases.
2. Targeted nextest, full fission-pcode and relevant normalize/core suites;
   cargo check pcode/decompiler; workspace fmt; boundary audit.
3. Build a new immutable native CLI; regenerate all 250 functions uncached;
   inspect anchored comparisons and changed final outputs.
4. Recompute GED/type/byte with unchanged evaluator and metadata adapter;
   preserve all 75 existing Union perfect rows and recompilation coverage.
5. Build Linux, compare the same anchors, exercise bounded comparator slices
   including 0xffffffff, high-bit payloads and negative order boundaries.
6. Mandatory paired external fission-benchmark local Docker dev6 + the fixed
   unscored32 regression pool, no stale decompilation/resume cache. Existing
   wrapper/adapter failures remain limitations, not semantic passes.
7. Publish local estimates only. No official ranking/Pages/release-tag claims.

## 6. AI firewall

No other model or external implementation-advice prompt. Source/debug records
are local investigation only. Production conditions contain no corpus identity.
Synthetic invariant tests supplement real rows; unscored pool is go/stop only.

## 7. Acceptance

Independent semantic benefit plus measured real output and full sample deltas.
If score or regression gates fail, record and reject the candidate rather than
claim a score improvement from casts or unit-test green.

### Phase 1 observation / phase 2 owner refinement

The initial uncached sweep emits a correct narrow `putc` comparison but gzip's
comparison is lowered while its binding still has the low-lane type; later
carrier reuse widens that same name. Thus current type alone cannot discharge
the view. Extend the same helper to retain a narrow register read when the
expression names the full carrier identified by the existing register model.
No mnemonic, ISA enum, explicit hardware name, or corpus identity in this rule.
Equal-width independent bindings and immutable input parameters stay unchanged.

The flag producer can stage the low register through a same-width COPY unique
varnode. Preserve the same storage-view rule through existing definition facts,
peeling only same-width COPY chains (bounded six links), never extensions or
truncations. This refines the existing owner proof, not a new pattern pass.

### Normalize ownership refinement before extending that owner

cleanup/casts.rs removes a cast whenever its variable type currently equals
the cast target. It already preserves unsigned-order operand views because
subsequent alias/type cleanup can change the binding. The identical contract
applies to equality and signed comparisons: a low-lane view cannot be erased
using an intermediate declaration and then replaced with a wider carrier.
Extend that existing comparison-boundary admission to all integer comparison
opcodes, with a targeted cast-preservation test. No new pass or printer repair.

Phase 1 all250: Union75 unchanged, no losses; GED mean33.6833333 (baseline
33.6291667), type mean.2654683 (baseline .2649895), byte mean.2108517 (baseline
.2035602), recompiles132/166 unchanged. This is not a rank gain. Existing
partial rows and carrier/accumulator defects limit full semantic claims.

### Exact builder probe and COPY conversion (before this extension)

The local raw-pcode builder probe establishes the exact route: gzip's builder
produces `uint uVar5; long long rax; uVar5 = rax; if (uVar5 == -1) ...`.
The comparison itself uses a genuinely narrow variable. Copy elimination then
replaces it with the wide carrier, discarding the assignment conversion.
Preserve the raw COPY input storage view using the same owner helper. In the
existing assignment-cast elision owner, resolve variable types from the supplied
binding table before removing a cast: treating every variable as Unknown makes
`(uint)wide` disappear despite its known narrowing conversion. This applies to
all typed COPY values, not comparison-shaped names. No new pass.
The temporary probe is removed; its raw AST log is retained as local evidence.

## 8. Final measured outcome and limits

Final CLI SHA256:
`abe35ec79f354b6f86a7cc1a0f600107609cc6c0787a42c2f763157632946c3e`.
Base main `a2ef7011394abb4816df0ac2e943b0431dcc603e`; production/test patch
SHA256 `eec63649f146062397d0c9539d177d0416f83a4a19b47c5003f5caad4c96ecbc`.
Two independent uncached 250-function generations have identical addresses,
NIR, HIR and variable metadata. No evaluator or adapter changes.

| Fresh local metric | Accepted PR185 | Candidate |
| --- | --- | --- |
| Union | 75/250 | 75/250; zero gained/lost |
| GED perfect / mean distance | 65/240 / 33.6291667 | 65/240 / 33.8041667 |
| Type perfect / mean score | 18/228 / .2649895 | 18/228 / .2654683 |
| Byte perfect / mean score | 1/166 / .2035602 | 1/166 / .2119100 |
| Recompiles | 132/166 | 133/166; zero losses |
| Fixed published91 Union | 23/91 | 23/91 |
| Fixed91 recompiles | 66/91 | 67/91 |

The compile gain is bzip2/O2/fallbackSort. The scoped correctness gain is
preserving low-width error/signed tests while leaving full-width carrier reads
intact. Native and Linux comparator slices extracted from the three measured
anchors exercise 30 inputs: baseline mismatch9, candidate mismatch0; full-width
carrier mismatch0. These are bounded expression checks, not full-function
semantic parity. Six Linux/native anchor outputs agree exactly.

The GED mean worsens by .175. Eleven individual GED rows change, including
the previously type-perfect grep/O0 row whose GED changes 6→14. Existing
perfect GED/Union rows remain perfect. This is an explicit tradeoff for the
observed storage semantics and byte/compilability gains, **not a structure or
ranking improvement**. The full-sample local projection remains rank8, fixed91
tied rank5 against the frozen 2026-09-23 rivals. Kuna still needs fourteen more
Union-perfect rows to exceed 88/250. No official ranking claim.

Call-site audit limits: a `running (` string is not a call. In coreutils/touch,
one sub_40c0 call was already behind an unconditional `if (1) goto`; that dead
text disappears. More seriously, the existing incorrect `rax == (uint)rax`
comparison is folded after both operands have the correct low-width view,
removing a textual sub_4110 loop. Assembly compares r12d with eax at 0x3b7b
and 0x3bad, so neither baseline nor candidate restores the correct argument
lifetime. This remains issue182, and this function is **not** evidence of
whole-function correctness or effect preservation. No metric-driven guard
or special case is added to conceal that defect.

Validation: 1,651 nextest passes across pcode/normalize/core, one existing
skip; pcode/decompiler checks, clippy with warnings denied, workspace format
and boundary audit pass. Mandatory paired external Docker dev6 and unscored32
have unchanged status contracts and bare compile counts (4/6 and 8/32).
Dev6 still has six wrapper compilation failures; unscored32 has 28 missing
wrappers and four adapter failures. These limited gates establish no new
whole-function semantic passing rows and supply no ranking evidence.

Evidence in the local artifact directory: copy-run.json, copy-all-overlays.json,
copy-rank.json, copy-intersection91.json, copy-recompiles.json,
copy-determinism.json, copy-call-audit.json, comparison-slices-provenance.json,
comparison-slices-{native,linux}.json, linux-anchor-comparison.json,
copy-external-comparison.json and the compiler/test/scorer logs.
