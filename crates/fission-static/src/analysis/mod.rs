//! Analysis Module - Binary analysis engines
//!
//! Contains decompilation, disassembly, binary loading, patching, detection, CFG analysis, and xrefs.

pub mod callgraph;
pub mod cfg;
pub mod decomp;
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

pub use fission_pcode as pcode;
pub use fission_pcode::disasm;

pub use callgraph::{CallEdge, CallGraph};
pub use cfg::{
    BasicBlock, BlockEdge, CfgAnalysis, CfgBuilder, CfgError, CfgMetrics, CfgResult, CfgVisualizer,
    ComplexityAnalyzer, ControlFlowGraph, DominatorTree, DotOptions, EdgeKind, Loop, LoopAnalyzer,
    LoopKind,
};
pub use optimizer::{Optimizer, OptimizerConfig};
pub use patch::{Patch, PatchManager, QuickPatch};
pub use xrefs::{Xref, XrefDatabase, XrefType};
