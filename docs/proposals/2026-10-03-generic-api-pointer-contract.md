# Generic API pointer acceptance does not declare a source variable

Recorded before production edits, 2026-10-03. Base: main 7e85c7272.

## 1. Measured baseline row anchors

Standalone DecBench, immutable native CLI SHA256
ac5da815906c7bf64852480bc38c91cd9fc6e6a6c8e48bb0aa2742128f86b669,
`/private/tmp/fission-kuna-copy-final/rows` and cache-disabled overlays.
All 223 originals match the current Hugging Face manifest SHA256s; all stripped
copies preserve executable sections and have no debug sections. Evaluation
source-only audit pins HF revision 7af6c5e19289b6a357404708e5373bc7bf6fcacd.

| Source / program / opt | Function / stripped address | Type / GED / byte | NIR lines / gotos | Observed declaration |
| --- | --- | --- | --- | --- |
| sources/shadow/chage.c, shadow O0 | close_files, bin_220.elf 0x6116 | .625 / 28 / .388157895 | 141 / 0 | saved_locale char* becomes void* (__ptr, local_48, local_38); passed to free |
| sources/libacl/setfacl.c, libacl O2-noinline | restore, bin_102.elf 0x5480 | .172413793 / 27 / 0 | 324 / 24 | path_p char* maps to local_60 void*; passed to free |
| sources/gzip/gzip.c, gzip O0 | get_method, bin_219.elf 0x72f1 | 0 / 45 / .130148270 | 730 / 16 | p/base char* map to __src/__dest void*; passed to memmove |

These are three functions across three programs and mixed optimization levels.
Their macro-expanded source bodies match published HF source blobs after
removing only reconstructed numeric line directives. DWARF and native variable
correspondence were inspected separately; types did not determine correspondence.
Semantic executable cases: not available for this static DecBench slice. No
whole-program behavior equivalence claim. Accepted Union baseline: 73/250.

## 2. Owner proof

Owner: normalize/type recovery, existing callsite_type_prop and
call_target_surface::apply_api_surface_type_transitively.

The current helper sets binding.surface_type_name before testing whether the
parameter declaration is generic void*. This locks an existing pointer binding
as void*, even though the existing structural pointer lattice is more specific.
Example NIR: strdup result is saved, then free(local_48); output local_48 is
void* while ground truth and source establish char*. The helper explicitly
retains this historical immediate-argument behavior; its existing test asserts
it. No printer or raw SLEIGH change is needed.

## 3. General invariant

An API parameter accepting any object pointer does not establish that the
argument variable was declared void*. Preserve existing pointee/declaration
information at the direct argument and through copies. Existing structural
pointer inference may still establish a previously unknown pointer. Specific API
parameter declarations continue through the existing owner and lifetime guards.

Why better without a metric: passing a char* to free or memmove must not erase
that variable's character pointee or prevent later specific constraints.
ISA-agnostic: no ABI/register/function/address guards; only API type contracts.
No code sees source ground truth at runtime. Synthetic tests cover direct
pointers, copy aliases, generic spellings, and later specific declarations.

## 4. Risk and ownership

Extend existing helper; no new pass, telemetry, dependencies, or metric changes.
No new owner-to-owner dependency. Existing specific surfaces stay locked.
Structurally unknown pointers still render void* unless other evidence refines
them. Some callers may retain less attractive storage integers; measure complete
outputs and reject claimed gains without evidence. Keep return-pointer inference,
argument expression evaluation, call targets/arity, and copy lifetime guards.
Reference: C object-pointer conversion and vendor Ghidra type propagation uses
operation-specific constraints; no vendor code copied or runtime dependency.

## 5. Validation matrix

- Targeted nextest generic pointer/callsite tests, then normalize+pcode full suites.
- Checks normalize/pcode/decompiler; cargo fmt --all, fmt check, scoped Clippy.
- Native release CLI, immutable fingerprint, fresh all250 stripped outputs;
  same standalone DecBench metric sources, cache-disabled GED/type/byte scores.
- Inspect all three anchored NIR/HIR outputs and actual type correspondences.
- GNU Linux local build and required external Docker dev + fixed unscored32 pool;
  same harness baseline, no cache/checkpoint recovery. Existing six dev wrapper
  errors and pool four adapter errors/28 no-wrapper statuses are limitations,
  not semantic passes. Bare compilation baseline 4/6 and 8/32 respectively.
- Synthetic boundary tests and zero scored-sample overlap in unscored pool.
- Preserve existing perfect rows; report partial losses and compile deltas.

## 6. AI review / firewall

No external model or subagent implementation advice. Local investigation only.
Production conditions contain no row identities. Do not tune against pool results
or promote local scores to official leaderboard/Pages. Sources are evaluation-only.

## 7. Review notes

No benchmark metric/source oracle changes. No new helper/pass architecture.
Recorded pre-implementation anchors; results follow after measurements.
