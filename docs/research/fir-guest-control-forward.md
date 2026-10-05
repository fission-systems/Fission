# Forward-only guest control between sequence instances

Implemented experimental slice: 2026-10-05. Extends `FirSequence`
(`fir-sequence-state-origin.md`) with an explicit guest next-PC contract. It is
separate from a body's internal CFG, uses the same canonical FIR, and adds no
NIR/HIR layer or P-code dependency.

## Contract

- `GuestPcRead` (`guest.pc.read`, u64) yields the origin address of the instance
  being executed. `GuestNextPcWrite` (`guest.next_pc.write`, u64) sets the next
  guest address. No write means fallthrough. A second write in one body
  invocation is `InvalidState`.
- `FieldRead` (`field.read`) yields a raw decoded field, zero-extended into an
  unsigned output at least as wide as the field. It is usable standalone.
- A body's `branch`/`return` blocks remain intra-body control flow. They are not
  guest control; only `GuestNextPcWrite` is.
- Guest PC ops need `.fslc` v8 (tags 17, 18, 19). The compiler still selects the
  minimal version, so v1–v7 packages and their bytes are unchanged.
- Standalone `execute_decoded` and standalone C/Rust emission refuse bodies using
  guest PC ops, since no origin exists. Diagnostic FIR output is still available.

## Sequence execution

A target is accepted only if it is strictly greater than the current address, at
most the window end, and word-aligned relative to the base. Landing exactly on the
end exits. Anything else returns `ExecutionStatus::BadBranchTarget` (status 4) and
the temporary state is discarded, so the caller's state is unchanged. Forward-only
targets bound execution to the instance count; loops are not supported.

Targets are computed at run time from the body, so they are checked at run time,
not proven statically at composition. Register-derived (indirect) targets are
expressible and obey the same checks. Every word in the window must still decode
and be admitted, including words a branch skips.

## Envelope and output

`.fslseq` v2 (`FSLSEQ\0\x02`) has the same 96-byte header. It is written only when a
selected body uses guest PC ops; otherwise v1 bytes are produced exactly as before.
Loading rejects v1 with guest control and v2 without it.

C/Rust output gains a `pc` variable and a per-step `if (pc == address)` dispatch
with the same target checks and status 4. Bodies using PC ops get extra
parameters `guest_pc`, `next_pc`, `next_pc_set`. Sequences without guest control
emit the previous text unchanged. CUDA/PTX and native JIT/AOT remain unsupported.

## Validation (2026-10-05, this commit)

`tests/guest_control.rs`, fixture `specs/guest-branch-forward.fsl` (synthetic,
not an ISA):

- Reference executor vs an independent interpreter over four programs (diamond,
  branch to end, unconditional chain, alias/join) on 43 states each.
- Bad targets (misaligned, past end, conditional taken past end) return status 4
  and preserve state.
- C/Rust O0/O2 recompilation matches reference and oracle on those four programs
  plus three bad-target programs, 43 states each.
- v2 round trip, version mismatch refusal, standalone refusal, v7 refusal.

These are synthetic fixtures. They are not binary lifting, whole-function
equivalence, GPU evidence, or loader-backed byte origins. The oracle checks
execution semantics only; no external byte-encoding oracle applies to the fixture.
C/Rust output comes from the same FIR, so their agreement does not prove the
fixture specification correct.

Not done: loader-backed file-offset/virtual-address origins, real ISA branch
specs, static target analysis, loops.
