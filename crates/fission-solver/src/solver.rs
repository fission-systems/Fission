use crate::ast::{Sort, SymExpr, SymNodeId};
use crate::cnf::Lit;
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

/// Value state for one symbol in a published `Sat` model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelValue {
    /// Every bit of the value was driven by SAT assignments or AIG constants.
    Assigned(u64),
    /// At least one bit was unconstrained by CNF/SAT (`LBool::Undef` or
    /// unlowered variable) and completed with default zero bits.
    DefaultCompleted(u64),
}

impl ModelValue {
    #[must_use]
    pub const fn raw_value(self) -> u64 {
        match self {
            Self::Assigned(v) | Self::DefaultCompleted(v) => v,
        }
    }

    #[must_use]
    pub const fn is_assigned(self) -> bool {
        matches!(self, Self::Assigned(_))
    }
}

/// Why a lookup into [`Solver::get_model_value`] has no concrete entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbsentModelReason {
    /// No active `Sat` query has published a model (initial state, invalidated,
    /// `Unsat`, or `Unknown`).
    NoPublishedModel,
    /// `solver.assertions` changed since the last published model.
    AssertionsModified,
    /// The queried `SymNodeId` was not registered in `solver.nodes`.
    UnregisteredNode,
    /// The registered node cannot be extracted as a `1..=64`-bit scalar (e.g.
    /// unsupported sort/width or an unconstrained `ArraySelect`).
    UnextractableSortOrWidth,
}

/// Result of querying a symbol against the currently published model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelLookup {
    Value(ModelValue),
    Absent(AbsentModelReason),
}

/// Immutable witness snapshot published strictly after a supported `Sat` verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishedModel {
    pub query_epoch: u64,
    pub oracle_epoch: Option<u64>,
    pub assumptions: Vec<SymExpr>,
    assertions_snapshot: Vec<SymExpr>,
    pub entries: HashMap<SymNodeId, ModelValue>,
}

impl PublishedModel {
    #[must_use]
    pub fn get_model_value(&self, var_id: SymNodeId) -> Option<ModelValue> {
        self.entries.get(&var_id).copied()
    }

    #[must_use]
    pub fn get_value(&self, var_id: SymNodeId) -> Option<u64> {
        self.entries.get(&var_id).map(|entry| entry.raw_value())
    }
}

/// Termination status of [`Solver::eval_detailed`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalTermination {
    /// Caller requested zero values (`n == 0`).
    ZeroRequested,
    /// All satisfying values within scope were enumerated and further exclusion was `Unsat`.
    Exhausted,
    /// Enumeration stopped after collecting the requested limit `n` values.
    LimitReached,
    /// Enumeration stopped because a query returned `Unknown`; partial `values`
    /// collected before `Unknown` are preserved without conflating with `Exhausted`/`Unsat`.
    Unknown(UnknownReason),
    /// Internal solver error terminated enumeration.
    Error(String),
    /// A repeated value was returned despite an exclusion constraint.
    StalledRepeat,
}

/// Individual value witness produced by [`Solver::eval_detailed`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalWitness {
    pub value: u64,
    pub query_epoch: u64,
    /// True iff all scalar bits were driven by SAT assignments or AIG constants.
    pub exact: bool,
}

/// Typed enumeration result returned by [`Solver::eval_detailed`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalReport {
    pub values: Vec<u64>,
    pub witnesses: Vec<EvalWitness>,
    pub termination: EvalTermination,
    pub base_satisfiable: bool,
}

pub trait MemoryOracle {
    fn read_concrete(&self, space_id: u64, addr: u64) -> Option<u8>;
}

/// A basic interface for an SMT Solver (CDCL/Bitvector skeleton).
pub struct Solver {
    /// The list of assertions (Path Conditions) that must be satisfied.
    pub assertions: Vec<SymExpr>,
    /// Compatibility mirror of the currently published `Sat` model. Cleared
    /// on assertion changes, query start, and every non-`Sat` outcome.
    pub model: HashMap<SymNodeId, u64>,
    /// Storage for AST nodes by ID.
    pub nodes: HashMap<SymNodeId, SymExpr>,

