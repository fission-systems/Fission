# Fission Agent Guide

Generated: 2026-04-21
Scope: repository root

## Overview

Fission is a Rust-first reverse-engineering/decompilation workspace. Ghidra-native lifting feeds Rust-owned NIR/HIR normalization, structuring and rendering. The product surface is the CLI and its JSON contract; there is no GUI or TUI, by design.

**Repository layout (directories + workspace crates):** see [`docs/PROJECT_MAP.md`](docs/PROJECT_MAP.md). Use this file plus the tree below when navigating; avoid duplicating full crate lists in multiple docs.

## Structure

```text
Fission/
├── benchmark/
│   ├── config/              # Gate policy
│   ├── artifacts/           # Local measurement outputs
│   └── patch_validation_pool/ # Held-out regression pool (go/stop evidence only)
├── crates/
│   ├── fission-pcode-ir/     # p-code IR, optimizer, block-level CFG facts (leaf crate)
│   ├── fission-cspec/        # cspec/ldefs/pspec parsing, register model, seed_nir_render_options
│   ├── fission-pcode/        # NIR/HIR, structuring, CFG, printer; re-exports fission-pcode-ir
│   ├── fission-decompiler/   # Orchestration + Rust-Sleigh bridge (re-exports IR crate)
│   ├── fission-sleigh/       # Sleigh decode/lift runtime
│   ├── fission-static/       # Static facts, native preparation, analysis services
│   ├── fission-loader/       # Binary parsing, symbols, sections, strings
│   ├── fission-analysis-db/  # Typed immutable program metadata snapshots
│   ├── fission-signatures/   # FID/signature data and lookup
│   ├── fission-cli/          # CLI surface
├── utils/                    # Checked-in signatures, type info, benchmark support data
├── vendor/                   # Ghidra, RetDec, other reference code
├── scripts/benchmark/        # Benchmark setup / history helpers
├── scripts/test/             # Fuzz, path-literal and fixture checks, crackme workflow
└── .github/workflows/        # CI/CD source of truth
```

## Child AGENTS

- `crates/fission-pcode/src/midend/AGENTS.md`
- `crates/fission-pcode/src/midend/structuring/AGENTS.md`
- `crates/fission-cli/AGENTS.md`
- `crates/fission-loader/AGENTS.md`

Read the nearest child file before editing those areas.

## Where To Look

| Task | Location | Notes |
|---|---|---|
| NIR structuring / canonicalization | `crates/fission-pcode/src/midend/structuring/` | Core algorithmic decompiler work lives here |
| NIR telemetry contract | `crates/fission-midend-core/src/ir/build_stats.rs` | `NirBuildStats` is canonical |
| Decompilation orchestration / Rust-Sleigh | `crates/fission-decompiler/` | Routing, workers, type-context assembly; consumes `fission-pcode` + `fission-static` facts |
| DecBench (main benchmark) | `docs/BENCHMARK_DECBENCH.md`; full-dataset sweep `~/fission-benchmark/decbench-full/sweep.py`; quick gate `scripts/quality/decbench_sample_set_gate.py` (scoring in `scripts/quality/decbench_scoring.py`) | The comparative and regression measurement for output-changing work, on the **whole** dataset (96,103 functions); published source CFGs are the answer key. Neither DecBench nor Ghidra is a correctness oracle ([ADR 0018](docs/adr/0018-decbench-is-not-a-correctness-oracle.md), [ADR 0017](docs/adr/0017-ghidra-is-not-an-oracle.md)) |
| Benchmark policy / held-out pool | `benchmark/config/`, `benchmark/patch_validation_pool/` | Gate policy and the patch-validation pool |
| CLI one-shot parsing / command ownership | `crates/fission-cli/src/cli/` | Keep subcommand UX and legacy shims separate from semantics |
| Runtime resource paths (signatures, DiE, FID, patterns, typeinfo) | `crates/fission-core/src/core/path_config.rs`, `resource_roots.rs` | `PATHS` / `PathConfig::detect`; overrides: CLI `--resource-root`, `FISSION_RESOURCE_ROOT`; operator docs: `docs/CLI.md` § *Runtime resource bundle* |
| Checked-in utility resources | `/Users/sjkim1127/Fission/utils` | Prefer existing resource/path config and utility loaders over hardcoded paths; use this tree when reusable signatures, type info, benchmark support data, or other checked-in resources already cover the need |
| Loader identity / binary provenance hints | `crates/fission-loader/src/loader/identity/` | Evidence-backed `BinaryIdentityReport` on `LoadedBinary`; not an IR/decompiler repair layer |
| Canonical program metadata view | `crates/fission-analysis-db/` | Deterministic IDs and provenance for memory blocks, functions, symbols, and relocations; read-only downstream contract |
| Binary loaders (PE, ELF, Mach-O, TE, COFF, etc.) | `crates/fission-loader/src/loader/` | Format-specific byte parsing, section mapping, relocation, and symbol resolution |
| Static facts and binary-derived analysis services | `crates/fission-static/src/analysis/` | Xrefs, discovery, patches, strings; fact extraction — not decompiler orchestration |
| Decomp-facing facts / native prep surface | `crates/fission-static/src/analysis/decomp/` | `FactStore` and related helpers consumed by `fission-decompiler` |
| Reference algorithms | `/Users/sjkim1127/Fission/vendor`, especially `/Users/sjkim1127/Fission/vendor/ghidra/` and `/Users/sjkim1127/Fission/vendor/retdec-5.0/` | Reference these often for invariants and behavior, but do not add runtime/build dependencies, bindings, or copied implementation shortcuts |

