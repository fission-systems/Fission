//! Dual-layer C presentation for structured IR (NIR / HIR print surfaces).
//!
//! Owner layer per ADR 0008 / 0011: consumes structured trees only; does not
//! perform semantic recovery. Implementation guide: `render/AGENTS.md`.
//!
//! ```text
//! nir (types / builder / normalize / structuring)
//!         │ consume structured tree only
//!         ▼
//!      render
//!        ├── layer        dual-surface contracts
//!        ├── printer      C print (Nir / Hir profiles)
//!        ├── presentation HIR-only tree polish
//!        └── pipeline     layered render orchestration
//! ```

// Structured-IR and option types used by printer / presentation submodules.
// Keep this bridge explicit so render does not depend on normalize/structuring.
pub(crate) use crate::midend::{
    HirBinaryOp, HirExpr, HirFunction, HirLValue, HirStmt, HirUnaryOp, MlilPreviewOptions,
    NirBinding, NirBindingOrigin, NirType, SWITCH_FALLTHROUGH_SENTINEL,
};

/// HIR-side counterpart to `crate::midend::support::expr_util::expr_type`
/// (which is DIR-typed, used by builder) -- `render` consumes the final
/// structured `HirFunction`, never `DirExpr`, so it needs its own
/// `&HirExpr`-typed twin rather than depending on builder's Dir-side copy.
pub(crate) fn expr_type(expr: &HirExpr) -> NirType {
    match expr {
        HirExpr::AddressOfGlobal(_) => NirType::Ptr(Box::new(NirType::Unknown)),
        HirExpr::Var(_) => NirType::Unknown,
        HirExpr::Const(_, ty)
        | HirExpr::Unary { ty, .. }
        | HirExpr::Binary { ty, .. }
        | HirExpr::Select { ty, .. }
        | HirExpr::Call { ty, .. }
        | HirExpr::Load { ty, .. }
        | HirExpr::FieldAccess { ty, .. }
        | HirExpr::Index { elem_ty: ty, .. } => ty.clone(),
        HirExpr::Cast { ty, .. } => ty.clone(),
        HirExpr::PtrOffset { .. } => NirType::Ptr(Box::new(NirType::Unknown)),
        HirExpr::AggregateCopy { size, .. } => NirType::Aggregate {
            size: *size,
            fields: vec![],
        },
    }
}

mod globals;
mod layer;
mod layered;
/// Facade re-exports for layered print + global recovery (see `pipeline.rs`).
mod pipeline;
mod presentation;
mod printer;

pub use layer::{LayeredPseudocode, PrintProfile, PseudocodeLayer};

pub(crate) use pipeline::{
    recover_global_symbol_accesses, render_hir_function_with_global_decls,
    render_layered_pseudocode,
};
pub(crate) use presentation::apply_hir_presentation;
pub use printer::render_contracted_wrapper_summary;
pub(crate) use printer::{
    print_expr, print_hir_function, print_hir_function_with_global_names,
    print_hir_function_with_profile, print_stmt, print_type,
};
