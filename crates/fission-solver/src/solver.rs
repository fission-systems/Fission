use crate::ast::{Sort, SymExpr, SymNodeId};
use crate::cnf::Lit;
use crate::theory::bitvector::LoweredAssumption;
use crate::validate::{validate_query_with_nodes, ValidatedQuery, ValidationError, VarSortEnv};
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
    /// The problem holds an ill-sorted expression, overflowing width, or
    /// conflicting variable ID declaration rejected prior to lowering.
    InvalidExpression(Vec<ValidationError>),
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
    /// The persistent assertions changed (appended, replaced, removed,
    /// reordered, truncated or cleared) since the last published model.
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
    /// [`Solver::assertion_revision`] at publication; a model answers only
    /// for the assertion set it was found under.
    assertion_revision: u64,
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
///
/// # One source of truth
///
/// The persistent assertions, the registered nodes, and the backend they are
/// lowered into (AIG, CNF, SAT clauses including learned ones, array-theory
/// terms, and the variable identity table) are private and change together:
///
/// - [`Solver::assert`] appends. The new assertion alone is validated against
///   the identity table and lowered on top of the existing backend, so
///   append-only incremental solving pays for each assertion once.
/// - Every other change ([`Solver::replace_assertion`],
///   [`Solver::remove_assertion`], [`Solver::truncate_assertions`],
///   [`Solver::clear_assertions`], [`Solver::swap_assertions`]) rebuilds the
///   backend from the remaining assertions. Clauses -- learned ones included --
///   derived from an assertion that is gone can never answer a later query.
/// - Every change bumps [`Solver::assertion_revision`] and withdraws the
///   published model.
///
/// The fields used to be public, and a caller who edited `assertions` in
/// place left the clauses of the old assertion in the SAT core: replacing
/// `x == 0` by `x == 1` and asking under `x == 1` answered `Unsat`.
pub struct Solver {
    /// The list of assertions (Path Conditions) that must be satisfied.
    assertions: Vec<SymExpr>,
    /// Compatibility mirror of the currently published `Sat` model. Cleared
    /// on assertion changes, query start, and every non-`Sat` outcome.
    model: HashMap<SymNodeId, u64>,
    /// Storage for AST nodes by ID.
    nodes: HashMap<SymNodeId, SymExpr>,

