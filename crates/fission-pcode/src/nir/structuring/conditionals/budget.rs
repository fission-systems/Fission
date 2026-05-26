use super::*;

use std::time::Instant;

impl IfLoweringBudget {
    pub(in crate::nir::structuring) fn new(
        options: &MlilPreviewOptions,
        idx: usize,
        block_addr: u64,
        label: &'static str,
        structuring_start: Option<Instant>,
    ) -> Self {
        Self {
            enabled: true,
            start: Instant::now(),
            subcalls: 0,
            tripped: false,
            idx,
            block_addr,
            label,
            structuring_start,
        }
    }

    pub(in crate::nir::structuring) fn checkpoint(&mut self, stage: &str) -> bool {
        if self.tripped || !self.enabled {
            return self.tripped;
        }
        self.subcalls += 1;

        if let Some(total_start) = self.structuring_start {
            let total_elapsed_ms = total_start.elapsed().as_secs_f64() * 1000.0;
            if total_elapsed_ms > 500.0 {
                self.tripped = true;
                if structuring_diag_enabled() {
                    eprintln!(
                        "[DIAG] TOTAL structuring budget exceeded: idx={} block=0x{:x} stage={} total_elapsed={:.3}s",
                        self.idx,
                        self.block_addr,
                        stage,
                        total_start.elapsed().as_secs_f64()
                    );
                }
                return true;
            }
        }

        let elapsed_ms = self.start.elapsed().as_secs_f64() * 1000.0;
        if self.subcalls > CONDITION_RECOVERY_SUBCALL_LIMIT
            || elapsed_ms > CONDITION_RECOVERY_BUDGET_MS
        {
            self.tripped = true;
            if structuring_diag_enabled() {
                eprintln!(
                    "[DIAG] {} budget_exceeded: idx={} block=0x{:x} stage={} elapsed={:.3}s subcalls={}",
                    self.label,
                    self.idx,
                    self.block_addr,
                    stage,
                    self.start.elapsed().as_secs_f64(),
                    self.subcalls
                );
            }
        }
        self.tripped
    }
}
