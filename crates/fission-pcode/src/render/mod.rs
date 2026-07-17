//! Dual-layer C presentation for structured IR (NIR / HIR print surfaces).
//!
//! Owner layer per ADR 0008 / 0011: consumes structured trees only; does not
//! perform semantic recovery. Implementation guide: `render/AGENTS.md`.
//!
//! Dependency direction: `render` → `nir` types/API; `nir` orchestration may
//! call into `render` for print/presentation (crate-local sibling cycle).

// Structured-IR and option types used by printer / presentation submodules.
// Keep this bridge explicit so render does not depend on normalize/structuring.
pub(crate) use crate::nir::{
    HirBinaryOp, HirExpr, HirFunction, HirLValue, HirStmt, HirUnaryOp, MlilPreviewOptions,
    NirBinding, NirBindingOrigin, NirType, expr_type,
};

mod hir_presentation;
mod layer;
mod pipeline;
mod printer;

/// Keep in sync with `nir::structuring::switch::SWITCH_FALLTHROUGH_SENTINEL`
/// (`"__fallthrough"`). Duplicated so crate-root `render` does not reach into
/// private `nir::structuring`.
pub(crate) const SWITCH_FALLTHROUGH_SENTINEL: &str = "__fallthrough";

pub use layer::{LayeredPseudocode, PrintProfile, PseudocodeLayer};

pub(crate) use pipeline::{
    recover_global_symbol_accesses, render_hir_function_with_global_decls,
    render_layered_pseudocode,
};
pub use printer::render_contracted_wrapper_summary;
pub(crate) use printer::{
    print_expr, print_hir_function, print_hir_function_with_global_names,
    print_hir_function_with_profile, print_stmt, print_type,
};
