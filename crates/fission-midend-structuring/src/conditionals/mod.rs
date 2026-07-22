//! Conditional structuring free functions (`try_lower_if*`, short-circuit).
//!
//! Entry points take [`crate::host::StructuringHost`]. Production host is
//! `PreviewBuilder` in `fission-pcode`.

mod if_else;
mod plain_if;
mod short_circuit;

pub use if_else::{try_lower_if_else, try_lower_return_chain_arm, try_reduce_if_else_with_follow};
pub use plain_if::try_lower_if;
pub use short_circuit::{
    try_lower_short_circuit_and, try_lower_short_circuit_and_else, try_lower_short_circuit_if,
    try_lower_short_circuit_or,
};

use crate::host::StructuringHost;
use crate::linear_types::{LinearExit, structuring_diag_enabled};
use fission_midend_core::ir::{DirExpr, DirLValue, DirStmt, MlilPreviewError};
use fission_midend_core::util_dir::expr_has_side_effecting_call;

/// Side-effect-free assign/expr statements that may sit in condition prefixes.
pub fn is_trivial_structuring_stmt(stmt: &DirStmt) -> bool {
    match stmt {
        DirStmt::Assign {
            lhs: DirLValue::Var(_),
            rhs,
        } => !expr_has_side_effecting_call(rhs),
        DirStmt::Expr(expr) => !expr_has_side_effecting_call(expr),
        _ => false,
    }
}

fn forward_join_idx_from_address(
    host: &impl StructuringHost,
    origin_idx: usize,
    address: u64,
) -> Option<usize> {
    host.find_block_index_by_address(address)
        .filter(|join_idx| *join_idx > origin_idx)
}

fn is_forward_exit_from(origin_idx: usize, exit: LinearExit) -> bool {
    match exit {
        LinearExit::Join(join_idx) => join_idx > origin_idx,
        LinearExit::Return | LinearExit::End => true,
    }
}

fn shared_forward_linear_exit(
    host: &mut impl StructuringHost,
    origin_idx: usize,
    lhs_idx: usize,
    rhs_idx: usize,
) -> Result<Option<LinearExit>, MlilPreviewError> {
    let Some(exit) = host.shared_linear_exit(lhs_idx, rhs_idx)? else {
        return Ok(None);
    };
    if is_forward_exit_from(origin_idx, exit) {
        Ok(Some(exit))
    } else {
        Ok(None)
    }
}

fn log_try_lower_if_reject_diag(
    diag: bool,
    idx: usize,
    block_addr: u64,
    reason: &str,
) {
    if diag {
        eprintln!(
            "[DIAG] try_lower_if {}: idx={} block=0x{:x}",
            reason, idx, block_addr
        );
    }
}

fn log_short_circuit_cache(
    host: &impl StructuringHost,
    diag: bool,
    kind: &str,
    start_idx: usize,
    exit: LinearExit,
) {
    if diag {
        eprintln!(
            "[DIAG] try_lower_short_circuit {} {}: start_idx={} exit={:?}",
            kind,
            if host.has_linear_body_cache(start_idx, exit) {
                "cache_hit"
            } else {
                "cache_miss"
            },
            start_idx,
            exit
        );
    }
}

#[allow(dead_code)]
fn _use_structuring_diag() {
    let _ = structuring_diag_enabled();
}
