use crate::aig::{AigLit, AigManager};
use crate::ast::{SymExpr, SymNodeId};
use crate::cnf::CnfBuilder;
use crate::cnf::Lit;
use crate::sat::SatSolver;
use crate::theory::{Theory, TheoryStatus};
use std::collections::HashMap;

/// Result of lowering a boolean expression for use as a SAT assumption.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoweredAssumption {
    /// A non-constant boolean represented by a SAT literal.
    Literal(Lit),
    /// The expression lowered to AIG constant true, so no SAT assumption is needed.
    TriviallyTrue,
    /// The expression lowered to AIG constant false, so the query is immediately UNSAT.
    TriviallyFalse,
    /// Lowering could not produce a usable assumption, or loading CNF found a conflict.
    Refuted,
}

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

    /// What lowering could not encode. Non-empty means no verdict about this
    /// problem can be trusted.
    pub fn unsupported(&self) -> &[String] {
        self.aig.unsupported()
    }

    /// Lower an expression into AIG, ensuring all its components are mapped.
    /// In an eager setting, we lower the whole constraint here.
    pub fn assert_expr(&mut self, expr: &SymExpr) {
        let bits = self.aig.lower_expr(expr);
        if bits.len() == 1 {
            // Assert that the boolean result of the condition is TRUE
            self.cnf.assert_lit(bits[0]);
        } else {
            // Dropping the assertion used to be silent, and a dropped
            // constraint makes a problem looser than the one asked -- a SAT
            // here could be a model of something else.
            tracing::warn!("Assertion size != 1, skipping in BvTheory: {:?}", expr);
            self.aig
                .note_unsupported(format!("assertion {} bits wide", bits.len()));
        }
    }

    /// Compiles the collected AIG constraints into CNF and loads them into the SAT solver.
    /// Returns false if trivially UNSAT during CNF load.
    pub fn load_into_sat(&mut self, sat: &mut SatSolver) -> bool {
        self.aig.to_cnf(&mut self.cnf);

        for clause in self.cnf.clauses.drain(..) {
            if !sat.add_clause(clause.0) {
                return false;
            }
        }

        true
    }

    /// Lower an expression into a SAT assumption.
    /// This also ensures the SAT solver gets any newly generated CNF clauses.
    pub fn lower_to_literal(&mut self, expr: &SymExpr, sat: &mut SatSolver) -> LoweredAssumption {
        let bits = self.aig.lower_expr(expr);
        if bits.len() != 1 {
            tracing::warn!("Expression size != 1, cannot lower to literal: {:?}", expr);
            self.aig
                .note_unsupported(format!("assumption {} bits wide", bits.len()));
            return LoweredAssumption::Refuted;
        }

        // Ensure the newly generated AIG nodes are loaded into the SAT solver.
        if !self.load_into_sat(sat) {
            return LoweredAssumption::Refuted;
        }

        match bits[0] {
            AigLit::TRUE => LoweredAssumption::TriviallyTrue,
            AigLit::FALSE => LoweredAssumption::TriviallyFalse,
            aig_lit => LoweredAssumption::Literal(self.cnf.get_cnf_lit(aig_lit)),
        }
    }

    /// Given a satisfying assignment from the SAT solver, reconstructs concrete
    /// values for bitvector variables and registered array selects, tracking
    /// whether each value came from SAT-assigned bits or default completion.
    pub fn extract_model_with_status(
        &mut self,
        sat: &SatSolver,
        nodes: &HashMap<SymNodeId, SymExpr>,
    ) -> HashMap<SymNodeId, crate::solver::ModelValue> {
        let mut extracted = HashMap::new();
        let var_nodes: Vec<(SymNodeId, SymExpr)> = nodes
            .iter()
            .filter_map(|(&id, expr)| {
                if matches!(expr, SymExpr::Var { .. } | SymExpr::ArraySelect { .. }) {
                    Some((id, expr.clone()))
                } else {
                    None
                }
            })
            .collect();

        for (node_id, expr) in &var_nodes {
            let width_bits = match expr.get_sort() {
                crate::ast::Sort::BitVector(width) | crate::ast::Sort::Float(width) => width,
                crate::ast::Sort::Array { .. } => continue,
            };
            if width_bits == 0 || width_bits > 64 {
                self.aig.note_unsupported(format!(
                    "cannot extract a {width_bits}-bit value into the u64 model"
                ));
                continue;
            }
            match expr {
                SymExpr::Var {
                    id: ast_id, sort, ..
                } => {
                    let expected_width = sort.expect_bv();
                    let Some(bits) = self.aig.get_var_bits(*ast_id).cloned() else {
                        if expected_width != width_bits {
                            self.aig.note_unsupported(
                                "model bit width differs from scalar sort".into(),
                            );
                            continue;
                        }
                        // Unconstrained variable never lowered into the AIG:
                        // explicitly report default zero completion rather than
                        // mutating AIG/CNF state.
                        extracted.insert(*node_id, crate::solver::ModelValue::DefaultCompleted(0));
                        continue;
                    };
                    if bits.len() != width_bits as usize || expected_width != width_bits {
                        self.aig
                            .note_unsupported("model bit width differs from scalar sort".into());
                        continue;
                    }
                    let (value, all_assigned) =
                        read_bits_in_model(&self.cnf, sat, &bits, width_bits);
                    let entry = if all_assigned {
                        crate::solver::ModelValue::Assigned(value)
                    } else {
                        crate::solver::ModelValue::DefaultCompleted(value)
                    };
                    extracted.insert(*node_id, entry);
                }
                SymExpr::ArraySelect { .. } => {
                    let Some(bits) = self.aig.get_array_select_bits(expr).cloned() else {
                        continue;
                    };
                    if bits.len() != width_bits as usize {
                        self.aig
                            .note_unsupported("model bit width differs from scalar sort".into());
                        continue;
                    }
                    let (value, all_assigned) =
                        read_bits_in_model(&self.cnf, sat, &bits, width_bits);
                    let entry = if all_assigned {
                        crate::solver::ModelValue::Assigned(value)
                    } else {
                        crate::solver::ModelValue::DefaultCompleted(value)
                    };
                    extracted.insert(*node_id, entry);
                }
                _ => {}
            }
        }
        extracted
    }

    /// Given a satisfying assignment from the SAT solver, reconstructs the concrete
    /// values for all bitvector variables into `model`.
    pub fn extract_model(
        &mut self,
        sat: &SatSolver,
        nodes: &HashMap<SymNodeId, SymExpr>,
        model: &mut HashMap<SymNodeId, u64>,
    ) {
        let extracted = self.extract_model_with_status(sat, nodes);
        for (id, entry) in extracted {
            model.insert(id, entry.raw_value());
        }
    }

    /// Evaluates `expr` against the current SAT assignment and returns
    /// `(value, exact)` where `exact` is true iff all scalar bits were driven by
    /// SAT assignments or AIG constants (no unassigned/undefined bits).
    pub fn evaluate_expr_in_model_with_status(
        &mut self,
        sat: &SatSolver,
        expr: &SymExpr,
    ) -> (u64, bool) {
        let width = match expr.get_sort() {
            crate::ast::Sort::BitVector(width) | crate::ast::Sort::Float(width)
                if (1..=64).contains(&width) =>
            {
                width
            }
            _ => {
                self.aig
                    .note_unsupported("model evaluation requires a 1..=64-bit scalar".into());
                return (0, false);
            }
        };
        let bits = self.aig.lower_expr(expr);
        if bits.len() != width as usize {
            self.aig
                .note_unsupported("model bit width differs from scalar sort".into());
            return (0, false);
        }
        read_bits_in_model(&self.cnf, sat, &bits, width)
    }

    /// Returns a scalar payload. The caller must check `unsupported()` before
    /// treating the payload as a model value; unsupported shapes return zero.
    pub fn evaluate_expr_in_model(&mut self, sat: &SatSolver, expr: &SymExpr) -> u64 {
        self.evaluate_expr_in_model_with_status(sat, expr).0
    }
}

