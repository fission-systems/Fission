use fission_loader::loader::types::InferredTypeInfo;

use super::options::NirFunctionHints;

/// Abstract interface for a decompilation context that can accept newly discovered facts.
///
/// This trait allows the `fission-pcode` normalizer and structurer passes to record
/// structural findings (like parameter counts from dominator tree analysis) without
/// depending directly on `fission-decompiler`'s `DecompContext` or `FactStore`.
pub trait DecompFacts {
    /// Record function-level hints discovered during structuring or normalization.
    ///
    /// ## Anti-overfitting contract
    /// Only call this from a pass with structural invariant justifications (e.g.
    /// dominance, post-dominance, or SCC), not for specific function names.
    fn record_discovered_hints(&mut self, addr: u64, hints: NirFunctionHints);

    /// Record a discovered type constraint or inference for an address.
    fn record_inferred_type(&mut self, addr: u64, type_info: InferredTypeInfo);
}
