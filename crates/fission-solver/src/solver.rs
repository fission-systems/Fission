use crate::ast::{Sort, SymExpr, SymNodeId};
use crate::theory::bitvector::LoweredAssumption;
use anyhow::Result;
use std::collections::{HashMap, HashSet};

/// Result of a SAT query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SatResult {
    Sat,
    Unsat,
    Unknown,
}

/// Why a query came back [`SatResult::Unknown`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnknownReason {
    /// The problem holds something lowering has no circuit for; these are
    /// the entries of `bv_theory.unsupported()`.
    Unsupported(Vec<String>),
    /// The search spent its conflict budget (see
    /// [`Solver::set_conflict_budget`]) without reaching a verdict.
    ConflictBudget,
    /// The memory oracle kept contradicting the model after the bounded
    /// number of refinement rounds.
    OracleRefinement,
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

    pub bv_theory: crate::theory::bitvector::BvTheorySolver,
    pub array_theory: crate::theory::array::ArrayTheory,
    pub sat: crate::sat::SatSolver,
    pub lowered_assertions: usize,
    /// Why the most recent query was `Unknown`, if it was.
    last_unknown: Option<UnknownReason>,
}

impl Default for Solver {
    fn default() -> Self {
        Self::new()
    }
}

fn collect_array_selects(expr: &SymExpr, seen: &mut HashSet<SymExpr>, out: &mut Vec<SymExpr>) {
    expr.visit_preorder(&mut |node| {
        if matches!(node, SymExpr::ArraySelect { .. }) && seen.insert(node.clone()) {
            out.push(node.clone());
        }
    });
}

fn read_oracle_value(
    oracle: &dyn MemoryOracle,
    space_id: u64,
    addr: u64,
    bit_width: u32,
) -> Option<u64> {
    if bit_width == 0 || bit_width > 64 {
        return None;
    }
    let byte_count = u64::from(bit_width.div_ceil(8));
    let mut val = 0u64;
    for byte_idx in 0..byte_count {
        let byte_addr = addr.checked_add(byte_idx)?;
        let byte = oracle.read_concrete(space_id, byte_addr)?;
        val |= u64::from(byte) << (byte_idx * 8);
    }
    let mask = if bit_width >= 64 {
        u64::MAX
    } else {
        (1u64 << bit_width) - 1
    };
    Some(val & mask)
}

impl Solver {
    pub fn new() -> Self {
        let mut bv_theory = crate::theory::bitvector::BvTheorySolver::new();
        bv_theory.aig.set_array_axioms_enabled(true);
        Self {
            assertions: Vec::new(),
            model: HashMap::new(),
            nodes: HashMap::new(),
            bv_theory,
            array_theory: crate::theory::array::ArrayTheory::new(),
            sat: crate::sat::SatSolver::new(),
            lowered_assertions: 0,
            last_unknown: None,
        }
    }

    pub fn register_node(&mut self, expr: SymExpr) -> SymNodeId {
        let id = crate::ast::VAR_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.nodes.insert(id, expr);
        id
    }

    /// Register a symbolic bitvector variable whose width is measured in bits.
    pub fn register_var(&mut self, name: String, width_bits: u32) -> SymNodeId {
        let id = crate::ast::VAR_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let expr = SymExpr::Var {
            id,
            name,
            sort: Sort::BitVector(width_bits),
        };
        self.nodes.insert(id, expr);
        id
    }

    /// Add a constraint (boolean expression) to the solver context.
    pub fn assert(&mut self, expr: SymExpr) {
        if expr.get_bit_width() != 1 {
            tracing::warn!("Asserted expression does not evaluate to a boolean (size != 1)");
        }
        self.assertions.push(expr);
        // Catch up through the same cursor used by queries, including any
        // assertions appended through the public compatibility list.
        self.lower_pending_assertions();
    }

    fn assert_array_lemmas_for(&mut self, expr: &SymExpr) {
        self.array_theory.add_expr(expr);
        for lemma in self.array_theory.take_lemmas() {
            self.bv_theory.assert_expr(&lemma);
        }
    }

    fn lower_pending_assertions(&mut self) -> bool {
        while self.lowered_assertions < self.assertions.len() {
            let expr = self.assertions[self.lowered_assertions].clone();
            self.lowered_assertions += 1;
            self.bv_theory.assert_expr(&expr);
            self.assert_array_lemmas_for(&expr);
        }
        self.bv_theory.load_into_sat(&mut self.sat)
    }

    pub fn check_sat(&mut self) -> Result<SatResult> {
        self.check_sat_with_oracle(None, &[])
    }