## Core Rules

1. Fix behavior at the canonical owner, not downstream UI/surface layers.
2. Prefer algorithmic CFG / dom / postdom / SCC facts over lexical or binary-specific shortcuts.
3. Use typed contracts; do not invent parallel telemetry payloads outside `NirBuildStats`.
4. Keep behavior deterministic when outputs feed snapshots, metrics, or benchmark comparisons.
5. Large refactors are acceptable when they reduce long-term complexity and tighten ownership.
6. Do not hardcode repository-local resource paths in code; when `/Users/sjkim1127/Fission/utils` has reusable signatures, type info, benchmark support data, or other checked-in resources, route access through existing `PathConfig`, `PATHS`, `resource_roots`, or utility loaders instead of embedding absolute paths.
7. Treat `/Users/sjkim1127/Fission/vendor` as a reference corpus only: consult it often for algorithms, invariants, and expected behavior, but keep Fission-owned Rust implementations dependency-free from that tree.
8. **ISA-agnostic semantic rules** ([`docs/adr/0009-isa-agnostic-semantic-rules.md`](docs/adr/0009-isa-agnostic-semantic-rules.md)): optimize measurement on x86/x86-64, but implement register/loop/join/return/cmov-class logic as shared CFG and ABI-*slot* invariants. Put ISA differences in cspec, register namer, calling-convention tables, and SLEIGH — not as copy-pasted control-structure cores gated on `X86_32` / mnemonic / EBP offset alone.
9. **Program metadata ownership** ([`docs/adr/0010-typed-program-metadata-substrate.md`](docs/adr/0010-typed-program-metadata-substrate.md)): loader facts flow into the immutable `fission-analysis-db` snapshot. Do not add new parallel program-fact maps to `fission-pcode`, CLI, or UI layers.
10. **Measurement-only quality claims (hard rule):** Decompiler quality claims — semantic **or** readability/pseudocode — are valid **only** when backed by **measured** benchmark/decompilation evidence on real corpus binaries (or an explicit focused row from that corpus). Synthetic unit tests, hand-written IR fixtures, and “common O0 shape” intuition may land as **mechanical** code + regression tests, but **must not** be described as quality improvement, readability wins, or release-note quality gains without a before/after measurement. The benchmark is DecBench ([`docs/BENCHMARK_DECBENCH.md`](docs/BENCHMARK_DECBENCH.md)), measured on the **full dataset** (`~/fission-benchmark/decbench-full/sweep.py`, every config); the 250-function sample-set gate is only the quick pre-check. Caches are disabled (`FISSION_BENCHMARK_NO_CACHE=1`). Estimates, assumed patterns, and nextest-only green are not quality evidence. **DecBench measures similarity to the source, not correctness** ([ADR 0018](docs/adr/0018-decbench-is-not-a-correctness-oracle.md)). It is reported for every output-changing PR, but a claim that output became *correct* needs correctness evidence: native replay, `fission_cli verify --tier ground-truth`, manuals, DWARF/PDB, call sites counted by instruction address, or the raw diff. A fix that makes wrong output right may land with a worse DecBench score when the PR shows that evidence and explains the regression.

