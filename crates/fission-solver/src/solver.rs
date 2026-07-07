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
    /// On SAT, populates `self.model` with concrete values for all registered Var nodes.
    pub fn check_sat(&mut self) -> Result<SatResult> {
        tracing::info!("Solver::check_sat called with {} assertions", self.assertions.len());

        let mut aig = crate::aig::AigManager::new();
        let mut cnf = crate::cnf::CnfBuilder::new();
        let mut sat = crate::sat::SatSolver::new();

        // 1. Lower all assertions into AIG and assert each output bit is TRUE
        for assertion in &self.assertions {
            let bits = aig.lower_expr(assertion);
            if bits.len() == 1 {
                cnf.assert_lit(bits[0]);
            } else {
                tracing::warn!("Assertion size != 1, skipping: {:?}", assertion);
            }
        }

        // 2. Convert AIG → CNF (Tseitin)
        aig.to_cnf(&mut cnf);

        // 3. Load CNF clauses into SAT solver; short-circuit on trivially UNSAT empty clause
        for clause in &cnf.clauses {
            if !sat.add_clause(clause.0.clone()) {
                return Ok(SatResult::Unsat);
            }
        }

        // 4. Solve
        if !sat.solve() {
            return Ok(SatResult::Unsat);
        }

        // 5. Model Extraction — for every Var node, read its bits from the SAT assignment.
        //    Pipeline: SymNodeId → AIG var bits (AigLit) → CNF var index → SAT LBool → bit value
        self.model.clear();

        // Collect Var nodes first to avoid borrow conflict
        let var_nodes: Vec<(SymNodeId, SymExpr)> = self.nodes
            .iter()
            .filter_map(|(&id, expr)| {
                if matches!(expr, SymExpr::Var { .. }) {
                    Some((id, expr.clone()))
                } else {
                    None
                }
            })
            .collect();

        for (node_id, expr) in &var_nodes {
            if let SymExpr::Var { id: ast_id, size, .. } = expr {
                // Retrieve or synthesize the AIG bits for this variable.
                // If the variable appeared in an assertion its bits are already in aig.var_map.
                // If not (unconstrained), synthesize them now — they'll be Undef in the model.
                let bits = if let Some(b) = aig.get_var_bits(*ast_id) {
                    b.clone()
                } else {
                    // Not constrained — create fresh bits (will be Undef, default 0)
                    aig.add_var(*ast_id, *size)
                };

                // Reconstruct the u64 value bit by bit (LSB first)
                let mut value: u64 = 0;
                for (bit_idx, aig_lit) in bits.iter().enumerate() {
                    let aig_node_idx = aig_lit.index();
                    let cnf_var = cnf.get_cnf_var_for_aig(aig_node_idx);
                    if cnf_var == 0 {
                        // This AIG node was never added to CNF (constant TRUE/FALSE or unseen)
                        // Constant TRUE = AigLit(1), constant FALSE = AigLit(0)
                        let bit_val = if *aig_lit == crate::aig::AigLit::TRUE {
                            1u64
                        } else {
                            0u64
                        };
                        value |= bit_val << bit_idx;
                        continue;
                    }

                    let assignment = sat.get_var_value(cnf_var);
                    let raw_bit = matches!(assignment, crate::sat::LBool::True);
                    // If the AigLit is inverted, flip the bit
                    let bit_val = if aig_lit.is_inverted() { !raw_bit } else { raw_bit } as u64;
                    value |= bit_val << bit_idx;
                }

                // Mask to the declared size
                let mask = if *size >= 64 { u64::MAX } else { (1u64 << (*size * 8)) - 1 };
                self.model.insert(*node_id, value & mask);

                tracing::debug!(
                    "Model: var_id={} name={:?} size={} value=0x{:X}",
                    node_id,
                    if let SymExpr::Var { name, .. } = expr { name } else { "" },
                    size,
                    value & mask
                );
            }
        }

        Ok(SatResult::Sat)
    }

    /// Retrieve the satisfying model for a given variable ID.
    pub fn get_value(&self, var_id: SymNodeId) -> Option<u64> {
        self.model.get(&var_id).copied()
    }
}
