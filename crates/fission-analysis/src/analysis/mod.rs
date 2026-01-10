//! Analysis Module - Binary analysis engines
//!
//! Contains decompilation, disassembly, binary loading, patching, detection, CFG analysis, and xrefs.

pub mod cfg;
pub mod decomp;
pub mod detector;
pub mod disasm;
pub mod dotnet;
pub mod loader;
pub mod optimizer;
pub mod patch;
pub mod pcode;
pub mod signatures;
pub mod xrefs;
pub mod strings;
pub mod string_xrefs;

pub use cfg::{
    BasicBlock, BlockEdge, CfgAnalysis, CfgBuilder, CfgError, CfgMetrics, CfgResult,
    CfgVisualizer, ComplexityAnalyzer, ControlFlowGraph, DominatorTree, DotOptions,
    EdgeKind, Loop, LoopAnalyzer, LoopKind,
};
pub use detector::{detect, Confidence, Detection, DetectionResult, DetectionType};
pub use dotnet::{
    disassemble_method_rva, parse_dotnet_metadata, DotNetError, DotNetMetadata, DotNetMethod,
    DotNetType, ILInstruction, IlDisassembler,
};
pub use loader::{FunctionInfo, LoadedBinary, SectionInfo};
pub use optimizer::{Optimizer, OptimizerConfig};
pub use patch::{Patch, PatchManager, QuickPatch};
pub use xrefs::{Xref, XrefDatabase, XrefType};
