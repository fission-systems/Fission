# FSL to Fission JVM `iadd` parity probe

This experiment checks whether one instruction described by the experimental
FSL pattern package agrees with Fission's existing JVM SLEIGH runtime. It does
not add an FSL decoder or execution path to Fission.

The fixture at
`crates/fission-sleigh/examples/fixtures/jvm-se26-iadd.patterns.json` was
generated from `specs/vm/jvm-se26-iadd.fsl` in the companion
`fission-research` repository with `tools/fslc_probe.py`. Its package records
the source SHA-256 and Oracle JVMS provenance. The probe checks the exact
one-byte opcode, `int32` two-pop/one-push wrapping-add effect, and linked
primary-specification source. It then decodes byte `0x60` through Fission's
checked-in JVM `.sla` and checks that the resulting P-code loads two 32-bit
stack values, adds them, and stores the result.

Run it from the Fission repository root:

```sh
cargo run -p fission-sleigh --example fsl_jvm_iadd_parity -- \
  crates/fission-sleigh/examples/fixtures/jvm-se26-iadd.patterns.json
```

The probe covers one opcode only. It does not parse JVM class files, decode
operand-bearing or variable-length instructions, model JVM verification, or
establish whole-program decompilation quality. The Fission runtime context
change in this PR supports up to 128 context bits so the checked-in JVM SLA's
fields at bits 96–99 can participate in normal template selection.
