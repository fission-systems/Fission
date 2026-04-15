use super::*;

use std::time::Instant;

impl IfLoweringBudget {
    pub(in crate::nir::structuring) fn new(
        options: &MlilPreviewOptions,
        idx: usize,
        block_addr: u64,
        label: &'static str,
    ) -> Self {
        Self {
            enabled: options.target_profile().if_lowering_budget_enabled(),
            start: Instant::now(),
            subcalls: 0,
            tripped: false,
            idx,
            block_addr,
            label,
        }
    }

    pub(in crate::nir::structuring) fn checkpoint(&mut self, stage: &str) -> bool {
        if self.tripped || !self.enabled {
            return self.tripped;
        }
        self.subcalls += 1;
        let elapsed_ms = self.start.elapsed().as_secs_f64() * 1000.0;
        if self.subcalls > X86_TRY_LOWER_IF_SUBCALL_LIMIT || elapsed_ms > X86_TRY_LOWER_IF_BUDGET_MS
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
