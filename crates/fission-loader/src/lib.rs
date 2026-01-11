//! Fission Loader - Binary format parsing and loading
//!
//! This crate provides functionality for loading and parsing various binary formats
//! including PE (Windows), ELF (Linux), Mach-O (macOS), and .NET assemblies.

#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_sign_loss)]

pub mod detector;
pub mod dotnet;
pub mod loader;
pub mod prelude;

// Re-exports
pub use detector::{Confidence, Detection, DetectionResult, DetectionType, detect};
pub use dotnet::{
    DotNetError, DotNetMetadata, DotNetMethod, DotNetType, ILInstruction, IlDisassembler,
    disassemble_method_rva, parse_dotnet_metadata,
};
pub use loader::{FunctionInfo, LoadedBinary, SectionInfo};
