//! Decompiler facts integration — type context assembly, call target resolution,
//! and debug-info (DWARF) ingestion.

mod facts;

// Re-export inner module contents at this level so that existing
// `crate::facts::build_nir_type_context` etc. paths continue to resolve.
pub(crate) use self::facts::*;

// Whole-program call-arity pre-analysis is the one entrypoint here meant to
// be driven from *outside* this crate (fission-serve's background
// discovery), so it needs a real `pub` re-export -- the blanket
// `pub(crate) use` above only reaches other modules within this crate.
pub use self::facts::seed_whole_program_call_arity_facts;
