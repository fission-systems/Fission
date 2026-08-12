# Rotated natural-loop admission before acyclic children

## 1. Baseline Row Anchor

- Binary: DecBench sample-set `bin_212.elf`
- Function: `main`
- Address: `0x6de7`
- Corpus command: release `fission_cli decomp` at the address in both `nir` and
  `hir`, plus the 224-binary / 250-function sample-set runner
- Current NIR output: 27 gotos, 434 logical lines, 10,686 bytes
- Current HIR output: 18 gotos, 372 logical lines, 9,720 bytes
- Reference outputs on the same function: Ghidra 0 gotos / 208 lines / 6,046
  bytes; angr 0 gotos / 178 lines / 4,977 bytes
- Semantic cases passed / total: unavailable for this reference-only DecBench
  binary; this proposal makes no semantic-score claim
- Failure category: loop body is structured as top-level acyclic regions before
  its later-indexed natural-loop header is admitted

Measured excerpt:

```text
Fission: goto block_71ac; ... block_6ea2: ... goto block_71a3;
         ... block_71ac: while (1) { ... goto block_6f5a; ... }
angr:    for (...) { nested if/else decision tree }
```

The raw CFG contains 70 blocks. `LoopBody` proves a reducible loop headed at
block 37 with body members 1 through 37 and one exit at block 38. Diagnostic
tracing shows root SESE construction starts without children covering members
1 through 37, admits conditional candidates at the lower indices first, and
only later succeeds in `try_lower_while(37)` through its full-subgraph path.

## 2. Owner Proof

- [x] Structuring

Raw CFG and loop discovery already contain the right cycle and ownership set.
The first wrong decision occurs in `build_sese_region_body_impl`: each fixed
point scan follows lexical block indices, accepts the first acyclic candidate,
and restarts. A natural-loop header whose index follows its body is therefore
considered only after those body blocks have become independent structure
nodes. `try_lower_while` can lower the complete body set, but its ordinary
`RegionProof` owns only the lexical `[header, exit)` range, so reconstruction
also emits the earlier body nodes outside the loop.

angr provides a reference invariant, not an implementation dependency:

- `region_identifier.py::_make_regions` repeatedly identifies and abstracts
  cyclic regions before it begins acyclic-region construction.
- `region_identifier.py::_make_cyclic_region` derives loop membership, entries,
  and exits from the graph rather than address order.
- `phoenix.py::_analyze_cyclic` then applies loop schemas within the admitted
  cyclic region.

## 3. Generality / Invariant Proof

Generalized rule:

```text
Before admitting an acyclic child inside a reducible natural loop whose header
sorts after at least one body member, admit the loop as one exclusive cyclic
owner iff its LoopBody membership is wholly inside the current region, the
header is the only external entry, the canonical exit exists, and no existing
structured child overlaps the membership set. The slice currently admits only
a region-entry, non-fallthrough, single-edge trampoline immediately preceding a
contiguous rotated body. Execute lowering on an isolated host; commit only a
candidate whose explicit-goto cost is lower than the ordinary loop plus its
entry jump. Its RegionProof owns every LoopBody member, including members below
the header index.
```

- [x] The rule is ISA-independent and uses CFG, dominance/back-edge, entry, and
  membership facts only.
- [x] No function, address, binary, compiler, mnemonic, or rendered-name guard
  is permitted.
- [x] Synthetic coverage will use a rotated block layout and a natural loop
  containing an internal branch.

Comparable coverage:

- Motivating shape: later-indexed while header with a 37-block branchy body.
- Negative shape: loop with an external entry into a non-header body member.
- Negative shape: loop membership overlapping an already-admitted child.
- Sentinel rows: the existing complex-arm/type-pollution cases and the prior
  unsafe exclusive-ownership row must remain byte-identical unless independently
  admitted by this exact cyclic invariant.

## 4. Risk And Ownership Check

- Existing owner: `LoopBody`, `try_lower_while`, and the SESE collapse driver.
- Shared substrate: CFG / dominance / natural-loop membership.
- New pass: none. The change adds a graph-first admission phase to the existing
  fixed-point driver and an isolated-host hook parallel to the existing
  committed complex-arm hook. The already-proven cyclic owner recursively runs
  the existing SESE driver inside its exact body membership set.
- State safety: a failed rotated-loop trial must not mutate materialization,
  type/name identity, CFG, collapse caches, or register-origin ordering. The
  production host is updated only after isolated lowering succeeds.
- Ownership safety: do not perform broad child pruning. Admission requires the
  loop body to be disjoint from every active child proof; on success, the proof
  explicitly owns the already-proven loop membership set.
