# SSA definition line provenance (mechanical transport)

## Baseline and owner

The fixed qualification run at dataset revision
`07dd95dbe6214b8a78fc1e4a55961ab515986489` emitted 244 NIR functions,
142 with direct variable PCs, but no line maps. Real transport anchors are
Linux Snappy O0 row 048, Windows Snappy O2 row 072, and macOS Ninja O2 row 040
in `benchmark/artifacts/cpp-type-policy/2026-10-13/live-adapter.json`.
Their unmodified NIR and direct variable metadata are the before baseline.
This is not a semantic or readability improvement; execution parity and
decompiler rankings are not claimed.

The existing builder `cover_diagnostics` projects bindings to scalar SSA high
variables. The shared printer knows the actual output line of each statement.
These owners can transport a proved result definition without rewriting IR.
HirStmt has no general occurrence provenance; variable lifetime PCs must not
be sprayed onto every textual occurrence.

## Invariants

* Admit only a uniquely materialized register-rooted high variable whose members
  are operation definitions at one machine instruction; no input/phi member.
* Reject duplicate address/sequence sites from block cloning and explicit join
  bindings. Partial byte accesses retain the existing whole-access guard.
* Require exactly one definition of the final binding, including initializers.
* Attach the definition PC only at the actual printed assignment/initializer
  line. It is a contributing result-definition origin, not complete lineage
  for every RHS, call, load, branch, or variable on the line.
* Do not map headers, ordinary declarations, control predicates, effect-only
  statements, multi-definition carriers, or inline for clauses without proof.
* Track NIR and HIR independently; CLI line deletion filters preserve line
  indices explicitly or drop mappings if the transform is not a subsequence.
* Export defined binding names so adapters never infer RHS occurrences from
  this partial line map. Existing direct variable PC evidence takes precedence.
* No ISA-specific rule, type inference, effect re-evaluation, new dependency,
  or new telemetry owner. The source instruction is existing p-code provenance.

## Validation matrix

Targeted Rust tests: single and repeated definitions, initializer, nesting,
ambiguous/phi/input provenance, cloned sites, prefix declarations, line filters,
and independent NIR/HIR surfaces. Then full pcode/CLI nextest, relevant checks,
release CLI build, workspace fmt and owner-boundary scans. Rerun actual stripped
anchors and fixed 250 inputs; verify exact before/after NIR text and variable
metadata apart from additive provenance, determinism, instruction membership,
and abstention on unproved lines. External semantic benchmark is required only
if a later semantic/readability quality claim is made.

## Measured result

The clean 52c81dd15 baseline and final candidate emitted 250/250 nonempty
functions with zero NIR/direct-variable differences. The candidate exported
237 scoped lines across 105 functions; all PCs passed native instruction checks.
Final-binary two-run probes on Linux/Windows/macOS were deterministic.
1,328 pcode/CLI nextest tests passed, one skipped; checks/release/fmt/owner scan
passed. This remains mechanical provenance transport; no execution parity,
semantic quality improvement or official ranking is claimed.