## Post-Audit Stabilization Workflow (Fission 1.0)

The eleven-perspective audit is complete; its findings are not automatically
reproduced or fixed. The development priority is now implementation,
verification, and stabilization, coordinated by
[#1293](https://github.com/fission-systems/Fission/issues/1293). Keep the
architecture/ADR ownership in #1168 and the release go/no-go criteria in
#1121. Do not treat this workflow as a new release gate replacing either one.

### Priorities and scope

- Correctness before Performance.
- Evidence before Assumption.
- Measurement before Optimization.
- Isolation before Parallelization.
- Validation before Publication.
- Explicit Unknown before False Certainty.

Prioritize reproduced false proofs, semantic corruption, evidence fabrication,
data loss, hangs, and unbounded resource retention within supported workflows.
Select work by risk and dependency, not issue number or closure count. Resolve
soundness and essential reliability prerequisites before promoting dependent
optimizations, experiments, or platform capabilities. Independent fixes can
proceed in parallel. Defer unrelated feature expansion; keep explicitly approved
research experiments isolated from the default production pipeline. Preserve
the CLI-only, Rust-native, agent-first ownership boundaries.

### Implementation and validation loop

For each implementation task:

1. **Inspect and scope.** Read the linked issue, current code, nearest child
   AGENTS, and related ADRs. Check whether the finding still applies and whether
   another change already addresses it. Record the base revision, canonical
   owner, affected contracts, and relevant prerequisite issues.
2. **Reproduce.** Capture the failing behavior with a minimal, deterministic
   reproducer or failing regression test where feasible. For preventive work
   or non-reproducible findings, state the evidence and limitation explicitly.
   Do not invent a failure or mark an unconfirmed finding as reproduced.
3. **Explain.** State the root cause, violated invariant, supported scope,
   and expected failure behavior before editing production code.
4. **Implement.** Fix at the canonical owner with the smallest coherent change.
   Avoid unrelated refactors and duplicate state, preserve compatibility and
   existing safeguards, and make partial/unsupported/cancelled outcomes explicit.
5. **Verify independently.** Choose an oracle appropriate to the changed
   contract (specification, native execution, independent reference model,
   property/metamorphic test, or checked witness). Where applicable include
   negative controls and ensure shared lifter/solver assumptions do not create
   circular proof. A timeout, unsupported operation, or incomplete exploration
   is not proof of equivalence.
6. **Regress and measure.** Run the relevant crate and integration checks,
   fault-injection tests for lifecycle changes, and the existing benchmark
   gates when required. Performance changes require reproducible before/after
   measurements on comparable configurations. The Decompiler Quality Loop,
   full-dataset DecBench requirement, AI Overfit Firewall and CI/release gates
   below remain in force.
7. **Submit for review.** Link the issue and report the invariant, reproducer,
   implementation scope, commands and actual results, remaining limitations,
   and any observed regressions. Identify checks not run and why; do not
   represent pending CI or unmeasured results as passed. Do not merge solely
   because an AI-generated patch compiles.
8. **Close only with evidence.** After the fix is merged and the applicable
   acceptance criteria are verified, record the commit and validation evidence
   on the issue. If a blocker remains, retain explicit partial status; do not
   close the umbrella roadmap merely because this workflow has been adopted.

The success criterion is a trustworthy, reproducible, bounded analysis core,
not LOC, commit count, feature count, issue count, or benchmark rank alone.

## Anti-Patterns

- Do not patch semantic gaps only in printer/UI output.
- Do not add one-off binary-specific shortcuts without invariant-based guards.
- Do not duplicate the same metric definition across pcode and benchmark scripts.
- Do not treat `fission-cli` as a semantic repair layer.
- Do not treat benchmark/reporting scripts as semantic repair layers.
- Do not bypass `PathConfig`, `PATHS`, `resource_roots`, or related helpers by embedding `/Users/sjkim1127/Fission/utils` directly in implementation logic.
- Do not link against, shell out to, bind to, or otherwise depend on `/Users/sjkim1127/Fission/vendor` code in production paths.
- Do not claim success from one targeted test if crate-level regression remains.
- Do not grow parallel x86-32 / x64 / ARM copies of the same materialize, loop-carried, join, or short-circuit rule; restate once as a common invariant and supply ISA data only through models (cspec/CC/SLEIGH).
- **Do not invent presentation/structuring “quality” passes from synthetic shapes alone** (e.g. hand-built `while`→`for` IR) and call the work a decompiler improvement. Without a measured motivating row (or multi-row sample) and a remeasured after, report only “mechanical change / test coverage.”
- **Do not use unit-test green as a substitute for benchmark measurement** when stating that pseudocode became more readable or more correct.

## Build / Test Commands

```bash
# CLI
cargo build -p fission-cli --release

# Common decompiler validation
cargo nextest run -p fission-pcode
cargo check -p fission-pcode
cargo check -p fission-decompiler

# DecBench quick pre-check: 250-function sample-set gate (see docs/BENCHMARK_DECBENCH.md)
~/fission-benchmark/.venv-decbench/bin/python scripts/quality/decbench_sample_set_gate.py check --cli target/release/fission_cli

# DecBench full-dataset sweep: what quality claims cite
~/fission-benchmark/.venv-decbench/bin/python -I ~/fission-benchmark/decbench-full/sweep.py run \
  --cli target/release/fission_cli --repo "$PWD" --label <name>

# Formatting -- CI runs exactly this
cargo fmt --all --check
```

The workspace is fully rustfmt-clean and CI enforces it across every file, so
run `cargo fmt --all` and commit the result with your change. Two things to
avoid: do not run `cargo fmt --all` as a separate sweep on top of unrelated
work (it used to surface three hundred files at once, which is why the debt
was paid off in one commit -- see `.git-blame-ignore-revs`), and do not format
by invoking `rustfmt` on a single file, which follows `mod` declarations into
submodules and rewrites files you never touched.

## Workflow Bias

- Use `cargo nextest run` as the default local Rust test runner. Use `cargo test` only when checking doctests, harness-specific behavior, or when nextest is unavailable.
- For NIR/structuring changes: targeted nextest filter → `cargo nextest run -p fission-pcode` → `cargo check -p fission-pcode`.
- For orchestration / Rust-Sleigh glue: also `cargo check -p fission-decompiler` (and the CLI as needed).
- For resource path / bundle resolution changes: `cargo nextest run -p fission-core` and smoke `fission_cli resources status`.
- If benchmark/reporting changes: validate with the DecBench gate (`scripts/quality/decbench_sample_set_gate.py`) and the full-dataset sweep; sweep results live under `~/fission-benchmark/decbench-full/runs/`, not in the repository.
- Use `.github/workflows/ci.yml` and `ci-heavy.yml` as CI source of truth.
- Release gate policy: [`docs/CI_RELEASE_GATES.md`](docs/CI_RELEASE_GATES.md) (L0 Fast / L1 Heavy / L2 Release E2E / L3 CD).
- Ship Git **release tags** via `.github/workflows/release-tag.yml` (`Release Tag (CI green)`). It requires successful **ci.yml main push** and **ci-heavy scheduled or dispatched** runs on the exact candidate SHA, then runs **release-e2e** and builds and smoke-tests the archives through **cd.yml**. It publishes those verified artifacts after creating the tag. Do not auto-tag every `main` push.

## Decompiler Quality Loop

Use this loop for source-semantic or pseudocode-quality work, especially when a concrete row/function motivated the change.

### Measurement-only gate (non-negotiable)

**Quality claims require measurement. Unit tests alone never prove quality.**

| Allowed as | Requires |
|---|---|
| Mechanical change (code + synthetic invariant tests) | nextest / crate checks; **no** “quality improved” language |
| Correctness claim (output was wrong, now right) | Correctness evidence ([ADR 0018](docs/adr/0018-decbench-is-not-a-correctness-oracle.md)): native replay, `verify --tier ground-truth`, manual, DWARF/PDB, call-site counts by instruction address, raw before/after hunks; **plus** the full-dataset sweep reported as-is. A score regression is explained, not a veto |
| Semantic quality claim | Anchored row(s) for the motivation (a DecBench row, a `verify` divergence, or a native replay mismatch), then a **full-dataset sweep** before/after: output-hash `diff` first, GED / type deltas on the rows whose output changed, total distance next to the perfect count |
| Readability / HIR presentation quality claim | Anchored real-binary decomp (or DecBench row) before/after; not fixture-only |
| Official ranking / release claim | Full-dataset DecBench sweep from a tagged build, compared with the published tools on the same rows (never a local run as an official ranking) |

**Order of work (always):**

1. **Observe** on a real binary/row (DecBench row, gate `--dump`, or `fission_cli decomp`).
2. **Record baseline** (scores, gotos, line size, fail category, snippet).
3. **Generalize** to an invariant (no function/address special cases).
4. **Implement** at the canonical owner.
5. **Remeasure** the same path with caches disabled.
6. **Report** “quality improved” only with evidence: correctness evidence for a correctness fix, a measured move for a similarity claim; otherwise “landed mechanical / tests only.” DecBench numbers are reported either way, never as the proof of correctness.

If there is no measured baseline, **do not start a “quality” implementation**. Optionally land a pure refactor or test harness, but label it non-quality.

Applies to **NIR, HIR presentation, structuring, normalize, materialize, SLEIGH, and printer** surfaces. Printer/UI still must not be the semantic owner; measurement still applies if readability is claimed.

### Mandatory DecBench measurement

Quality claims are measured on the **whole** DecBench dataset (96,103
functions, configs `unoptimized`, `optimized`, `inlined`, `large`,
`sample-set`) against a **local** Fission build, not on the 250-function sample
set and not with `fission_cli` one-shots or unit tests. Full procedure:
[`docs/BENCHMARK_DECBENCH.md`](docs/BENCHMARK_DECBENCH.md).

```bash
cargo build -p fission-cli --release
PY=~/fission-benchmark/.venv-decbench/bin/python
# quick pre-check (minutes): refuses a drop on the sample set
$PY scripts/quality/decbench_sample_set_gate.py check --cli target/release/fission_cli --dump /tmp/after.json
# the measurement a claim cites (hours): every function of every config
$PY -I ~/fission-benchmark/decbench-full/sweep.py run --cli target/release/fission_cli --repo "$PWD" --label after
$PY -I ~/fission-benchmark/decbench-full/sweep.py diff ~/fission-benchmark/decbench-full/runs/before ~/fission-benchmark/decbench-full/runs/after
$PY -I ~/fission-benchmark/decbench-full/sweep.py report ~/fission-benchmark/decbench-full/runs/after
```

- The sample-set gate is the quick pre-check, not evidence: a claim cites the
  full-dataset sweep, per config, with the published tools on the same rows.
- Compare **output hashes before scores** (`sweep.py diff`); a score delta on a
  row whose text did not change is scorer noise.
- Report **total distance next to the perfect count**; the count alone has
  ranked a regression as a win. Report failures, and CPU time next to wall time.
- Build once, then measure; do not rebuild `fission_cli` while a run is in flight.

Never promote local runs to an official ranking. The docker multi-decompiler
loop ([`docs/BENCHMARK_DOCKER.md`](docs/BENCHMARK_DOCKER.md)) is retired.

## AI Overfit Firewall

AI-assisted decompiler quality work must not leak benchmark identity into the
proposal prompt. When asking another model for review or implementation ideas,
describe only the structural failure pattern, owner evidence, invariant
candidates, forbidden shortcuts, and validation matrix. Redact function names,
addresses, binary paths, corpus row ids, and compiler tuples unless the prompt is
strictly local and will not be used for implementation advice.

Use [`docs/templates/AI_DECOMPILER_REVIEW_PROMPT.md`](docs/templates/AI_DECOMPILER_REVIEW_PROMPT.md)
for external or cross-model review prompts. The template exists to keep prompts
about invariants, not benchmark rows. Ghidra may be cited as a cleanroom
reference for algorithms and edge cases, never as proof of correctness
([ADR 0017](docs/adr/0017-ghidra-is-not-an-oracle.md)), and do not ask models to reproduce Ghidra-specific
output style or known readability artifacts such as side effects hidden inside
conditions.

Do not repeatedly tune against dev/holdout corpus results. A semantic quality
claim needs either an unseen patch-validation-pool signal or a synthetic
invariant test in addition to the focused motivating row. Patch validation pool
results are go/stop regression evidence only; they must not be promoted into
dashboard rankings or used as a new tuning target.

This firewall is an architecture contract before it is a CI rule. Static scans
can audit for leaks, but the required gate is that any AI suggestion or benchmark
observation must be restated as an owner-native invariant in the canonical
semantic layer before production code changes.

### Fitting the metric instead of the row

The Pass-Gate Lint catches the crude form -- a `func_name ==` or an
`addr == 0x...` in the pipeline -- and that is the only form it can catch. The
form that has actually cost this project is different and passes every static
scan, because it contains no benchmark identity at all: **optimising a metric
along an axis the metric cannot see.**

It has happened once already. Goto density scored a structuring that replaced
38 transfers with 2,743 short-circuit operators as a clean win, because the
counter could not see that `&&` is a branch. Nothing about that change named a
row. See `structuring_quality`'s module docs, where every axis is there because
some driver scored an improvement while emitting something worse.

The same shape is available on structure distance. DecBench's graph edit
distance prices node count and degree and *nothing else* -- statement content
costs zero -- so merging basic blocks, replacing every `if`/`else` with a
ternary, or duplicating code to avoid a join all improve the score and can
make the output worse.

**The test, applied to every quality change:**

> State why the new output is better without mentioning the metric. If the
> sentence cannot be written, the change is fitting the metric, not fixing a
> defect.

Two worked examples. *"The old output returned an indeterminate value on a
path that does not exist"* passes and needs no benchmark to be true.
*"This emits fewer basic blocks"* fails -- fewer than what, and better for
whom?

### Row-driven investigation without row fitting

Reading individual benchmark rows to find defects is correct and encouraged;
the risk is stopping at the row. When investigating a bucket of near-miss
functions:

1. **Act only on a shape that repeats** -- at minimum three functions across
   two different programs, and not all at the same optimisation level. One
   function exhibiting something is a note in the proposal, not a change.
2. **Justify without the metric**, per the test above.
3. **Measure on a split the benchmark does not score.**
   `~/fission-benchmark/corpus/scale` carries published source CFGs for 64,231
   functions that are not the scored sample-set, which makes fitting the
   scored rows structurally impossible rather than merely discouraged. Note
   its own bias, though: it is `-O0` with symbols, so a shape visible only
   there is a different overfit -- hence the mixed-optimisation requirement in
   rule 1.

### Pre-implementation gate

Before adding production code for builder, materialize, normalize, structuring, type/data recovery, **or HIR presentation readability** fixes intended as quality work, fill out [`docs/templates/DECOMPILER_CHANGE_PROPOSAL.md`](docs/templates/DECOMPILER_CHANGE_PROPOSAL.md) as required by [`docs/adr/0006-decompiler-quality-change-gate.md`](docs/adr/0006-decompiler-quality-change-gate.md). The proposal must show **measured** row anchor, owner proof, invariant proof, and validation matrix before implementation starts. **No row measurement → no quality implementation.**

Default to extending the existing owner/pass. Add a new pass, helper, or metric only when the proposal shows that no existing owner covers the invariant. Do not claim success from a targeted test alone; crate-level tests, focused row rerun, and DecBench regression checks are part of the quality claim.

1. **Anchor the row (measured):** record the source file, binary, address, function name, current behavior status, case pass count, semantic/static scores, top missing/extra features, and a short pseudocode/goto baseline. Synthetic IR alone is not an anchor.
2. **Find the owner:** prove whether the bug belongs to SLEIGH/raw p-code, NIR materialization, type recovery, structuring, cleanup, HIR presentation, printer, or benchmark scripts. Fix behavior at that owner (not printer-only semantic patches).
3. **Add focused coverage:** add or update the smallest targeted Rust/Python test that captures the invariant. Synthetic tests are necessary but not sufficient for decompiler-quality claims.
4. **Make the scoped change:** keep production changes invariant-based, not function/address/sample-specific. Do not add runtime/build dependencies on `vendor/` reference tools.
5. **Run local checks:** run the targeted test first, then the relevant crate checks/builds from the Build/Test section. If a known unrelated test is already failing, call it out explicitly.
6. **Run the focused benchmark / real decomp:** rerun the exact DecBench row (gate `--dump`) or the same PE/function decomp with no stale decompilation or behavior cache. Compare behavior status, case progress, stdout/stderr, line/byte size, static feature gaps, and readability proxies (e.g. goto count) as applicable.
7. **Check regressions:** after a focused improvement, run the broader DecBench measurement on the same rows. Existing perfect rows must not regress, and total distance and type scores should not get worse without an explicit tradeoff. A correctness fix is such a tradeoff: when making the output right costs score, say what the output now prints and why the source-shaped score dropped ([ADR 0018](docs/adr/0018-decbench-is-not-a-correctness-oracle.md)).
8. **Report both bars:** distinguish “mechanically changed” from “quality improved.” A merged test-only, telemetry-only, or fixture-only presentation change is **not** a quality fix unless the measured row (or agreed multi-row sample) moves.

## Regression-Prevention Workflow Prompt

Use this prompt as the standing operating model for decompiler-quality cycles:

```text
Current priority order:

1. x86 / x86-64 decompilation correctness and readable pseudocode quality.
   SLEIGH lift coverage is now good enough that day-to-day quality work should focus on source-semantic correctness and human-readable pseudocode for x86/x86-64 sample binaries.
   The goal is not only mechanically correct p-code/NIR, but final output that reads like useful C pseudocode.

   Focus areas:
   - control-flow recovery
   - if / else / switch / loop / break / continue structuring
   - pointer, array, struct, and field-access expressions
   - calling convention, parameter, and local-variable recovery
   - removal of unnecessary temporaries
   - C-friendly recovery of pointer arithmetic into array/index/field forms
   - return-value, accumulator, and loop-induction-variable cleanup
   - function-level pseudocode readability compared with Ghidra

2. Type and data abstraction.
   Improve struct, pointer, array, field access, calling convention, parameter, and local recovery at the NIR/HIR semantic layer, not by output-only substitution.

3. Large and hard function structuring.
   Improve small sample functions first, then extend to complex x86/x86-64 functions using CFG, dominance, post-dominance, SCC, dataflow, and fixed-point analysis.

4. Maintain SLEIGH lift correctness and prevent regressions.
   Do not add manual mappings in the SLEIGH engine. Keep `.sla` ConstructTpl execution as the success source, and do not grow legacy token cursor, BoundOperand fallback, or compatibility-classifier debt.
   When SLEIGH changes are necessary, validate row-level raw p-code parity first, then the canonical gate, then benchmarks.
   Parity means against Fission's previous output; each intended difference needs native-execution or architecture-manual evidence.
   Upstream Ghidra specs can be wrong (AArch64 FCVT/FRINT, #834/#838); fix them as `utils/sleigh-specs/README.md` records ([ADR 0017](docs/adr/0017-ghidra-is-not-an-oracle.md)).

5. FID/name recovery.
   Gradually improve packed `.fidb`, exact hash inputs, and program seeker coverage relative to Ghidra Function ID / signature / symbol ecosystems.

6. Architecture and file-format breadth.
   Expand to ARM, MIPS, PPC, ELF, and Mach-O advanced cases only after x86/x86-64 quality is strong enough.

Required principles:

1. Improve sample binary quality before real-world binary breadth.
2. Treat Ghidra as a cleanroom reference for algorithms, invariants, and edge cases, not as a correctness oracle: Fission lifts from Ghidra's SLEIGH specs, so their errors make both tools agree on the wrong answer. Correctness comes from native execution, architecture manuals, and source-level ground truth ([ADR 0017](docs/adr/0017-ghidra-is-not-an-oracle.md)).
3. Default to zero new production dependencies.
4. Prefer CFG, dominance, dataflow, fixed-point, and constraint-based reasoning over brittle pattern matching or temporary heuristics.
5. Avoid overfitting to one ISA/compiler, while keeping x86/x86-64 as the current optimization target.
   **Optimization target is not rule language:** motivate on m32/x64 rows, implement CFG/register/ABI-slot invariants that another ISA can reuse (ADR 0009).
6. Consider Rust libraries only when a confirmed long-term bottleneck cannot be solved internally. Do not add C++ bindings.
7. Prefer long-term maintainability and generalizable architecture over short-term output patches.
8. Make proposals and implementations valid across multiple future quality cycles, with explicit observability and verification.
9. Do not use estimates as evidence. Base claims on measured, reproducible data only (Core Rule 10).
10. The final success criterion is actual improvement in DecBench measurements — never synthetic fixtures alone.
11. If a change was not motivated by a measured row, do not implement it as a “quality” pass; either measure first or ship it as non-quality mechanical work without quality claims.
11. Prefer extending shared helpers (loop-carried update, same-block forward branch, return-join live-in, unconditional-copy merge) over new end-of-pipeline passes or ISA-local cleanups.

Resource rules:

- `/Users/sjkim1127/Fission/utils` contains reusable Fission resources, type information, signatures, and benchmark support data.
- Prefer existing resource loaders, `PathConfig`, `PATHS`, and resource-root mechanisms over hardcoded paths or duplicate implementations.
- Use `utils` only when it supports a maintainable semantic-layer design, not as a workaround.
- `/Users/sjkim1127/Fission/vendor` and especially `/Users/sjkim1127/Fission/vendor/ghidra/ghidra-Ghidra_12.0.4_build` are reference-only sources.
- Do not add runtime/build dependencies on vendor code, do not copy implementations, and do not add C++ bindings.

Regression-prevention workflow:

1. Start from a concrete DecBench row or small sample function.
2. Record baseline behavior, case pass count, semantic/static score, stdout/stderr, line/byte size, and top feature gaps.
3. Diagnose the canonical owner before editing: SLEIGH/raw p-code, NIR materialization, type/data recovery, structuring, cleanup, printer, or benchmark scripts.
4. Make the smallest invariant-based production change at that owner.
5. Add targeted coverage for the invariant before or with the fix.
6. Run the targeted test, relevant crate checks/tests, and release CLI build when benchmark validation needs it.
7. Run the focused DecBench measurement with caches disabled (`FISSION_BENCHMARK_NO_CACHE=1`) for semantic changes.
8. Compare against the baseline and inspect candidate artifacts, not just aggregate scores.
9. Run the broader DecBench regression check after focused improvement.
10. Report whether quality improved, stayed unchanged, or regressed. Separate test/telemetry changes from semantic fixes.
11. Commit and push intermittently only from `main`, and stage only intended hunks in a dirty worktree.
```

## References

- `docs/architecture/ARCHITECTURE.md`
- `docs/adr/` — architectural decisions (ADR index lives alongside numbered entries)
- [Build and locate the CLI](docs/CLI.md#build-and-locate)
- `README.md`
- `.github/workflows/ci.yml`
- `.github/workflows/ci-heavy.yml`
- `.github/workflows/release-tag.yml`

## NIR vs HIR Policy
There is one output surface ([`docs/adr/0015-single-output-surface.md`](docs/adr/0015-single-output-surface.md)).
1. **NIR (Normalized Intermediate Representation)**: the internal representation. Must be 100% semantically identical to the source; owns semantics and the structuring choice (selected on `SelectionAxis::NodeEstimate`). It is no longer a printed surface.
2. **HIR (High-level Intermediate Representation)**: the tree after presentation, and the only output (`code`). It may prioritize readability and C-friendly structure, but only under the presentation contract: same evaluation count/order for effects, clone-only polish, structural firewall. See [`docs/adr/0011-hir-presentation-contract.md`](docs/adr/0011-hir-presentation-contract.md) and `crates/fission-pcode/src/render/AGENTS.md`. Because this text is what gets scored, report total distance next to the perfect count when making quality claims.
