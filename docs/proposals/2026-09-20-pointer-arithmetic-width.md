# Pointer Arithmetic Width Model

## Baseline / issue anchor

- Issue: #41
- Owner: `crates/fission-midend-normalize/src/memory/ptr_arith.rs`
- Current defect: pointer byte-size and generated pointer-sized integer types
  assume 8-byte/64-bit pointers even though `PreHirFunction::is_64bit` is
  available at the pass entrypoint.
- Observable invariant: a 32-bit pointer array uses a 4-byte stride and a
  pointer-to-integer cast must use a 32-bit unsigned integer type.
- This change is an invariant/mechanical correctness repair. Synthetic tests
  provide the focused regression; no broad readability or benchmark-quality
  claim is made without a real-corpus before/after measurement.

## Owner proof

The error is created in the normalize pointer-arithmetic owner before printing:
`type_byte_size` returns 8 for `NirType::Ptr(_)`, causing pointer-array stride
matching and pointer access-size checks to reject valid 32-bit shapes. The same
file creates `NirType::Int { bits: 64, signed: false }` for pointer casts.

## Generalized rule

Derive one `PointerLayout { bytes, bits }` from `PreHirFunction::is_64bit` at
the pass boundary. Thread that layout through pointer-arithmetic recovery and
use it only where the semantic unit is the machine pointer: pointer type byte
size, generated pointer-sized integer casts, and generated pointer-derived
index constants. Preserve element-width inference from observed strides; an
8-byte `int64` element remains 64-bit on both 32-bit and 64-bit targets.

No function, address, binary, compiler, or ISA-name guard is needed.

## Validation matrix

- 32-bit pointer-array access with `Ptr(Ptr(Int32))` and stride 4 recovers an
  `Index` expression.
- 32-bit pointer-to-integer arithmetic emits `Int { bits: 32, signed: false }`.
- Existing 64-bit pointer arithmetic tests remain unchanged.
- `cargo nextest run -p fission-midend-normalize`.
- `cargo check -p fission-pcode` and `cargo fmt --all --check`.
- Report the result as mechanical correctness unless a real corpus row is
  remeasured separately.
