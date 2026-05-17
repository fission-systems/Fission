//! C-like rendering helpers for HIR/NIR output.

pub(super) use super::*;

mod printer;

pub(in crate::nir) use printer::{print_expr, print_hir_function, print_stmt, print_type};
