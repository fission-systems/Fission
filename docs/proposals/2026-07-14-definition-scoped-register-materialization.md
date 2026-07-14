# Definition-Scoped Register Materialization

## Baseline

- A large x86-32 O0 loop reuses one hardware register for several unrelated
  scalar and address calculations in a single p-code block.
- Materialization currently classifies every self-reading register write in the
  loop as loop-carried when the block can reach a backedge. Earlier writes that
  are killed by a later write therefore reuse one hardware-register binding.
- An entry stack-adjustment flag write is also treated as a cross-block merge
  input even when a later flag definition kills it before the reported use.
- The observable result is mixed pointer/scalar C bindings and a stale
  stack-pointer flag expression. In the optimized shape, a scalar entry value
  and a later address-valued definition of the primary ABI return register are
  also collapsed even though the address definition is killed before return.

## Owner Proof

- Raw p-code contains distinct ordered register definitions. The lift has not
  lost the register writes or their block identities.
- `nir/builder/materialize/scans.rs` owns materialization-time def-use scans.
  Its cross-block query currently enumerates reachable blocks without proving
  a kill-free path from the exact definition.
- `nir/builder/materialize/loop_carried/shape.rs` owns admission of a register
  write to a stable loop-carried binding. It checks block reachability and
  self-read shape, but not whether that exact definition reaches a backedge.
- `nir/builder/mod.rs::ensure_temp_binding_for_output` is the fallback owner for
  definitions that did not receive a proven stable binding. It checks parameter
  and local-name collisions before selecting a hardware-register surface name,
  but not the temp registry. Distinct fallback definitions can therefore
  silently reuse the first definition's binding without any proof.
- The primary-return materialization path is a second stable-name owner. It
  previously selected the ABI register name from storage identity alone,
  without proving that the exact definition remained live to a return.
- `nir/normalize/recovery/variable_merge.rs` is a downstream coalescing owner.
  Its speculative disjoint-range phase previously admitted `TempPreserved`
  hardware names and compared later candidates against a stale pre-merge range.
  That could undo a correct materialize split or transitively merge two values
  that still co-occur in one expression.

## Invariant Proof

1. A p-code use belongs to a definition only when a CFG path connects the exact
   definition site to that use without an intervening overlapping definition.
2. A loop-carried binding is admitted only when the candidate definition both
   receives the prior-iteration value before any kill and reaches a backedge
   without another kill.
3. Definition and use sites are represented by typed identities. The API that
   reuses a stable loop binding consumes an opaque proof rather than a boolean
   inferred from variable or register names.
4. Value-preserving same-register copies and extensions may preserve the chain;
   unrelated writes to an overlapping register range kill it.
5. Fallback materialization never reuses an existing temp binding. Stable-name
   reuse is available only through APIs that consume a loop, merge, or exact
   same-block definition proof.
6. A same-block self-update may reuse the nearest prior binding when the current
   operation reads that exact register value before redefining it. A definition
   that does not read the prior value starts a new phase once the old interval
   has been consumed.
7. Architectural status flags retain their canonical state names because flag
   recovery owns their reaching-definition selection. Dead entry flag values
   are removed by liveness, not by disguising later flag definitions as generic
   temporaries.
8. The rule depends only on p-code operation order, overlapping varnode storage,
   and CFG edges. It does not inspect an ISA enum, compiler tuple, function
   name, address, binary, or corpus row.
9. A primary ABI return-register definition receives the canonical ABI name
   only when an opaque proof shows a kill-free, call-free path from that exact
   definition to a machine return terminator. A later address or scratch role
   that is overwritten before every return receives an independent binding.
10. Normalize may speculatively coalesce synthetic temporaries only. A hardware
    register binding remains architecture-owned even when its origin is
    `TempPreserved`. A transitive merge group consumes its current unioned live
    range and all original member identities; any co-occurring member pair
    rejects the merge.

## Risk And Architecture Boundary

- This extends the existing materialize def-use substrate, loop-carried owner,
  and variable-merge candidate contract. It does not add a new normalize pass.
- The proof is fail-closed. An incomplete CFG or ambiguous path yields no
  stable binding reuse and leaves independent temporaries for later recovery.
- Return live-out is not inferred from a register name or function return type;
  it is an exact CFG reachability property consumed by the materializer.
- Existing induction, accumulator, partial-register, and multi-block loop tests
  must continue to prove their carried definitions.
- Cross-block consumers at joins must remain discoverable when at least one
  kill-free predecessor path reaches the use.

## Validation Matrix

- Synthetic same-block phase reuse: an early self-read definition killed before
  the backedge is not loop-carried.
- Synthetic fallback register reuse: two distinct definitions of the same
  hardware register receive distinct bindings unless an owner proof explicitly
  joins them.
- Synthetic self-loop induction: a definition read on the next iteration and
  not killed before the backedge remains loop-carried.
- Synthetic cross-block kill: a successor redefinition prevents a stale
  predecessor definition from claiming a later use.
- Existing loop-carried materialize test module.
- `cargo nextest run -p fission-pcode --no-fail-fast`.
- `cargo check -p fission-pcode -p fission-decompiler`.
- Focused x86 O0 row with a fresh local bundle: no entry stack-pointer flag
  residue, no pointer/scalar declaration collapse, and improved semantic cases.
- Existing passing x86/x64 O0/O2 rows must not regress.
- Synthetic SCC address provenance: loop-carried pointer/int roundtrips retain
  every root-reaching member even when DFS edge order encounters a backedge.
- Synthetic return live-out: a killed primary-register definition cannot claim
  the ABI name, while a kill-free definition reaching return still can.
- Synthetic normalize boundary: a hardware register cannot absorb a disjoint
  synthetic temporary, and a transitive merge group cannot absorb a value that
  co-occurs with any existing group member.
- Focused local validation result: all eight compiler/optimization variants
  pass all five semantic cases; the previously failing optimized 32-bit row
  improves from 0/5 to 5/5 without a passing-row regression.
- Focused O0 surface result: the entry `esp` flag is absent and `edx` is limited
  to one self-updating scalar live range instead of being reused for the loop
  predicate, state accumulator, and output cursor phases.

## AI Use

- No external model proposal is used.
- Production conditions are stated as definition reachability and kill facts;
  benchmark identifiers remain confined to validation evidence.
- Reference-decompiler output is correctness evidence only and is not copied as
  an output-style target.
