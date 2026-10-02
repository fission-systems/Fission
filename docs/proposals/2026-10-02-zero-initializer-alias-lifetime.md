# Decompiler Change Proposal: zero initializer alias lifetime

## 1. Measured baseline anchors

Base: main `7080acbf660d8a8011bb9868c036ad776877adab`; immutable baseline CLI
SHA256 `e13940b77821eeb385c48fe475dec270c4453e9c734e75272177184115556b83`.
The measured 250-function baseline is Union 73, GED perfect 65/240,
types perfect 16/228, byte perfect 1/158. These are local results.

| Program / variant / function | Binary / address | GED | Types | Byte |
| --- | --- | ---: | ---: | ---: |
| kmod / O0 / index__checkstring | bin_197.elf / 0x9757 | 3 | 1 | .263158 |
| coreutils pr / O0 / balance | bin_157.elf / 0x5098 | 7 | .8 | .164179 |
| gnutls certtool / O2-noinline / yyparse | bin_058.elf / 0x253e0 | 215 | 0 | 0 (does not compile) |

No executable semantic oracle is available for these rows; semantic case counts
are unmeasured. The failure is observed value loss in the real decompilation.
`index__checkstring` loads a byte, assigns the result carrier, then emits a zero
assignment before and inside `while (carrier)`. `balance` reads a global count,
then overwrites it with zero before the loop exit comparison. `yyparse` copies
a loaded stack-guard value through a carrier into a local; type inference
changes that copy into zero. These repeat across three programs and two
optimization levels. Extra observations are investigation evidence, not scores.

Baseline command: immutable CLI `decomp BINARY --addr ADDRESS --json --no-db`.
A diagnostic-only build captured builder output and changed normalize passes;
the instrumentation was removed before implementation. Evidence is under
`benchmark/artifacts/decbench/2026-10-02-zero-alias-lifetime/` in the primary
checkout. `fission-zero-alias-investigation.json` preserves the repeated shape.

## 2. Owner proof

Owner: existing type/data recovery helper
`types/type_infer/pointer_roles.rs::rewrite_scalar_zero_alias_assignments` in
`fission-midend-normalize`.

```text
Builder: carrier = loaded_byte; carrier = (u64)carrier;
Cleanup: carrier = loaded_byte; carrier = carrier;
Type inference: carrier = loaded_byte; carrier = 0;

Builder/cleanup: carrier = loaded_guard; saved_guard = carrier;
Type inference: carrier = loaded_guard; saved_guard = 0;
```

`zero_initializer_aliases` currently collects declaration initializers only,
then substitutes every scalar `dst = source` throughout the function. It does
not check writes to source, including source self assignments, or address
escapes. `DefUseMap` and `collect_address_taken_locals` already own those facts.
The first incorrect rewrite occurs in type inference, before rendering.

Reference consulted: vendored Ghidra `RuleCollapseConstants::applyOp` requires
an operation to be collapsible. The invariant is constant value at the use,
not a declaration initializer irrespective of subsequent definitions. No
reference code is copied or used as a runtime/build dependency.

## 3. Invariant and generality

A declaration's zero initializer can justify a function-wide zero alias only
for a local with no body write and no address escape, including escapes in
other declaration initializers. Parameters are runtime inputs, not proven
constant locals. Mutable carriers retain their current value. Existing SCCP
may prove narrower, use-specific constant lifetimes independently.

This change preserves loaded values and call results instead of replacing
them with a stale entry value. That justification does not mention a metric.
The condition uses shared def-use/address facts, with no ISA, ABI register name,
compiler tuple, function/address guard, new pass, dependency or telemetry.

## 4. Ownership and risk

Extend the existing helper using `DefUseMap::build` and the existing
address-taken collector. Conservative rejection can leave extra copies where
SCCP cannot prove a constant, but cannot invent a new zero value. Existing
immutable pointer-zero-to-scalar rewriting must remain covered. Preserve
pointer destinations, branch writes, loop writes, loop-carried self copies,
address-taken sources, initializer escapes, and runtime parameters.

## 5. Validation matrix

