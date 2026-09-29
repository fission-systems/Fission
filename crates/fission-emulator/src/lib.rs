pub mod arch;
mod control_flow;
pub mod core;
pub mod interp;
pub mod jit;
pub mod loader;
pub mod metrics;
pub mod observe;
pub mod os;
pub mod pcode;
pub mod snapshot;
pub mod srd;
pub mod sym;
pub mod taint;
pub mod trace;

/// Convert a byte-oriented emulator size to the solver's bit-width unit.
pub(crate) const fn bit_width_from_byte_size(byte_size: u32) -> u32 {
    byte_size.saturating_mul(8)
}

pub use arch::{ArchInfo, Endianness};
pub use core::{Emulator, RunOutcome};
pub use metrics::{BudgetReport, EmulatorMetrics, SandboxMetricsReport};
pub use observe::{BehaviorEvent, BehaviorLog, Coverage, ObserveMask, Observer, ShadowMode};
pub use os::{BareMetalEnv, HleResult, LinuxEnv, OsEnvironment, WindowsEnv};
pub use pcode::eval::Evaluator;
pub use pcode::state::MachineState;
pub use snapshot::EmulatorSnapshot;
pub use srd::{
    CaptureOpts, FieldDelta, MallocngProbe, OwnerLayer, SemanticReplayDelta, SemanticReplaySnapshot,
};
pub use taint::{TaintDependencyKind, TaintHit, TaintSource, TaintState};

#[cfg(test)]
mod tests {
    use super::bit_width_from_byte_size;
    use fission_solver::solver::Solver;

    #[test]
    fn byte_sizes_cross_into_solver_as_bit_widths() {
        for (bytes, expected_bits) in [(1, 8), (4, 32), (8, 64)] {
            assert_eq!(bit_width_from_byte_size(bytes), expected_bits);
            let mut solver = Solver::new();
            let id = solver.register_var("boundary".to_string(), bit_width_from_byte_size(bytes));
            assert_eq!(solver.nodes[&id].get_bit_width(), expected_bits);
        }
    }
}
