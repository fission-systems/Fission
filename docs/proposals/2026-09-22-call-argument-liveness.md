# Call-site ABI carrier liveness proposal

## Status

Measured proposal for issue #78. Implementation is intentionally limited to
the existing p-code call-argument recovery owner in
`midend/builder/calls/call_recovery.rs`.

## Motivating row and baseline

- Binary: `/Users/sjkim1127/fission-benchmark/corpus/dev/binaries/c/control_flow_gcc_O2.exe`
- Function: `main` at `0x140002830`
- Command: `target/release/fission_cli decomp --project --no-header --no-warnings ...`
- Before: the project prototype declares `clamp(int value, int lo, int hi)`,
  while the body emits `clamp(8, 0)` and therefore fails the C17 compile gate.
- Assembly at the call boundary contains `RCX=8`, `RDX=0`, and the unchanged
  `R8D=5`; the immediately preceding `count_bits` call leaves no p-code
  output for `R8`.
- Raw p-code defines `REGISTER:0x80:8 <- const(5)` at `0x140002846`, calls
  `count_bits` at `0x140002853`, then stages `RCX`, `RDX`, and `R9` before the
  `clamp` call at `0x140002862`.

## Owner proof

The SLEIGH lift contains the register definition and the call in the expected
order. The loss occurs in generic `recover_call_args_from_block_with_mode`:

1. reverse scanning stops at the earlier `Call`, so it never reaches the
   unchanged `R8` definition;
2. the fallback carrier path finds that definition but
   `check_ancestor_realistic` rejects it because the register is caller-saved.

This is call-site ABI carrier recovery, not a printer, prototype, or
architecture-specific problem.

## Invariant

Call recovery should model the ABI register snapshot at the call boundary.
An unchanged register slot may be a live operand even when an earlier call is
between its last explicit definition and the current call. That slot may be
recovered only when the current call already has explicit same-block carrier
evidence after the intervening call; this proves that the scan is assembling
the current call's argument snapshot rather than replaying a prior call.

The recovery remains bounded at the next older call and never imports a
predecessor block's carriers across a call. Existing dominance, alias, and
contiguous-prefix rules remain unchanged. A call site with no current carrier
evidence continues to produce no inherited arguments.

## Regression matrix

1. Positive synthetic Windows x64 p-code: `RCX`/`RDX` are staged after an
   earlier call while `R8` retains a constant defined before it; the call must
   render all three required arguments.
2. Negative synthetic Windows x64 p-code: a second call with no new carrier
   staging must not inherit the first call's arguments, including when both
   calls share one block.
3. Existing cross-block caller-saved regression
   `x86_64_argument_registers_do_not_survive_an_intervening_call` must remain
   green.
4. The real `control_flow_gcc_O2.exe` `main` row is re-decompiled and its
   project output is checked for `clamp(8, 0, 5)`; the focused compiler output
   must no longer contain the previous `clamp` arity diagnostic. The full
   project still has unrelated pre-existing compile errors.

## Validation

- focused p-code tests;
- `cargo nextest run -p fission-pcode`;
- `cargo check -p fission-pcode` and `cargo build -p fission-cli --release`;
- `cargo fmt --all --check` and `git diff --check`;
- focused real-binary decompilation and project compile gate.

This change is a mechanical call-recovery correction until the real row is
remeasured after the fix; no quality or ranking claim is made from the
synthetic tests alone.
