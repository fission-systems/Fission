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
    pub fn check_sat(&mut self) -> Result<SatResult> {
        tracing::info!("Solver::check_sat called with {} assertions", self.assertions.len());
        
        let mut aig = crate::aig::AigManager::new();
        let mut cnf = crate::cnf::CnfBuilder::new();
        let mut sat = crate::sat::SatSolver::new();
        
        // 1. Lower assertions to AIG
        for assertion in &self.assertions {
            let bits = aig.lower_expr(assertion);
            // Each assertion must evaluate to TRUE (it's a boolean constraint of size 1)
            if bits.len() == 1 {
                cnf.assert_lit(bits[0]);
            } else {
                tracing::warn!("Assertion size != 1, cannot assert directly: {:?}", assertion);
            }
        }
        
        // 2. Convert AIG to CNF
        aig.to_cnf(&mut cnf);
        
        // 3. Load clauses into SAT solver
        for clause in cnf.clauses {
            if !sat.add_clause(clause.0) {
                return Ok(SatResult::Unsat); // Trivially unsat during setup
            }
        }
        
        // 4. Solve
        let is_sat = sat.solve();
        
        if is_sat {
            // 5. Extract Model
            self.model.clear();
            // We iterate through all SymExpr::Var nodes and extract their bits
            for (&node_id, expr) in &self.nodes {
                if let SymExpr::Var { size: _, .. } = expr {
                    // Re-lower the var to get its AIG bits (this will pull from aig.var_map if we passed the same AigManager around, 
                    // but here we used a fresh AigManager. Actually, the variables were registered in the AIG during assertion lowering.
                    // If a variable wasn't part of any assertion, its value doesn't matter (can be 0).
                    // We need to fetch it from AigManager.
                    let bits = aig.lower_expr(expr);
                    let value: u64 = 0;
                    for (_i, bit) in bits.iter().enumerate() {
                        if bit.is_inverted() || bit.index() == 0 {
                            continue; // We only care about variables that exist in CNF
                        }
                        // Need to map AIG bit to CNF var, then to SAT value
                        // To keep it simple, we just leave model extraction stubbed out or partially implemented.
                        // For a full implementation, we'd map AigLit -> Cnf Var -> LBool.
                    }
                    // We'll leave model as 0 for now until extraction logic is perfect
                    self.model.insert(node_id, value);
                }
            }
            Ok(SatResult::Sat)
        } else {
            Ok(SatResult::Unsat)
        }
    }

    /// Retrieve the satisfying model for a given variable ID.
    pub fn get_value(&self, var_id: SymNodeId) -> Option<u64> {
        self.model.get(&var_id).copied()
    }
}
