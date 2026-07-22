//! Per-host-ISA machine code emitters.
//!
//! `compiler.rs` (the p-code translator) is written against whichever
//! `Asm` this module re-exports for the *host* architecture Fission itself
//! is compiled for -- not the *guest* architecture being emulated (that's
//! always p-code by the time it reaches here, already architecture-neutral).
//!
//! Both [`aarch64`] and [`x86_64`] are implemented and verified end to end
//! (mmap the generated code, call it, check the result -- see each
//! module's own unit tests plus `compiler.rs`'s integration test and the
//! full `selfjit::differential` suite, all of which pass on both). This
//! workspace's own dev machine is Apple Silicon, so [`x86_64`] couldn't be
//! verified on real x86-64 silicon -- it was built and tested via
//! `rustup target add x86_64-apple-darwin` + `cargo test --target
//! x86_64-apple-darwin`, which runs under Rosetta 2. Rosetta translates a
//! process's *own* runtime-generated machine code (not just its
//! statically-linked instructions) transparently, so this is a real,
//! meaningful verification of the encodings -- just not on real x86-64
//! hardware or under a real Linux/Windows SysV64 host, which is why this
//! is flagged as a real (if narrow) verification gap rather than treated
//! as equivalent to `aarch64`'s native-silicon confidence level.

#[cfg(target_arch = "aarch64")]
pub mod aarch64;
#[cfg(target_arch = "aarch64")]
pub use aarch64::{
    Asm, Cond, ARG0, ARG1, ARG2, ARG3, ARG4, A_VAL_SLOT, B_VAL_SLOT, EMU_PTR_SLOT, RESULT_SLOT,
    RET,
};

#[cfg(target_arch = "x86_64")]
pub mod x86_64;
#[cfg(target_arch = "x86_64")]
pub use x86_64::{
    Asm, Cond, ARG0, ARG1, ARG2, ARG3, ARG4, A_VAL_SLOT, B_VAL_SLOT, EMU_PTR_SLOT, RESULT_SLOT,
    RET,
};

/// A forward-branch fixup site: an instruction slot already emitted (as a
/// placeholder word) whose real encoding depends on a target offset not
/// known yet. Opaque -- construct via `Asm::placeholder`, resolve via
/// `Asm::patch_b`/`patch_b_cond`.
#[derive(Clone, Copy)]
pub struct Label(pub(crate) usize);
