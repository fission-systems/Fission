//! Pure DIR helpers shared by builder / normalize / structuring hosts --
//! the `DirStmt`/`DirExpr` counterpart to [`crate::util`] (which stays
//! `HirStmt`/`HirExpr`-typed for `fission-pcode`'s `render`/printer layer).
//! Each function here is an independently-defined twin of its `util`
//! namesake, not a shared generic implementation -- see
//! `crate::ir::hir`'s module doc for why DIR and HIR are kept as genuinely
//! separate types throughout this crate, not one type under two names.

pub mod expr;
pub mod label_cleanup;
pub mod logic;
pub mod print;
pub mod temp;
pub mod var_rename;

pub use expr::{expr_has_side_effecting_call, expr_type, is_pure_intrinsic_call};
pub use label_cleanup::{
    cleanup_redundant_labels, collect_referenced_label_counts, collect_referenced_labels,
};
pub use logic::{fold_logical_chain, negate_expr, simplify_logical_expr, strip_casts};
pub use print::{format_expr_key, format_lvalue_key};
pub use temp::next_temp_name;
pub use var_rename::rename_vars_in_stmts;
