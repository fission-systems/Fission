//! Translation-block compiler backend abstraction.
//!
//! The emulator has exactly one execution engine: whatever compiles a
//! [`GuestInsn`] sequence down to a callable `extern "C" fn(*mut Emulator)
//! -> u64` (see [`crate::core::Emulator::run_instruction`]'s "Cache miss ->
//! collect TB -> compile -> insert -> run" path). Today that's
//! [`crate::jit::JitCompiler`], which uses Cranelift. This trait is the seam
//! a self-implemented backend (`crate::selfjit`) plugs into so the two can
//! be swapped, and eventually differentially tested against each other,
//! without touching `core.rs`'s TB cache/chaining logic at all.
//!
//! Both implementors must honor the exact same host ABI: the returned
//! function pointer, once cast to `extern "C" fn(*mut Emulator) -> u64` and
//! called with the live `Emulator`, must leave all guest-visible state
//! (registers, memory) exactly as if the guest instructions had run
//! natively, and return the next guest PC.

use anyhow::Result;

pub use crate::jit::compiler::GuestInsn;

/// A pluggable translation-block compiler.
///
/// `register_space` is the SLA register-space id used for zero-callout host
/// register-file loads/stores (see [`crate::pcode::state::MachineState::register_space`]).
pub trait TbBackend {
    fn new() -> Result<Self>
    where
        Self: Sized;

    /// Compile `insns` (one translation block, i.e. one or more consecutive
    /// guest instructions already lifted to p-code) into a host function
    /// pointer. The returned pointer's only valid use is a transmute to
    /// `extern "C" fn(*mut Emulator) -> u64` and a call with the live
    /// emulator -- see the module docs for the exact contract.
    fn compile_translation_block(
        &mut self,
        insns: &[GuestInsn],
        register_space: u64,
    ) -> Result<*const u8>;
}

impl TbBackend for crate::jit::JitCompiler {
    fn new() -> Result<Self> {
        crate::jit::JitCompiler::new()
    }

    fn compile_translation_block(
        &mut self,
        insns: &[GuestInsn],
        register_space: u64,
    ) -> Result<*const u8> {
        crate::jit::JitCompiler::compile_translation_block(self, insns, register_space)
    }
}
