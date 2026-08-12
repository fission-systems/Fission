# Non-fallthrough label alias elimination

## 1. Baseline Row Anchor

- Binary set: DecBench sample set, 224 binaries / 250 functions
- Function set: `functions.json` public functions
- Corpus command: clean release build followed by
  `python3 vendor/decbench-evalkit/decbench-evalkit-sample-set/run_fission.py`
- Current NIR output: 2,108 gotos, 38,719 logical lines, 1,065,457 bytes
- Current HIR output: 2,002 gotos, 35,756 logical lines, 965,148 bytes
- Failure category: redundant unstructured label aliases after structured lowering
- Observation: 18 NIR and 18 HIR sites have the exact sequential shape below
  across 13 binaries. The alias is entered only by explicit gotos because its
  preceding sibling cannot fall through.

```text
return/goto/break/continue;
alias:
goto target;
```

Representative measured rows are `bin_007`, `bin_014`, `bin_033`, and
`bin_209`. They exercise return-, goto-, and break-terminated predecessors.

## 2. Owner Proof

- [x] Structuring

Structuring cleanup already owns redundant goto and function-scoped label
cleanup. Instrumenting `bin_007` proved that `block_a451` is not yet an alias
inside `finalize_structured_body`: dead temporary assignments still separate
the label from its goto. The existing post-structuring
`eliminate_redundant_var_assigns` call removes those assignments immediately
before the canonical PreHIR-to-HIR boundary and exposes the measured alias.
The rule therefore belongs at that boundary, using the existing structuring
label cleanup owner after dead-assignment cleanup. Raw p-code, expressions,
types, and CFG branch predicates do not need to change.

Reference contracts agree on the required ownership update:

- Ghidra `BlockGraph::spliceBlock` moves the outgoing edges of a removable
  single-successor block to its predecessor before removing the block.
- RetDec `Statement::removeStatement` retargets goto predecessors to the
  removed statement's successor before unlinking it.

## 3. Generality / Invariant Proof

Generalized rule:

```text
For a sequential statement list, Label(alias); Goto(target) may be removed
and all function-scoped Goto(alias) references retargeted to target iff:
1. the immediately preceding sibling is a total transfer
   (Return, Goto, Break, or Continue),
2. alias has exactly one definition in the function,
3. target is a distinct, defined label,
4. alias is not an LSDA-protected landing pad, and
5. the accepted alias relation is acyclic.
```

- [x] The rule is ISA-independent and uses only structured control semantics.
- [x] No binary, function, address, compiler, or rendered-name guard is used.
- [x] Synthetic tests describe only the statement-list invariant.

Comparable measured coverage:

- Return predecessor: `bin_014`, `bin_021`, `bin_054`, `bin_124`
- Goto predecessor: `bin_007`, `bin_008`, `bin_009`, `bin_117`, `bin_129`,
  `bin_146`, `bin_194`, `bin_209`
- Break predecessor: `bin_033`

## 4. Risk And Ownership Check

- Existing owner: structuring cleanup and its recursive label rewrite helper,
  invoked after post-structuring dead-assignment cleanup exposes the alias.
- Shared analysis candidate: none; this is an owner-local structured AST rule.
- New dependency: none.
- Telemetry impact: none.
- Must not change: fallthrough predecessors, duplicate label definitions,
  missing/self/cyclic targets, LSDA landing pads, expressions, types, branch
  predicates, or statement order outside the removed two-statement alias.

## 5. Validation Matrix

- [x] Targeted positive and negative cleanup tests
  - Command: `cargo nextest run -p fission-midend-structuring`
  - Signal: return/goto/break/continue aliases rewrite recursively; protected,
    fallthrough, duplicate, undefined, self, and cyclic aliases remain.
- [x] Crate-level gates
  - Commands: `cargo nextest run -p fission-pcode` and
    `cargo check -p fission-pcode`
- [x] Focused real-binary rerun
  - Rows: the 13 measured candidate binaries, both NIR and HIR
  - Signal: only goto targets and redundant alias pairs change.
- [x] Full sample-set rerun
  - Signal: 224/224 binaries and 250/250 functions; goto count decreases in
    both layers with no candidate diff outside the proven rewrite.
- [x] Sentinels
  - Rows: `bin_073` complex-arm case and `bin_103` type-pollution case
  - Signal: byte-identical unless they contain an independently proven alias.
- [x] Boundary audit
  - Command: `python3 scripts/audit/nir_boundary_scan.py --root .`

## 6. AI Review / Prompt Firewall

- [x] No external AI model was asked for implementation advice.
- [x] Vendor code is used as a reference contract only.
- [x] Production code will contain no benchmark identity.

## 7. Review Notes

- [x] The change extends the existing cleanup owner and label rewrite helper.
- [x] It introduces no pass, metric, production dependency, or ISA branch.
- [x] Quality language remains conditional on the full measured after-run.

## 8. Measured Result

Release CLI, full DecBench sample set, 224/224 binaries and 250/250 functions:

| Layer | Gotos before | Gotos after | Delta | Lines before | Lines after |
|---|---:|---:|---:|---:|---:|
| NIR | 2,108 | 2,090 | -18 | 38,719 | 38,683 |
| HIR | 2,002 | 1,984 | -18 | 35,756 | 35,719 |

The NIR diff changed 13 files. An automated line-class audit confirmed that
every NIR change was a goto target rewrite or removal of the admitted label and
goto; no expression, assignment, type, condition, or statement-order change
occurred. `bin_073` and `bin_103` remained byte-identical in both layers.

Validation completed:

- `cargo nextest run -p fission-midend-structuring`: 131 passed.
- `cargo nextest run -p fission-pcode`: 998 passed, 1 skipped.
- `cargo check -p fission-pcode`: passed.
- `cargo build -p fission-cli --release`: passed.
- `python3 scripts/audit/nir_boundary_scan.py --root .`: 0 findings.
- External local Docker smoke, `dev`, Fission-only, 5 functions x 1 variant,
  cache disabled: valid result, 5/5 direct-function coverage, semantic mean
  0.80 with 4/5 perfect rows, GED mean 0.80 with 4/5 perfect rows. The one
  non-perfect row (`list_sum`, gcc O0) was a compile error. The artifact is
  intentionally non-publishable because it is a local/non-official run:
  `/Users/sjkim1127/fission-benchmark/results/local_alias_cleanup_focused_e4294d1ac_dirty.json`.
