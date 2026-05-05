//! Binary-level static analysis services (xrefs, call graphs, function discovery, patches, strings).
//!
//! Decompilation semantics and orchestration live in `fission-pcode` / `fission-decompiler`; this
//! crate supplies facts under `decomp` and analyzer utilities loaded binaries can use without owning IR policy.

pub mod callgraph;
pub mod decomp;
pub mod function_discovery;
pub mod optimizer;
pub mod patch;
pub mod string_xrefs;
pub mod strings;
pub mod xrefs;
pub mod xref_index;

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
pub use xrefs::{Xref, XrefDatabase, XrefType, OPERAND_INDEX_MNEMONIC};
pub use xref_index::{
    build_xref_index, resolve_enclosing_function, FunctionXrefsSummary, XrefEvidence, XrefId,
    XrefIndex, XrefIndexBuilder, XrefIndexSummary, XrefKind, XrefRecord, XrefSource,
    XrefSourceCategory, XrefSourceLayer, XrefTarget,
};
