//! Pass pipeline orchestration (`normalize_hir_function` and helpers).

mod run;

pub(crate) use run::{
    is_large_hir_function, normalize_expr, normalize_function_body, normalize_hir_function,
    normalize_stmt,
};