fn read_bits_in_model(
    cnf: &CnfBuilder,
    sat: &SatSolver,
    bits: &[AigLit],
    width_bits: u32,
) -> (u64, bool) {
    let mut value: u64 = 0;
    let mut all_assigned = true;
    for (bit_idx, aig_lit) in bits.iter().enumerate() {
        let aig_node_idx = aig_lit.index();
        let cnf_var = cnf.get_cnf_var_for_aig(aig_node_idx);
        let raw_bit = if cnf_var == 0 {
            if aig_node_idx != 0 {
                all_assigned = false;
            }
            false
        } else {
            match sat.get_var_value(cnf_var) {
                crate::sat::LBool::True => true,
                crate::sat::LBool::False => false,
                crate::sat::LBool::Undef => {
                    all_assigned = false;
                    false
                }
            }
        };
        let bit_val = u64::from(if aig_lit.is_inverted() {
            !raw_bit
        } else {
            raw_bit
        });
        value |= bit_val << bit_idx;
    }
    let mask = if width_bits >= 64 {
        u64::MAX
    } else {
        (1u64 << width_bits) - 1
    };
    (value & mask, all_assigned)
}

impl Theory for BvTheorySolver {
    fn check(&mut self, _assignments: &[Lit]) -> TheoryStatus {
        // Lazy bit2bool mapping and generating constraints
        TheoryStatus::Satisfied
    }
}
