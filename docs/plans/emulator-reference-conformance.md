# Emulator reference conformance matrix

This matrix records small, deterministic semantic cases for the Fission
interpreter and JIT. Expected results are fixed in Fission-owned tests, so the
normal test lane does not build or execute vendor tools.

## References

| Reference | Version | Use |
|---|---:|---|
| `vendor/ghidra/ghidra-Ghidra_12.0.4_build/Ghidra/Framework/Emulation/src/main/java/ghidra/pcode/exec/PcodeExecutor.java` and `pcode/opbehavior/` | Ghidra 12.0.4 | P-code execution and operation semantics |
| `vendor/unicorn-2.1.4/bindings/python/tests/test_x86.py` | Unicorn 2.1.4 | x86 register, memory, and control-flow examples |
| `vendor/qemu-11.0.2/target/i386/tcg/translate.c` | QEMU 11.0.2 | x86 instruction-to-TCG behavior |

These are read-only references. The test cases below do not link, shell out to,
or copy code from them. Unicorn and QEMU operate at the ISA layer; their cases
only apply where the corresponding Fission fixture lifts to the same behavior.

## Initial cases

| Layer / case | Input | Expected observable result | Fission test |
|---|---|---|---|
| P-code `COPY` truncation | 64-bit `0x1122334455667788` to 32 bits | `0x55667788` | `pcode_integer_boundaries_match_fixed_results_in_both_engines` |
| P-code `INT_SEXT` | 8-bit `0x80` to 64 bits | `0xffffffffffffff80` | `pcode_integer_boundaries_match_fixed_results_in_both_engines` |
| P-code `INT_ZEXT` | 8-bit `0xff` to 64 bits | `0xff` | same |
| P-code `INT_LEFT`, `INT_RIGHT`, `INT_SRIGHT` | 64-bit boundary count `64`, plus 8-bit operands with count `128` | left/right produce `0`; signed right fills the sign bit (`-1`) | `pcode_integer_boundaries_match_fixed_results_in_both_engines` |
| P-code `INT_SDIV` | signed 32-bit `-6 / -1` | `6` | same |
| P-code signed compare and carry outputs | `-1 <= 0`; `0xff + 1`; signed `0x7f + 1` | true; carry true; signed overflow true | same |
| P-code absolute `CBRANCH` | condition `1` or `0`, target `0x2000` | branch to `0x2000`, or fall through to `0x1004`; skipped write remains absent on the taken path | `absolute_conditional_branch_has_the_expected_target_and_fallthrough` |
| P-code partial `STORE` | bytes `[11,22,33,44]`, store 16-bit `0xbeef` | `[ef,be,33,44]` | `partial_width_store_preserves_adjacent_memory_bytes_in_both_engines` |
| x86-64 instruction stream | safe guest reads `A` or `B` into stack memory and compares it | `A` gives exit/RDI `0`; `B` gives exit/RDI `1`; read byte remains in guest memory | `x86_64_fixture_matches_its_expected_branch_and_memory_results` |

Each P-code row is run through both the test-time P-code evaluator and JIT and
checked against the listed expected value. The x86-64 fixture runs through the
normal SLEIGH lift with each existing execution mode and checks its source-
specified result. These tests complement `interp_differential.rs`, which
checks that the engines agree but does not by itself establish which result is
correct.

Division by zero is intentionally absent from this initial matrix: Fission's
current emulator policy returns zero, while the cited Ghidra operation
behavior raises. Any future row for that case must state the policy being
validated instead of treating the two tools as interchangeable oracles.
