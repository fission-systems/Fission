//! Canonical cross-reference index (`XrefIndex`): loader seeds + disassembly layer (+ future pcode).

mod build;
mod model;

pub use build::{
    XrefIndex, XrefIndexBuilder, build_xref_index, push_disassembly_layer, push_loader_seeds,
    resolve_enclosing_function,
};
pub use model::{
    FunctionXrefsSummary, XrefEvidence, XrefId, XrefIndexSummary, XrefKind, XrefRecord, XrefSource,
    XrefSourceCategory, XrefSourceLayer, XrefTarget,
};
