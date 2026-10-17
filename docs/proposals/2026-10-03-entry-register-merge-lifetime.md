# Entry register values and later merge carriers

**Decision: rejected. Production files restored to main670629f189.**

Recorded before production edits; base main670629f189, issue182.

## 1. Measured baseline anchors

Fresh250 outputs/223 stripped binaries from the current release CLI exactly
match accepted code_nir, code_hir and native variable provenance. All caches
are disabled for generation. Existing accepted uncached type/GED/byte results
therefore remain the baseline. Artifact tree:
benchmark/artifacts/decbench/2026-10-03-entry-carrier-split in primary checkout.

| Program/optimization/binary | Function/kit/address | Type / GED / byte | NIR lines / gotos |
| --- | --- | --- | --- |
| findutils/O2/find | process_all_startpoints/bin_027.elf/0x8070 | 0.13333333333333333 / 212.0 / 0.26280623608017817 | 244 / 10 |
| sysvinit/O2-noinline/wall | wall/bin_012.elf/0x2d50 | 0.10526315789473684 / 89.0 / 0.0 | 360 / 19 |
| sysvinit/O2-noinline/sulogin | getpasswd/bin_209.elf/0x3b40 | 0.05 / 49.0 / 0.0 | 219 / 9 |
| gnutls/O2-noinline/psktool | process_options/bin_008.elf/0x2ed0 | 0.09090909090909091 / 503.0 / 0.1975609756097561 | 329 / 18 |

Four functions, three programs, mixed O2/O2-noinline. No executable whole-source
semantic cases are available in this slice. Incoming source declarations use
pinned HF revision7af6c5e19289b6a357404708e5373bc7bf6fcacd for evaluation only.
No source/DWARF enters the CLI. Prior anchored width probe showed why an input
scalar declaration cannot also store a later full-width filename pointer.

All250 baseline: Union74, type17/228, GED65/240, byte1/166 and compiles132/166.
Fixed published91 intersection baseline: Union22, type7/85, GED18/90,
byte1/91, compiles66/91. Rival snapshot2026-09-23; local projection only,
no official publication. The fixed91 mask is recorded and never selected by
score. Static compilation is not executed semantic equivalence.

## 2. Owner proof

[x] Builder/materialize, before normalize or printing.

Recorded existing EMIT-TRACE confirms ensure_explicit_merge_binding_for_block
chooses param_1 for later merge storage solely because the varnode occupies
an ABI input slot. Examples:

- find: 0x842f merge reserves param_1; predecessor writes stream storage and1.
- wall: 0x3013 and0x308c merges reserve param_1 for constants/descriptors/streams.
- getpasswd: 0x3de6 merge reserves param_1 for later stderr preparation.
- psktool: 0x331e phi reserves param_1 and later full-width arithmetic targets
  the same binding while the initial input was copied to a saved register.

Raw p-code snapshots and PreHIR/NIR corroborate full-width later definitions
of the ABI storage. These are slot-value conflations, not bad lifting or a
printer spelling issue. The API experiments changed declarations and were
rejected because they left these carriers conflated.

## 3. General invariant and justification without the metric

A mutable merge carrier may reuse an ABI input binding only while that exact
storage's incoming value can still reach the merge entry. When every CFG path
from entry overwrites the full storage first, reserve a separate local carrier.
It must retain the full materialized width and the existing predecessor/phi
assignment protocol. Do not globally rename the input or later output text.

Why better: unrelated later pointer values must not change the source input's
declaration or be narrowed by it. Entry-only reads and saved copies retain
input identity; later full-width values get separate storage. No row, address,
name, compiler, mnemonic or ISA enum enters the condition.

Use a conservative owner-local reaching-input proof on the existing lifted
CFG, next to the existing definition-reachability scans. Follow paths from
entry to the requested block entry. Only an unconditional write that covers
the whole requested storage kills a path. Partial writes, instruction-local
guarded writes and untouched arms remain entry-carrying. Cycles are bounded
by visited blocks. Missing/unreachable CFG evidence keeps legacy input reuse.
Use the existing register-storage coverage relation and guarded-write facts;
no new dataflow framework, semantic pass, dependency or telemetry payload.

