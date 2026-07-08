use fission_solver::SymExpr;

/// A concolic execution state.
/// Instead of deep-copying the entire emulator, we rely on TTD (Time-Travel Debugging).
/// A `SimState` simply points to a step in the execution history and tracks the path condition.
#[derive(Debug, Clone)]
pub struct SimState {
    /// The TTD step index this state is currently at.
    pub step_index: u64,
    /// The current Program Counter (PC) of this state.
    pub pc: u64,
    /// Constraints accumulated along this specific execution path.
    pub history: SimStateHistory,
}

#[derive(Debug, Clone, Default)]
pub struct SimStateHistory {
    /// The list of branch constraints that were taken to reach this state.
    pub constraints: Vec<SymExpr>,
}

impl SimState {
    pub fn new(step_index: u64, pc: u64) -> Self {
        Self {
            step_index,
            pc,
            history: SimStateHistory::default(),
        }
    }

    pub fn with_constraint(&self, constraint: SymExpr, next_step: u64, next_pc: u64) -> Self {
        let mut new_state = self.clone();
        new_state.history.constraints.push(constraint);
        new_state.step_index = next_step;
        new_state.pc = next_pc;
        new_state
    }
}
