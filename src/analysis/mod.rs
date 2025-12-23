//! Analysis Module - Binary analysis engines
//!
//! Contains decompilation, disassembly, binary loading, and patching.

pub mod decomp;
pub mod disasm;
pub mod dotnet;
pub mod loader;
pub mod patch;

pub use loader::{LoadedBinary, FunctionInfo, SectionInfo};
pub use patch::{Patch, PatchManager, QuickPatch};
pub use dotnet::{
    parse_dotnet_metadata, disassemble_method_rva, DotNetError, DotNetMetadata, DotNetMethod,
    DotNetType, ILInstruction, IlDisassembler,
};
