# DecBench large reducible functions: preserve structural admission

## 1. Baseline Row Anchor

- Binary: `O0/openssh-portable/ssh`
- Function: `process_config_line_depth`
- Address: `0x18f1d`
- Corpus row or benchmark command: local DecBench HF `unoptimized` full run; direct reproduction with `target/release/fission_cli decomp ... --addr 0x18f1d --layer nir --json`
- Current output summary: GED `1397`, type match `0.18`; source CFG `590` nodes versus Fission `52`; 331 rendered lines.
- Semantic cases passed / total: not available in the DecBench GED/type-only local harness; the row is used here as a structural quality anchor.
- Failure category: `structuring_partial`; `forced_linear_structuring_count=1`, `structuring_force_linear_extreme_budget_count=1`.
- Relevant benchmark/static/readability observations: the x86-64 switch has 101 selector values. The indirect-target fixed point resolves 100 targets and lifts 583 blocks / 12,878 p-code operations, but the standalone `total_ops > 10_000` admission clause selects `ExtremeBudget`, so graph-collapse structuring is never attempted. The same admission outcome appears on the paired `ssh-keysign` row (GED `1397`), `sshd/process_server_config_line_depth` (GED `1373`), `bzip2/BZ2_decompress` (GED `613`), and zlib `inflate` rows.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code:
- [ ] Builder/materialize:
- [ ] Normalize:
- [x] Structuring:
- [ ] Type/data recovery:
- [ ] Printer:
- [ ] Benchmark/automation:

Evidence:

```text
FISSION_JT_TRACE=1 FISSION_PREVIEW_DIAG=1 fission_cli decomp ... --addr 0x18f1d

[JT] entry=0x18f1d round_blocks=33 dispatches=1 targets=100
[JT] entry=0x18f1d round_blocks=583 dispatches=0 targets=0
[DIAG] structuring start: blocks=583 edges=848 force_linear=true
[DIAG] structuring linear done: ... admission=ExtremeBudget
```

The jump-table resolver has already supplied the case targets. The loss occurs
at `fission-midend-structuring::admission::decide_structuring_admission`,
before SESE/graph-collapse structuring. The emitted body is therefore the
linear fallback, not a failed proof of the recovered CFG.

## 3. Generality / Invariant Proof

Generalized rule:

```text
Do not reject graph-collapse structuring solely because the number of p-code
operations is high. A large operation count can come from many independent,
reducible switch arms. Admission must couple operation budget to structural
complexity (CFG size/density and SCC shape), while retaining a fail-closed
fallback for genuinely extreme graphs.
```

ISA-agnostic check:

- [x] Production condition is not gated on an ISA, address, function name, or binary.
- [x] Jump-table recovery remains a shared CFG fact; no architecture-specific structuring rule is added.
- [x] The regression test will state only admission metrics and reducibility facts.

Comparable coverage:

- `O0/openssh-portable/ssh-keysign/process_config_line_depth`: same 101-arm switch shape, GED `1397`.
- `O0/openssh-portable/sshd/process_server_config_line_depth`: large switch family, GED `1373`.
- `O0/bzip2/bzip2/BZ2_decompress` and zlib `inflate`: independent high-operation real-binary rows that currently hit the same extreme-budget path.
- Synthetic invariant test: reducible high-operation / bounded-CFG admission is accepted; genuinely oversized or structurally dense graphs remain rejected.

## 4. Risk And Ownership Check

- Existing pass/owner that might already own this behavior: `fission-midend-structuring::admission`.
- Shared analysis/substrate candidate:
  - [x] CFG / dominance / postdominance fact
  - [ ] Def-use / reaching-definition fact
  - [ ] Type constraint / calling-convention fact
  - [ ] Memory alias / stack-slot fact
  - [ ] P-code semantic contract
  - [ ] None; owner-local rule is justified
- Why extending that owner is sufficient: admission already receives block count, operation count, edge count, predecessor fan-in, SCC irreducibility, and maximum SCC size. The rule can be corrected without changing p-code, builder, or printer contracts.
- Possible interaction with existing normalize/structuring passes: graph collapse may spend more time on large functions and may expose existing switch/region proof failures. Keep the fallback and measure wall time; do not force a prettier candidate past a failed proof.
- New or changed owner-to-owner dependency:
  - [x] None
- Telemetry impact, if any: existing admission/forced-linear counters should show the changed path; no new metric is required initially.
- Known cases that must not change: explicit force-linear previews; irreducible and structurally dense/extreme graphs; existing admission unit tests for 601 blocks and explicit overrides.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-midend-structuring -- admission`
  - Expected signal: bounded structural complexity with high operation count admits graph collapse; existing extreme cases remain linear.
- [ ] Crate-level gate:
  - Command: `cargo nextest run -p fission-pcode`
  - Expected signal: no regression.
- [x] Focused benchmark row:
  - Command: local DecBench HF harness with `ONLY=openssh-portable/ssh,openssh-portable/ssh-keysign,openssh-portable/sshd,bzip2/bzip2,zlib` and fresh decomp/GED caches.
  - Expected row-level improvement: lower GED / larger recovered CFG on the anchored rows, or a documented no-change if graph-collapse proof rejects them.
- [ ] Smoke or automation sample:
  - Command: local DecBench sample of the affected binaries plus the existing source-semantic smoke lane.
  - Expected no-regression signal: no previously scorable row loses decompilation or type output.
- [x] Optional related checks:
  - Command: `cargo fmt --all --check`, `cargo check`, `cargo build -p fission-cli --locked --release`, `git diff --check`.
  - Expected signal: clean.
- [x] Boundary audit, if a new pass/helper/dependency was added:
  - Command: not applicable; no new pass or dependency is planned.
  - Expected signal: existing owner boundary remains intact.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
  - [ ] Yes, using `docs/templates/AI_DECOMPILER_REVIEW_PROMPT.md`
- Information exposed in an external AI prompt: none.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from dashboard or benchmark-only
  edits:
  - [x] Confirmed
- Any new metric/pass/helper does not duplicate an existing owner:
  - [x] Confirmed; the change is admission policy only.
