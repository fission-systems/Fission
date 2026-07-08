use crate::ast::{SymExpr, SymNodeId, Sort};
use anyhow::Result;
use std::collections::HashMap;

/// Result of a SAT query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SatResult {
    Sat,
    Unsat,
    Unknown,
}

pub trait MemoryOracle {
    fn read_concrete(&self, space_id: u64, addr: u64) -> Option<u8>;
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
        let expr = SymExpr::Var { id, name, sort: Sort::BitVector(size) };
        self.nodes.insert(id, expr);
        id
    }

    /// Add a constraint (boolean expression) to the solver context.
    pub fn assert(&mut self, expr: SymExpr) {
        if expr.get_size() != 1 {
            tracing::warn!("Asserted expression does not evaluate to a boolean (size != 1)");
        }
        self.assertions.push(expr);
    }

    pub fn check_sat(&mut self) -> Result<SatResult> {
        self.check_sat_with_oracle(None)
    }

    pub fn check_sat_with_oracle(&mut self, oracle: Option<&dyn MemoryOracle>) -> Result<SatResult> {
        let mut loop_count = 0;
        
        loop {
            loop_count += 1;
            if loop_count > 10 {
                tracing::warn!("CEGAR loop exceeded 10 iterations, bailing out");
                return Ok(SatResult::Unknown);
            }

            tracing::info!("Solver::check_sat_with_oracle (CEGAR iter {}) called with {} assertions", loop_count, self.assertions.len());

            let mut bv_theory = crate::theory::bitvector::BvTheorySolver::new();
            let mut sat = crate::sat::SatSolver::new();

            // 1. Lower all assertions into the Bitvector Theory Solver
            for assertion in &self.assertions {
                bv_theory.assert_expr(assertion);
            }

            // 2. Load constraints from Theory Solver into SAT core
            if !bv_theory.load_into_sat(&mut sat) {
                return Ok(SatResult::Unsat);
            }

            // 3. Solve pure boolean SAT problem
            if !sat.solve() {
                return Ok(SatResult::Unsat);
            }

            // 4. Model Extraction
            self.model.clear();
            bv_theory.extract_model(&sat, &self.nodes, &mut self.model);
            
            // 5. Memory CEGAR
            if let Some(oracle) = oracle {
                let mut new_lemmas = Vec::new();
                for (_, expr) in &self.nodes {
                    if let SymExpr::ArraySelect { array, index } = expr {
                        if let SymExpr::Var { name, .. } = array.as_ref() {
                            if name.starts_with("space_") {
                                let space_id: u64 = name["space_".len()..].parse().unwrap_or(0);
                                let c_idx = bv_theory.evaluate_expr_in_model(&sat, index);
                                let c_val = bv_theory.evaluate_expr_in_model(&sat, expr);
                                
                                if let Some(oracle_val) = oracle.read_concrete(space_id, c_idx) {
                                    if c_val != oracle_val as u64 {
                                        // Lemma: index == c_idx => ArraySelect == oracle_val
                                        let eq_idx = SymExpr::Eq(index.clone(), Box::new(SymExpr::new_const(c_idx, index.get_size())));
                                        let eq_val = SymExpr::Eq(Box::new(expr.clone()), Box::new(SymExpr::new_const(oracle_val as u64, expr.get_size())));
                                        let implies = SymExpr::Or(Box::new(SymExpr::new_not(eq_idx)), Box::new(eq_val));
                                        new_lemmas.push(implies);
                                    }
                                }
                            }
                        }
                    }
                }
                if !new_lemmas.is_empty() {
                    for lemma in new_lemmas {
                        self.assert(lemma);
                    }
                    continue; // Restart CEGAR loop
                }
            }

            for (node_id, val) in &self.model {
                tracing::debug!("Model: var_id={} value=0x{:X}", node_id, val);
            }

            return Ok(SatResult::Sat);
        }
    }

    /// Retrieve the satisfying model for a given variable ID.
    pub fn get_value(&self, var_id: SymNodeId) -> Option<u64> {
        self.model.get(&var_id).copied()
    }

    // ── High-level API (inspired by angr SimSolver) ───────────────────────────
    //
    // All query entry-points first try the "concrete shortcut": if the expression
    // is a constant, return the concrete value immediately without invoking the
    // SAT backend (reference: angr solver.py `@concrete_path_*` decorators).

    /// Check if the current path constraints + optional extra constraints are
    /// satisfiable. On SAT, the model is populated.
    pub fn satisfiable(&mut self, extra: &[SymExpr]) -> bool {
        self.satisfiable_with_oracle(extra, None)
    }

    pub fn satisfiable_with_oracle(&mut self, extra: &[SymExpr], oracle: Option<&dyn MemoryOracle>) -> bool {
        if extra.is_empty() {
            return matches!(self.check_sat_with_oracle(oracle).unwrap_or(SatResult::Unknown), SatResult::Sat);
        }
        // Temporarily push extra constraints
        self.push();
        for e in extra { self.assert(e.clone()); }
        let result = matches!(self.check_sat_with_oracle(oracle).unwrap_or(SatResult::Unknown), SatResult::Sat);
        self.pop();
        result
    }

    /// Evaluate the expression and return up to `n` concrete values that satisfy
    /// the current path constraints.
    ///
    /// Concrete shortcut: if `expr` is a constant, returns `vec![const_val]`
    /// immediately without invoking the SAT core.
    pub fn eval(&mut self, expr: &SymExpr, n: usize) -> Vec<u64> {
        // Concrete shortcut (angr pattern #13)
        if let SymExpr::Const { val, .. } = expr {
            return vec![*val];
        }

        let mut results = Vec::new();
        if !matches!(self.check_sat().unwrap_or(SatResult::Unknown), SatResult::Sat) {
            return results;
        }

        // Get first solution from model by looking up by structure
        // For a Var node: look it up directly
        if let SymExpr::Var { id, .. } = expr {
            if let Some(val) = self.model.get(id) {
                results.push(*val);
            }
        }

        // For additional solutions (up to n), exclude each found solution and re-solve
        // This is standard SMT enumeration: assert (expr != prev_val) and re-check
        let mut extra_exclusions: Vec<SymExpr> = Vec::new();
        while results.len() < n {
            let last = *results.last().unwrap_or(&0);
            let exclusion = SymExpr::new_neq(expr.clone(), SymExpr::new_const(last, expr.get_size()));
            extra_exclusions.push(exclusion);

            self.push();
            for excl in &extra_exclusions {
                self.assert(excl.clone());
            }
            let sat = matches!(self.check_sat().unwrap_or(SatResult::Unknown), SatResult::Sat);
            if sat {
                if let SymExpr::Var { id, .. } = expr {
                    if let Some(val) = self.model.get(id) {
                        results.push(*val);
                    } else { self.pop(); break; }
                } else { self.pop(); break; }
            } else {
                self.pop();
                break;
            }
            self.pop();
        }

        results
    }

    /// Returns true if `expr` is definitely true under all satisfying assignments.
    /// Concrete shortcut: if const, compare directly.
    pub fn is_true(&mut self, expr: &SymExpr) -> bool {
        match expr {
            SymExpr::Const { val, .. } => *val != 0,
            _ => {
                // Check that NOT(expr) is UNSAT
                let negated = SymExpr::new_eq(expr.clone(), SymExpr::new_const(0, 1));
                !self.satisfiable(&[negated])
            }
        }
    }

    /// Returns true if `expr` is definitely false under all satisfying assignments.
    /// Concrete shortcut: if const, compare directly.
    pub fn is_false(&mut self, expr: &SymExpr) -> bool {
        match expr {
            SymExpr::Const { val, .. } => *val == 0,
            _ => {
                // Check that expr is UNSAT
                let positive = SymExpr::new_neq(expr.clone(), SymExpr::new_const(0, 1));
                !self.satisfiable(&[positive])
            }
        }
    }

    /// Find the minimum concrete value of `expr` satisfying the current constraints.
    /// Uses binary search over constraint space (reference: angr min/max with signed flag).
    pub fn min(&mut self, expr: &SymExpr) -> Option<u64> {
        // Concrete shortcut
        if let SymExpr::Const { val, .. } = expr { return Some(*val); }

        let solutions = self.eval(expr, 1);
        if solutions.is_empty() { return None; }

        let mut lo = 0u64;
        let mut hi = solutions[0];
        let mut best = hi;

        // Binary search downward
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let constraint = SymExpr::Ule(
                Box::new(expr.clone()),
                Box::new(SymExpr::new_const(mid, expr.get_size())),
            );
            if self.satisfiable(&[constraint]) {
                best = mid;
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        Some(best)
    }

    /// Find the maximum concrete value of `expr` satisfying the current constraints.
    pub fn max(&mut self, expr: &SymExpr) -> Option<u64> {
        // Concrete shortcut
        if let SymExpr::Const { val, .. } = expr { return Some(*val); }

        let solutions = self.eval(expr, 1);
        if solutions.is_empty() { return None; }

        let mut lo = solutions[0];
        let size_bits = expr.get_size() * 8;
        let mut hi = if size_bits >= 64 { u64::MAX } else { (1u64 << size_bits) - 1 };
        let mut best = lo;

        // Binary search upward
        while lo < hi {
            let mid = lo + (hi - lo + 1) / 2;
            let constraint = SymExpr::Ult(
                Box::new(SymExpr::new_const(mid, expr.get_size())),
                Box::new(expr.clone()),
            );
            if self.satisfiable(&[constraint]) {
                best = mid;
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        Some(best)
    }
}
