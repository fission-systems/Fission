//! Function-level Ghidra-style iterative collapse helpers (env-gated).
//!
//! Free functions take [`StructuringHost`] so residual driver code can call them
//! without living as `PreviewBuilder` methods.
//!
//! Unstructured edge pick order (Ghidra `CollapseStructure::selectGoto` + FAS):
//! 1. [`crate::collapse_structure::select_goto`] — LoopBody `emitLikelyEdges`
//!    then TraceDAG likely-gotos
//! 2. [`select_bad_edge`] residual — multi-out join / FAS when no loop/DAG candidate

use crate::cfg_analysis::select_bad_edge;
use crate::collapse_structure::select_goto;
use crate::host::StructuringHost;
use fission_midend_core::ir::MlilPreviewError;

/// Collapse-loop virtualization (TraceDAG / FAS unstructured edge pick).
///
/// Default **off**. `347a4343` briefly made this default-on (Ghidra always
/// runs block structure collapse with likely-gotos) but that regressed real,
/// long-standing switch-structuring tests (`test_switch_case_fallthrough`,
/// `test_switch_skips_to_exit` -- both passing since before this virtualization
/// existed, both losing their `switch (...)` structuring once it defaulted on).
/// Back to opt-in via `FISSION_COLLAPSE_LOOP=1` until that regression is
/// root-caused; the TraceDAG/heritage work itself is untouched and still
/// available behind the flag.
pub fn collapse_loop_admission_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        match std::env::var("FISSION_COLLAPSE_LOOP") {
            Ok(v) if matches!(v.as_str(), "1" | "true" | "on" | "yes") => true,
            // Unset or anything else → disabled.
            _ => false,
        }
    })
}

/// Virtualize one unstructured edge for the collapse fixed-point.
///
/// Prefers Ghidra-aligned `CollapseStructure::selectGoto` (loop-body likely
/// exits + TraceDAG). Falls back to FAS / multi-pred join scoring only when
/// no loop/DAG candidate remains.
pub fn try_virtualize_one_bad_edge(
    host: &mut impl StructuringHost,
    entry: usize,
    exit: usize,
) -> Result<bool, MlilPreviewError> {
    // 1) CollapseStructure selectGoto: LoopBody emitLikelyEdges + TraceDAG.
    if let Some((from, to)) = select_goto(
        entry,
        exit,
        host.successors(),
        host.predecessors(),
        host.fas_virtual_edges(),
    ) {
        return Ok(apply_virtual_goto_edge(host, from, to));
    }
    // 2) Residual FAS / multi-out join (no loop/DAG candidate left).
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
