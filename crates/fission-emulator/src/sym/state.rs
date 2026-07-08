use fission_solver::SymExpr;
use crate::pcode::state::MachineState;

/// A concolic execution state.
/// Instead of deep-copying the entire emulator, we rely on TTD (Time-Travel Debugging).
/// A `SimState` simply points to a step in the execution history and tracks the path condition.
#[derive(Clone)]
pub struct SimState {
    /// The TTD step index this state is currently at.
    pub step_index: u64,
    /// The current Program Counter (PC) of this state.
    pub pc: u64,
    /// Constraints accumulated along this specific execution path.
    pub history: SimStateHistory,
    /// Copy-on-Write memory context.
    pub machine_state: MachineState,
}

impl std::fmt::Debug for SimState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SimState")
         .field("step_index", &self.step_index)
         .field("pc", &self.pc)
         .field("history", &self.history)
         .finish()
    }
}

#[derive(Debug, Clone, Default)]
pub struct SimStateHistory {
    /// The list of branch constraints that were taken to reach this state.
    pub constraints: Vec<SymExpr>,
}

impl SimState {
    pub fn new(step_index: u64, pc: u64, machine_state: MachineState) -> Self {
        Self {
            step_index,
            pc,
            history: SimStateHistory::default(),
            machine_state,
        }
    }

    pub fn with_constraint(&self, constraint: SymExpr, next_step: u64, next_pc: u64, machine_state: MachineState) -> Self {
        let mut new_state = self.clone(); // Clones history
        new_state.history.constraints.push(constraint);
        new_state.step_index = next_step;
        new_state.pc = next_pc;
        new_state.machine_state = machine_state;
        new_state
    }
}
