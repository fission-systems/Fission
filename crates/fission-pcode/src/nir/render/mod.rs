//! C-like rendering helpers for HIR/NIR output.

pub(super) use super::*;

mod pipeline;
mod printer;

pub(in crate::nir) use pipeline::{render_hir_function_with_global_decls, recover_global_symbol_accesses};
pub(in crate::nir) use printer::{print_expr, print_hir_function, print_stmt, print_type};
pub use printer::render_contracted_wrapper_summary;
