//! IL Disassembler Module
//!
//! CIL bytecode disassembler for .NET methods

mod decoder;
mod disassembler;
mod opcodes;
mod types;

// Re-export public API
pub use disassembler::IlDisassembler;
pub use types::ILInstruction;
