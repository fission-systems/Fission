use crate::core::Emulator;
use crate::sym::state::SimState;
use crate::sym::exploration::ExplorationTechnique;
use fission_solver::SymExpr;
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
    /// Safety bound on `step` iterations during `explore` / `step_all`.
    pub max_steps: u64,
    /// How many `step()` calls have completed in this explore session.
    pub steps_taken: u64,
}

impl SimulationManager {
    /// Convenience constructor for CLI use: creates an initial state from the emulator's current PC.
    pub fn new(emu: Emulator) -> Self {
        let machine_state = emu.state.clone();
        let initial_state = SimState::new(emu.inst_count, emu.pc, machine_state);
        Self::with_initial_state(emu, initial_state)
    }

    pub fn with_initial_state(mut emu: Emulator, initial_state: SimState) -> Self {
        // Exploration needs the JIT symbolic gate to stop at tainted branches.
        emu.concolic_stop_on_branch = true;
        let mut stashes = HashMap::new();
        stashes.insert("active".to_string(), vec![initial_state]);
        stashes.insert("deadended".to_string(), Vec::new());
        stashes.insert("unsat".to_string(), Vec::new());
        stashes.insert("unconstrained".to_string(), Vec::new());
        stashes.insert("deferred".to_string(), Vec::new());
        stashes.insert("found".to_string(), Vec::new());
        stashes.insert("avoid".to_string(), Vec::new());

        Self {
            emu,
            stashes,
            techniques: Vec::new(),
            max_steps: 64,
            steps_taken: 0,
        }
    }

    pub fn with_max_steps(mut self, n: u64) -> Self {
        self.max_steps = n;
        self
    }

    pub fn use_technique(&mut self, mut tech: Box<dyn ExplorationTechnique>) {
        tech.setup(&mut self.stashes);
        self.techniques.push(tech);
    }

    pub fn stash_len(&self, name: &str) -> usize {
        self.stashes.get(name).map(|v| v.len()).unwrap_or(0)
    }

    /// Step all states in the `active` stash.
    pub fn step(&mut self) -> Result<()> {
        self.steps_taken = self.steps_taken.saturating_add(1);
        let active_states = self
            .stashes
            .get_mut("active")
            .unwrap()
            .drain(..)
            .collect::<Vec<_>>();
        let mut next_active = Vec::new();
        let mut next_deadended = Vec::new();
        let next_unsat: Vec<SimState> = Vec::new();

        for state in active_states {
            // Hot-swap the state via Copy-On-Write instead of TTD seek.
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

            if run_result.is_err() {
                next_deadended.push(SimState::new(
                    self.emu.inst_count,
                    self.emu.pc,
                    self.emu.state.clone(),
                ));
                continue;
            }

            let branches = std::mem::take(&mut self.emu.sym_events);
            if branches.is_empty() {
                // Halted or ran to limit without a symbolic branch.
                next_deadended.push(SimState::new(
                    self.emu.inst_count,
                    self.emu.pc,
                    self.emu.state.clone(),
                ));
                continue;
            }

            // Fork on the first symbolic branch.
            let branch = branches.into_iter().next().unwrap();
            let Some(cond_node) = branch.condition_node else {
                next_deadended.push(SimState::new(
                    self.emu.inst_count,
                    self.emu.pc,
                    self.emu.state.clone(),
                ));
                continue;
            };
            let Some(cond_expr) = self.emu.solver.nodes.get(&cond_node).cloned() else {
                next_deadended.push(SimState::new(
                    self.emu.inst_count,
                    self.emu.pc,
                    self.emu.state.clone(),
                ));
                continue;
            };

            let true_expr = cond_expr.clone();
            let false_expr = SymExpr::Eq(
                Box::new(cond_expr),
                Box::new(SymExpr::Const { val: 0, size: 1 }),
            );

            // Concrete path already executed to `emu.pc` (selected by gate stop).
            let concrete_pc = self.emu.pc;
            let alt_pc = branch.alt_addr.unwrap_or(concrete_pc);
            let fork_ms = self.emu.state.clone();
            let step = self.emu.inst_count;

            let (concrete_constraint, alt_constraint) = if branch.condition_val_taken {
                (true_expr, false_expr)
            } else {
                (false_expr, true_expr)
            };

            let concrete_state =
                state.with_constraint(concrete_constraint, step, concrete_pc, fork_ms.clone());
            let alt_state = state.with_constraint(alt_constraint, step, alt_pc, fork_ms);

            // Always keep both forks active for now. Full path-condition SAT is
            // still incomplete (can panic on some AST shapes); prune later when
            // the solver is hardened. Constraints remain on SimState for later.
            next_active.push(concrete_state);
            next_active.push(alt_state);
        }

        self.stashes
            .get_mut("active")
            .unwrap()
            .extend(next_active);
        self.stashes
            .get_mut("deadended")
            .unwrap()
            .extend(next_deadended);
        self.stashes.get_mut("unsat").unwrap().extend(next_unsat);

        let mut techniques = std::mem::take(&mut self.techniques);
        for tech in techniques.iter_mut() {
            tech.step(&mut self.stashes);
        }
        self.techniques = techniques;

        Ok(())
    }

    /// Step until no states remain in the `active` stash, a technique completes,
    /// or `max_steps` is hit.
    pub fn step_all(&mut self) -> Result<()> {
        while self.steps_taken < self.max_steps {
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
