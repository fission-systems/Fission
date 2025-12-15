//! Analysis Module - Binary analysis engines
//!
//! Contains decompilation, disassembly, and binary loading.

pub mod decomp;
pub mod disasm;
pub mod dotnet;
pub mod loader;

pub use loader::{LoadedBinary, FunctionInfo, SectionInfo};
pub use dotnet::{
    parse_dotnet_metadata, disassemble_method_rva, DotNetError, DotNetMetadata, DotNetMethod,
    DotNetType, ILInstruction, IlDisassembler,
};
