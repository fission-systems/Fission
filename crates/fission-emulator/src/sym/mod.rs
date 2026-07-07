use crate::core::{Emulator, SymBranch};
use fission_solver::{SatResult, SymExpr};
use anyhow::Result;

/// Full solver-backed concolic exploration engine.
///
/// Each execution path is:
///   1. Run concretely with TTD recording.
///   2. Symbolic branches are collected in `emu.sym_events`.
///   3. For each unexplored branch, the condition is negated via `solver.assert(!cond)`.
///   4. `solver.check_sat()` verifies the alternate path is feasible.
///   5. If SAT, the model (concrete stdin bytes) is extracted and injected into the emulator.
///   6. The emulator rewinds to the snapshot nearest the branch and re-runs with new input.
pub struct SymbolicExecutor {
    pub emu: Emulator,
    /// Unexplored branch events waiting to be explored.
    pub queue: Vec<SymBranch>,
}

impl SymbolicExecutor {
    pub fn new(emu: Emulator) -> Self {
        Self { emu, queue: Vec::new() }
    }

    /// Main exploration loop.
    pub fn explore(&mut self) -> Result<()> {
        tracing::info!("Starting TTD-based Symbolic Exploration");

        let mut path_id = 1usize;
        loop {
            tracing::info!("--- Exploring Path {} ---", path_id);

            // Run until halt / instruction limit
            if let Err(e) = self.emu.run() {
                tracing::warn!("Path {} terminated with error: {}", path_id, e);
            }

            // Drain newly discovered unexplored branches into queue
            let new_branches = std::mem::take(&mut self.emu.sym_events);
            if !new_branches.is_empty() {
                tracing::info!("Path {} produced {} new branch events", path_id, new_branches.len());
                self.queue.extend(new_branches);
            }

            // Pick the next unexplored branch
            let Some(branch) = self.queue.pop() else {
                tracing::info!("Exploration complete — no more unexplored paths.");
                break;
            };

            path_id += 1;
            tracing::info!(
                "Next: step={} PC=0x{:X} taken={} sym_cond={:?}",
                branch.step_index, branch.pc, branch.condition_val_taken, branch.condition_node
            );

            // ── Solver-backed feasibility check ───────────────────────────────
            if let Some(cond_node_id) = branch.condition_node {
                // Look up the AST expression for the condition
                if let Some(cond_expr) = self.emu.solver.nodes.get(&cond_node_id).cloned() {
                    // Invert: if the path was taken (cond == true), assert cond == false for alternate
                    let inverted = if branch.condition_val_taken {
                        // Original was TRUE → invert to FALSE → assert (cond == 0)
                        SymExpr::new_eq(cond_expr, SymExpr::new_const(0, 1))
                    } else {
                        // Original was FALSE → invert to TRUE → assert (cond != 0)
                        SymExpr::new_neq(cond_expr, SymExpr::new_const(0, 1))
                    };

                    self.emu.solver.push();
                    self.emu.solver.assert(inverted);

                    match self.emu.solver.check_sat() {
                        Ok(SatResult::Sat) => {
                            tracing::info!("Branch at PC=0x{:X} is SAT — extracting model", branch.pc);

                            // Extract concrete stdin bytes from model and inject them
                            self.inject_model_into_stdin();

                            // Rewind TTD to snapshot at/before branch step
                            if let Err(e) = self.emu.ttd_seek(branch.step_index) {
                                tracing::error!("TTD seek failed: {}", e);
                                self.emu.solver.pop();
                                continue;
                            }

                            // Don't pop: keep path constraints for this sub-exploration
                            // The solver context will be popped naturally at the end of this path.
                            // For simplicity: pop now (we already injected the concrete input)
                            self.emu.solver.pop();

                            // Continue to run from rewound state with new concrete input
                        }
                        Ok(SatResult::Unsat) => {
                            tracing::info!("Branch at PC=0x{:X} is UNSAT — skipping", branch.pc);
                            self.emu.solver.pop();
                            continue;
                        }
                        Ok(SatResult::Unknown) | Err(_) => {
                            tracing::warn!("Branch at PC=0x{:X} — solver returned Unknown, falling back to PC-force", branch.pc);
                            self.emu.solver.pop();
                            self.force_branch(&branch);
                        }
                    }
                } else {
                    tracing::warn!("condition_node id={} not found in solver nodes — falling back to PC-force", cond_node_id);
                    self.force_branch_after_seek(&branch)?;
                }
            } else {
                // No taint on condition — use TTD seek + PC-force (concolic scaffolding)
                self.force_branch_after_seek(&branch)?;
            }
        }

        Ok(())
    }

    /// Inject concrete SAT model values back into the emulator's stdin buffer.
    /// Scans solver.nodes for Var nodes whose names start with "stdin" and writes their
    /// model values into the corresponding stdin_buffer positions.
    fn inject_model_into_stdin(&mut self) {
        let mut injections: Vec<(usize, u8)> = Vec::new();

        for (&node_id, expr) in &self.emu.solver.nodes {
            if let fission_solver::SymExpr::Var { name, .. } = expr {
                // Name format: "stdin_<hex_addr>" or "stdin_console_<hex_addr>"
                let addr_str = name
                    .strip_prefix("stdin_console_")
                    .or_else(|| name.strip_prefix("stdin_"))
                    .and_then(|s| u64::from_str_radix(s, 16).ok());

                if let Some(_addr) = addr_str {
                    if let Some(value) = self.emu.solver.model.get(&node_id) {
                        // We don't have a direct addr→stdin_buffer index mapping here,
                        // so we use the sequential node appearance order.
                        // A more robust approach would store (var_id, stdin_idx) at taint time.
                        // For now, collect and sort by addr to reconstruct ordering.
                        injections.push((_addr as usize, *value as u8));
                    }
                }
            }
        }

        if injections.is_empty() {
            tracing::debug!("inject_model_into_stdin: no stdin vars in model");
            return;
        }

        // Sort by address to reconstruct sequential stdin order
        injections.sort_by_key(|(addr, _)| *addr);

        // Find the base address (lowest) to compute buffer indices
        let base_addr = injections[0].0;
        let mut new_stdin: Vec<u8> = vec![0u8; injections.last().map(|(a, _)| a - base_addr + 1).unwrap_or(0)];
        for (addr, byte) in &injections {
            let idx = addr - base_addr;
            if idx < new_stdin.len() {
                new_stdin[idx] = *byte;
            }
        }

        tracing::info!(
            "Injecting model into stdin ({} bytes): {:?}",
            new_stdin.len(),
            new_stdin.iter().map(|b| format!("0x{:02X}", b)).collect::<Vec<_>>()
        );

        self.emu.stdin_buffer = Some(new_stdin);
    }

    /// Fallback: seek TTD and force the PC to the alternate branch target.
    fn force_branch_after_seek(&mut self, branch: &SymBranch) -> Result<()> {
        self.emu.ttd_seek(branch.step_index)?;
        self.force_branch(branch);
        Ok(())
    }

    /// Force the emulator PC to the alternate branch target (no solver).
    fn force_branch(&mut self, branch: &SymBranch) {
        if let Some(addr) = branch.alt_addr {
            tracing::info!("Forcing branch to external PC=0x{:X}", addr);
            self.emu.pc = addr;
            self.emu.inst_count += 1;
        } else if let Some(rel) = branch.alt_rel_idx {
            tracing::warn!(
                "Alternate branch is internal p-code rel_idx={}. Full support requires \
                 instruction-level rewrite — skipping.",
                rel
            );
        }
    }
}