An earlier merge binding that is itself input-owned cannot be reused after
this proof excludes the input. Non-input merge carriers keep existing stable
naming, preserving linked joins and loops. This is conservative separation,
not full SSA lifetime splitting; other parameter-carrier owners remain out of
scope. Do not claim issue182 fully closed by this change.

## 4. Validation matrix and risks

Synthetic cases: both diamond arms fully overwrite; one untouched arm;
partial-width writes; instruction-local guarded writes; loop backedge with
an entry-carrying bypass; unreachable/missing graph; previously reserved
input-owned merge vs local-owned merge. Check stable names and full width.

Targeted nextest -> full pcode/normalize suites -> pcode/normalize/decompiler
checks, fmt --all/check and scoped Clippy. Fresh native/Linux release builds.
Regenerate all250/223, package unchanged native adapter metadata, uncached
standalone GED/type/byte with exact paired GNU toolchain. Compare perfect-row
sets and every changed NIR/HIR body, plus fixed91 projection. Mandatory external
local Docker paired dev6/fixed unscored32, caches/resume disabled; report
wrapper/no-wrapper failures separately. No tuning on the pool or ranking it.

Inspect anchored high-pointer-bit preservation with a capture stub and
benchmark GCC14.2 x86-64 Docker execution. No real execvp or corpus malware
execution. A compilable candidate or mean-score gain alone is insufficient.
If new uninitialized carriers, effect/width loss, lost perfect functions or
semantic regression appears, reject and preserve evidence before rollback.

## 5. AI firewall and reference boundary

No external models/subagents. No vendor copying or runtime/build dependencies.
The existing reachability/materialization owner implements a CFG/storage
invariant, not a metric shortcut. Current non-input-value separation is the
next direction recorded in the rejected experiment proposal and Kuna #875,
#832/#882 references; upstream reported gains are not Fission evidence.

Reference inspection: Ghidra12.0.4 merge.cc/mergeTestRequired keeps input,
address-tied, locked-type and other variable properties distinct from physical
storage. This informed the identity invariant only; no implementation copied.

## 6. Results and rejection

The initial candidate used raw full-storage writes plus guarded-write exclusion.
Its exact crates diff SHA256 is
b541c55c9d90d4f0f263ee8fa57469bd977d0ea350cfce720b30a5f6823b3b0e;
source-measured-initial.patch and immutable CLI/run fingerprints are archived.
All250 native outputs were freshly generated and all three standalone metrics
were reevaluated with DECBENCH_NO_CACHE=1 and the same GCC14.2 toolchain.
No adapter, matcher, source inputs or rival scores were changed.

| Measurement | Accepted baseline | Rejected initial candidate |
| --- | --- | --- |
| Union250 | 74/250 | 74/250; no gained/lost perfect rows |
| GED perfect / measured | 65/240 | 65/240 |
| GED mean distance (lower better) | 33.629166667 | 33.75 |
| Type perfect / measured | 17/228 | 17/228 |
| Type mean (higher better) | .264112288280 | .265021715969 |
| Byte perfect / measured | 1/166 | 1/166 |
| Byte mean (higher better) | .203560186089 | .203418951580 |
| Compiles | 132/166 | 132/166 |
| Fixed91 Union | 22/91 | 22/91 |
| Fixed91 type perfect | 7/85 | 7/85 |
| Fixed91 GED perfect | 18/90 | 18/90 |
| Fixed91 compiles | 66/91 | 66/91 |

Type scores improved on find/process_all_startpoints (.133333 -> .2),
psktool/process_options (.090909 -> .181818), cp/copy_reg (.117647 -> .205882),
and regressed on ssh-keygen/ssh_rsa_verify (.115384 -> .076923). GED changed
on seven imperfect rows; six increased, one decreased. Byte changed on five
imperfect rows; four decreased, one increased. These aggregate tradeoffs do
not establish semantic correctness or any ranking gain.

