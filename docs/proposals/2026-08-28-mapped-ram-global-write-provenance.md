# Preserve mapped-RAM provenance on direct p-code outputs

## 1. Baseline Row Anchor

- Binaries: DecBench sample-set `bin_009.elf` and `bin_033.elf`.
- Functions: `sub_3920` and `main`.
- Addresses: `0x3920` and `0xf390`.
- Corpus command: bounded `fission_cli raw-pcode` through the next function
  symbol, compared with `fission_cli decomp --layer both` and the sample-set
  global-store probe.
- Current output: direct RAM writes reach NIR/HIR, but mapped addresses such as
  `tmp_1a120` and the named global `rcvbuf` are declared as function locals.
- Semantic cases passed / total: byte equality is not reached; this audit is a
  static side-effect check because corpus binaries must never be executed.
- Failure category: observable global writes are redirected to automatic local
  storage in the emitted C.
- Baseline: global writes represented in emitted C improved from 1/46 to 29/46;
  the remaining failure set is confined to these two functions. This proposal
  addresses the storage-class loss for writes that reach HIR; it does not
  assume every absent statement has the same owner.

## 2. Owner Proof

- [ ] SLEIGH/raw p-code
- [x] Builder/materialize
- [x] Normalize
- [ ] Structuring
- [ ] Type/data recovery
- [x] Printer
- [ ] Benchmark/automation

Evidence:

```text
raw p-code: Copy v(space=3,off=0xaa1c0,size=4) <- rax
NIR/HIR:    int rcvbuf; ... rcvbuf = local_14;

materialize emits PreHirLValue::Var("rcvbuf") without a binding;
rescue_undeclared_bindings consequently creates a Temp local;
render then treats that local as intentional shadowing and excludes the
loader global with the same name from file-scope declarations.
```

The raw p-code retains the correct RAM-space output. The first owner that
erases memory provenance is mapped-output materialization.

## 3. Generality / Invariant Proof

Generalized rule:

```text
An operation whose output varnode is proven to be mapped RAM remains a memory
lvalue through PreHIR/HIR. It must not be represented as an unqualified local
variable merely because the C spelling of the address is an identifier.
```

ISA-agnostic check:

- [x] The rule is based on p-code address space and loader mapping, not an ISA.
- [x] Runtime/legacy space-number interpretation remains in the existing owner.
- [x] Synthetic coverage uses only mapped RAM, a runtime-space marker, and a
  direct output operation.

Comparable coverage:

- Similar shape 1: a named loader global (`rcvbuf`) written directly by an op.
- Similar shape 2: an unnamed mapped address (`tmp_1a120`) written directly.
- Synthetic invariant test: the materialized lvalue contains
  `AddressOfGlobal` and does not allocate a Temp local.

## 4. Risk And Ownership Check

- Existing owner: `PreviewBuilder::maybe_materialize_output_stmt` already
  recognizes mapped-RAM outputs and preserves otherwise-dead writes.
- Shared substrate: p-code semantic contract and loader memory mapping.
- Extend that owner by using the existing `AddressOfGlobal` memory expression;
  no new pass, IR field, or parallel provenance map is needed.
- Render must admit address-spelled mapped globals into the existing global
  declaration collector after the structured tree rewrites the dereference to
  its direct C spelling.
- New owner dependency: none.
- Telemetry impact: none.
- Must not change: legacy unique-space temporaries, unmapped outputs, stack
  locals, or a genuine non-temp local that happens to share a symbol name.

## 5. Validation Matrix

- [x] Targeted materialize and layered-render invariant tests.
- [x] `cargo nextest run -p fission-pcode -- materialized_mapped_ram` plus the
  address-named global declaration test.
- [x] `cargo nextest run -p fission-pcode` (1,033 passed, 1 skipped).
- [x] Focused decompilation of both anchored sample rows on NIR and HIR.
- [x] Direct writes that survive to HIR now render at file scope, including
  named `rcvbuf` and address-spelled `tmp_1a130` declarations.
- [x] The focused bounded audit found a distinct remaining failure: writes at
  `0x1a120` and `0x1a140` materialize successfully, but their reachable raw-CFG
  blocks (`0x3cf4` and `0x3da4`) are not placed in the final PreHIR body. This
  is structuring/block ownership work, not mapped-output provenance work.
- [x] Full cache-disabled sample-set rescore: HIR GED 64/233, structural Types
  14/222, Union 72/250; unchanged from the recorded baseline.
- [x] `cargo check -p fission-pcode` and `cargo fmt --all --check`.

## 6. AI Review / Prompt Firewall

- [x] No separate model was asked for implementation advice.
- [x] Production code contains only the structural mapped-RAM invariant.
- [x] Validation includes synthetic coverage and measured real-corpus rows.

## 7. Review Notes

- This is a semantic storage-class correction, not a byte-match-specific
  printer substitution.
- No function name, address, binary id, or corpus identity enters production
  code.
