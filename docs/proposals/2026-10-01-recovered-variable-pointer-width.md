# Recovered variable pointer widths follow the target

## 1. Baseline Row Anchor

The current DecBench eval-kit was decompiled without database overlays using
`fission_cli decomp --addresses-file ... --json --layer both --no-db`.
The original run is at `/private/tmp/decbench-coherent-baseline/`; its pointer
census records 755 pointer-shaped variables reporting size 8 on 32-bit targets,
across 81 functions and 12 programs. This is metadata correctness evidence;
it is not a source-semantic or readability improvement claim.

Three concrete anchors spanning programs, optimisation levels and containers:

| Binary | Project / optimisation | Address | Variable | Reported | Target pointer size |
| --- | --- | --- | --- | --- | --- |
| bin_004.elf | crazyflie / O0 | 0x805d034 | param_1 (`float *`) | 8 | 4 |
| bin_031.elf | libopencm3 / O2-noinline | 0x8000764 | param_1 (`uint *`) | 8 | 4 |
| bin_172.dll | dexter / O2 | 0x69945e00 | param_1 (`uchar *`) | 8 | 4 |

The eval-kit private format metadata identifies the ELF targets as ELF32-ARM
and the PE target as PE32-i386. Semantic cases: not measured for these ARM/PE
rows. DecBench type/Union change: pending remeasurement, not assumed.

## 2. Owner Proof

- [x] Structured-variable serialization (`render/variables.rs`)
- [ ] Builder, normalization, structuring, or C printer semantics

`byte_size(NirType::Ptr(_))` unconditionally returns `Some(8)`.
The surrounding `recovered_variables` receives `HirFunction.is_64bit`, but
does not pass the target width to `describe`. The existing IR options construct
pointer size as `if binary.is_64bit { 8 } else { 4 }`; metadata must use that
same target contract rather than the host's width.

## 3. Generality / Invariant Proof

A pointer binding's serialized byte size equals the function's target pointer
size, independently of its pointee, argument/local role, or machine ISA.
Scalar, aggregate and unknown sizes retain their existing type-derived rules.
This fixes a wrong fact consumed by variable viewers and ABI/stack evaluators
even when no leaderboard metric changes.

- [x] No ISA, symbol, address, program or compiler guard in production.
- [x] Use existing target-width data, no new type or telemetry contract.
- [x] Synthetic coverage includes both argument and local pointer bindings on
      32-bit and 64-bit targets, nested pointers and non-pointer controls.

The current function model expresses 32/64-bit width. This change does not
claim to introduce a 16-bit pointer model.

## 4. Risk And Ownership Check

Extend the existing variable-data serializer; no new pass or dependency.
No C/NIR/HIR tree mutations, no changes to evaluations, names, offsets, or
instruction-address provenance. 64-bit pointer sizes must remain 8. Named
surface types backed by non-pointer lattice types retain their existing size.

## 5. Validation Matrix

- Targeted: `cargo nextest run -p fission-pcode -E 'test(render::variables)'`.
- Crate: `cargo nextest run -p fission-pcode`; `cargo check -p fission-pcode`.
- Build: release CLI; `cargo fmt --all --check` and owner-boundary check.
- Real binary: rerun the three anchors and affected pointer-bearing rows;
  compare JSON widths, and require unchanged `code`, `code_nir`, `code_hir`.
- External Docker: baseline and candidate local runner, no checkpoint reuse.
  The initial six dev rows all have compile errors; report this limit rather
  than claiming semantic success from the runner completing.
- DecBench: cache-disabled type re-evaluation; preserve separate metric
  coverage and label any partial score overlay as a local rank projection.

## 6. AI Review / Prompt Firewall

No external model or subagent was asked for implementation advice. The
production invariant uses only target width and structural type. Synthetic
tests cover target-width behavior independently of benchmark identities.

## 7. Review Notes

This is a measured correction to structured variable facts, not a new
decompiler-quality pass or a claim about reconstructed source semantics.
The rank checkpoint uses DecBench's existing scoreboard implementation.
Results and validation status will be recorded after measurement.

## 8. Measured Results

- Baseline and candidate both generated 250 functions from 223 current-kit
  binaries, based on main `6a6835aa0` plus the instruction-provenance change.
- 81 functions changed only structured metadata: 755 pointer sizes became 4
  instead of 8. Across all 250 functions, `code`, `code_nir` and `code_hir`
  stayed identical. Other variable fields stayed identical.
- Full official TypeMatch re-evaluation ran with `DECBENCH_NO_CACHE=1`:
  228 measured functions, all 228 scores unchanged, no lost or gained rows.
- Local type-only rank overlay remains Union 74/250 (29.6%), projected eighth
  against the public 2026-09-23 snapshot, refreshed on 2026-10-01. GED and
  byte-match scores were retained from that public snapshot; this is not a
  complete new-build ranking or a publication claim.
- Eval-kit packaging includes 250/250 targets. The initial import lost six
  generated names; joining the trusted target identities by exact address
  restored all 250 checkpoint functions, without changing their bodies.
- Targeted variable tests: 4 passed. Full `fission-pcode`: 1,161 passed,
  1 skipped. Pcode/decompiler checks, release build, formatting and owner
  boundaries passed. Separate DecBench eval-kit/checkpoint tests: 47 passed.
- External Docker local loop: six motivating dev rows produced identical
  output, semantic score and case counts before/after. All six still fail
  compilation. Candidate measurement disabled caches and checkpoint reuse.
  This does not demonstrate source-semantic improvement.

Evidence is archived locally in
`benchmark/artifacts/decbench/2026-10-01-pointer-width/`, including submission
zips, score overlays, before/after comparisons, source/CLI fingerprints, test
logs and the frozen ranking snapshot. Artifacts are not committed or published.
