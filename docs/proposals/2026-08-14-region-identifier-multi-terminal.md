# RegionIdentifier-style multi-terminal region structuring

## 1. Baseline Row Anchor

- Binary: `vendor/decbench-evalkit/decbench-evalkit-sample-set/binaries/bin_033.elf`
- Function/address: `main` at `0xf390`
- Command: release `fission_cli decomp .../bin_033.elf --layer nir --json --no-header --no-warnings --addr 0xf390`
- Current measured output: NIR 34 gotos; HIR 34 gotos.
- References on the same function: Ghidra 12 gotos; angr 2 gotos.
- Semantic cases/static score: unavailable in the DecBench sample-set lane.
- Current driver evidence:

```text
DREAM declined: cycle-survived
DREAM+virtual-gotos declined: guards-too-large
DREAM-hierarchical declined: cycle-survived
DREAM-hierarchical+virtual-gotos declined: guards-too-large
```

Temporary read-only diagnostics after explicit virtual-transfer materialization
measured a 65-node acyclic live graph with two real terminals. Applying angr's
RegionIdentifier candidate test to that graph found two proper real-follow
subregions and one 65-node region whose frontier is the synthetic common exit.
The existing hierarchical driver already abstracts the real-follow regions,
then still asks one global reaching-condition formula to describe the outer
multi-terminal region. That formula exceeds the existing 8,000-node safety
bound.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [ ] Builder/materialize
- [ ] Normalize
- [x] Structuring
- [ ] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

The CFG becomes acyclic and retains all reachable blocks. The refusal occurs
inside `reaching_driver` after loop folding and explicit transfer
materialization. The missing owner behavior is region hierarchy/admission, not
instruction semantics or rendering.

Reference comparison:

- angr `RegionIdentifier._make_acyclic_region` attaches all real terminals to
  a dummy end, walks the immediate-postdominator chain, validates region
  boundaries with dominance frontiers, and abstracts each accepted region.
- angr `RecursiveStructurer` structures subregions before parents. Its Phoenix
  family can virtualize a remaining edge inside an already identified region
  and retry schema matching.
- Fission already owns the required pieces independently: typed `VirtualExit`
  postdominance, a shrinking `CollapseGraph`, isolated lowering, schema
  matching, and sound goto/label materialization for conceded edges. They are
  not yet composed for an outer multi-terminal acyclic region.

## 3. Generality / Invariant Proof

Generalized invariant:

```text
1. Analysis runs over compact live nodes only; retired CollapseGraph slots are
   not terminals.
2. Multiple live terminals are connected to one analysis-only VirtualExit.
3. A candidate is the forward slice from a head to a real or virtual
   postdominator frontier. Fission proves the ownership boundary with an
   equivalent closed-frontier plus sole-entry test rather than importing
   angr's dominance-frontier implementation. VirtualExit is never emitted as
   a block or label.
4. Proper subregions are structured before their parent. A multi-terminal root
   is then reduced with the existing schemas. If no schema applies, one edge
   may be removed only after its equivalent guarded/unconditional goto and one
   target label have been materialized.
5. All lowering is isolated. Analysis is pure, and a candidate commits only
   after raw, normalized, and actual post-layout goto admission selects it.
```

ISA-agnostic check:

- [x] Uses only CFG, dominance/postdominance, terminator, and AST facts.
- [x] No binary/function/address/compiler/ISA identity in production logic.
- [x] No runtime or build dependency on `vendor/angr-master`.

Required negative cases:

- retired slots must not become fake terminals;
- a side entry rejects a candidate;
- an edge leaving anywhere except the proved frontier rejects a candidate;
- a conceded conditional edge must preserve the other successor and guard the
  emitted goto;
- an unplaceable terminator declines the isolated candidate without changing
  the baseline host.

## 4. Risk And Ownership Check

- Extend `reaching_driver`; do not add a new end-of-pipeline pass.
- Keep `MAX_GUARD_FORMULA_SIZE = 8_000`; the measured 160k-formula population
  remains excluded.
- Do not use global snapshot/rollback. Preview and commit reruns use the
  existing `lower_observed` / `lower_isolated` contract.
- Concessions are bounded by graph size and explicit-goto count, and the final
  candidate remains subject to `structuring_quality`'s switch, empty-if,
  nesting, guard, and cleaned-output gates.
- Switch-bearing baselines must remain protected by the comparator.

## 5. Validation Matrix

- [x] Pure region tests: distinct exits produce a VirtualExit root; nested
  real-follow regions are selected before it; side-entry and escaping-edge
  shapes are rejected; retired slots do not count as exits.
- [x] Existing explicit-concession path retained: an edge is removed only after
  its goto/label pair is materialized, duplicate labels are suppressed, and
  final block coverage is checked.
- [x] Targeted Rust tests, then `cargo nextest run -p fission-midend-structuring`
  (244 passed).
- [x] `cargo nextest run -p fission-pcode` (1,000 passed, 1 skipped) and
  `cargo check -p fission-pcode`.
- [x] Release CLI focused rerun of bin_033 in both NIR and HIR, recording goto,
  line, byte, switch, and return deltas.
- [x] Per-file 224-binary/250-function NIR and HIR corpus comparison; no claim
  from aggregate totals alone.
- [x] Existing phase-2 execution differential and boundary scan.
- [x] External fission-benchmark Docker lane against the local build, with
  caches disabled where supported; local output is not published as official.

Measured result:

- The motivating bin_033 candidate is correctly rejected by the new
  post-layout gate: NIR remains 34 gotos / 363 lines / 8,746 bytes and HIR
  remains 34 gotos / 341 lines. Its HIR identifier spelling varied between
  processes, so no readability claim is made for that row.
- The RegionIdentifier candidate is admitted on five other files in both
  layers: bin_039 66 -> 41, bin_068 26 -> 20, bin_069 13 -> 4, bin_117
  15 -> 5, and bin_179 13 -> 6.
- Full DecBench totals move NIR 985 -> 928 and HIR 977 -> 920. Exactly five
  files improve and zero regress in goto count. Output expands by 224 NIR
  lines / 16,133 bytes and 317 HIR lines / 18,849 bytes; therefore this is a
  control-flow structuring gain with an explicit size tradeoff, not an
  unqualified readability win.
- On the 204-file Fission/Ghidra/angr intersection, Fission now has 887 NIR
  gotos and 879 HIR gotos versus Ghidra 691 and angr 420.
- The external no-resume dev run produced 382 rows, 316 direct-function rows,
  and 316 semantic-tested rows. The run is non-publishable and its aggregate
  validity is false because full-profile adapter coverage is 82.72%, below the
  runner threshold. On the 102 rows shared with the previous local run, NIR
  text and goto counts are identical; semantic timeout outcomes varied despite
  identical decompilation text and are not attributed to this change.

## 6. AI Review / Prompt Firewall

- No external model was asked for implementation advice.
- The vendor source was used only to identify owner invariants and traversal
  order.
- Production tests describe graph shapes; production code contains no DecBench
  row identity.
