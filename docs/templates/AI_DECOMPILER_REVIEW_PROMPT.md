# AI Decompiler Review Prompt

Use this template when asking another AI model for decompiler-quality review or
implementation ideas. Keep the prompt structural. Do not include benchmark
identity.

## Task

You are reviewing a decompiler semantic failure. Propose invariant-based fixes
only. Do not propose branches keyed on function names, addresses, binary paths,
corpus rows, compiler tuples, byte strings, or benchmark-specific labels.

## Structural Failure Pattern

```text
Describe the abstract failure shape here.
Example format: "A loop-carried byte accumulator is widened, then later used as
a byte pointer offset without preserving low-byte wrap semantics."
```

## Owner Evidence

```text
Paste the smallest redacted p-code, NIR/HIR, CFG, or output excerpt that proves
the owner. Replace concrete row identity with placeholders such as <function>,
<address>, <binary>, and <compiler>.
```

## Invariant Candidates

```text
List the ABI/ISA rule, p-code semantic, CFG/dominance/postdominance fact,
def-use fact, type constraint, calling-convention fact, or memory-alias fact
that should hold beyond the motivating row.
```

## Forbidden Shortcuts

- Do not branch on function names.
- Do not branch on addresses.
- Do not branch on binary paths or corpus row ids.
- Do not match row-identifying compiler labels.
- Do not copy or depend on vendor/reference implementation code.
- Do not mimic Ghidra presentation quirks when clearer equivalent pseudocode is
  possible.

## Validation Matrix

- Targeted invariant test:
- Crate-level check:
- Focused redacted row rerun:
- Patch validation pool or synthetic unseen signal:
- Smoke/automation no-regression signal:

## Requested Output

Return:

1. likely owner,
2. generalized invariant,
3. minimal implementation location,
4. tests that would falsify overfitting,
5. risks and non-goals.
