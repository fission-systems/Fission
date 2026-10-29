//! Render pipeline facade: final-surface print + global recovery.
//!
//! Split implementation:
//! - [`super::layered`] — HIR print and global decls
//! - [`super::globals`] — pre-print global symbol access recovery

pub(crate) use super::globals::recover_global_symbol_accesses;
pub(crate) use super::layered::render_hir_function_with_global_decls;
#[cfg(test)]
pub(crate) use super::layered::render_layered_pseudocode;
