//! Conditional structuring — thin wrappers over free functions in
//! `fission-midend-structuring::conditionals`.

use super::*;

pub use fission_midend_structuring::conditionals::is_trivial_structuring_stmt;
pub use fission_midend_structuring::{
    try_lower_if, try_lower_if_else, try_lower_return_chain_arm, try_lower_short_circuit_and,
    try_lower_short_circuit_and_else, try_lower_short_circuit_if, try_lower_short_circuit_or,
    try_reduce_if_else_with_follow,
};

impl<'a> PreviewBuilder<'a> {
    pub(in crate::midend::structuring) fn try_lower_if(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        try_lower_if(self, idx)
    }

    pub(in crate::midend::structuring) fn try_lower_if_else(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        try_lower_if_else(self, idx)
    }

    pub(in crate::midend::structuring) fn try_reduce_if_else_with_follow(
        &mut self,
        idx: usize,
        follow: Option<usize>,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        try_reduce_if_else_with_follow(self, idx, follow)
    }

    pub(in crate::midend::structuring) fn try_lower_short_circuit_if(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        try_lower_short_circuit_if(self, idx)
    }

    pub(in crate::midend::structuring) fn try_lower_short_circuit_and(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        try_lower_short_circuit_and(self, idx)
    }

    pub(in crate::midend::structuring) fn try_lower_short_circuit_and_else(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        try_lower_short_circuit_and_else(self, idx)
    }

    pub(in crate::midend::structuring) fn try_lower_short_circuit_or(
        &mut self,
        idx: usize,
    ) -> Result<Option<(HirStmt, usize)>, MlilPreviewError> {
        try_lower_short_circuit_or(self, idx)
    }

    /// Compatibility wrapper for callers that still pass a diag flag.
    pub(crate) fn log_try_lower_if_reject(
        &self,
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
}
