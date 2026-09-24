//! Canonical cross-reference index (`XrefIndex`): loader, relocation, disassembly, and optional p-code layers.

mod build;
mod model;
mod pcode;

pub use build::{
    XrefIndex, XrefIndexBuilder, build_xref_index, build_xref_index_with_options,
    push_disassembly_layer, push_loader_seeds, resolve_enclosing_function,
};
pub use model::{
    FunctionXrefsSummary, XrefEvidence, XrefId, XrefIndexSummary, XrefKind, XrefRecord, XrefSource,
    XrefSourceCategory, XrefSourceLayer, XrefTarget,
};
pub use pcode::push_pcode_layer;