    /// Bound every later query to this many SAT conflicts; one that needs
    /// more answers `Unknown` instead of searching on. `None`, the default,
    /// never gives up. A budget is counted in conflicts so that a verdict
    /// does not depend on how busy the machine is.
    pub fn set_conflict_budget(&mut self, budget: Option<u64>) {
        self.sat.set_conflict_budget(budget);
    }

    /// Why the most recent query answered `Unknown`; `None` if it reached a
    /// verdict. `eval`, `min`, `max`, `is_true` and `is_false` are built from
    /// queries, so after one of them this describes the last query it made.
    pub fn last_unknown(&self) -> Option<&UnknownReason> {
        self.last_unknown.as_ref()
    }

    pub fn check_sat_with_oracle(
        &mut self,
        oracle: Option<&dyn MemoryOracle>,
        extra: &[SymExpr],
    ) -> Result<SatResult> {
        let result = self.check_sat_inner(oracle, extra)?;
        self.last_unknown = match result {
            SatResult::Unknown => Some(self.last_unknown.take().unwrap_or_else(|| {
                UnknownReason::Unsupported(self.bv_theory.unsupported().to_vec())
            })),
            _ => None,
        };
        Ok(result)
    }

    fn check_sat_inner(
        &mut self,
        oracle: Option<&dyn MemoryOracle>,
        extra: &[SymExpr],
    ) -> Result<SatResult> {
        self.last_unknown = None;
        let mut loop_count = 0;

        // Concrete-boolean short-circuit (avoids incomplete AIG/CNF for Const 0/1).
        let is_false = |e: &SymExpr| matches!(e, SymExpr::Const { val: 0, size: 1 });
        let is_true = |e: &SymExpr| matches!(e, SymExpr::Const { val: 1, size: 1 });
        for e in self.assertions.iter().chain(extra.iter()) {
            if is_false(e) {
                return Ok(SatResult::Unsat);
            }
        }

        if !self.lower_pending_assertions() {
            return Ok(self.refuted());
        }

        for e in extra {
            self.assert_array_lemmas_for(e);
        }
        if !self.bv_theory.load_into_sat(&mut self.sat) {
            return Ok(self.refuted());
        }

        loop {
            loop_count += 1;
            if loop_count > 10 {
                tracing::warn!("CEGAR loop exceeded 10 iterations, bailing out");
                self.last_unknown = Some(UnknownReason::OracleRefinement);
                return Ok(SatResult::Unknown);
            }

            // Lower extra constraints to assumptions
            let mut assumptions = Vec::new();
            for e in extra {
                if is_true(e) {
                    continue;
                }
                if is_false(e) {
                    return Ok(SatResult::Unsat);
                }
                match self.bv_theory.lower_to_literal(e, &mut self.sat) {
                    LoweredAssumption::Literal(lit) => assumptions.push(lit),
                    LoweredAssumption::TriviallyTrue => {}
                    LoweredAssumption::TriviallyFalse => return Ok(SatResult::Unsat),
                    LoweredAssumption::Refuted => return Ok(self.refuted()),
                }
            }

            // 3. Solve pure boolean SAT problem with assumptions
            match self
                .sat
                .solve_limited(Some(&mut self.array_theory), &assumptions)
            {
                Some(true) => {}
                Some(false) => return Ok(self.refuted()),
                None => {
                    self.last_unknown = Some(UnknownReason::ConflictBudget);
                    return Ok(SatResult::Unknown);
                }
            }

            // 4. Model Extraction
            self.model.clear();
            self.bv_theory
                .extract_model(&self.sat, &self.nodes, &mut self.model);

            // 5. Memory CEGAR
            if let Some(oracle) = oracle {
                let mut seen_selects = HashSet::new();
                let mut selects = Vec::new();
                let mut node_ids: Vec<SymNodeId> = self.nodes.keys().copied().collect();
                node_ids.sort_unstable();
                for id in node_ids {
                    if let Some(expr) = self.nodes.get(&id) {
                        collect_array_selects(expr, &mut seen_selects, &mut selects);
                    }
                }
                for expr in &self.assertions {
                    collect_array_selects(expr, &mut seen_selects, &mut selects);
                }
                for expr in extra {
                    collect_array_selects(expr, &mut seen_selects, &mut selects);
                }
                for expr in self.array_theory.select_terms() {
                    collect_array_selects(expr, &mut seen_selects, &mut selects);
                }

                let mut new_lemmas = Vec::new();
                for expr in selects {
                    let SymExpr::ArraySelect { array, index } = &expr else {
                        continue;
                    };
                    let SymExpr::Var { name, .. } = array.as_ref() else {
                        continue;
                    };
                    let Some(suffix) = name.strip_prefix("space_") else {
                        continue;
                    };
                    let Ok(space_id) = suffix.parse::<u64>() else {
                        continue;
                    };
                    let bit_width = match expr.get_sort() {
                        Sort::BitVector(bits) | Sort::Float(bits) if (1..=64).contains(&bits) => {
                            bits
                        }
                        _ => continue,
                    };
                    let c_idx = self.bv_theory.evaluate_expr_in_model(&self.sat, index);
                    let c_val = self.bv_theory.evaluate_expr_in_model(&self.sat, &expr);

                    if let Some(oracle_val) = read_oracle_value(oracle, space_id, c_idx, bit_width)
                    {
                        if c_val != oracle_val {
                            // Lemma: index == c_idx => ArraySelect == oracle_val
                            let eq_idx = SymExpr::Eq(
                                index.clone(),
                                Box::new(SymExpr::new_const(c_idx, index.get_bit_width())),
                            );
                            let eq_val = SymExpr::Eq(
                                Box::new(expr.clone()),
                                Box::new(SymExpr::new_const(oracle_val, bit_width)),
                            );
                            let implies =
                                SymExpr::Or(Box::new(SymExpr::new_not(eq_idx)), Box::new(eq_val));
                            new_lemmas.push(implies);
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

            // A model of a problem with an unencoded operation is a model of
            // a looser problem, not of this one.
            return Ok(if self.bv_theory.unsupported().is_empty() {
                SatResult::Sat
            } else {
                SatResult::Unknown
            });
        }
    }

    /// The verdict when the SAT core finds no model: `Unsat` only if every
    /// operation in the problem was encoded. An unencoded one was lowered to
    /// unconstrained bits, so "no model" is not a statement about the problem
    /// that was asked.
    ///
    /// The record is per solver and sticky, so a miss in one query's extra
    /// constraints makes later queries on the same solver `Unknown` too --
    /// conservative, and never wrong.
    fn refuted(&self) -> SatResult {
        if self.bv_theory.unsupported().is_empty() {
            SatResult::Unsat
        } else {
            SatResult::Unknown
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

    pub fn satisfiable_with_oracle(
        &mut self,
        extra: &[SymExpr],
        oracle: Option<&dyn MemoryOracle>,
    ) -> bool {
        matches!(
            self.check_sat_with_oracle(oracle, extra)
                .unwrap_or(SatResult::Unknown),
            SatResult::Sat
        )
    }

    /// Evaluate the expression and return up to `n` concrete values that satisfy
    /// the current path constraints.
    ///
    /// Constants obey the same satisfiability and supported-width checks as
    /// symbolic queries; an inconsistent context has no concrete values.
    pub fn eval(&mut self, expr: &SymExpr, n: usize) -> Vec<u64> {
        if n == 0 {
            return Vec::new();
        }

        // Lower the queried expression before solving.  `check_sat` only
        // lowers assertions; without this step a compound expression queried
        // after the path constraints would have no CNF-backed AIG nodes and
        // model evaluation would read the new gates as zero.
        self.bv_theory.aig.lower_expr(expr);
        // And give its array reads their axioms. With array axioms on, the
        // AIG no longer flags a select as unsupported, so a select that only
        // the query mentions -- `select(a, j)` when the constraints talk
        // about `select(a, i)` and `i == j` -- would otherwise be free bits,
        // and the first model value an arbitrary one. The lemmas are axioms
        // of the array, true in every query, so they stay asserted.
        self.assert_array_lemmas_for(expr);
        let mut results = Vec::new();
        if !matches!(
            self.check_sat().unwrap_or(SatResult::Unknown),
            SatResult::Sat
        ) {
            return results;
        }

        // Evaluate the complete expression in the satisfying AIG model, not
        // just a top-level Var lookup.  This also handles Extract, Ite, and
        // other compound expressions.
        results.push(self.bv_theory.evaluate_expr_in_model(&self.sat, expr));

        // For additional solutions (up to n), exclude each found solution and re-solve
        // This is standard SMT enumeration: assert (expr != prev_val) and re-check
        let mut extra_exclusions: Vec<SymExpr> = Vec::new();
        while results.len() < n {
            let last = *results.last().expect("the first model value is recorded");
            let exclusion =
                SymExpr::new_neq(expr.clone(), SymExpr::new_const(last, expr.get_bit_width()));
            extra_exclusions.push(exclusion);

            let sat = matches!(
                self.check_sat_with_oracle(None, &extra_exclusions)
                    .unwrap_or(SatResult::Unknown),
                SatResult::Sat
            );
            if sat {
                let value = self.bv_theory.evaluate_expr_in_model(&self.sat, expr);
                if results.contains(&value) {
                    // A repeated value means the model did not move outside
                    // the accumulated exclusions.  Do not spin indefinitely
                    // if a future backend reports an inconsistent model.
                    break;
                }
                results.push(value);
            } else {
                break;
            }
        }

        results
    }

    /// Returns true if `expr` is definitely true under all satisfying assignments.
    /// Concrete shortcut: if const, compare directly.
    pub fn is_true(&mut self, expr: &SymExpr) -> bool {
        match expr {
            SymExpr::Const { val, .. } => *val != 0,
            _ => {
                // NOT(expr) must be *refuted*. `!satisfiable(..)` also counted
                // `Unknown` as proof, which made "could not tell" read as
                // "always true".
                let negated = SymExpr::new_eq(expr.clone(), SymExpr::new_const(0, 1));
                matches!(
                    self.check_sat_with_oracle(None, &[negated])
                        .unwrap_or(SatResult::Unknown),
                    SatResult::Unsat
                )
            }
        }
    }

    /// Returns true if `expr` is definitely false under all satisfying assignments.
    /// Concrete shortcut: if const, compare directly.
    pub fn is_false(&mut self, expr: &SymExpr) -> bool {
        match expr {
            SymExpr::Const { val, .. } => *val == 0,
            _ => {
                // `expr` must be refuted, for the same reason as `is_true`.
                let positive = SymExpr::new_neq(expr.clone(), SymExpr::new_const(0, 1));
                matches!(
                    self.check_sat_with_oracle(None, &[positive])
                        .unwrap_or(SatResult::Unknown),
                    SatResult::Unsat
                )
            }
        }
    }

    /// Find the minimum concrete value of `expr` satisfying the current constraints.
    /// Uses binary search over constraint space (reference: angr min/max with signed flag).
    pub fn min(&mut self, expr: &SymExpr) -> Option<u64> {
        let solutions = self.eval(expr, 1);
        if solutions.is_empty() {
            return None;
        }
        if matches!(expr, SymExpr::Const { .. }) {
            return solutions.first().copied();
        }

        let mut lo = 0u64;
        let mut hi = solutions[0];
        let mut best = hi;

        // Binary search downward
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let constraint = SymExpr::Ule(
                Box::new(expr.clone()),
                Box::new(SymExpr::new_const(mid, expr.get_bit_width())),
            );
            // A bound the solver could not decide is not a bound.
            match self
                .check_sat_with_oracle(None, &[constraint])
                .unwrap_or(SatResult::Unknown)
            {
                SatResult::Sat => {
                    best = mid;
                    hi = mid;
                }
                SatResult::Unsat => lo = mid + 1,
                SatResult::Unknown => return None,
            }
        }
        Some(best)
    }

    /// Find the maximum concrete value of `expr` satisfying the current constraints.
    pub fn max(&mut self, expr: &SymExpr) -> Option<u64> {
        let solutions = self.eval(expr, 1);
        if solutions.is_empty() {
            return None;
        }
        if matches!(expr, SymExpr::Const { .. }) {
            return solutions.first().copied();
        }

        let mut lo = solutions[0];
        // Bit-vector sizes in SymExpr/AIG are already expressed in bits.
        // Multiplying by eight makes an 8-bit query search a 64-bit range;
        // the resulting truncated probe values are not monotonic, so the
        // binary search can return a value that is not the maximum.
        let size_bits = expr.get_bit_width();
        let mut hi = if size_bits >= 64 {
            u64::MAX
        } else {
            (1u64 << size_bits) - 1
        };
        let mut best = lo;

        // Binary search upward
        while lo < hi {
            let span = hi - lo;
            let mid = lo + span / 2 + span % 2;
            let constraint = SymExpr::Ule(
                Box::new(SymExpr::new_const(mid, expr.get_bit_width())),
                Box::new(expr.clone()),
            );
            match self
                .check_sat_with_oracle(None, &[constraint])
                .unwrap_or(SatResult::Unknown)
            {
                SatResult::Sat => {
                    best = mid;
                    lo = mid;
                }
                SatResult::Unsat => hi = mid - 1,
                SatResult::Unknown => return None,
            }
        }
        Some(best)
    }
}
