# Transitive Address Constraint Precedence

## Baseline

- A formal ABI input is copied through one or more integer-width aliases before
  a casted cursor is used by byte loads and stores.
- Forward type recovery identifies the formal input as an address contributor,
  but the following use-driven round can propagate an older scalar copy type
  backward and weaken the formal input again.
- The observable result is a non-compiling prototype even though the HIR body
  contains direct memory access evidence.

## Owner Proof

- Raw p-code contains the expected ABI inputs, address arithmetic, byte loads,
  and byte stores. The lift is not missing an operation.
- Builder live-interval splitting already separates reused loaded-byte values in
  the validated simple cases.
- `normalize/types/use_type_infer.rs` owns backward copy constraints and the
  fixed-point merge that weakens the recovered pointer.
- `DefinitionDependencyMap::address_contributors` already provides the reusable
  transitive address fact. No new pass or binary-specific rule is required.

## Invariant Proof

1. If a binding lies on a def-use path from a pointer root to an actual load,
   store, index, or field address, stale scalar copy constraints must not weaken
   that binding during the same type fixed point.
2. A plain pointer-looking cast without a memory use is insufficient; the rule
   requires address-contributor evidence.
3. Register reuse remains owned by live-interval splitting. The type merge does
   not use names, addresses, compiler tuples, or corpus identities.

## Validation Matrix

- Synthetic alias-chain test with neutral names.
- Existing scaled pointer assignment and pointer/scalar role regression tests.
- `cargo nextest run -p fission-pcode --no-fail-fast`.
- Focused local semantic rows for the affected alias-heavy state setup and the
  previously fixed checksum/sum array guards.

## Validation Results

- `cargo nextest run -p fission-pcode --no-fail-fast`: 1,181 passed, 9 skipped.
- `cargo check -p fission-pcode -p fission-decompiler`: passed.
- The x86-64 GCC O0 state-setup row now recovers
  `void rc4_init(uchar *, uchar *, uint)` and passes 5/5 semantic cases.
- The x86 GCC O0 row reaches execution and passes 3/5 cases. Its remaining
  failures are state-update semantics, not a missing pointer prototype.
- Optimized/SIMD and Clang rows remain separate aggregate/type and body-recovery
  blockers; this proposal does not claim to solve them.
- Guard rows remain stable: every GCC O0/O2 x86/x64 `sum_array` and `checksum`
  row passes 5/5 cases. `reverse_in_place` retains native-width unsigned length
  parameters, while its remaining failures are body-semantics regressions.

## AI Use

- No external model proposal is used.
- Production conditions use only def-use and memory-address evidence.
- Reference output style is not copied.
