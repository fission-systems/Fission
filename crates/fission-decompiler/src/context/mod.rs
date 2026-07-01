//! Live decompilation context — single source of truth for per-function pipeline state.
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
//! ## Phase rollout
//!
//! **Phase 1 (current):** `DecompContext` is used internally to consolidate FactStore +
//! NirTypeContext construction. Public API is unchanged.
//!
//! **Phase 2:** `FactStore` gains a write API; `DecompContext` exposes `record_*` methods.
//!
//! **Phase 3:** `PassCtx` receives `Option<&mut DecompFacts>` so normalize passes can
//! write discovered facts back to the live context.
//!
//! **Phase 4:** Fixed-point normalize ↔ structure feedback loop enabled by the live context.

use crate::facts::build_nir_type_context;
use fission_loader::loader::LoadedBinary;
use fission_pcode::nir::{NirTypeContext, NirFunctionHints};
use fission_static::analysis::decomp::facts::FactStore;

/// Live decompilation context for a single function.
///
/// Holds all per-function state that the pipeline needs. Analogous to Ghidra's
/// `Program` object — all pipeline stages that receive a `&mut DecompContext` can
/// read and (in Phase 2+) write back to it.
///
/// # Lifetime
/// `'bin` ties the context to the binary it was built from. The context must not
/// outlive the [`LoadedBinary`].
pub struct DecompContext<'bin> {
    /// Immutable binary reference. Never changes during decompilation.
    pub binary: &'bin LoadedBinary,

    /// Live fact store.
    ///
    /// In Phase 1 this is effectively read-only after construction.
    /// Phase 2 adds write methods (`record_param_hints`, `record_inferred_type`, …)
    /// so that normalize/structuring passes can publish discovered facts.
    pub facts: FactStore,

    /// NIR type context for the function at `address`.
    ///
    /// Built once from `binary` + `facts`. In Phase 3+, structuring passes can
    /// call `ctx.update_type_context(hints)` to feed new information back so that
    /// the next normalize round can consume it.
    pub type_context: NirTypeContext,
}

impl<'bin> DecompContext<'bin> {
    /// Build a complete decompilation context for the function at `address`.
    ///
    /// This is the canonical single entry point that replaces all three of the
    /// previously scattered constructions:
    ///
    /// ```text
    /// // Before (scattered):
    /// let facts = FactStore::from_binary(binary);          // A
    /// let ctx   = build_nir_type_context(binary, &facts, address);  // C
    ///
    /// // After:
    /// let ctx = DecompContext::new(binary, address);
    /// ```
    pub fn new(binary: &'bin LoadedBinary, address: u64) -> Self {
        let facts = FactStore::from_binary(binary);
        let type_context = build_nir_type_context(binary, &facts, address);
        Self { binary, facts, type_context }
    }

    /// Build a context using a pre-existing [`FactStore`].
    ///
    /// Use this when the caller has already constructed a `FactStore` (e.g. from a
    /// cached or externally-supplied store). Avoids the redundant `from_binary` clone.
    pub fn from_facts(binary: &'bin LoadedBinary, facts: FactStore, address: u64) -> Self {
        let type_context = build_nir_type_context(binary, &facts, address);
        Self { binary, facts, type_context }
    }

    // ── Phase 2+ write API ─────────────────────────────────────────────────────

    /// Record function-level hints discovered during structuring.
    ///
    /// Wired up in Phase 2. Structuring passes call this after detecting
    /// parameter count, local variable layout, or return type from CFG structure.
    ///
    /// The hints are written back to `self.facts` via `record_structuring_hints`,
    /// then `self.type_context` is **immediately rebuilt** from the updated facts.
    /// This means the next normalize round will automatically consume the new hints
    /// without any additional coordination.
    ///
    /// ## Anti-overfitting contract
    /// Only call this from a pass with `InvariantBasis::DominatorTree` or
    /// `InvariantBasis::StronglyConnectedComponents` justification. Do NOT call
    /// for function-name-specific or address-specific heuristics.
    pub fn record_discovered_hints(&mut self, addr: u64, hints: NirFunctionHints) {
        // Write hints back to the live FactStore.
        self.facts.record_structuring_hints(addr, hints);
        // Rebuild type_context from the updated facts so the next normalize round
        // sees the new information without any additional indirection.
        self.type_context = build_nir_type_context(self.binary, &self.facts, addr);
    }
}
