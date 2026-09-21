# Copy-Alias Merge Must Respect Later Value Divergence

## 1. Baseline Row Anchor

- Issue: #113
- Binary: `math_gcc_O2.exe`
- Function: `fibonacci`
- Address: `0x140001530`
- Corpus row or benchmark command:
  `target/release/fission_cli decomp corpus/dev/binaries/c/math_gcc_O2.exe --addr 0x140001530 --no-db --profile quality --layer both`
- Current output summary: the final NIR/HIR masks `r12` in place and then uses
  the masked value for the parity correction.
- Semantic cases passed / total: external DecBench focused row measured with the
  same nine compiler variants before and after the fix: `24/54 -> 30/54`.
- Perfect semantic rows: `3/9 -> 5/9`; `gcc -O2` and `gcc -O3` each improved
  from `3/6` cases to `6/6` cases.
- Failure category: normalize variable-merge aliasing; a copied register value
  is incorrectly treated as the same storage as the source after the copy is
  modified.
- Relevant observations: raw p-code has `EAX = R12D`, `AND EAX, 0xfffffffe`,
  and later `AND R12D, 1`; raw builder HIR preserves the same separation. The
  normalized output instead contains `r12 &= 4294967294` followed by
  `r12 & 1`.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [ ] Builder/materialize
- [x] Normalize
- [ ] Structuring
- [ ] Type/data recovery
- [ ] Printer
- [ ] Benchmark/automation

Evidence:

```text
raw p-code:  Copy EAX <- R12D
            IntAnd EAX <- EAX, 0xfffffffe
            IntAnd R12D <- R12D, 1

raw builder HIR: uVar5 = r12; uVar5 &= 4294967294; rdi -= uVar5;
                 iVar36 = r12 & 1;

normalize after cleanup_stmt_fold: the same uVar5/r12 separation remains.
normalize after variable_merge: uVar5 is gone and r12 &= 4294967294.
```

The first incorrect rewrite is therefore `recovery::variable_merge`'s
`transitive_copy_aliases` path, which applies `rename_vars_in_stmts` to every
use of a direct-copy alias without proving that the two names remain equal for
their complete remaining live ranges.

## 3. Generality / Invariant Proof

Generalized rule:

> A direct copy establishes a merge candidate, not unconditional variable
> identity. If either side is subsequently assigned a non-copy value and the
> other side is read after that assignment, their values coexist and the pair
> must remain distinct.

The rule is a def-use/liveness invariant and does not depend on an ISA,
register name, function, address, binary, or compiler tuple.

ISA-agnostic check ([ADR 0009](../adr/0009-isa-agnostic-semantic-rules.md)):

- [x] Production condition is based on copy/read/write ordering, not an ISA
      or register guard.
- [x] No architecture-specific control-structure rule is added.
- [x] The synthetic test expresses a generic copied-value-then-mutation shape.

Comparable coverage:

- Similar shape 1: a temporary copied from a register and masked before a
  later use of the original register.
- Similar shape 2: either side of a direct copy is reassigned before the other
  side is read.
- Synthetic invariant test: `variable_merge_preserves_source_after_copy_is_mutated`.

## 4. Risk And Ownership Check

- Existing owner: `recovery::variable_merge::transitive_copy_aliases` and its
  copy-merge barrier set.
- Shared analysis/substrate candidate: def-use/read-write ordering; the rule is
  small enough to extend the existing variable-merge candidate filter without
  adding a new public pass.
- Why extending the owner is sufficient: the unsafe rename is created there;
  downstream cleanup and printer layers only observe the already-corrupted
  name.
- Possible interaction: some valid copy coalescing candidates will be
  rejected conservatively when later mutation/read ordering is ambiguous.
  Existing co-occurrence, load-derived, stack-state, and hardware-register
  protections remain unchanged.
- Known cases that must not change: direct copies whose values do not diverge,
  safe temporary coalescing, and existing variable-merge regression tests.

## 5. Validation Matrix

- [x] Targeted invariant test:
  - Command: `cargo nextest run -p fission-midend-normalize variable_merge_preserves_source_after_copy_is_mutated`
  - Expected signal: the source and copied value remain distinct after the
    merge pass.
- [x] Crate-level gate:
  - Command: `cargo nextest run -p fission-midend-normalize`
  - Expected signal: existing variable-merge and normalize tests remain green.
- [x] Focused benchmark row:
  - Command: the baseline decomp command above, plus the matching local
    DecBench row when the release binary is rebuilt.
  - Observed: `gcc -O2` and `gcc -O3` moved from `assertion_fail` at `3/6` to
    `6/6`; the real output keeps `r12 = n - 2` and puts the mask in `uVar5`.
    No function/address-specific logic is present.
- [ ] Smoke or automation sample:
  - Command: `cargo nextest run -p fission-pcode` and the relevant local
    DecBench smoke row.
  - Expected no-regression signal: existing p-code tests and unrelated rows
    remain green.
- [x] Optional related checks:
  - Commands: `cargo check`, `cargo fmt --all --check`, `git diff --check`,
    `cargo build -p fission-cli --release`.

## 6. AI Review / Prompt Firewall

- Was an AI model asked for implementation advice?
  - [x] No
- The investigation and implementation use only repository-local evidence and
  the issue's real-binary reproduction.

## 7. Review Notes

- Production code contains no hardcoded binary/function/address/corpus guards:
  - [x] Confirmed
- The change does not claim semantic improvement from synthetic tests alone:
  - [x] Confirmed
- No new metric, pass, or parallel owner is introduced:
  - [x] Confirmed
