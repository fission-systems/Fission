//! Binary-level static analysis services (xrefs, call graphs, function discovery, patches, strings).
//!
//! Decompilation semantics and orchestration live in `fission-pcode` / `fission-decompiler`; this
//! crate supplies facts under `decomp` and analyzer utilities loaded binaries can use without owning IR policy.

pub mod callgraph;
pub mod calling_convention;
pub mod control_flow_facts;
pub mod decomp;
pub mod external_symbol;
pub mod function_discovery;
pub mod function_provenance;
pub mod optimizer;
pub mod patch;
pub mod prototype_hint;
pub mod string_xrefs;
pub mod strings;
pub mod value_set;
pub mod xref_index;
pub mod xrefs;

// Re-export types from separate crates
pub use fission_loader::{
    Confidence, Detection, DetectionResult, DetectionType, FunctionInfo, LoadedBinary, SectionInfo,
    detect,
};

pub use callgraph::{CallEdge, CallGraph};
pub use control_flow_facts::{
    control_flow_facts_for, decode_memory_context_for, function_max_bytes, ControlFlowFacts,
    FunctionControlFlowFacts,
};
pub use external_symbol::{
    ExternalSymbolIdentity, ExternalSymbolIndex, build_external_symbol_index,
    normalize_library_key, parse_external_identity_from_loader_string,
};
pub use function_discovery::{
    FunctionDiscoveryProfile, FunctionDiscoveryReport, discover_functions_with_runtime,
};
pub use function_provenance::{
    FunctionProvenanceIndex, FunctionProvenanceKind, FunctionProvenanceRecord,
    build_function_provenance_index,
};
pub use optimizer::{Optimizer, OptimizerConfig};
pub use patch::{Patch, PatchManager, QuickPatch};
pub use prototype_hint::win_api_prototype_hint_json;
pub use xref_index::{
    FunctionXrefsSummary, XrefEvidence, XrefId, XrefIndex, XrefIndexBuilder, XrefIndexSummary,
    XrefKind, XrefRecord, XrefSource, XrefSourceCategory, XrefSourceLayer, XrefTarget,
    build_xref_index, resolve_enclosing_function,
};
pub use string_xrefs::{
    StringWithXrefs, StringXrefAnalysis, StringXrefStats, analyze_string_xrefs,
};
pub use strings::{ExtractedString, StringType, extract_strings, build_string_lookup};
pub use value_set::{AbstractValue, ValueSetAnalyzer, ValueState};
pub use xrefs::{OPERAND_INDEX_MNEMONIC, Xref, XrefDatabase, XrefType};
