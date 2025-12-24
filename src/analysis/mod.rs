//! Analysis Module - Binary analysis engines
//!
//! Contains decompilation, disassembly, binary loading, patching, detection, and xrefs.

pub mod decomp;
pub mod detector;
pub mod disasm;
pub mod dotnet;
pub mod loader;
pub mod patch;
pub mod xrefs;

pub use loader::{LoadedBinary, FunctionInfo, SectionInfo};
pub use patch::{Patch, PatchManager, QuickPatch};
pub use detector::{detect, Detection, DetectionResult, DetectionType, Confidence};
pub use xrefs::{XrefDatabase, Xref, XrefType};
pub use dotnet::{
    parse_dotnet_metadata, disassemble_method_rva, DotNetError, DotNetMetadata, DotNetMethod,
    DotNetType, ILInstruction, IlDisassembler,
};
