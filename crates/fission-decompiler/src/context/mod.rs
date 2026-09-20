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
use crate::facts::{
    build_nir_type_context, record_interprocedural_arity_facts,
    refine_nir_type_context_with_callee_effect_summaries,
};
use fission_analysis_db::ProgramSnapshot;
use fission_loader::loader::LoadedBinary;
use fission_pcode::midend::{NirFunctionHints, NirTypeContext};
use fission_pcode::{PcodeFunction, PreHirFunction};
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
    binary: &'bin LoadedBinary,

    /// Immutable program metadata plus mutable analysis overlays.
    facts: FactStore,

    /// NIR type context for the function at `address`.
    ///
    /// Built from the canonical program view plus function overlays.
    type_context: NirTypeContext,

    /// Set to true if a pass wrote to this context, indicating a new round is needed.
    hints_changed: bool,
}

impl<'bin> DecompContext<'bin> {
    pub fn new(binary: &'bin LoadedBinary, address: u64) -> Self {
        let facts = FactStore::from_binary_without_signature_matches(binary);
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

    /// Return the immutable binary view used to build this context.
    pub fn binary(&self) -> &LoadedBinary {
        self.binary
    }

    /// Return the current fact view without exposing mutable storage.
    pub fn facts(&self) -> &FactStore {
        &self.facts
    }

    /// Return the derived type context for the current render round.
    pub fn type_context(&self) -> &NirTypeContext {
        &self.type_context
    }

    /// Consume the context and return the learned fact store.
    pub fn into_facts(self) -> FactStore {
        self.facts
    }

    /// Start a new feedback-loop round.
    pub(crate) fn begin_round(&mut self) {
        self.hints_changed = false;
    }

    /// Refresh derived callee effects before a render round.
    pub(crate) fn refine_type_context(&mut self, pcode: &PcodeFunction) {
        refine_nir_type_context_with_callee_effect_summaries(
            self.binary,
            pcode,
            &mut self.type_context,
        );
    }

    /// Record cross-function arity facts discovered by the completed render.
    ///
    /// These facts belong to the live decompilation session, not to the render
    /// function's local control flow, so the context remains the owner of the
    /// write and of the fact store returned to the caller.
    pub(crate) fn record_interprocedural_arity_facts(
        &mut self,
        raw_hir: &PreHirFunction,
        self_address: u64,
    ) {
        record_interprocedural_arity_facts(
            &mut self.facts,
            &self.type_context,
            raw_hir,
            self_address,
        );
    }

    /// Whether the last render round published feedback into this context.
    pub(crate) fn hints_changed(&self) -> bool {
        self.hints_changed
    }

    fn rebuild_type_context(&mut self, address: u64) {
        self.type_context = build_nir_type_context(self.binary, &self.facts, address);
    }
}

impl<'bin> fission_pcode::midend::DecompFacts for DecompContext<'bin> {
    fn record_discovered_hints(&mut self, addr: u64, hints: NirFunctionHints) {
        self.facts.record_structuring_hints(addr, hints);
        self.rebuild_type_context(addr);
        self.hints_changed = true;
    }

    fn record_inferred_type(
        &mut self,
        addr: u64,
        type_info: fission_loader::loader::types::InferredTypeInfo,
    ) {
        self.facts
            .ingest_native_function_types(addr, vec![type_info]);
        self.rebuild_type_context(addr);
        self.hints_changed = true;
    }
}

#[cfg(test)]
mod tests {
    use super::DecompContext;
    use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder};
    use fission_pcode::midend::{DecompFacts, NirFunctionHints};
    use fission_static::analysis::decomp::facts::FactStore;

    #[test]
    fn context_owns_feedback_lifecycle_and_exports_learned_facts() {
        let binary = LoadedBinaryBuilder::new("sample.bin".to_string(), DataBuffer::Heap(vec![]))
            .format("ELF")
            .is_64bit(true)
            .build()
            .expect("build test binary");
        let mut context = DecompContext::from_facts(&binary, FactStore::default(), 0x401000);

        assert!(!context.hints_changed());
        DecompFacts::record_discovered_hints(
            &mut context,
            0x401000,
            NirFunctionHints {
                param_names: vec!["arg0".to_string()],
                ..Default::default()
            },
        );

        assert!(context.hints_changed());
        assert!(context.facts().structuring_hints(0x401000).is_some());

        context.begin_round();
        assert!(!context.hints_changed());
        let learned_facts = context.into_facts();
        assert!(learned_facts.structuring_hints(0x401000).is_some());
    }
}