External local Docker paired dev6 and unscored scale32 was uncached, with
zero recovered checkpoints and unchanged output/status on all38 rows.
Limits: dev6 has six wrapper compile errors, scale32 has four adapter errors
and28 rows without executable wrappers. Bare compile4/6 and8/32 was unchanged.
This is regression observation, not an executed whole-function semantic pass.
The unscored pool is allO0; mixed optimizations come from the motivating rows.
Linux four-anchor checks matched native statements/provenance after two
bijective temporary renamings for find. No official release bake or Pages run.

An actual emitted psktool execvp tail preserved 0x123456789abcdef0 and the
fallback string on native Clang and network-disabled x86-64 Linux/GCC14.2.
Calls use capture stubs; no actual execvp or corpus binary was executed.
This resolves the prior candidate's width failure on that slice only.

### New initialization failure blocks acceptance

ssh-keygen/ssh_rsa_verify, bin_128.elf/0x52cb0, has a reachable branch:

```c
block_52f40:
    rsa = rbx[1];
    rax = RSA_size(rsa);
    xVar80 = *slot_30;
    xVar81 = rax;
    xVar82 = xVar80;
    if (xVar81 < xVar82) {
        xVar55 = (uint8_t *)(&stack_frame) + 64;
        goto block_52e1f;
    }
// ...
block_52e1f:
    xVar263 = xVar49;
```

The new local xVar49 has no reaching emitted assignment on that direct branch.
The accepted output read the initialized formal param_5 on this path instead.
An extracted branch/first-use probe uses exact emitted statements, initialized
storage, and a local RSA_size stub to select this branch. GCC14.2 with
-Werror=uninitialized/-Werror=maybe-uninitialized accepts the baseline and
rejects the candidate for xVar49. The baseline capture executes in isolated
Linux and preserves the sentinel. The undefined candidate is not executed.
Artifacts: initialization-probe/{baseline,candidate}.c, compile logs and
result.json. This is a new definite initialization failure; it does not assert
that the entire accepted function is semantically correct.

Raw storage overwrite reachability is insufficient to certify that the new
out-of-SSA carrier has an assignment on every emitted predecessor path. Call
argument preparation and filtered definitions can break that implication.
The next owner investigation must link source definitions, emitted binding
assignments, merge/phi operands and dominance before allowing input separation.
A blanket local initialization from the input would conceal this gap and is
not an accepted repair.

The final conservative experiment additionally reused the existing definition
kill predicate to preserve same-storage copies/casts, with a sixth invariant
test. Six of250 native outputs differed from the initially scored version;
final-output-parity.json records them. No scores from the initial candidate
are assigned to this final version. The initialization witness remained, so
both versions were rejected without tuning or rescoring the final version.
The final code was checked mechanically:6 targeted tests;1,630 pcode/normalize
tests passed,1 skipped; relevant cargo checks and scoped Clippy passed.
Only the initial version received the Linux build/full external measurement;
that distinction is retained in the artifacts. All production/test changes
were archived and restored; this document ships evidence only. Issue182 stays
open. No decompiler quality or public-ranking improvement is claimed.

### Current accepted ranking checkpoint

The fixed91 mask is
883255f7b14081af9347c6c510b6e36d48a30887b0b28155e9100ab5300cfae3.
The public HF export omits Ventris. Intersection of every available sample-set
backend's output-success flag already leaves exactly the published91 rows,
and every available backend's Union numerator matches the public table.
Adding Ventris cannot remove a row while retaining91, establishing the mask
without score-driven row selection. Exclude the optimized-only Codex preset.

Accepted local22/91 projects to rank6, tied with historical Hex-Rays22/91,
ahead of historical Kuna20/91. Codex47/91 requires at least48/91, hence26
additional perfect rows with no losses. All250 remains74/250; Kuna88/250
requires89/250, hence15 additional perfect rows. Rivals are frozen at
2026-09-23; these are local projections, not official current ranks. Compilers
and metric-measurability can differ from the historical release bake.
