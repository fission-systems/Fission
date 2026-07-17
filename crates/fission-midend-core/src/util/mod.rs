//! Pure HIR helpers shared by normalize / structuring / builder hosts.

pub mod expr;
pub mod label_cleanup;
pub mod logic;
pub mod print;
pub mod temp;
pub mod var_rename;

pub use expr::{expr_has_side_effecting_call, expr_type, is_pure_intrinsic_call};
pub use label_cleanup::{cleanup_redundant_labels, collect_referenced_label_counts, collect_referenced_labels};
pub use logic::{fold_logical_chain, negate_expr, simplify_logical_expr, strip_casts};
pub use print::{print_expr, print_lvalue};
pub use temp::next_temp_name;
pub use var_rename::rename_vars_in_stmts;
