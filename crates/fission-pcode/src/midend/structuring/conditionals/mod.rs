use super::*;

mod budget;
mod if_else;
mod plain_if;
mod short_circuit;

#[derive(Debug)]
struct PlainIfCandidate {
    cond_prefix: Vec<HirStmt>,
    cond: HirExpr,
    body_idx: usize,
    exit: LinearExit,
    block_addr: u64,
}

impl<'a> PreviewBuilder<'a> {
    fn log_try_lower_if_reject(&self, diag: bool, idx: usize, block_addr: u64, reason: &str) {
        if diag {
            eprintln!(
                "[DIAG] try_lower_if {}: idx={} block=0x{:x}",
                reason, idx, block_addr
            );
        }
    }

    fn log_short_circuit_cache(&self, diag: bool, kind: &str, start_idx: usize, exit: LinearExit) {
        if diag {
            eprintln!(
                "[DIAG] try_lower_short_circuit {} {}: start_idx={} exit={:?}",
                kind,
                if self.has_linear_body_cache(start_idx, exit) {
                    "cache_hit"
                } else {
                    "cache_miss"
                },
                start_idx,
                exit
            );
        }
    }

    fn forward_join_idx_from_address(&self, origin_idx: usize, address: u64) -> Option<usize> {
        self.find_block_index_by_address(address)
            .filter(|join_idx| *join_idx > origin_idx)
    }

    fn is_forward_exit_from(&self, origin_idx: usize, exit: LinearExit) -> bool {
        match exit {
            LinearExit::Join(join_idx) => join_idx > origin_idx,
            LinearExit::Return | LinearExit::End => true,
        }
    }

    fn shared_forward_linear_exit(
        &mut self,
        origin_idx: usize,
        lhs_idx: usize,
        rhs_idx: usize,
    ) -> Result<Option<LinearExit>, MlilPreviewError> {
        let Some(exit) = self.shared_linear_exit(lhs_idx, rhs_idx)? else {
            return Ok(None);
        };
        if self.is_forward_exit_from(origin_idx, exit) {
            Ok(Some(exit))
        } else {
            Ok(None)
        }
    }
}
