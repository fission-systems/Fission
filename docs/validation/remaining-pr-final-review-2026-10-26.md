# Remaining PR admission review — 2026-10-26

## Scope

This candidate integrates #677 and #678 with owner-local admission guards and
accepts only the BUILD-scope repair from #587. Its original array widening and
silent out-of-range context accesses are excluded: the current 64-bit runtime
still fails closed above its supported word range. Context expansion in #551
remains open. #176 is explicitly excluded at the user's request, including its
FSL crate, Cargo changes, 128-bit context migration and research documents.

The older #604/#588 proposals are superseded by #677/#678. #602's deterministic
naming and ABI-slot corrections already landed in #675; the disputed byte-carrier
propagation is not revived and #519 stays open. #562's independently reviewed
components already landed through narrower PRs; its byte-copy gap is addressed
by the guarded #678 here. These old alternatives should be closed as superseded,
not merged as additional implementations.

## Corrections

- Double-precision word-pair collapse preserves target endianness and rejects
  pointer expressions that read memory or call code. Structural equality alone
  cannot justify evaluating such a pointer once instead of twice.
- Byte-sum masks require a truncating destination or unsigned-byte operation
  with no signed declared source binding. An i8 -56 copied to i32 must not be
  changed to 200 by a mask. The previous test asserting this unsafe mask is
  replaced by an exact body-preservation assertion.
- BUILD exports use constructor-local maps saved/restored on recursive entry.
  Child exports are resolved within the child scope; only its declared result
  enters the caller's scope. Cross-build and delay-slot instruction emissions
  also preserve caller exports. Error paths restore the parent BUILD state.
  There are no state-pointer keys or cross-scope fallback searches.

Reference: Ghidra `SleighBuilder::appendBuild` pushes/pops the operand walker,
and `PcodeBuilder::build` saves/restores recursive label state. This is a
reference invariant, not a runtime dependency or copied implementation.

## Baseline and validation

Baseline commit: 5cf2a8c11. The local Linux Docker bundle is built from that
clean source with its SOURCE_FINGERPRINT retained. External dev rows:

| Function | Variants | Semantic cases | Existing failures |
| --- | --- | --- | --- |
| reverse_string | gcc O0/O1/O2 | 5/15 | O0/O1 compile_error |
| checksum | gcc O0/O1/O2 | 15/15 | none |
| fibonacci | gcc O0/O1/O2 | 18/18 | none |

The local benchmark runner disables metric/decompilation caches and checkpoint
reuse. All results remain local; no official ranking/Pages data is modified.
The real PE CLI observation and 40 raw P-code instructions are also retained.
These rows measure regression behavior, not occurrence of the synthetic defects.
No measured quality improvement is claimed.

Four focused counterexample tests failed before the owner corrections. The
initial green focused suite passed 23 tests. Final workspace, CI, raw-P-code,
and same-row after results are recorded in the aggregate PR description and
ignored artifacts under benchmark/artifacts/pr-review-final-2026-10-26/.

Commits are made on main in the separate review clone. The user's dirty primary
main worktree is never modified.