- Targeted normalize tests: immutable zero (existing), mutable source,
  self-copy after load, branch/loop writes, address escape, initializer escape,
  parameter input. Use arbitrary names and ISA-independent typed IR.
- Full normalize crate and fission-pcode nextest gates; normalize/pcode/decompiler
  checks; cargo fmt --all --check; release CLI build.
- Fresh 250-function generation through the standalone DecBench adapter;
  fresh GED/type/byte metrics with caches disabled; compare all identities to
  the 73/250 local baseline. No score improvement is assumed in advance.
- Mandatory external local Docker measurement with frozen unscored validation
  pool and external dev smoke; exact anchors also compared on Linux.
- Inspect output changes, compilation rates, perfect-row losses and partial
  score declines. Do not publish local results to official latest/Pages.

## 6. AI firewall

No external or cross-model implementation advice. Local investigation used
real rows to diagnose an existing owner. Production rule is shared immutable
local-value evidence, independent of row identity. Synthetic invariants and
unscored frozen validation pool are separate regression evidence.

## 7. Review

Production implementation contains no benchmark identity. No printer or
adapter semantic repair is proposed. Remeasurement below does not establish a ranking improvement; test green alone
is insufficient.


## 8. Remeasured result and explicit tradeoff

Fresh standalone DecBench generation: 223 binaries, 250 functions, zero CLI
errors. All 250 identities are explicitly supplied per metric (including nulls),
with no retained public Fission scores. Type matching was rerun for both the
baseline and candidate using the same matcher; baseline scores reproduced
exactly. Caches/checkpoint resume were disabled for measurements.

| Measurement | Baseline | Candidate |
| --- | ---: | ---: |
| Union | 73/250 | 73/250 |
| GED perfect | 65/240 | 65/240 |
| Types perfect | 16/228 | 16/228 |
| Byte perfect | 1/158 | 1/158 |
| Recompiles | 124/158 | 124/158 |
| Mean type match | .261269 | .260075 |
| Mean byte match | .204823 | .204993 |

Nine scored function outputs changed. No perfect row was gained or lost;
all GED distances are unchanged. Partial type scores decreased in two rows
(.8 to .5 and 1/6 to 1/9) and increased in one (0 to 1/12). Byte scores
increased in two rows and decreased in four. The accepted tradeoff is removal
of an incorrect function-wide constant substitution: mutable loaded/call-result
values must survive even when preserving them exposes other type-recovery
limitations. This is a demonstrated defect correction with **no measured
Union/rank improvement**, not a claim of general decompiler quality gains.

The frozen 2026-09-23 rival snapshot projects rank 8 at 29.2%; Kuna remains
88/250 and needs 89/250 to exceed it. No official latest/Pages publication.

Validation: targeted 8/8; full normalize+pcode 1611/1611 with one existing
skip; normalize/pcode/decompiler checks; native release CLI; Linux release CLI;
workspace format check. Linux Docker before/after decompilation of all three
anchors confirms that loaded byte/count/guard copies survive instead of becoming
zero. There is no executable semantic oracle for these anchors.

Mandatory external local Docker: 6 dev rows retain their six existing compile
errors. A fixed 32-row unscored, zero-overlap validation pool was rerun on both
baseline and candidate. Three output units changed; three existing adapter
errors persist. One additional adapter error is explicitly unresolved by the
harness: its `len(code) >= 7900` heuristic classifies a complete, single-function
7987-character output as truncated (baseline 7816 characters). Independent C
parser inspection reports one function and no syntax errors in both outputs.
This is a harness limitation, not a successful 32/32 semantic regression gate;
the O0-only pool has no executable behavior oracle and is not a tuning target.

Evidence: primary checkout
`benchmark/artifacts/decbench/2026-10-02-zero-alias-lifetime/README.md`.
Production patch SHA256:
`a9c0f3b08d846be567e00a7ee2ed09ec95e2126ab821a22c28a6c006cb690b31`.
Native measured CLI SHA256:
`00549ea9563b2ee4b67dcda30db6545e3febd7f715fd6b50abbd289d15443b43`.
Docker source fingerprint:
`1d4463bddc03b12a279807ac4678d533ce3764d1aee5bd2b612d8cc6ffedca57`.
