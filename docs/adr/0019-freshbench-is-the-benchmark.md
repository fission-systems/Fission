# ADR 0019: FreshBench is the benchmark

**Status:** Accepted
**Last verified:** 2026-11-26
**Supersedes:** the DecBench measurement rule in `AGENTS.md` and
`docs/BENCHMARK_DECBENCH.md` (removed)
**Amends:** [ADR 0018](0018-decbench-is-not-a-correctness-oracle.md). Its
principle stays and now applies to FreshBench: a similarity benchmark is not a
correctness oracle.
**Related:** [ADR 0006](0006-decompiler-quality-change-gate.md) (what counts
as evidence for a quality change)

## Context

Fission measured decompiler quality on DecBench (`noelo-lab/decbench-dataset`).
This had three problems:
- Its functions come from widely used projects, the kind of code decompilers
  are tuned on.
- Its harness and scorer live outside this repository in Python, pinned by
  hand, and the recompile metric could not run locally.
- The full-dataset sweep and the sample-set gate scored differently in places,
  which `docs/BENCHMARK_DECBENCH.md` had to document row by row.

[FreshBench](https://github.com/fission-systems/freshbench) is the
organisation's own static decompiler benchmark:
- Its programs are recent and license-clean. DecBench's functions are excluded
  by fingerprint.
- Its scorer is written in Rust and has one CFG builder. It reports GED, Joern
  GED, types against DWARF, recompilation and call sites.
- Its M1 corpus is pinned by revision and hash: C, 20 projects, x86-64, i686,
  AArch64 and ARMv7 Thumb-2, GCC and Clang, six optimization levels, 285,875
  rows.
- `freshbench sweep run / report / diff` is a before/after loop for one
  decompiler under development. It compares output hashes first and refuses
  runs scored by different scorers.

## Decision

1. **FreshBench is the benchmark for Fission.** Output-changing work is
   measured with `freshbench sweep` on the M1 corpus, as described in
   [`docs/BENCHMARK_FRESHBENCH.md`](../BENCHMARK_FRESHBENCH.md). Quality
   claims cite it, not DecBench.
2. **A benchmark is not a correctness oracle.** FreshBench says so itself: it
   measures closeness to the source, not whether the output computes the same
   thing. ADR 0018's rules carry over unchanged:
   - Correctness comes from native replay, `fission_cli verify`, manuals,
     DWARF/PDB and call sites counted by instruction address.
   - A fix that makes wrong output right may score worse when the PR shows
     that evidence.
3. **Historical records keep their numbers.** Proposals, validation reports,
   changelog entries and earlier ADRs that cite DecBench describe
   measurements as they were taken. They are not rewritten.
4. **CI moves its inputs to FreshBench without losing a gate.** The
   determinism, memory-ceiling, base-vs-head and sweep jobs use DecBench
   binaries as inputs today. They switch to FreshBench M1 inputs before the
   DecBench jobs and scripts are removed. A required check is replaced, never
   dropped.

## Consequences

- `AGENTS.md`, contributor guides, templates and code comments point at
  FreshBench.
- A DecBench row is no longer a quality anchor. A FreshBench row
  (`<build>/<project>/<binary>@<address>`), a `verify` divergence or a native
  replay mismatch is.
- Numbers on the two benchmarks are not comparable. A before/after compares
  two FreshBench runs with the same scorer and corpus id, which `sweep diff`
  enforces.