    pub bv_theory: crate::theory::bitvector::BvTheorySolver,
    pub array_theory: crate::theory::array::ArrayTheory,
    pub sat: crate::sat::SatSolver,
    pub lowered_assertions: usize,
    /// Why the most recent query was `Unknown`, if it was.
    last_unknown: Option<UnknownReason>,
    query_epoch: u64,
    oracle_epoch: u64,
    published_model: Option<PublishedModel>,
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

/// A constant's value: its low `size` bits, which is all lowering reads.
fn constant_value(expr: &SymExpr) -> u64 {
    match expr {
        SymExpr::Const { val, size } if *size < 64 => val & ((1u64 << size) - 1),
        SymExpr::Const { val, .. } => *val,
        _ => unreachable!("only called on a constant"),
    }
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
            query_epoch: 0,
            oracle_epoch: 0,
            published_model: None,
        }
    }

    /// Invalidate any previously published model without disturbing previously
    /// cloned [`PublishedModel`] snapshots held by callers.
    pub fn invalidate_model(&mut self) {
        self.model.clear();
        self.published_model = None;
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
        self.invalidate_model();
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

    /// Lower temporary constraints before publishing any new array axioms.
    /// Definitional CNF clauses can be retained even when the query is rejected.
    fn lower_query_assumptions(
        &mut self,
        extra: &[SymExpr],
    ) -> std::result::Result<Vec<Lit>, SatResult> {
        if !self.bv_theory.unsupported().is_empty() {
            return Err(SatResult::Unknown);
        }
        let mut lower = |expressions: &[SymExpr]| {
            let mut literals = Vec::new();
            for expression in expressions {
                let lowered = self.bv_theory.lower_to_literal(expression, &mut self.sat);
                let unsupported = self.bv_theory.aig.take_unsupported();
                if !unsupported.is_empty() {
                    tracing::debug!(?unsupported, "cannot lower query constraint");
                    // Taken out of the AIG's record so that it does not
                    // outlive this query -- which is also why it has to be
                    // reported here: `last_unknown` falls back to that record.
                    self.last_unknown = Some(UnknownReason::Unsupported(unsupported));
                    return Err(SatResult::Unknown);
                }
                match lowered {
                    LoweredAssumption::Literal(lit) => literals.push(lit),
                    LoweredAssumption::TriviallyTrue => {}
                    LoweredAssumption::Refuted | LoweredAssumption::TriviallyFalse => {
                        return Err(SatResult::Unsat);
                    }
                }
            }
            Ok(literals)
        };

        let assumptions = lower(extra)?;
        if extra
            .iter()
            .any(|expr| self.array_theory.has_new_terms(expr))
        {
            let mut array_theory = self.array_theory.clone();
            for expr in extra {
                array_theory.add_expr(expr);
            }
            for lemma in lower(&array_theory.take_lemmas())? {
                if !self.sat.add_clause(vec![lemma]) {
                    return Err(SatResult::Unsat);
                }
            }
            self.array_theory = array_theory;
        }
        Ok(assumptions)
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
        self.query_epoch = self.query_epoch.saturating_add(1);
        let oracle_epoch = oracle.map(|_| {
            self.oracle_epoch = self.oracle_epoch.saturating_add(1);
            self.oracle_epoch
        });
        self.invalidate_model();
        let result = self.check_sat_inner(oracle, extra, oracle_epoch)?;
        if result != SatResult::Sat {
            self.invalidate_model();
        }
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
        oracle_epoch: Option<u64>,
    ) -> Result<SatResult> {
        self.last_unknown = None;
        let mut loop_count = 0;
        let mut remaining_conflicts = self.sat.conflict_budget();

        // Concrete-boolean short-circuit (avoids incomplete AIG/CNF for Const 0/1).
        let is_false = |e: &SymExpr| matches!(e, SymExpr::Const { val: 0, size: 1 });
        for e in self.assertions.iter().chain(extra.iter()) {
            if is_false(e) {
                return Ok(SatResult::Unsat);
            }
        }

        if !self.lower_pending_assertions() {
            return Ok(self.refuted());
        }

        let assumptions = match self.lower_query_assumptions(extra) {
            Ok(assumptions) => assumptions,
            Err(result) => return Ok(result),
        };

        loop {
            loop_count += 1;
            if loop_count > 10 {
                tracing::warn!("CEGAR loop exceeded 10 iterations, bailing out");
                self.last_unknown = Some(UnknownReason::OracleRefinement);
                return Ok(SatResult::Unknown);
            }

            // 3. Solve pure boolean SAT problem with assumptions
            match self.sat.solve_with_remaining_budget(
                Some(&mut self.array_theory),
                &assumptions,
                &mut remaining_conflicts,
            ) {
                Some(true) => {}
                Some(false) => return Ok(self.refuted()),
                None => {
                    self.last_unknown = Some(UnknownReason::ConflictBudget);
                    return Ok(SatResult::Unknown);
                }
            }

            // 4. Memory CEGAR (before publishing any candidate model)
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

            // 5. Extract candidate model only after CEGAR succeeds; publish only if
            //    no unsupported operations tainted the verdict.
            let candidate_entries = self
                .bv_theory
                .extract_model_with_status(&self.sat, &self.nodes);
            if !self.bv_theory.unsupported().is_empty() {
                self.invalidate_model();
                return Ok(SatResult::Unknown);
            }

            let raw_model: HashMap<SymNodeId, u64> = candidate_entries
                .iter()
                .map(|(&id, entry)| (id, entry.raw_value()))
                .collect();
            for (node_id, val) in &raw_model {
                tracing::debug!("Model: var_id={} value=0x{:X}", node_id, val);
            }
            self.model = raw_model;
            self.published_model = Some(PublishedModel {
                query_epoch: self.query_epoch,
                oracle_epoch,
                assumptions: extra.to_vec(),
                assertions_snapshot: self.assertions.clone(),
                entries: candidate_entries,
            });
            return Ok(SatResult::Sat);
        }
    }

    /// The verdict when the SAT core finds no model: `Unsat` only if every
    /// operation in the problem was encoded. An unencoded one was lowered to
    /// unconstrained bits, so "no model" is not a statement about the problem
    /// that was asked.
    ///
    /// Lowering misses in permanent constraints remain active in every query.
    fn refuted(&self) -> SatResult {
        if self.bv_theory.unsupported().is_empty() {
            SatResult::Unsat
        } else {
            SatResult::Unknown
        }
    }

    /// Returns the currently published [`PublishedModel`] iff the last query
    /// returned a supported `Sat` verdict and `assertions` has not been mutated
    /// since publication.
    #[must_use]
    pub fn published_model(&self) -> Option<&PublishedModel> {
        self.published_model
            .as_ref()
            .filter(|published| published.assertions_snapshot == self.assertions)
    }

    /// Inspect a symbol in the active published model, distinguishing between
    /// SAT-assigned values, explicit default zero completion, and absent/stale
    /// entries.
    #[must_use]
    pub fn get_model_value(&self, var_id: SymNodeId) -> ModelLookup {
        let Some(published) = self.published_model.as_ref() else {
            return ModelLookup::Absent(AbsentModelReason::NoPublishedModel);
        };
        if published.assertions_snapshot != self.assertions {
            return ModelLookup::Absent(AbsentModelReason::AssertionsModified);
        }
        if let Some(&entry) = published.entries.get(&var_id) {
            return ModelLookup::Value(entry);
        }
        match self.nodes.get(&var_id) {
            Some(_) => ModelLookup::Absent(AbsentModelReason::UnextractableSortOrWidth),
            None => ModelLookup::Absent(AbsentModelReason::UnregisteredNode),
        }
    }

    /// Retrieve a model value for `var_id` iff the current model is valid and
    /// consistent with `self.assertions`.
    pub fn get_value(&self, var_id: SymNodeId) -> Option<u64> {
        match self.get_model_value(var_id) {
            ModelLookup::Value(entry) => Some(entry.raw_value()),
            ModelLookup::Absent(_) => None,
        }
    }

    /// Retrieve only a SAT-assigned model value (returning `None` for missing,
    /// stale, or default-completed entries).
    pub fn get_assigned_value(&self, var_id: SymNodeId) -> Option<u64> {
        match self.get_model_value(var_id) {
            ModelLookup::Value(ModelValue::Assigned(val)) => Some(val),
            _ => None,
        }
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

    /// Transactional enumeration returning distinct satisfying values of `expr`
    /// (up to `n`), witness provenance, and an explicit [`EvalTermination`]
    /// distinguishing complete UNSAT exhaustion from limit cutoffs or `Unknown`.
    ///
    /// Unsupported temporary expressions fail cleanly for this query without
    /// poisoning subsequent supported queries on the solver.
    pub fn eval_detailed(&mut self, expr: &SymExpr, n: usize) -> EvalReport {
        if n == 0 {
            return EvalReport {
                values: Vec::new(),
                witnesses: Vec::new(),
                termination: EvalTermination::ZeroRequested,
                base_satisfiable: false,
            };
        }

        self.invalidate_model();
        // Catch up any pending permanent assertions first so pre-existing
        // persistent unsupported errors stay distinct from this temporary query.
        if !self.lower_pending_assertions() {
            let result = self.refuted();
            if result == SatResult::Unsat {
                self.last_unknown = None;
                return EvalReport {
                    values: Vec::new(),
                    witnesses: Vec::new(),
                    termination: EvalTermination::Exhausted,
                    base_satisfiable: false,
                };
            }
            let reason = UnknownReason::Unsupported(self.bv_theory.unsupported().to_vec());
            self.last_unknown = Some(reason.clone());
            return EvalReport {
                values: Vec::new(),
                witnesses: Vec::new(),
                termination: EvalTermination::Unknown(reason),
                base_satisfiable: false,
            };
        }
        if !self.bv_theory.unsupported().is_empty() {
            let reason = UnknownReason::Unsupported(self.bv_theory.unsupported().to_vec());
            self.last_unknown = Some(reason.clone());
            return EvalReport {
                values: Vec::new(),
                witnesses: Vec::new(),
                termination: EvalTermination::Unknown(reason),
                base_satisfiable: false,
            };
        }

        let prior_unsupported_len = self.bv_theory.unsupported().len();
        let saved_array_theory = self.array_theory.clone();

        // Lower `expr` in a query-local transaction before running SAT so
        // unsupported temporary expressions neither fabricate zero models nor
        // poison subsequent queries.
        self.bv_theory.aig.lower_expr(expr);
        let new_unsupported = self
            .bv_theory
            .aig
            .drain_unsupported_since(prior_unsupported_len);
        if !new_unsupported.is_empty() {
            self.array_theory = saved_array_theory;
            let reason = UnknownReason::Unsupported(new_unsupported);
            self.last_unknown = Some(reason.clone());
            return EvalReport {
                values: Vec::new(),
                witnesses: Vec::new(),
                termination: EvalTermination::Unknown(reason),
                base_satisfiable: false,
            };
        }

        // Compute any array-theory axioms needed for `expr` on a temporary clone
        // first, committing them only if lowering stays completely supported.
        if self.array_theory.has_new_terms(expr) {
            let prior_clauses_len = self.bv_theory.cnf.clauses.len();
            let mut next_array = self.array_theory.clone();
            next_array.add_expr(expr);
            let lemmas = next_array.take_lemmas();
            for lemma in &lemmas {
                self.bv_theory.assert_expr(lemma);
            }
            let lemma_unsupported = self
                .bv_theory
                .aig
                .drain_unsupported_since(prior_unsupported_len);
            if !lemma_unsupported.is_empty() {
                self.bv_theory.cnf.clauses.truncate(prior_clauses_len);
                self.array_theory = saved_array_theory;
                let reason = UnknownReason::Unsupported(lemma_unsupported);
                self.last_unknown = Some(reason.clone());
                return EvalReport {
                    values: Vec::new(),
                    witnesses: Vec::new(),
                    termination: EvalTermination::Unknown(reason),
                    base_satisfiable: false,
                };
            }
            self.array_theory = next_array;
        }

        let first_verdict = match self.check_sat() {
            Ok(v) => v,
            Err(err) => {
                return EvalReport {
                    values: Vec::new(),
                    witnesses: Vec::new(),
                    termination: EvalTermination::Error(err.to_string()),
                    base_satisfiable: false,
                };
            }
        };
        match first_verdict {
            SatResult::Sat => {}
            SatResult::Unsat => {
                return EvalReport {
                    values: Vec::new(),
                    witnesses: Vec::new(),
                    termination: EvalTermination::Exhausted,
                    base_satisfiable: false,
                };
            }
            SatResult::Unknown => {
                let reason = self
                    .last_unknown
                    .clone()
                    .unwrap_or(UnknownReason::ConflictBudget);
                return EvalReport {
                    values: Vec::new(),
                    witnesses: Vec::new(),
                    termination: EvalTermination::Unknown(reason),
                    base_satisfiable: false,
                };
            }
        }

        let (first_val, first_exact) = self
            .bv_theory
            .evaluate_expr_in_model_with_status(&self.sat, expr);
        let eval_unsupported = self
            .bv_theory
            .aig
            .drain_unsupported_since(prior_unsupported_len);
        if !eval_unsupported.is_empty() {
            self.invalidate_model();
            self.array_theory = saved_array_theory;
            let reason = UnknownReason::Unsupported(eval_unsupported);
            self.last_unknown = Some(reason.clone());
            return EvalReport {
                values: Vec::new(),
                witnesses: Vec::new(),
                termination: EvalTermination::Unknown(reason),
                base_satisfiable: false,
            };
        }

        let mut values = vec![first_val];
        let mut witnesses = vec![EvalWitness {
            value: first_val,
            query_epoch: self.query_epoch,
            exact: first_exact,
        }];
        let mut extra_exclusions: Vec<SymExpr> = Vec::new();
        let termination = loop {
            if values.len() >= n {
                break EvalTermination::LimitReached;
            }
            let last = *values.last().expect("at least one witness is recorded");
            extra_exclusions.push(SymExpr::new_neq(
                expr.clone(),
                SymExpr::new_const(last, expr.get_bit_width()),
            ));
            match self.check_sat_with_oracle(None, &extra_exclusions) {
                Ok(SatResult::Sat) => {
                    let (value, exact) = self
                        .bv_theory
                        .evaluate_expr_in_model_with_status(&self.sat, expr);
                    let loop_unsupported = self
                        .bv_theory
                        .aig
                        .drain_unsupported_since(prior_unsupported_len);
                    if !loop_unsupported.is_empty() {
                        self.invalidate_model();
                        let reason = UnknownReason::Unsupported(loop_unsupported);
                        self.last_unknown = Some(reason.clone());
                        break EvalTermination::Unknown(reason);
                    }
                    if values.contains(&value) {
                        break EvalTermination::StalledRepeat;
                    }
                    values.push(value);
                    witnesses.push(EvalWitness {
                        value,
                        query_epoch: self.query_epoch,
                        exact,
                    });
                }
                Ok(SatResult::Unsat) => break EvalTermination::Exhausted,
                Ok(SatResult::Unknown) => {
                    let reason = self
                        .last_unknown
                        .clone()
                        .unwrap_or(UnknownReason::ConflictBudget);
                    break EvalTermination::Unknown(reason);
                }
                Err(err) => break EvalTermination::Error(err.to_string()),
            }
        };

        EvalReport {
            values,
            witnesses,
            termination,
            base_satisfiable: true,
        }
    }

    /// Compatibility wrapper returning up to `n` satisfying concrete values
    /// for `expr`. Prefer [`Self::eval_detailed`] when the caller must
    /// distinguish incomplete enumeration (`Unknown`) from UNSAT exhaustion.
    pub fn eval(&mut self, expr: &SymExpr, n: usize) -> Vec<u64> {
        self.eval_detailed(expr, n).values
    }

    /// Returns true if `expr` is definitely true under all satisfying assignments.
    /// Concrete shortcut: if const, compare directly.
    pub fn is_true(&mut self, expr: &SymExpr) -> bool {
        match expr {
            SymExpr::Const { .. } => constant_value(expr) != 0,
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
            SymExpr::Const { .. } => constant_value(expr) == 0,
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
