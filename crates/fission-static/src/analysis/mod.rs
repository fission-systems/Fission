//! Analysis Module - Binary analysis engines
//!
//! Contains decompilation, disassembly, binary loading, patching, detection, CFG analysis, and xrefs.

pub mod callgraph;
pub mod decomp;
pub mod function_discovery;
pub mod optimizer;
pub mod patch;
pub mod string_xrefs;
pub mod strings;
pub mod xrefs;

// Re-export types from separate crates
pub use fission_loader::{
    Confidence, Detection, DetectionResult, DetectionType, FunctionInfo, LoadedBinary, SectionInfo,
    detect,
};

pub use callgraph::{CallEdge, CallGraph};
pub use function_discovery::{
    FunctionDiscoveryProfile, FunctionDiscoveryReport, discover_functions_with_runtime,
};
pub use optimizer::{Optimizer, OptimizerConfig};
pub use patch::{Patch, PatchManager, QuickPatch};
pub use xrefs::{Xref, XrefDatabase, XrefType};
