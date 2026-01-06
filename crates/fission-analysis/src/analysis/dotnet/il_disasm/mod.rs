//! IL Disassembler Module
//!
//! CIL bytecode disassembler for .NET methods

mod types;
mod opcodes;
mod decoder;
mod disassembler;

// Re-export public API
pub use types::ILInstruction;
pub use disassembler::IlDisassembler;