- Telemetry: reuse existing while-subgraph and region-proof telemetry; no new
  metric contract.
- New runtime/build dependency: none; vendor code remains reference-only.

## 5. Validation Matrix

- [x] Targeted structuring tests
  - Command: `cargo nextest run -p fission-midend-structuring`
  - Signal: rotated loop is eligible only with complete membership, sole entry,
    canonical exit, and disjoint children; negative cases are rejected.
- [x] Crate-level gates
  - Commands: `cargo nextest run -p fission-pcode` and
    `cargo check -p fission-pcode`
- [x] Focused real-binary rerun
  - Command: release decompile of the anchored function in NIR and HIR
  - Signal: goto count falls in both layers; expressions, types, calls, and
    branch predicates are inspected, not inferred from counts alone.
- [x] Full sample-set rerun
  - Command: DecBench 224-binary / 250-function NIR and HIR runs
  - Signal: all functions complete and aggregate gotos do not regress.
- [x] Reference intersection comparison
  - Signal: refresh fair common-function Fission/angr and Fission/Ghidra ratios.
- [x] External local benchmark smoke, cache disabled
  - Signal: direct-function coverage remains valid and existing semantic rows do
    not regress; local results remain non-publishable.
- [x] Boundary audit
  - Command: `python3 scripts/audit/nir_boundary_scan.py --root .`

## 6. AI Review / Prompt Firewall

- [x] No external AI model was asked for implementation advice.
- [x] angr was read only as a cleanroom control-structure reference.
- [x] Production code will contain no benchmark identity or compiler tuple.
- [x] A synthetic invariant test and a broader regression run are required in
  addition to the motivating row.

## 7. Review Notes

- [x] The measured row exists before production implementation.
- [x] The canonical structuring owner creates the wrong admission order.
- [x] The proposed rule extends existing loop/SESE ownership rather than adding
  a printer cleanup or benchmark workaround.
- [x] Quality language remains conditional on measured after-results.

## 8. Measured Result

Focused exact-baseline comparison on the motivating function:

| Layer | Gotos before | Gotos after | Lines before | Lines after |
|---|---:|---:|---:|---:|
| NIR | 27 | 26 | 435 | 433 |
| HIR | 18 | 17 | 373 | 369 |

The leading `goto` into the later-indexed loop header and its detached header
label are removed; the natural-loop body is emitted under the loop owner. The
rendered byte count increased because isolated lowering assigns a different set
of generated temporary names, so this change claims only the measured control-
flow/readability improvement, not a size improvement.

Full DecBench sample set, release CLI, 224/224 binaries and 250/250 functions:

| Layer | Gotos before | Gotos after | Delta | Lines before | Lines after |
|---|---:|---:|---:|---:|---:|
| NIR | 2,090 | 2,089 | -1 | 38,683 | 38,681 |
| HIR | 1,984 | 1,983 | -1 | 35,719 | 35,715 |

Only `bin_212` changed goto count in either layer. Exact release-binary
before/after checks kept the prior type-pollution (`bin_103`), unsafe ownership
(`bin_039`, NIR), and complex-arm (`bin_073`) sentinels byte-identical. HIR's
existing generated pointer-alias naming can vary between processes; its goto
counts and control structure did not change on the sentinels.

Updated fair common-function comparison against the already measured reference
runs:

| Intersection | Functions | Fission NIR | Fission HIR | Reference | NIR ratio | HIR ratio |
|---|---:|---:|---:|---:|---:|---:|
| Ghidra common | 242 | 2,032 | 1,928 | 716 | 2.84x | 2.69x |
| angr common | 159 | 1,904 | 1,820 | 382 | 4.98x | 4.76x |

Validation completed:

- `cargo nextest run -p fission-midend-structuring`: 134 passed.
- `cargo nextest run -p fission-pcode`: 999 passed, 1 skipped.
- `cargo check -p fission-pcode`: passed.
- `cargo build --release -p fission-cli`: passed.
- `python3 scripts/audit/nir_boundary_scan.py --root .`: 0 findings.
- External local Docker smoke, Fission-only, 5 functions x 1 variant,
  `--no-resume`: valid result, 5/5 direct-function coverage, semantic mean 0.80
  with 4/5 perfect rows, GED mean 0.80 with 4/5 perfect rows. The existing
  `list_sum` gcc O0 row remained a compile error. The local result is
  intentionally non-publishable:
  `/Users/sjkim1127/fission-benchmark/results/local_rotated_loop_focused_dcb544ca_dirty.json`.
