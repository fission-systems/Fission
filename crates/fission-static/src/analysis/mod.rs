//! Binary-level static analysis services (xrefs, call graphs, function discovery, patches, strings).
//!
//! Decompilation semantics and orchestration live in `fission-pcode` / `fission-decompiler`; this
//! crate supplies facts under `decomp` and analyzer utilities loaded binaries can use without owning IR policy.

pub mod callgraph;
pub mod control_flow_facts;
pub mod decomp;
pub mod external_symbol;
pub mod function_discovery;
pub mod function_provenance;
pub mod noreturn;
pub mod patch;
pub mod prototype_hint;
pub mod string_xrefs;
pub mod strings;
pub mod value_set;
pub mod xref_coverage;
pub mod xref_index;
pub mod xrefs;

// Re-export types from separate crates
pub use fission_loader::{
    Confidence, Detection, DetectionResult, DetectionType, FunctionInfo, LoadedBinary, SectionInfo,
    detect,
};

pub use callgraph::{CallEdge, CallGraph, NamedCallGraphView};
pub use control_flow_facts::{
    ControlFlowFacts, FunctionControlFlowFacts, control_flow_facts_for, decode_memory_context_for,
    function_max_bytes,
};
pub use external_symbol::{
    ExternalSymbolIdentity, ExternalSymbolIndex, build_external_symbol_index,
    normalize_library_key, parse_external_identity_from_loader_string,
};
pub use function_discovery::{
    FunctionDiscoveryReport, decode_context_for_address, discover_functions_with_runtime,
    image_executes_thumb,
};
pub use function_provenance::{
    FunctionProvenanceIndex, FunctionProvenanceKind, FunctionProvenanceRecord,
    build_function_provenance_index,
};
pub use patch::{Patch, PatchManager, QuickPatch};
pub use prototype_hint::win_api_prototype_hint_json;
pub use string_xrefs::{
    StringWithXrefs, StringXrefAnalysis, StringXrefStats, analyze_string_xrefs,
};
pub use strings::{ExtractedString, StringType, build_string_lookup, extract_strings};
pub use value_set::{AbstractValue, ValueSetAnalyzer, ValueState};
pub use xref_coverage::{
    XrefAnalysisLayer, XrefAnalysisReport, XrefAnalysisState, XrefCoverageUnit, XrefLayerCoverage,
    XrefOmissionReason, XrefUnsupportedReason,
};
pub use xref_index::{
    FunctionXrefsSummary, XrefEvidence, XrefId, XrefIndex, XrefIndexBuilder, XrefIndexSummary,
    XrefKind, XrefRecord, XrefSource, XrefSourceCategory, XrefSourceLayer, XrefTarget,
    build_xref_index, build_xref_index_with_options, push_pcode_layer, resolve_enclosing_function,
};
pub use xrefs::{OPERAND_INDEX_MNEMONIC, Xref, XrefDatabase, XrefType};

/// Indices into `binary.functions` of the first non-import entry at each
/// address, in list order.
///
/// The loader keeps every entry that claims an address -- an ELF symbol and
/// the `.eh_frame` FDE for the same code are two entries -- and leaves the
/// choice between them to the canonical views. A pass whose per-function work
/// depends only on the address (decoding the code that starts there) does that
/// work once per address, not once per entry.
pub(crate) fn distinct_code_entries(binary: &fission_loader::loader::LoadedBinary) -> Vec<usize> {
    let mut seen = std::collections::HashSet::with_capacity(binary.functions.len());
    binary
        .functions
        .iter()
        .enumerate()
        .filter(|(_, function)| !function.is_import && seen.insert(function.address))
        .map(|(index, _)| index)
        .collect()
}

#[cfg(test)]
mod tests {
    use fission_loader::loader::{DataBuffer, FunctionInfo, LoadedBinaryBuilder};

    #[test]
    fn distinct_code_entries_keeps_the_first_non_import_entry_per_address_in_order() {
        let mut binary =
            LoadedBinaryBuilder::new("entries.bin".to_string(), DataBuffer::Heap(vec![0; 16]))
                .format("ELF")
                .arch_spec("x86:LE:64:default")
                .is_64bit(true)
                .build()
                .expect("binary");
        let entry = |address, is_import| FunctionInfo {
            address,
            is_import,
            ..Default::default()
        };
        binary.functions = vec![
            entry(0x30, false),
            entry(0x10, true),  // an import at 0x10 ...
            entry(0x10, false), // ... and code at the same address
            entry(0x30, false), // the FDE beside the symbol at 0x30
            entry(0x20, true),  // import only: nothing to decode
            entry(0x10, false),
        ];
        assert_eq!(super::distinct_code_entries(&binary), vec![0, 2]);
    }
}
