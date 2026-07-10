//! C-like rendering helpers for HIR/NIR output.

pub(super) use super::*;

mod hir_presentation;
mod layer;
mod pipeline;
mod printer;

pub use layer::{LayeredPseudocode, PrintProfile, PseudocodeLayer};

pub(in crate::nir) use pipeline::{
    recover_global_symbol_accesses, render_hir_function_with_global_decls,
    render_layered_pseudocode,
};
pub use printer::render_contracted_wrapper_summary;
pub(in crate::nir) use printer::{
    print_expr, print_hir_function, print_hir_function_with_global_names,
    print_hir_function_with_profile, print_stmt, print_type,
};
