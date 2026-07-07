use crate::ast::{SymExpr, SymNodeId};
use anyhow::Result;
use std::collections::HashMap;

/// Result of a SAT query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SatResult {
    Sat,
    Unsat,
    Unknown,
}

/// A basic interface for an SMT Solver (CDCL/Bitvector skeleton).
pub struct Solver {
    /// The list of assertions (Path Conditions) that must be satisfied.
    pub assertions: Vec<SymExpr>,
    /// Simplified model mapping variable IDs to concrete values.
    pub model: HashMap<SymNodeId, u64>,
    /// Storage for AST nodes by ID.
    pub nodes: HashMap<SymNodeId, SymExpr>,
    /// Stack of frame boundaries (indices into the assertions list) for push/pop.
    pub frames: Vec<usize>,
}

impl Default for Solver {
    fn default() -> Self {
        Self::new()
    }
}

impl Solver {
    pub fn new() -> Self {
        Self {
            assertions: Vec::new(),
            model: HashMap::new(),
            nodes: HashMap::new(),
            frames: Vec::new(),
        }
    }

    /// Push a new context frame.
    pub fn push(&mut self) {
        self.frames.push(self.assertions.len());
    }

    /// Pop the most recent context frame, reverting assertions added since then.
    pub fn pop(&mut self) {
        if let Some(prev_len) = self.frames.pop() {
            self.assertions.truncate(prev_len);
        } else {
            tracing::warn!("Solver::pop called with no frames on the stack");
        }
    }

    pub fn register_node(&mut self, expr: SymExpr) -> SymNodeId {
        let id = crate::ast::VAR_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.nodes.insert(id, expr);
        id
    }

    pub fn register_var(&mut self, name: String, size: u32) -> SymNodeId {
        let id = crate::ast::VAR_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.nodes.insert(id, SymExpr::Var { id, name, size });
        id
    }

    /// Add a constraint (boolean expression) to the solver context.
    pub fn assert(&mut self, expr: SymExpr) {
        if expr.get_size() != 1 {
            tracing::warn!("Asserted expression does not evaluate to a boolean (size != 1)");
        }
        self.assertions.push(expr);
    }

    /// Check if the current set of assertions is satisfiable.
    /// This is currently a stub that always returns SAT for the skeleton.
    pub fn check_sat(&mut self) -> Result<SatResult> {
        tracing::info!("Solver::check_sat called with {} assertions", self.assertions.len());
        // TODO: Implement DPLL / CDCL Bit-blasting logic here.
        // For now, we return Sat so the execution path can continue.
        Ok(SatResult::Sat)
    }

    /// Retrieve the satisfying model for a given variable ID.
    pub fn get_value(&self, var_id: SymNodeId) -> Option<u64> {
        self.model.get(&var_id).copied()
    }
}
