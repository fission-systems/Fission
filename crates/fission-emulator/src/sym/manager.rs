use crate::core::Emulator;
use crate::sym::state::SimState;
use crate::sym::exploration::ExplorationTechnique;
use fission_solver::{SatResult, SymExpr};
use anyhow::Result;
use std::collections::HashMap;

/// An angr-style Simulation Manager that orchestrates states across different stashes.
pub struct SimulationManager {
    /// The underlying emulator, used for stepping states via TTD.
    pub emu: Emulator,
    /// Stashes categorize states based on their current status.
    pub stashes: HashMap<String, Vec<SimState>>,
    /// Active exploration techniques.
    pub techniques: Vec<Box<dyn ExplorationTechnique>>,
}

impl SimulationManager {
    /// Convenience constructor for CLI use: creates an initial state from the emulator's current PC.
    pub fn new(emu: Emulator) -> Self {
        let machine_state = emu.state.clone();
        let initial_state = SimState::new(emu.inst_count, emu.pc, machine_state);
        Self::with_initial_state(emu, initial_state)
    }

    pub fn with_initial_state(emu: Emulator, initial_state: SimState) -> Self {
        let mut stashes = HashMap::new();
        stashes.insert("active".to_string(), vec![initial_state]);
        stashes.insert("deadended".to_string(), Vec::new());
        stashes.insert("unsat".to_string(), Vec::new());
        stashes.insert("unconstrained".to_string(), Vec::new());
        stashes.insert("deferred".to_string(), Vec::new());
        stashes.insert("found".to_string(), Vec::new());
        stashes.insert("avoid".to_string(), Vec::new());

        Self { emu, stashes, techniques: Vec::new() }
    }

    pub fn use_technique(&mut self, mut tech: Box<dyn ExplorationTechnique>) {
        tech.setup(&mut self.stashes);
        self.techniques.push(tech);
    }

    /// Step all states in the `active` stash.
    pub fn step(&mut self) -> Result<()> {
        let active_states = self.stashes.get_mut("active").unwrap().drain(..).collect::<Vec<_>>();
        let mut next_active = Vec::new();
        let mut next_deadended = Vec::new();
        let mut next_unsat = Vec::new();

        for state in active_states {
            // Hot-swap the state via O(1) Copy-On-Write instead of TTD seek
            self.emu.state = state.machine_state.clone();
            self.emu.pc = state.pc;
            self.emu.inst_count = state.step_index;

            // Run until the next symbolic branch or halt (Phase D gate).
            self.emu.sym_events.clear();
            self.emu.sym_stop_requested = false;
            self.emu.halt_requested = false;
            let run_result = self.emu.run();
            // Clear stop so subsequent forks can run again.
            self.emu.sym_stop_requested = false;

            if let Err(_) = run_result {
                // If it halted or errored, it's deadended
                let final_step = self.emu.inst_count;
                let final_pc = self.emu.pc;
                let final_ms = self.emu.state.clone();
                next_deadended.push(SimState::new(final_step, final_pc, final_ms));
                continue;
            }

            // Check if any symbolic branches were emitted
            let branches = std::mem::take(&mut self.emu.sym_events);
            if branches.is_empty() {
                // No branches, just deadended normally
                next_deadended.push(SimState::new(self.emu.inst_count, self.emu.pc, self.emu.state.clone()));
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

                        // Extract the emulator memory state at the branch point
                        let fork_ms = self.emu.state.clone();

                        // True path state
                        let taken_state = state.with_constraint(
                            if branch.condition_val_taken { true_expr.clone() } else { false_expr.clone() },
                            branch.step_index,
                            branch.pc,
                            fork_ms.clone()
                        );

                        // Alternate path state
                        let alt_state = state.with_constraint(
                            if branch.condition_val_taken { false_expr } else { true_expr },
                            branch.step_index,
                            branch.alt_addr.unwrap_or(branch.pc), // simplify
                            fork_ms
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
                        // Note: state still needs to be updated with the advanced machine state
                        let mut advanced_state = state.clone();
                        advanced_state.step_index = self.emu.inst_count;
                        advanced_state.pc = self.emu.pc;
                        advanced_state.machine_state = self.emu.state.clone();
                        next_deadended.push(advanced_state);
                    }
                }
            }
        }

        self.stashes.get_mut("active").unwrap().extend(next_active);
        self.stashes.get_mut("deadended").unwrap().extend(next_deadended);
        self.stashes.get_mut("unsat").unwrap().extend(next_unsat);

        // Run techniques
        let mut techniques = std::mem::take(&mut self.techniques);
        for tech in techniques.iter_mut() {
            tech.step(&mut self.stashes);
        }
        self.techniques = techniques;

        Ok(())
    }

    /// Step until no states remain in the `active` stash, or a technique signals completion.
    pub fn step_all(&mut self) -> Result<()> {
        loop {
            if self.stashes.get("active").unwrap().is_empty() {
                break;
            }
            if self.techniques.iter().any(|t| t.is_complete(&self.stashes)) {
                break;
            }
            self.step()?;
        }
        Ok(())
    }

    /// Alias for `step_all()` — used by the CLI `--sym-explore` flag.
    pub fn explore(&mut self) -> Result<()> {
        self.step_all()
    }
}