    bv_theory: crate::theory::bitvector::BvTheorySolver,
    array_theory: crate::theory::array::ArrayTheory,
    sat: crate::sat::SatSolver,
    /// How many leading assertions are validated into `identity` and lowered
    /// into the backend. Only `assert` advances it; only a rebuild lowers it.
    lowered_assertions: usize,
    /// Sorts of every variable the backend knows: declared in `nodes`, used
    /// by a lowered assertion, or lowered by a validated query since the last
    /// rebuild. Extended atomically, so a rejected expression adds nothing.
    identity: VarSortEnv,
    /// The first persistent problem (an ill-formed assertion, or a registered
    /// variable that conflicts with the identity table). Every query answers
    /// `Unknown(InvalidExpression)` until it is removed.
    persistent_error: Option<ValidationError>,
    /// A lowered assertion is the constant false.
    has_false_assertion: bool,
    /// Bumped on every change to the assertion list.
    assertion_revision: u64,
    conflict_budget: Option<u64>,
    restart_unit: Option<u64>,
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
        let sat = crate::sat::SatSolver::new();
        let restart_unit = sat.restart_unit();
        Self {
            assertions: Vec::new(),
            model: HashMap::new(),
            nodes: HashMap::new(),
            bv_theory: Self::fresh_bv_theory(),
            array_theory: crate::theory::array::ArrayTheory::new(),
            sat,
            lowered_assertions: 0,
            identity: VarSortEnv::new(),
            persistent_error: None,
            has_false_assertion: false,
            assertion_revision: 0,
            conflict_budget: None,
            restart_unit,
            last_unknown: None,
            query_epoch: 0,
            oracle_epoch: 0,
            published_model: None,
        }
    }

    fn fresh_bv_theory() -> crate::theory::bitvector::BvTheorySolver {
        let mut bv_theory = crate::theory::bitvector::BvTheorySolver::new();
        bv_theory.aig.set_array_axioms_enabled(true);
        bv_theory
    }

    // ── Read-only views ──────────────────────────────────────────────────────

    /// The persistent assertions, in the order they were added.
    #[must_use]
    pub fn assertions(&self) -> &[SymExpr] {
        &self.assertions
    }

    /// How many leading assertions are lowered into the backend. Equal to
    /// `assertions().len()` unless an assertion failed validation.
    #[must_use]
    pub fn lowered_assertion_count(&self) -> usize {
        self.lowered_assertions
    }

    /// Changes on every edit to the assertion list (append, replace, remove,
    /// reorder, truncate, clear).
    #[must_use]
    pub fn assertion_revision(&self) -> u64 {
        self.assertion_revision
    }

    /// Registered nodes by ID.
    #[must_use]
    pub fn nodes(&self) -> &HashMap<SymNodeId, SymExpr> {
        &self.nodes
    }

    /// One registered node.
    #[must_use]
    pub fn node(&self, id: SymNodeId) -> Option<&SymExpr> {
        self.nodes.get(&id)
    }

    /// Compatibility mirror of the published model (raw values only); empty
    /// unless the last query was a supported `Sat`. Prefer
    /// [`Self::get_model_value`].
    #[must_use]
    pub fn model(&self) -> &HashMap<SymNodeId, u64> {
        &self.model
    }

    /// The bit-vector backend, read-only.
    #[must_use]
    pub fn bv_theory(&self) -> &crate::theory::bitvector::BvTheorySolver {
        &self.bv_theory
    }

    /// The array-theory backend, read-only.
    #[must_use]
    pub fn array_theory(&self) -> &crate::theory::array::ArrayTheory {
        &self.array_theory
    }

    /// The SAT core, read-only.
    #[must_use]
    pub fn sat(&self) -> &crate::sat::SatSolver {
        &self.sat
    }

    /// The variable identity table the backend is checked against.
    #[must_use]
    pub fn identity(&self) -> &VarSortEnv {
        &self.identity
    }

    // ── Assertion-store mutation ─────────────────────────────────────────────

    /// Withdraw the published model: its revision no longer matches, so
    /// lookups report [`AbsentModelReason::AssertionsModified`] until the next
    /// query replaces it.
    fn note_assertions_changed(&mut self) {
        self.assertion_revision = self.assertion_revision.wrapping_add(1);
        self.model.clear();
    }

    /// Throw the backend away and lower the current assertions again. Called
    /// after any change other than an append: SAT clauses (learned ones
    /// included), AIG gates, array-theory terms and identity bindings derived
    /// from a removed or replaced assertion must not outlive it.
    fn rebuild_backend(&mut self) {
        self.bv_theory = Self::fresh_bv_theory();
        self.array_theory = crate::theory::array::ArrayTheory::new();
        self.sat = crate::sat::SatSolver::new();
        self.sat.set_conflict_budget(self.conflict_budget);
        self.sat.set_restart_unit(self.restart_unit);
        self.lowered_assertions = 0;
        self.has_false_assertion = false;
        self.persistent_error = None;
        self.identity = VarSortEnv::new();
        let mut ids: Vec<SymNodeId> = self.nodes.keys().copied().collect();
        ids.sort_unstable();
        for key in ids {
            if let Some(SymExpr::Var { id, sort, .. }) = self.nodes.get(&key) {
                if let Err(err) = self.identity.try_bind_var(*id, sort) {
                    self.persistent_error.get_or_insert(err);
                }
            }
        }
        self.note_assertions_changed();
        let _ = self.sync_backend();
    }

    /// Replace the assertion at `index`, returning the old one; `None` (and
    /// no change) when `index` is out of range. Rebuilds the backend.
    pub fn replace_assertion(&mut self, index: usize, expr: SymExpr) -> Option<SymExpr> {
        let slot = self.assertions.get_mut(index)?;
        let old = std::mem::replace(slot, expr);
        self.rebuild_backend();
        Some(old)
    }

    /// Remove the assertion at `index`, returning it; `None` (and no change)
    /// when `index` is out of range. Rebuilds the backend.
    pub fn remove_assertion(&mut self, index: usize) -> Option<SymExpr> {
        if index >= self.assertions.len() {
            return None;
        }
        let old = self.assertions.remove(index);
        self.rebuild_backend();
        Some(old)
    }

    /// Keep the first `len` assertions (a scope pop). No change when `len` is
    /// not shorter than the list. Rebuilds the backend otherwise.
    pub fn truncate_assertions(&mut self, len: usize) {
        if len >= self.assertions.len() {
            return;
        }
        self.assertions.truncate(len);
        self.rebuild_backend();
    }

    /// Remove every assertion. Registered nodes are kept.
    pub fn clear_assertions(&mut self) {
        if self.assertions.is_empty() {
            return;
        }
        self.assertions.clear();
        self.rebuild_backend();
    }

    /// Exchange two assertions. Out-of-range indices change nothing and
    /// return `false`. The conjunction is unchanged, but the backend is still
    /// rebuilt so that which assertion is lowered never depends on position.
    pub fn swap_assertions(&mut self, a: usize, b: usize) -> bool {
        if a >= self.assertions.len() || b >= self.assertions.len() {
            return false;
        }
        if a != b {
            self.assertions.swap(a, b);
            self.rebuild_backend();
        }
        true
    }

    /// Validate pending assertions into the identity table and lower them.
    /// `Err` carries the persistent problem that makes every query invalid;
    /// `Ok(false)` means loading the clauses found the problem unsatisfiable.
    fn sync_backend(&mut self) -> std::result::Result<bool, ValidationError> {
        if let Some(err) = &self.persistent_error {
            return Err(err.clone());
        }
        while self.lowered_assertions < self.assertions.len() {
            let index = self.lowered_assertions;
            if let Err(err) = self.identity.try_bind_assertion(&self.assertions[index]) {
                self.persistent_error = Some(err.clone());
                return Err(err);
            }
            self.lowered_assertions += 1;
            let expr = &self.assertions[index];
            if matches!(expr, SymExpr::Const { val: 0, size: 1 }) {
                self.has_false_assertion = true;
            }
            self.bv_theory.assert_expr(expr);
            self.array_theory.add_expr(expr);
            for lemma in self.array_theory.take_lemmas() {
                self.bv_theory.assert_expr(&lemma);
            }
        }
        Ok(self.bv_theory.load_into_sat(&mut self.sat))
    }

    /// Invalidate any previously published model without disturbing previously
    /// cloned [`PublishedModel`] snapshots held by callers.
    pub fn invalidate_model(&mut self) {
        self.model.clear();
        self.published_model = None;
    }

    /// Bind a registered variable declaration into the identity table. A
    /// conflict becomes the persistent problem, as it would if the
    /// declaration were validated together with every query.
    fn declare_registered(&mut self, expr: &SymExpr) {
        if let SymExpr::Var { id, sort, .. } = expr {
            if let Err(err) = self.identity.try_bind_var(*id, sort) {
                self.persistent_error.get_or_insert(err);
            }
        }
    }

    pub fn register_node(&mut self, expr: SymExpr) -> SymNodeId {
        let id = crate::ast::VAR_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.declare_registered(&expr);
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
        self.declare_registered(&expr);
        self.nodes.insert(id, expr);
        id
    }

    /// Store `expr` under a caller-chosen `id` (for a variable built with
    /// [`SymExpr::new_var`], its own ID). A variable whose sort conflicts with
    /// the identity table is rejected and nothing is stored. Replacing an
    /// existing entry withdraws the published model, whose entry for `id`
    /// described the old node.
    pub fn insert_node(
        &mut self,
        id: SymNodeId,
        expr: SymExpr,
    ) -> std::result::Result<Option<SymExpr>, ValidationError> {
        if let SymExpr::Var {
            id: var_id, sort, ..
        } = &expr
        {
            self.identity.try_bind_var(*var_id, sort)?;
        }
        let old = self.nodes.insert(id, expr);
        if old.is_some() {
            self.invalidate_model();
        }
        Ok(old)
    }

    /// Validate all assertions currently recorded in [`Self::assertions`] and
    /// registered variable declarations in [`Self::nodes`].
    pub fn validate_assertions(&self) -> std::result::Result<ValidatedQuery, ValidationError> {
        validate_query_with_nodes(&self.assertions, &[], &self.nodes)
    }

    /// Validate all permanent assertions, temporary `extra` assumptions, and
    /// registered variable declarations under one shared sort/identity table.
    pub fn validate_query(
        &self,
        extra: &[SymExpr],
    ) -> std::result::Result<ValidatedQuery, ValidationError> {
        validate_query_with_nodes(&self.assertions, extra, &self.nodes)
    }

    /// Validate one query expression against the identity table after
    /// catching up the persistent assertions. Its variable bindings are kept
    /// only when it is well formed (it is about to be lowered); a rejected
    /// expression leaves the table unchanged.
    fn validate_scalar_query_expr(
        &mut self,
        expr: &SymExpr,
        require_bool: bool,
    ) -> std::result::Result<(), ValidationError> {
        self.sync_backend()?;
        if require_bool {
            self.identity.try_bind_assertion(expr)
        } else {
            match self.identity.try_bind_expr(expr)? {
                sort @ Sort::Array { .. } => {
                    Err(ValidationError::NonScalarOperand { op: "eval", sort })
                }
                _ => Ok(()),
            }
        }
    }

    /// Add a constraint (boolean expression) to the solver context.
    ///
    /// Only the new assertion is validated (against the identity table) and
    /// lowered. An ill-formed one is kept and makes every later query
    /// `Unknown(InvalidExpression)` until it is removed; use
    /// [`Self::try_assert`] to refuse it instead.
    pub fn assert(&mut self, expr: SymExpr) {
        self.assertions.push(expr);
        self.note_assertions_changed();
        if let Err(err) = self.sync_backend() {
            tracing::warn!("asserted expression is invalid: {err}");
        }
    }

    /// Append `expr` only if it is well formed against the identity table;
    /// otherwise return the error and leave the solver unchanged.
    pub fn try_assert(&mut self, expr: SymExpr) -> std::result::Result<(), ValidationError> {
        self.sync_backend()?;
        self.identity.try_bind_assertion(&expr)?;
        // Bound already; `sync_backend` re-validates it against bindings that
        // include its own, which cannot fail.
        self.assertions.push(expr);
        self.note_assertions_changed();
        self.sync_backend().map(|_| ())
    }

    /// Bound every later query to this many SAT conflicts; one that needs
    /// more answers `Unknown` instead of searching on. `None`, the default,
    /// never gives up. A budget is counted in conflicts so that a verdict
    /// does not depend on how busy the machine is.
    pub fn set_conflict_budget(&mut self, budget: Option<u64>) {
        self.conflict_budget = budget;
        self.sat.set_conflict_budget(budget);
    }

    /// Set the SAT core's Luby restart unit (`None` turns restarts off). Kept
    /// across backend rebuilds.
    pub fn set_restart_unit(&mut self, unit: Option<u64>) {
        self.sat.set_restart_unit(unit);
        self.restart_unit = self.sat.restart_unit();
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

        // Elaborate and validate every variable sort, identity, and operator
        // well-formedness rule before short-circuiting or lowering. Pending
        // persistent assertions are validated once, when they are lowered;
        // `extra` is validated against the identity table they built and is
        // bound into it only if the whole query is well formed.
        let loaded = match self.sync_backend() {
            Ok(loaded) => loaded,
            Err(err) => {
                self.last_unknown = Some(UnknownReason::InvalidExpression(vec![err]));
                return Ok(SatResult::Unknown);
            }
        };
        if let Err(err) = self.identity.try_bind_assertions(extra) {
            self.last_unknown = Some(UnknownReason::InvalidExpression(vec![err]));
            return Ok(SatResult::Unknown);
        }

        // Concrete-boolean short-circuit (avoids incomplete AIG/CNF for Const 0/1).
        let is_false = |e: &SymExpr| matches!(e, SymExpr::Const { val: 0, size: 1 });
        if self.has_false_assertion || extra.iter().any(is_false) {
            return Ok(SatResult::Unsat);
        }

        if !loaded {
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
                assertion_revision: self.assertion_revision,
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
            .filter(|published| published.assertion_revision == self.assertion_revision)
    }

    /// Inspect a symbol in the active published model, distinguishing between
    /// SAT-assigned values, explicit default zero completion, and absent/stale
    /// entries.
    #[must_use]
    pub fn get_model_value(&self, var_id: SymNodeId) -> ModelLookup {
        let Some(published) = self.published_model.as_ref() else {
            return ModelLookup::Absent(AbsentModelReason::NoPublishedModel);
        };
        if published.assertion_revision != self.assertion_revision {
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

    /// Evaluate `expr` in the published model: `None` when no model is
    /// current, `expr` is ill formed against the identity table or not a
    /// `1..=64`-bit scalar, or lowering it needs an operation without a
    /// circuit. A failure here is local to this call; it does not mark the
    /// solver's problem unsupported. Bits the SAT core never assigned read
    /// as zero and make the value [`ModelValue::DefaultCompleted`].
    pub fn eval_in_model(&mut self, expr: &SymExpr) -> Option<ModelValue> {
        self.published_model()?;
        match self.identity.try_bind_expr(expr) {
            Ok(Sort::BitVector(bits) | Sort::Float(bits)) if (1..=64).contains(&bits) => {}
            _ => return None,
        }
        let prior = self.bv_theory.unsupported().len();
        let (value, exact) = self
            .bv_theory
            .evaluate_expr_in_model_with_status(&self.sat, expr);
        if !self.bv_theory.aig.drain_unsupported_since(prior).is_empty() {
            return None;
        }
        Some(if exact {
            ModelValue::Assigned(value)
        } else {
            ModelValue::DefaultCompleted(value)
        })
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
        if let Err(err) = self.validate_scalar_query_expr(expr, false) {
            let reason = UnknownReason::InvalidExpression(vec![err]);
            self.last_unknown = Some(reason.clone());
            return EvalReport {
                values: Vec::new(),
                witnesses: Vec::new(),
                termination: EvalTermination::Unknown(reason),
                base_satisfiable: false,
            };
        }
        // Pending permanent assertions were caught up by the validation above,
        // so pre-existing persistent unsupported errors stay distinct from
        // this temporary query.
        if !self.bv_theory.load_into_sat(&mut self.sat) {
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
        if let Err(err) = self.validate_scalar_query_expr(expr, true) {
            self.last_unknown = Some(UnknownReason::InvalidExpression(vec![err]));
            return false;
        }
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
        if let Err(err) = self.validate_scalar_query_expr(expr, true) {
            self.last_unknown = Some(UnknownReason::InvalidExpression(vec![err]));
            return false;
        }
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
