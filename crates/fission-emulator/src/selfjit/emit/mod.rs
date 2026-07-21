//! Per-host-ISA machine code emitters.
//!
//! `compiler.rs` (the p-code translator) is written against whichever
//! `Asm` this module re-exports for the *host* architecture Fission itself
//! is compiled for -- not the *guest* architecture being emulated (that's
//! always p-code by the time it reaches here, already architecture-neutral).
//!
//! Only [`aarch64`] is implemented -- this workspace's own dev machine is
//! Apple Silicon, so it's the only target that could actually be built
//! *and verified* (mmap the generated code, call it, check the result) in
//! this skeleton. [`x86_64`] is a stub with the same `Asm` shape but no
//! real encodings, so `compiler.rs` can be written against a stable
//! interface either way -- filling it in is real, separate follow-up work,
//! not attempted here without a way to test it.

#[cfg(target_arch = "aarch64")]
pub mod aarch64;
#[cfg(target_arch = "aarch64")]
pub use aarch64::Asm;

#[cfg(target_arch = "x86_64")]
pub mod x86_64;
#[cfg(target_arch = "x86_64")]
pub use x86_64::Asm;

/// A forward-branch fixup site: an instruction slot already emitted (as a
/// placeholder word) whose real encoding depends on a target offset not
/// known yet. Opaque -- construct via `Asm::placeholder`, resolve via
/// `Asm::patch_b`/`patch_b_cond`.
#[derive(Clone, Copy)]
pub struct Label(pub(crate) usize);
