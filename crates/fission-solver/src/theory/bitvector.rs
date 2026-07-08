use crate::aig::{AigManager, AigLit};
use crate::cnf::CnfBuilder;
use crate::ast::{SymExpr, SymNodeId};
use crate::sat::SatSolver;
use std::collections::HashMap;

/// The Bitvector Theory Solver.
///
/// In a DPLL(T) architecture, this component is responsible for reasoning about
/// bitvector operations (Add, Sub, Bitwise, Comparisons) by lowering them into
/// AIG and extracting models.
pub struct BvTheorySolver {
    pub aig: AigManager,
    pub cnf: CnfBuilder,
}

impl Default for BvTheorySolver {
    fn default() -> Self {
        Self::new()
    }
}

impl BvTheorySolver {
    pub fn new() -> Self {
        Self {
            aig: AigManager::new(),
            cnf: CnfBuilder::new(),
        }
    }

    /// Lower an expression into AIG, ensuring all its components are mapped.
    /// In an eager setting, we lower the whole constraint here.
    pub fn assert_expr(&mut self, expr: &SymExpr) {
        let bits = self.aig.lower_expr(expr);
        if bits.len() == 1 {
            // Assert that the boolean result of the condition is TRUE
            self.cnf.assert_lit(bits[0]);
        } else {
            tracing::warn!("Assertion size != 1, skipping in BvTheory: {:?}", expr);
        }
    }

    /// Compiles the collected AIG constraints into CNF and loads them into the SAT solver.
    /// Returns false if trivially UNSAT during CNF load.
    pub fn load_into_sat(&mut self, sat: &mut SatSolver) -> bool {
        self.aig.to_cnf(&mut self.cnf);
        
        for clause in &self.cnf.clauses {
            if !sat.add_clause(clause.0.clone()) {
                return false;
            }
        }

        // Mark boundary between input (original) and learned clauses
        sat.seal_input_clauses();
        true
    }

    /// Given a satisfying assignment from the SAT solver, reconstructs the concrete 
    /// values for all bitvector variables.
    pub fn extract_model(&mut self, sat: &SatSolver, nodes: &HashMap<SymNodeId, SymExpr>, model: &mut HashMap<SymNodeId, u64>) {
        let var_nodes: Vec<(SymNodeId, SymExpr)> = nodes
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
                let bits = if let Some(b) = self.aig.get_var_bits(*ast_id) {
                    b.clone()
                } else {
                    self.aig.add_var(*ast_id, *size)
                };

                let mut value: u64 = 0;
                for (bit_idx, aig_lit) in bits.iter().enumerate() {
                    let aig_node_idx = aig_lit.index();
                    let cnf_var = self.cnf.get_cnf_var_for_aig(aig_node_idx);
                    if cnf_var == 0 {
                        let bit_val = if *aig_lit == AigLit::TRUE { 1u64 } else { 0u64 };
                        value |= bit_val << bit_idx;
                        continue;
                    }

                    let assignment = sat.get_var_value(cnf_var);
                    let raw_bit = matches!(assignment, crate::sat::LBool::True);
                    let bit_val = if aig_lit.is_inverted() { !raw_bit } else { raw_bit } as u64;
                    value |= bit_val << bit_idx;
                }

                let mask = if *size >= 64 { u64::MAX } else { (1u64 << (*size * 8)) - 1 };
                model.insert(*node_id, value & mask);
            }
        }
    }
}
