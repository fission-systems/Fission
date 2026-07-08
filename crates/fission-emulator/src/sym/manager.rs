use crate::core::Emulator;
use crate::sym::state::SimState;
use fission_solver::{SatResult, SymExpr};
use anyhow::Result;
use std::collections::HashMap;

/// An angr-style Simulation Manager that orchestrates states across different stashes.
pub struct SimulationManager {
    /// The underlying emulator, used for stepping states via TTD.
    pub emu: Emulator,
    /// Stashes categorize states based on their current status.
    pub stashes: HashMap<String, Vec<SimState>>,
}

impl SimulationManager {
    /// Convenience constructor for CLI use: creates an initial state from the emulator's current PC.
    pub fn new(emu: Emulator) -> Self {
        let initial_state = SimState::new(emu.inst_count, emu.pc);
        Self::with_initial_state(emu, initial_state)
    }

    pub fn with_initial_state(emu: Emulator, initial_state: SimState) -> Self {
        let mut stashes = HashMap::new();
        stashes.insert("active".to_string(), vec![initial_state]);
        stashes.insert("deadended".to_string(), Vec::new());
        stashes.insert("unsat".to_string(), Vec::new());
        stashes.insert("unconstrained".to_string(), Vec::new());

        Self { emu, stashes }
    }

    /// Step all states in the `active` stash.
    pub fn step(&mut self) -> Result<()> {
        let active_states = self.stashes.get_mut("active").unwrap().drain(..).collect::<Vec<_>>();
        let mut next_active = Vec::new();
        let mut next_deadended = Vec::new();
        let mut next_unsat = Vec::new();

        for state in active_states {
            // Seek the emulator to this state's TTD snapshot.
            // If no snapshot is available (e.g. step=0 before first checkpoint),
            // fall through and run from the current emulator position.
            let seek_ok = self.emu.ttd_seek(state.step_index).is_ok();
            if !seek_ok {
                tracing::debug!("No TTD snapshot at step {}; running from current position", state.step_index);
            }

            // Set the PC from the state record
            self.emu.pc = state.pc;

            // Run until the next symbolic branch or halt
            self.emu.sym_events.clear();
            let run_result = self.emu.run();

            if let Err(_) = run_result {
                // If it halted or errored, it's deadended
                let final_step = self.emu.inst_count;
                let final_pc = self.emu.pc;
                next_deadended.push(SimState::new(final_step, final_pc));
                continue;
            }

            // Check if any symbolic branches were emitted
            let branches = std::mem::take(&mut self.emu.sym_events);
            if branches.is_empty() {
                // No branches, just deadended normally
                next_deadended.push(SimState::new(self.emu.inst_count, self.emu.pc));
            } else {
                // A branch occurred! We fork the state.
                let branch = branches.into_iter().next().unwrap(); // Take the first branch
                
                // For a symbolic branch, we have the taken path and the alternate path
                if let Some(cond_node) = branch.condition_node {
                    if let Some(cond_expr) = self.emu.solver.nodes.get(&cond_node).cloned() {
                        let true_expr = cond_expr.clone();
                        let false_expr = SymExpr::Eq(
                            Box::new(cond_expr.clone()),
                            Box::new(SymExpr::Const { val: 0, size: 1 })
                        );

                        // True path state
                        let taken_state = state.with_constraint(
                            if branch.condition_val_taken { true_expr.clone() } else { false_expr.clone() },
                            branch.step_index,
                            branch.pc
                        );

                        // Alternate path state
                        let alt_state = state.with_constraint(
                            if branch.condition_val_taken { false_expr } else { true_expr },
                            branch.step_index,
                            branch.alt_addr.unwrap_or(branch.pc) // simplify
                        );

                        // Feasibility check
                        let solver = &mut self.emu.solver;
                        let state_oracle = &self.emu.state;
                        
                        if solver.satisfiable_with_oracle(&taken_state.history.constraints, Some(state_oracle)) {
                            next_active.push(taken_state);
                        } else {
                            next_unsat.push(taken_state);
                        }

                        if solver.satisfiable_with_oracle(&alt_state.history.constraints, Some(state_oracle)) {
                            next_active.push(alt_state);
                        } else {
                            next_unsat.push(alt_state);
                        }
                    } else {
                        // Unconstrained or missing node
                        next_deadended.push(state);
                    }
                }
            }
        }

        self.stashes.get_mut("active").unwrap().extend(next_active);
        self.stashes.get_mut("deadended").unwrap().extend(next_deadended);
        self.stashes.get_mut("unsat").unwrap().extend(next_unsat);

        Ok(())
    }

    /// Step until no states remain in the `active` stash.
    pub fn step_all(&mut self) -> Result<()> {
        while !self.stashes.get("active").unwrap().is_empty() {
            self.step()?;
        }
        Ok(())
    }

    /// Alias for `step_all()` — used by the CLI `--sym-explore` flag.
    pub fn explore(&mut self) -> Result<()> {
        self.step_all()
    }
}
