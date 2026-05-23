//! Decompiler facts integration — type context assembly, call target resolution,
//! and debug-info (DWARF) ingestion.

mod facts;

// Re-export inner module contents at this level so that existing
// `crate::facts::build_nir_type_context` etc. paths continue to resolve.
pub(crate) use self::facts::*;
