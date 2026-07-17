//! Per-function decompilation context over an immutable program snapshot.
//!
//! [`DecompContext`] consolidates the four translation boundaries that were previously
//! scattered across the decompiler pipeline:
//!
//! | Boundary | Old location | New location |
//! |---|---|---|
//! | A: `FactStore::from_binary` | `routing.rs`, `render_finish.rs` | `DecompContext::new` |
//! | B: `NirRenderOptions::from_loaded_binary` | `render_finish.rs`, `render.rs` | caller-supplied or `DecompContext` |
//! | C: `build_nir_type_context` | `render.rs::build_nir_type_context_from_facts` | `DecompContext::new` |
//! | D: `apply_spec_overrides` | `render_finish.rs` | future: `DecompContext::with_spec_overrides` |
//!
use crate::facts::build_nir_type_context;
use fission_analysis_db::ProgramSnapshot;
use fission_loader::loader::LoadedBinary;
use fission_pcode::midend::{NirFunctionHints, NirTypeContext};
use fission_static::analysis::decomp::facts::FactStore;
use std::sync::Arc;

/// Live decompilation context for a single function.
///
/// Holds mutable analysis overlays and the type context for one function. The
/// program-level view is the immutable `ProgramSnapshot` owned by `FactStore`;
/// this context must not grow parallel function, symbol, or relocation maps.
///
/// # Lifetime
/// `'bin` ties the context to the binary it was built from. The context must not
/// outlive the [`LoadedBinary`].
#[derive(Clone)]
pub struct DecompContext<'bin> {
    /// Immutable binary reference. Never changes during decompilation.
    pub binary: &'bin LoadedBinary,

    /// Immutable program metadata plus mutable analysis overlays.
    pub facts: FactStore,

    /// NIR type context for the function at `address`.
    ///
    /// Built from the canonical program view plus function overlays.
    pub type_context: NirTypeContext,

    /// Set to true if a pass wrote to this context, indicating a new round is needed.
    pub hints_changed: bool,
}

impl<'bin> DecompContext<'bin> {
    pub fn new(binary: &'bin LoadedBinary, address: u64) -> Self {
        let facts = FactStore::from_binary(binary);
        let type_context = build_nir_type_context(binary, &facts, address);
        Self {
            binary,
            facts,
            type_context,
            hints_changed: false,
        }
    }

    pub fn from_program(
        binary: &'bin LoadedBinary,
        program: Arc<ProgramSnapshot>,
        address: u64,
    ) -> Self {
        Self::from_facts(binary, FactStore::from_program(binary, program), address)
    }

    pub fn from_facts(binary: &'bin LoadedBinary, facts: FactStore, address: u64) -> Self {
        let type_context = build_nir_type_context(binary, &facts, address);
        Self {
            binary,
            facts,
            type_context,
            hints_changed: false,
        }
    }
}

impl<'bin> fission_pcode::midend::DecompFacts for DecompContext<'bin> {
    fn record_discovered_hints(&mut self, addr: u64, hints: NirFunctionHints) {
        self.facts.record_structuring_hints(addr, hints);
        self.type_context = build_nir_type_context(self.binary, &self.facts, addr);
        self.hints_changed = true;
    }

    fn record_inferred_type(
        &mut self,
        addr: u64,
        type_info: fission_loader::loader::types::InferredTypeInfo,
    ) {
        self.facts
            .ingest_native_function_types(addr, vec![type_info]);
        self.type_context = build_nir_type_context(self.binary, &self.facts, addr);
        self.hints_changed = true;
    }
}
