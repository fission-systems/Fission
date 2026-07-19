//! Function-level Ghidra-style iterative collapse helpers (env-gated).
//!
//! Free functions take [`StructuringHost`] so residual driver code can call them
//! without living as `PreviewBuilder` methods.

use crate::cfg_analysis::select_bad_edge;
use crate::host::StructuringHost;
use fission_midend_core::ir::MlilPreviewError;

/// Env gate for the collapse-loop structuring path.
pub fn collapse_loop_admission_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("FISSION_COLLAPSE_LOOP").is_some())
}

/// Virtualize one irreducible back-edge selected by `select_bad_edge`.
pub fn try_virtualize_one_bad_edge(
    host: &mut impl StructuringHost,
    entry: usize,
    exit: usize,
) -> Result<bool, MlilPreviewError> {
    let Some((from, to)) = select_bad_edge(
        entry,
        exit,
        host.successors(),
        host.predecessors(),
        host.fas_virtual_edges(),
    ) else {
        return Ok(false);
    };
    Ok(apply_virtual_goto_edge(host, from, to))
}

/// Remove CFG edge `(from → to)` and record it as a FAS virtual goto.
pub fn apply_virtual_goto_edge(host: &mut impl StructuringHost, from: usize, to: usize) -> bool {
    if host
        .fas_virtual_edges()
        .iter()
        .any(|&(src, dst)| src == from && dst == to)
    {
        return false;
    }
    let Some(pos) = host
        .successors()
        .get(from)
        .and_then(|succs| succs.iter().position(|&succ| succ == to))
    else {
        return false;
    };
    host.successors_mut()[from].remove(pos);
    if let Some(preds) = host.predecessors_mut().get_mut(to) {
        preds.retain(|&pred| pred != from);
    }
    host.fas_virtual_edges_mut().push((from, to));
    host.bump_fas_virtual_goto();
    host.bump_select_bad_edge();
    host.invalidate_terminator_cache(from);
    host.refresh_cfg_fact_cache();
    true
}

/// Whether `(from → to)` was recorded as a FAS virtual goto.
pub fn is_virtual_goto_edge(host: &impl StructuringHost, from: usize, to: usize) -> bool {
    host.fas_virtual_edges()
        .iter()
        .any(|&(src, dst)| src == from && dst == to)
}
