//! Shared symbolic sort and variable-identity elaboration checks.
//!
//! All query entry points (`Solver`, `smtlib`, and algebraic simplification
//! shortcuts on `SymExpr`) elaborate expressions against one set of rules so
//! malformed sorts, conflicting variable IDs, overflowing bit widths, and
//! ill-typed array/ITE terms are rejected uniformly before constant folding,
//! bit-blasting, or SMT-LIB printing.
//!
//! # Mixed-width contract
//!
//! - **Unsigned bitvector operations and ITE branches** (`Add`, `Sub`, `Mul`,
//!   `Udiv`, `Urem`, `And`, `Or`, `Xor`, `Eq`, `Neq`, `Ult`, `Ule`, and
//!   two-BV `Ite`) intentionally accept differing non-zero bitvector widths by
//!   zero-extending the narrower operand to `max(lhs_bits, rhs_bits)`,
//!   matching both [`crate::aig::AigManager`] and [`crate::smtlib`].
//! - **Equal-width operations** (`Sdiv`, `Srem`, `Smod`, `Slt`, `Sle`, `Sgt`,
//!   `Shl`, `Lshr`, `Ashr`, binary float operations, and `Float`-sorted
//!   `Eq`/`Neq`/`Ite`) require matching operand bit widths with no implicit
//!   padding or extension.
//! - **Concrete bit-pattern constants**: `SymExpr::Const { size, .. }` may
//!   supply an IEEE bit-pattern payload to float operations and float
//!   comparisons/ITEs when `size` matches the peer float width.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::ast::{Sort, SymExpr, SymNodeId};

/// Maximum expression AST and nested array sort recursion depth accepted by
/// elaboration.
pub const MAX_EXPR_DEPTH: usize = 2048;

/// Default maximum AST node visits per validation call before returning a
/// structured budget error.
pub const DEFAULT_MAX_VALIDATION_VISITS: usize = 1_000_000;

/// Default maximum unique hash-consed DAG nodes constructed during validation.
pub const DEFAULT_MAX_VALIDATION_UNIQUE_NODES: usize = 250_000;

static ENV_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Opaque handle to an immutable hash-consed node inside a [`ValidatedDag`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DagNodeId(pub u32);

/// Node operator inside a [`ValidatedDag`] with child operands stored as
/// [`DagNodeId`] handles rather than recursive `Box<SymExpr>` trees.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DagOp {
    Const {
        val: u64,
        size: u32,
    },
    Var {
        id: SymNodeId,
        name: String,
        sort: Sort,
    },
    Add(DagNodeId, DagNodeId),
    Sub(DagNodeId, DagNodeId),
    Mul(DagNodeId, DagNodeId),
    Udiv(DagNodeId, DagNodeId),
    Urem(DagNodeId, DagNodeId),
    Sdiv(DagNodeId, DagNodeId),
    Srem(DagNodeId, DagNodeId),
    Smod(DagNodeId, DagNodeId),
    And(DagNodeId, DagNodeId),
    Or(DagNodeId, DagNodeId),
    Xor(DagNodeId, DagNodeId),
    Shl(DagNodeId, DagNodeId),
    Lshr(DagNodeId, DagNodeId),
    Ashr(DagNodeId, DagNodeId),
    Eq(DagNodeId, DagNodeId),
    Neq(DagNodeId, DagNodeId),
    Ult(DagNodeId, DagNodeId),
    Ule(DagNodeId, DagNodeId),
    Slt(DagNodeId, DagNodeId),
    Sle(DagNodeId, DagNodeId),
    Sgt(DagNodeId, DagNodeId),
    FAdd(DagNodeId, DagNodeId),
    FSub(DagNodeId, DagNodeId),
    FMul(DagNodeId, DagNodeId),
    FDiv(DagNodeId, DagNodeId),
    FNeg(DagNodeId),
    FAbs(DagNodeId),
    FSqrt(DagNodeId),
    FEq(DagNodeId, DagNodeId),
    FNeq(DagNodeId, DagNodeId),
    FLt(DagNodeId, DagNodeId),
    FLe(DagNodeId, DagNodeId),
    FIsNan(DagNodeId),
    Ite {
        cond: DagNodeId,
        t: DagNodeId,
        f: DagNodeId,
    },
    Extract {
        expr: DagNodeId,
        lsb: u32,
        size: u32,
    },
    Concat(DagNodeId, DagNodeId),
    ArraySelect {
        array: DagNodeId,
        index: DagNodeId,
    },
    ArrayStore {
        array: DagNodeId,
        index: DagNodeId,
        value: DagNodeId,
    },
}

/// Elaborated node stored inside a [`ValidatedDag`] with memoized sort, pure-BV
/// eligibility, and subtree height.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DagNode {
    pub op: DagOp,
    pub sort: Sort,
    pub is_pure_bv: bool,
    pub height: usize,
}

/// Explicit resource bounds enforced during iterative validation and DAG
/// elaboration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidationBudget {
    pub max_depth: usize,
    pub max_expr_visits: usize,
    pub max_unique_nodes: usize,
}

impl Default for ValidationBudget {
    fn default() -> Self {
        Self {
            max_depth: MAX_EXPR_DEPTH,
            max_expr_visits: DEFAULT_MAX_VALIDATION_VISITS,
            max_unique_nodes: DEFAULT_MAX_VALIDATION_UNIQUE_NODES,
        }
    }
}

/// Instrumentation collected while elaborating [`SymExpr`] terms into a
/// [`ValidatedDag`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DagBuildStats {
    pub tree_occurrences: u64,
    pub unique_nodes: u64,
    pub cache_hits: u64,
    pub width_inference_visits: u64,
    pub max_depth_seen: usize,
}

/// Structured well-formedness error raised during expression, variable, or
/// query elaboration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    DepthExceeded {
        limit: usize,
    },
    VisitBudgetExceeded {
        limit: usize,
    },
    UniqueNodeBudgetExceeded {
        limit: usize,
    },
    ZeroWidth {
        context: &'static str,
    },
    ConflictingVarSort {
        id: SymNodeId,
        existing: Sort,
        found: Sort,
    },
    NonScalarOperand {
        op: &'static str,
        sort: Sort,
    },
    FloatOperandInBitVectorOp {
        op: &'static str,
        sort: Sort,
    },
    InvalidFloatOperand {
        op: &'static str,
        sort: Sort,
    },
    MismatchedOperandWidths {
        op: &'static str,
        lhs_bits: u32,
        rhs_bits: u32,
    },
    InvalidExtract {
        lsb: u32,
        size: u32,
        source_bits: u32,
    },
    ConcatWidthOverflow {
        lhs_bits: u32,
        rhs_bits: u32,
    },
    NonArrayTarget {
        op: &'static str,
        sort: Sort,
    },
    ArrayIndexSortMismatch {
        op: &'static str,
        expected: Sort,
        found: Sort,
    },
    ArrayValueSortMismatch {
        op: &'static str,
        expected: Sort,
        found: Sort,
    },
    InvalidIteCondition {
        sort: Sort,
    },
    IncompatibleIteBranches {
        then_sort: Sort,
        else_sort: Sort,
    },
    InvalidAssertionSort {
        sort: Sort,
    },
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DepthExceeded { limit } => {
                write!(f, "expression nesting exceeds depth limit ({limit})")
            }
            Self::VisitBudgetExceeded { limit } => {
                write!(f, "validation visit budget exceeded (limit {limit})")
            }
            Self::UniqueNodeBudgetExceeded { limit } => {
                write!(
                    f,
                    "validation unique DAG node budget exceeded (limit {limit})"
                )
            }
            Self::ZeroWidth { context } => write!(f, "zero-width sort in {context}"),
            Self::ConflictingVarSort {
                id,
                existing,
                found,
            } => write!(
                f,
                "variable id {id} declared with conflicting sorts: {existing:?} vs {found:?}"
            ),
            Self::NonScalarOperand { op, sort } => {
                write!(f, "{op} expects scalar operand, got {sort:?}")
            }
            Self::FloatOperandInBitVectorOp { op, sort } => {
                write!(f, "{op} requires bitvector operand, got {sort:?}")
            }
            Self::InvalidFloatOperand { op, sort } => {
                write!(
                    f,
                    "{op} requires float operand or bit-pattern constant, got {sort:?}"
                )
            }
            Self::MismatchedOperandWidths {
                op,
                lhs_bits,
                rhs_bits,
            } => write!(
                f,
                "{op} requires matching operand widths, got {lhs_bits} and {rhs_bits} bits"
            ),
            Self::InvalidExtract {
                lsb,
                size,
                source_bits,
            } => write!(
                f,
                "invalid Extract(lsb={lsb}, size={size}) on {source_bits}-bit value"
            ),
            Self::ConcatWidthOverflow { lhs_bits, rhs_bits } => {
                write!(f, "Concat width overflows u32 ({lhs_bits} + {rhs_bits})")
            }
            Self::NonArrayTarget { op, sort } => {
                write!(f, "{op} expects an array target, got {sort:?}")
            }
            Self::ArrayIndexSortMismatch {
                op,
                expected,
                found,
            } => write!(
                f,
                "{op} index sort mismatch: expected {expected:?}, got {found:?}"
            ),
            Self::ArrayValueSortMismatch {
                op,
                expected,
                found,
            } => write!(
                f,
                "{op} value sort mismatch: expected {expected:?}, got {found:?}"
            ),
            Self::InvalidIteCondition { sort } => {
                write!(f, "Ite condition must be BitVector(1), got {sort:?}")
            }
            Self::IncompatibleIteBranches {
                then_sort,
                else_sort,
            } => write!(
                f,
                "incompatible Ite branch sorts: {then_sort:?} vs {else_sort:?}"
            ),
            Self::InvalidAssertionSort { sort } => {
                write!(f, "assertion must have sort BitVector(1), got {sort:?}")
            }
        }
    }
}

impl std::error::Error for ValidationError {}

/// Elaborated variable-sort summary produced by validating a query.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValidatedQuery {
    pub vars: BTreeMap<SymNodeId, Sort>,
    pub has_arrays: bool,
}

/// Variable-sort consistency environment accumulated across one query, script,
/// or expression pair.
///
/// A long-lived environment (such as the identity table a [`crate::Solver`]
/// keeps for its backend) is extended with the `try_bind_*` methods, which are
/// atomic: an expression that fails validation leaves no binding behind, so a
/// rejected query cannot change how a later one is judged.
#[derive(Debug)]
pub struct VarSortEnv {
    env_id: u64,
    vars: BTreeMap<SymNodeId, Sort>,
    has_arrays: bool,
    /// IDs first bound inside the open transaction, if one is open.
    journal: Option<Vec<SymNodeId>>,
    rollback_count: u64,
}

impl Default for VarSortEnv {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for VarSortEnv {
    fn clone(&self) -> Self {
        Self {
            env_id: ENV_ID_COUNTER.fetch_add(1, Ordering::Relaxed),
            vars: self.vars.clone(),
            has_arrays: self.has_arrays,
            journal: self.journal.clone(),
            rollback_count: self.rollback_count,
        }
    }
}

impl VarSortEnv {
    #[must_use]
    pub fn new() -> Self {
        Self {
            env_id: ENV_ID_COUNTER.fetch_add(1, Ordering::Relaxed),
            vars: BTreeMap::new(),
            has_arrays: false,
            journal: None,
            rollback_count: 0,
        }
    }

    #[must_use]
    pub(crate) fn env_id(&self) -> u64 {
        self.env_id
    }

    #[must_use]
    pub(crate) fn rollback_count(&self) -> u64 {
        self.rollback_count
    }

    pub fn bind_var(&mut self, id: SymNodeId, sort: &Sort) -> Result<(), ValidationError> {
        validate_sort_depth(sort, MAX_EXPR_DEPTH)?;
        if matches!(sort, Sort::Array { .. }) {
            self.has_arrays = true;
        }
        if let Some(existing) = self.vars.get(&id) {
            if existing != sort {
                return Err(ValidationError::ConflictingVarSort {
                    id,
                    existing: existing.clone(),
                    found: sort.clone(),
                });
            }
        } else {
            self.vars.insert(id, sort.clone());
            if let Some(journal) = self.journal.as_mut() {
                journal.push(id);
            }
        }
        Ok(())
    }

    /// The sort bound to `id`, if any.
    #[must_use]
    pub fn sort_of(&self, id: SymNodeId) -> Option<&Sort> {
        self.vars.get(&id)
    }

    /// Number of distinct variable IDs bound.
    #[must_use]
    pub fn len(&self) -> usize {
        self.vars.len()
    }

    /// True when no variable is bound.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.vars.is_empty()
    }

    /// Run `f` as one transaction: on `Err` every binding it added is removed
    /// again and `has_arrays` is restored, so the environment is unchanged.
    fn transaction<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, ValidationError>,
    ) -> Result<T, ValidationError> {
        debug_assert!(
            self.journal.is_none(),
            "VarSortEnv transactions do not nest"
        );
        let had_arrays = self.has_arrays;
        self.journal = Some(Vec::new());
        let result = f(self);
        let journal = self.journal.take().unwrap_or_default();
        if result.is_err() {
            for id in journal {
                self.vars.remove(&id);
            }
            self.has_arrays = had_arrays;
            self.rollback_count = self.rollback_count.wrapping_add(1);
        }
        result
    }

    /// Atomically bind one declared variable.
    pub fn try_bind_var(&mut self, id: SymNodeId, sort: &Sort) -> Result<(), ValidationError> {
        self.transaction(|env| env.bind_var(id, sort))
    }

    /// Atomically validate `expr` against the bindings so far and keep its
    /// variable bindings only if it is well formed. Returns its sort.
    pub fn try_bind_expr(&mut self, expr: &SymExpr) -> Result<Sort, ValidationError> {
        self.transaction(|env| validate_expr_in_env(expr, env))
    }

    /// Atomically validate `expr` as an assertion (sort `BitVector(1)`).
    pub fn try_bind_assertion(&mut self, expr: &SymExpr) -> Result<(), ValidationError> {
        self.transaction(|env| validate_assertion_in_env(expr, env))
    }

    /// Atomically validate every expression in `exprs` as an assertion: either
    /// all of them are bound or none is.
    pub fn try_bind_assertions(&mut self, exprs: &[SymExpr]) -> Result<(), ValidationError> {
        self.transaction(|env| {
            let mut dag = ValidatedDag::new();
            for expr in exprs {
                let id = dag.intern_validated(expr, env, ValidationBudget::default())?;
                let sort = dag.sort(id);
                if *sort != Sort::BitVector(1) {
                    return Err(ValidationError::InvalidAssertionSort { sort: sort.clone() });
                }
            }
            Ok(())
        })
    }

    #[must_use]
    pub fn into_summary(self) -> ValidatedQuery {
        ValidatedQuery {
            vars: self.vars,
            has_arrays: self.has_arrays,
        }
    }
}

/// Scalar bit width of a [`Sort`]: array element width, or `0` for nested arrays.
#[must_use]
pub(crate) fn sort_bit_width(sort: &Sort) -> u32 {
    match sort {
        Sort::BitVector(bits) | Sort::Float(bits) => *bits,
        Sort::Array { range, .. } => match range.as_ref() {
            Sort::BitVector(bits) | Sort::Float(bits) => *bits,
            Sort::Array { .. } => 0,
        },
    }
}

/// Iterative depth/zero-width validator for nested `Sort::Array` descriptors.
fn validate_sort_depth(sort: &Sort, max_depth: usize) -> Result<(), ValidationError> {
    let mut stack = vec![(sort, 0usize)];
    while let Some((cur, depth)) = stack.pop() {
        if depth > max_depth {
            return Err(ValidationError::DepthExceeded { limit: max_depth });
        }
        match cur {
            Sort::BitVector(0) => {
                return Err(ValidationError::ZeroWidth {
                    context: "BitVector sort",
                });
            }
            Sort::Float(0) => {
                return Err(ValidationError::ZeroWidth {
                    context: "Float sort",
                });
            }
            Sort::BitVector(_) | Sort::Float(_) => {}
            Sort::Array { domain, range } => {
                stack.push((range.as_ref(), depth + 1));
                stack.push((domain.as_ref(), depth + 1));
            }
        }
    }
    Ok(())
}

/// Query-local, hash-consed symbolic DAG with memoized sorts, pure-BV status,
/// and iterative bounded elaboration.
#[derive(Debug, Clone)]
pub struct ValidatedDag {
    nodes: Vec<DagNode>,
    intern: HashMap<DagOp, DagNodeId>,
    bound_vars: BTreeMap<SymNodeId, Sort>,
    verified_env: Option<(u64, u64)>,
    stats: DagBuildStats,
}

impl Default for ValidatedDag {
    fn default() -> Self {
        Self::new()
    }
}

struct DagWalkFrame<'a> {
    expr: &'a SymExpr,
    depth: usize,
    next_child: u8,
    children: [Option<DagNodeId>; 3],
}

impl ValidatedDag {
    #[must_use]
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            intern: HashMap::new(),
            bound_vars: BTreeMap::new(),
            verified_env: None,
            stats: DagBuildStats::default(),
        }
    }

    #[must_use]
    pub fn node(&self, id: DagNodeId) -> &DagNode {
        &self.nodes[id.0 as usize]
    }

    #[must_use]
    pub fn sort(&self, id: DagNodeId) -> &Sort {
        &self.node(id).sort
    }

    #[must_use]
    pub fn bit_width(&self, id: DagNodeId) -> u32 {
        sort_bit_width(self.sort(id))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    #[must_use]
    pub fn stats(&self) -> DagBuildStats {
        self.stats
    }

    /// Elaborate `expr` iteratively into hash-consed DAG nodes checked against
    /// `env` and bounded by `budget`.
    ///
    /// On error, any newly inserted DAG nodes added during this call are rolled
    /// back so the DAG remains clean and consistent with `env`.
    pub fn intern_validated(
        &mut self,
        expr: &SymExpr,
        env: &mut VarSortEnv,
        budget: ValidationBudget,
    ) -> Result<DagNodeId, ValidationError> {
        self.intern_with_mode(expr, env, budget, false)
    }

    fn sync_env_bindings(&mut self, env: &mut VarSortEnv) -> Result<(), ValidationError> {
        let key = (env.env_id(), env.rollback_count());
        if self.verified_env == Some(key) {
            return Ok(());
        }
        for (&id, sort) in &self.bound_vars {
            env.bind_var(id, sort)?;
        }
        self.verified_env = Some((env.env_id(), env.rollback_count()));
        Ok(())
    }

    fn rollback_to(&mut self, prior_nodes_len: usize, prior_vars: &[SymNodeId]) {
        for idx in prior_nodes_len..self.nodes.len() {
            let op = &self.nodes[idx].op;
            self.intern.remove(op);
        }
        self.nodes.truncate(prior_nodes_len);
        for id in prior_vars {
            self.bound_vars.remove(id);
        }
    }

    fn push_interned(
        &mut self,
        op: DagOp,
        sort: Sort,
        is_pure_bv: bool,
        height: usize,
        budget: ValidationBudget,
    ) -> Result<DagNodeId, ValidationError> {
        if let Some(&id) = self.intern.get(&op) {
            self.stats.cache_hits += 1;
            return Ok(id);
        }
        if self.nodes.len() >= budget.max_unique_nodes {
            return Err(ValidationError::UniqueNodeBudgetExceeded {
                limit: budget.max_unique_nodes,
            });
        }
        let id = DagNodeId(u32::try_from(self.nodes.len()).unwrap_or(u32::MAX));
        self.nodes.push(DagNode {
            op: op.clone(),
            sort,
            is_pure_bv,
            height,
        });
        self.intern.insert(op, id);
        self.stats.unique_nodes += 1;
        self.stats.width_inference_visits += 1;
        Ok(id)
    }

    fn intern_with_mode(
        &mut self,
        expr: &SymExpr,
        env: &mut VarSortEnv,
        budget: ValidationBudget,
        pure_bv_only: bool,
    ) -> Result<DagNodeId, ValidationError> {
        self.sync_env_bindings(env)?;
        let prior_nodes_len = self.nodes.len();
        let mut new_vars: Vec<SymNodeId> = Vec::new();
        let mut ptr_cache: HashMap<*const SymExpr, DagNodeId> = HashMap::new();

        let res = self.intern_inner(
            expr,
            env,
            budget,
            pure_bv_only,
            &mut new_vars,
            &mut ptr_cache,
        );
        match res {
            Ok(root_id) => {
                self.verified_env = Some((env.env_id(), env.rollback_count()));
                Ok(root_id)
            }
            Err(err) => {
                self.rollback_to(prior_nodes_len, &new_vars);
                Err(err)
            }
        }
    }

    fn intern_inner(
        &mut self,
        root: &SymExpr,
        env: &mut VarSortEnv,
        budget: ValidationBudget,
        pure_bv_only: bool,
        new_vars: &mut Vec<SymNodeId>,
        ptr_cache: &mut HashMap<*const SymExpr, DagNodeId>,
    ) -> Result<DagNodeId, ValidationError> {
        let mut visits = 0usize;
        let mut stack = vec![DagWalkFrame {
            expr: root,
            depth: 0,
            next_child: 0,
            children: [None, None, None],
        }];

        while let Some(frame) = stack.last_mut() {
            let cur = frame.expr;
            let cur_depth = frame.depth;

            if frame.next_child == 0 {
                if cur_depth > budget.max_depth {
                    return Err(ValidationError::DepthExceeded {
                        limit: budget.max_depth,
                    });
                }
                visits += 1;
                self.stats.tree_occurrences += 1;
                if cur_depth > self.stats.max_depth_seen {
                    self.stats.max_depth_seen = cur_depth;
                }
                if visits > budget.max_expr_visits {
                    return Err(ValidationError::VisitBudgetExceeded {
                        limit: budget.max_expr_visits,
                    });
                }

                let ptr = std::ptr::from_ref(cur);
                if let Some(&cached_id) = ptr_cache.get(&ptr) {
                    let (height, cached_pure_bv, cached_sort) = {
                        let cached = self.node(cached_id);
                        (cached.height, cached.is_pure_bv, cached.sort.clone())
                    };
                    let deepest = cur_depth.saturating_add(height);
                    if deepest > budget.max_depth {
                        return Err(ValidationError::DepthExceeded {
                            limit: budget.max_depth,
                        });
                    }
                    if deepest > self.stats.max_depth_seen {
                        self.stats.max_depth_seen = deepest;
                    }
                    self.stats.cache_hits += 1;
                    if pure_bv_only && !cached_pure_bv {
                        return Err(ValidationError::NonScalarOperand {
                            op: "pure-BV",
                            sort: cached_sort,
                        });
                    }
                    stack.pop();
                    if let Some(parent) = stack.last_mut() {
                        let slot = (parent.next_child - 1) as usize;
                        parent.children[slot] = Some(cached_id);
                    } else {
                        return Ok(cached_id);
                    }
                    continue;
                }

                // Leaf / immediate checks before child recursion.
                match cur {
                    SymExpr::Const { val, size } => {
                        if pure_bv_only {
                            if !(1..=64).contains(size) {
                                return Err(ValidationError::ZeroWidth {
                                    context: "unsupported pure-BV constant width",
                                });
                            }
                        } else if *size == 0 {
                            return Err(ValidationError::ZeroWidth {
                                context: "Const size",
                            });
                        }
                        let is_pure = (1..=64).contains(size);
                        let id = self.push_interned(
                            DagOp::Const {
                                val: *val,
                                size: *size,
                            },
                            Sort::BitVector(*size),
                            is_pure,
                            0,
                            budget,
                        )?;
                        ptr_cache.insert(ptr, id);
                        stack.pop();
                        if let Some(parent) = stack.last_mut() {
                            let slot = (parent.next_child - 1) as usize;
                            parent.children[slot] = Some(id);
                        } else {
                            return Ok(id);
                        }
                        continue;
                    }
                    SymExpr::Var {
                        id: var_id,
                        name,
                        sort,
                    } => {
                        if pure_bv_only {
                            let Sort::BitVector(bits) = sort else {
                                return Err(ValidationError::NonScalarOperand {
                                    op: "pure-BV Var",
                                    sort: sort.clone(),
                                });
                            };
                            if !(1..=64).contains(bits) {
                                return Err(ValidationError::ZeroWidth {
                                    context: "unsupported pure-BV variable width",
                                });
                            }
                        }
                        validate_sort_depth(sort, MAX_EXPR_DEPTH)?;
                        let was_bound = self.bound_vars.contains_key(var_id);
                        env.bind_var(*var_id, sort)?;
                        if !was_bound {
                            self.bound_vars.insert(*var_id, sort.clone());
                            new_vars.push(*var_id);
                        }
                        let is_pure =
                            matches!(sort, Sort::BitVector(bits) if (1..=64).contains(bits));
                        let node_id = self.push_interned(
                            DagOp::Var {
                                id: *var_id,
                                name: name.clone(),
                                sort: sort.clone(),
                            },
                            sort.clone(),
                            is_pure,
                            0,
                            budget,
                        )?;
                        ptr_cache.insert(ptr, node_id);
                        stack.pop();
                        if let Some(parent) = stack.last_mut() {
                            let slot = (parent.next_child - 1) as usize;
                            parent.children[slot] = Some(node_id);
                        } else {
                            return Ok(node_id);
                        }
                        continue;
                    }
                    SymExpr::FAdd(..)
                    | SymExpr::FSub(..)
                    | SymExpr::FMul(..)
                    | SymExpr::FDiv(..)
                    | SymExpr::FNeg(..)
                    | SymExpr::FAbs(..)
                    | SymExpr::FSqrt(..)
                    | SymExpr::FEq(..)
                    | SymExpr::FNeq(..)
                    | SymExpr::FLt(..)
                    | SymExpr::FLe(..)
                    | SymExpr::FIsNan(..)
                        if pure_bv_only =>
                    {
                        return Err(ValidationError::FloatOperandInBitVectorOp {
                            op: "pure-BV",
                            sort: Sort::Float(1),
                        });
                    }
                    SymExpr::ArraySelect { .. } | SymExpr::ArrayStore { .. } if pure_bv_only => {
                        return Err(ValidationError::NonScalarOperand {
                            op: "pure-BV",
                            sort: Sort::Array {
                                domain: Box::new(Sort::BitVector(1)),
                                range: Box::new(Sort::BitVector(1)),
                            },
                        });
                    }
                    _ => {}
                }
            }

            // Determine child to visit next (or finalize when all children visited),
            // checking left-child sort errors immediately before entering right child.
            let next_child_expr = match (cur, frame.next_child) {
                (
                    SymExpr::FNeg(a)
                    | SymExpr::FAbs(a)
                    | SymExpr::FSqrt(a)
                    | SymExpr::FIsNan(a)
                    | SymExpr::Extract { expr: a, .. },
                    0,
                ) => Some(a.as_ref()),
                (
                    SymExpr::Add(a, _)
                    | SymExpr::Sub(a, _)
                    | SymExpr::Mul(a, _)
                    | SymExpr::Udiv(a, _)
                    | SymExpr::Urem(a, _)
                    | SymExpr::Sdiv(a, _)
                    | SymExpr::Srem(a, _)
                    | SymExpr::Smod(a, _)
                    | SymExpr::And(a, _)
                    | SymExpr::Or(a, _)
                    | SymExpr::Xor(a, _)
                    | SymExpr::Shl(a, _)
                    | SymExpr::Lshr(a, _)
                    | SymExpr::Ashr(a, _)
                    | SymExpr::Eq(a, _)
                    | SymExpr::Neq(a, _)
                    | SymExpr::Ult(a, _)
                    | SymExpr::Ule(a, _)
                    | SymExpr::Slt(a, _)
                    | SymExpr::Sle(a, _)
                    | SymExpr::Sgt(a, _)
                    | SymExpr::FAdd(a, _)
                    | SymExpr::FSub(a, _)
                    | SymExpr::FMul(a, _)
                    | SymExpr::FDiv(a, _)
                    | SymExpr::FEq(a, _)
                    | SymExpr::FNeq(a, _)
                    | SymExpr::FLt(a, _)
                    | SymExpr::FLe(a, _)
                    | SymExpr::Concat(a, _),
                    0,
                )
                | (SymExpr::Ite { cond: a, .. }, 0)
                | (SymExpr::ArraySelect { array: a, .. }, 0)
                | (SymExpr::ArrayStore { array: a, .. }, 0) => Some(a.as_ref()),
                (
                    SymExpr::Add(_, b)
                    | SymExpr::Sub(_, b)
                    | SymExpr::Mul(_, b)
                    | SymExpr::Udiv(_, b)
                    | SymExpr::Urem(_, b)
                    | SymExpr::Sdiv(_, b)
                    | SymExpr::Srem(_, b)
                    | SymExpr::Smod(_, b)
                    | SymExpr::And(_, b)
                    | SymExpr::Or(_, b)
                    | SymExpr::Xor(_, b)
                    | SymExpr::Shl(_, b)
                    | SymExpr::Lshr(_, b)
                    | SymExpr::Ashr(_, b)
                    | SymExpr::Eq(_, b)
                    | SymExpr::Neq(_, b)
                    | SymExpr::Ult(_, b)
                    | SymExpr::Ule(_, b)
                    | SymExpr::Slt(_, b)
                    | SymExpr::Sle(_, b)
                    | SymExpr::Sgt(_, b)
                    | SymExpr::FAdd(_, b)
                    | SymExpr::FSub(_, b)
                    | SymExpr::FMul(_, b)
                    | SymExpr::FDiv(_, b)
                    | SymExpr::FEq(_, b)
                    | SymExpr::FNeq(_, b)
                    | SymExpr::FLt(_, b)
                    | SymExpr::FLe(_, b)
                    | SymExpr::Concat(_, b),
                    1,
                )
                | (SymExpr::Ite { t: b, .. }, 1)
                | (SymExpr::ArraySelect { index: b, .. }, 1)
                | (SymExpr::ArrayStore { index: b, .. }, 1) => {
                    let c0 = frame.children[0].expect("child 0 visited");
                    self.validate_first_child_early(cur, c0, pure_bv_only)?;
                    Some(b.as_ref())
                }
                (SymExpr::Ite { f: c, .. }, 2) | (SymExpr::ArrayStore { value: c, .. }, 2) => {
                    let c0 = frame.children[0].expect("child 0 visited");
                    let c1 = frame.children[1].expect("child 1 visited");
                    self.validate_second_child_early(cur, c0, c1, pure_bv_only)?;
                    Some(c.as_ref())
                }
                _ => None,
            };

            if let Some(child_expr) = next_child_expr {
                frame.next_child += 1;
                stack.push(DagWalkFrame {
                    expr: child_expr,
                    depth: cur_depth + 1,
                    next_child: 0,
                    children: [None, None, None],
                });
                continue;
            }

            // All children for this frame are ready: elaborate node bottom-up.
            let done = stack.pop().expect("active frame");
            let id = self.finalize_node(&done, budget, pure_bv_only)?;
            ptr_cache.insert(std::ptr::from_ref(done.expr), id);
            if let Some(parent) = stack.last_mut() {
                let slot = (parent.next_child - 1) as usize;
                parent.children[slot] = Some(id);
            } else {
                return Ok(id);
            }
        }

        unreachable!("validation loop returns on root completion")
    }

    fn validate_first_child_early(
        &self,
        parent: &SymExpr,
        c0: DagNodeId,
        pure_bv_only: bool,
    ) -> Result<(), ValidationError> {
        let s0 = &self.node(c0).sort;
        match parent {
            SymExpr::Add(..)
            | SymExpr::Sub(..)
            | SymExpr::Mul(..)
            | SymExpr::Udiv(..)
            | SymExpr::Urem(..)
            | SymExpr::And(..)
            | SymExpr::Or(..)
            | SymExpr::Xor(..)
            | SymExpr::Sdiv(..)
            | SymExpr::Srem(..)
            | SymExpr::Smod(..)
            | SymExpr::Shl(..)
            | SymExpr::Lshr(..)
            | SymExpr::Ashr(..) => {
                let op = op_name(parent);
                let _ = expect_bv_sort(s0, op, pure_bv_only)?;
            }
            SymExpr::Ult(..)
            | SymExpr::Ule(..)
            | SymExpr::Slt(..)
            | SymExpr::Sle(..)
            | SymExpr::Sgt(..) => {
                let op = if pure_bv_only {
                    if matches!(parent, SymExpr::Ult(..) | SymExpr::Ule(..)) {
                        "pure-BV cmp"
                    } else {
                        "pure-BV signed cmp"
                    }
                } else {
                    op_name(parent)
                };
                let _ = expect_bv_sort(s0, op, pure_bv_only)?;
            }
            SymExpr::Eq(..) | SymExpr::Neq(..) if pure_bv_only => {
                let _ = expect_bv_sort(s0, "pure-BV cmp", true)?;
            }
            SymExpr::Concat(..) => {
                let _ = expect_bv_sort(s0, "Concat", pure_bv_only)?;
            }
            SymExpr::FAdd(..)
            | SymExpr::FSub(..)
            | SymExpr::FMul(..)
            | SymExpr::FDiv(..)
            | SymExpr::FEq(..)
            | SymExpr::FNeq(..)
            | SymExpr::FLt(..)
            | SymExpr::FLe(..) => {
                let op = op_name(parent);
                let _ = expect_float_like_node(self.node(c0), op)?;
            }
            SymExpr::Ite { .. } => {
                if *s0 != Sort::BitVector(1) {
                    return Err(ValidationError::InvalidIteCondition { sort: s0.clone() });
                }
            }
            SymExpr::ArraySelect { .. } => {
                if !matches!(s0, Sort::Array { .. }) {
                    return Err(ValidationError::NonArrayTarget {
                        op: "ArraySelect",
                        sort: s0.clone(),
                    });
                }
            }
            SymExpr::ArrayStore { .. } => {
                if !matches!(s0, Sort::Array { .. }) {
                    return Err(ValidationError::NonArrayTarget {
                        op: "ArrayStore",
                        sort: s0.clone(),
                    });
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn validate_second_child_early(
        &self,
        parent: &SymExpr,
        c0: DagNodeId,
        c1: DagNodeId,
        pure_bv_only: bool,
    ) -> Result<(), ValidationError> {
        let s0 = &self.node(c0).sort;
        let s1 = &self.node(c1).sort;
        match parent {
            SymExpr::Ite { .. } if pure_bv_only => {
                let _ = expect_bv_sort(s1, "Ite", true)?;
            }
            SymExpr::ArrayStore { .. } => {
                let Sort::Array { domain, .. } = s0 else {
                    return Err(ValidationError::NonArrayTarget {
                        op: "ArrayStore",
                        sort: s0.clone(),
                    });
                };
                if s1 != domain.as_ref() {
                    return Err(ValidationError::ArrayIndexSortMismatch {
                        op: "ArrayStore",
                        expected: domain.as_ref().clone(),
                        found: s1.clone(),
                    });
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn finalize_node(
        &mut self,
        frame: &DagWalkFrame<'_>,
        budget: ValidationBudget,
        pure_bv_only: bool,
    ) -> Result<DagNodeId, ValidationError> {
        let expr = frame.expr;
        let c0 = frame.children[0];
        let c1 = frame.children[1];
        let c2 = frame.children[2];

        let (op, sort, is_pure_bv, height) = match expr {
            SymExpr::Add(..)
            | SymExpr::Sub(..)
            | SymExpr::Mul(..)
            | SymExpr::Udiv(..)
            | SymExpr::Urem(..)
            | SymExpr::And(..)
            | SymExpr::Or(..)
            | SymExpr::Xor(..) => {
                let a = c0.unwrap();
                let b = c1.unwrap();
                let na = self.node(a);
                let nb = self.node(b);
                let op_str = if pure_bv_only {
                    "pure-BV op"
                } else {
                    op_name(expr)
                };
                let wa = expect_bv_sort(&na.sort, op_str, pure_bv_only)?;
                let wb = expect_bv_sort(&nb.sort, op_str, pure_bv_only)?;
                let pure = na.is_pure_bv
                    && nb.is_pure_bv
                    && (1..=64).contains(&wa)
                    && (1..=64).contains(&wb);
                let h = na.height.max(nb.height) + 1;
                (
                    build_bin_op(expr, a, b),
                    Sort::BitVector(wa.max(wb)),
                    pure,
                    h,
                )
            }
            SymExpr::Sdiv(..)
            | SymExpr::Srem(..)
            | SymExpr::Smod(..)
            | SymExpr::Shl(..)
            | SymExpr::Lshr(..)
            | SymExpr::Ashr(..) => {
                let a = c0.unwrap();
                let b = c1.unwrap();
                let na = self.node(a);
                let nb = self.node(b);
                let op_str = if pure_bv_only {
                    "pure-BV op"
                } else {
                    op_name(expr)
                };
                let wa = expect_bv_sort(&na.sort, op_str, pure_bv_only)?;
                let wb = expect_bv_sort(&nb.sort, op_str, pure_bv_only)?;
                if wa != wb {
                    return Err(ValidationError::MismatchedOperandWidths {
                        op: op_str,
                        lhs_bits: wa,
                        rhs_bits: wb,
                    });
                }
                let pure = na.is_pure_bv && nb.is_pure_bv && (1..=64).contains(&wa);
                let h = na.height.max(nb.height) + 1;
                (build_bin_op(expr, a, b), Sort::BitVector(wa), pure, h)
            }
            SymExpr::Eq(..) | SymExpr::Neq(..) => {
                let a = c0.unwrap();
                let b = c1.unwrap();
                let na = self.node(a);
                let nb = self.node(b);
                let h = na.height.max(nb.height) + 1;
                if pure_bv_only {
                    let _ = expect_bv_sort(&na.sort, "pure-BV cmp", true)?;
                    let _ = expect_bv_sort(&nb.sort, "pure-BV cmp", true)?;
                    let pure = na.is_pure_bv && nb.is_pure_bv;
                    (build_bin_op(expr, a, b), Sort::BitVector(1), pure, h)
                } else {
                    let op_str = op_name(expr);
                    validate_eq_neq_nodes(op_str, na, nb)?;
                    let pure = na.is_pure_bv
                        && nb.is_pure_bv
                        && matches!(na.sort, Sort::BitVector(_))
                        && matches!(nb.sort, Sort::BitVector(_));
                    (build_bin_op(expr, a, b), Sort::BitVector(1), pure, h)
                }
            }
            SymExpr::Ult(..) | SymExpr::Ule(..) => {
                let a = c0.unwrap();
                let b = c1.unwrap();
                let na = self.node(a);
                let nb = self.node(b);
                let op_str = if pure_bv_only {
                    "pure-BV cmp"
                } else {
                    op_name(expr)
                };
                let _ = expect_bv_sort(&na.sort, op_str, pure_bv_only)?;
                let _ = expect_bv_sort(&nb.sort, op_str, pure_bv_only)?;
                let pure = na.is_pure_bv && nb.is_pure_bv;
                let h = na.height.max(nb.height) + 1;
                (build_bin_op(expr, a, b), Sort::BitVector(1), pure, h)
            }
            SymExpr::Slt(..) | SymExpr::Sle(..) | SymExpr::Sgt(..) => {
                let a = c0.unwrap();
                let b = c1.unwrap();
                let na = self.node(a);
                let nb = self.node(b);
                let op_str = if pure_bv_only {
                    "pure-BV signed cmp"
                } else {
                    op_name(expr)
                };
                let wa = expect_bv_sort(&na.sort, op_str, pure_bv_only)?;
                let wb = expect_bv_sort(&nb.sort, op_str, pure_bv_only)?;
                if wa != wb {
                    return Err(ValidationError::MismatchedOperandWidths {
                        op: op_str,
                        lhs_bits: wa,
                        rhs_bits: wb,
                    });
                }
                let pure = na.is_pure_bv && nb.is_pure_bv;
                let h = na.height.max(nb.height) + 1;
                (build_bin_op(expr, a, b), Sort::BitVector(1), pure, h)
            }
            SymExpr::FNeg(..) | SymExpr::FAbs(..) | SymExpr::FSqrt(..) => {
                let a = c0.unwrap();
                let na = self.node(a);
                let w = expect_float_like_node(na, op_name(expr))?;
                let op = match expr {
                    SymExpr::FNeg(..) => DagOp::FNeg(a),
                    SymExpr::FAbs(..) => DagOp::FAbs(a),
                    _ => DagOp::FSqrt(a),
                };
                (op, Sort::Float(w), false, na.height + 1)
            }
            SymExpr::FIsNan(..) => {
                let a = c0.unwrap();
                let na = self.node(a);
                let _ = expect_float_like_node(na, "FIsNan")?;
                (DagOp::FIsNan(a), Sort::BitVector(1), false, na.height + 1)
            }
            SymExpr::FAdd(..) | SymExpr::FSub(..) | SymExpr::FMul(..) | SymExpr::FDiv(..) => {
                let a = c0.unwrap();
                let b = c1.unwrap();
                let na = self.node(a);
                let nb = self.node(b);
                let op_str = op_name(expr);
                let wa = expect_float_like_node(na, op_str)?;
                let wb = expect_float_like_node(nb, op_str)?;
                if wa != wb {
                    return Err(ValidationError::MismatchedOperandWidths {
                        op: op_str,
                        lhs_bits: wa,
                        rhs_bits: wb,
                    });
                }
                (
                    build_bin_op(expr, a, b),
                    Sort::Float(wa),
                    false,
                    na.height.max(nb.height) + 1,
                )
            }
            SymExpr::FEq(..) | SymExpr::FNeq(..) | SymExpr::FLt(..) | SymExpr::FLe(..) => {
                let a = c0.unwrap();
                let b = c1.unwrap();
                let na = self.node(a);
                let nb = self.node(b);
                let op_str = op_name(expr);
                let wa = expect_float_like_node(na, op_str)?;
                let wb = expect_float_like_node(nb, op_str)?;
                if wa != wb {
                    return Err(ValidationError::MismatchedOperandWidths {
                        op: op_str,
                        lhs_bits: wa,
                        rhs_bits: wb,
                    });
                }
                (
                    build_bin_op(expr, a, b),
                    Sort::BitVector(1),
                    false,
                    na.height.max(nb.height) + 1,
                )
            }
            SymExpr::Ite { .. } => {
                let cond = c0.unwrap();
                let t = c1.unwrap();
                let f = c2.unwrap();
                let nc = self.node(cond);
                let nt = self.node(t);
                let nf = self.node(f);
                let h = nc.height.max(nt.height).max(nf.height) + 1;
                if nc.sort != Sort::BitVector(1) {
                    return Err(ValidationError::InvalidIteCondition {
                        sort: nc.sort.clone(),
                    });
                }
                if pure_bv_only {
                    let wt = expect_bv_sort(&nt.sort, "Ite", true)?;
                    let wf = expect_bv_sort(&nf.sort, "Ite", true)?;
                    let pure = nc.is_pure_bv && nt.is_pure_bv && nf.is_pure_bv;
                    (
                        DagOp::Ite { cond, t, f },
                        Sort::BitVector(wt.max(wf)),
                        pure,
                        h,
                    )
                } else {
                    let sort = match (&nt.sort, &nf.sort) {
                        (Sort::BitVector(wt), Sort::BitVector(wf)) => {
                            Sort::BitVector((*wt).max(*wf))
                        }
                        (Sort::Float(wt), Sort::Float(wf)) if wt == wf => Sort::Float(*wt),
                        (Sort::Float(wt), Sort::BitVector(wf))
                            if wt == wf && matches!(nf.op, DagOp::Const { .. }) =>
                        {
                            Sort::Float(*wt)
                        }
                        (Sort::BitVector(wt), Sort::Float(wf))
                            if wt == wf && matches!(nt.op, DagOp::Const { .. }) =>
                        {
                            Sort::Float(*wf)
                        }
                        (Sort::Array { .. }, Sort::Array { .. }) if nt.sort == nf.sort => {
                            nt.sort.clone()
                        }
                        _ => {
                            return Err(ValidationError::IncompatibleIteBranches {
                                then_sort: nt.sort.clone(),
                                else_sort: nf.sort.clone(),
                            });
                        }
                    };
                    let pure = nc.is_pure_bv
                        && nt.is_pure_bv
                        && nf.is_pure_bv
                        && matches!(sort, Sort::BitVector(w) if (1..=64).contains(&w));
                    (DagOp::Ite { cond, t, f }, sort, pure, h)
                }
            }
            SymExpr::Extract { lsb, size, .. } => {
                let inner = c0.unwrap();
                let ni = self.node(inner);
                let src_bits = expect_bv_sort(&ni.sort, "Extract", pure_bv_only)?;
                if *size == 0 || lsb.checked_add(*size).is_none_or(|end| end > src_bits) {
                    return Err(ValidationError::InvalidExtract {
                        lsb: *lsb,
                        size: *size,
                        source_bits: src_bits,
                    });
                }
                let pure = ni.is_pure_bv && (1..=64).contains(size);
                (
                    DagOp::Extract {
                        expr: inner,
                        lsb: *lsb,
                        size: *size,
                    },
                    Sort::BitVector(*size),
                    pure,
                    ni.height + 1,
                )
            }
            SymExpr::Concat(..) => {
                let a = c0.unwrap();
                let b = c1.unwrap();
                let na = self.node(a);
                let nb = self.node(b);
                let wa = expect_bv_sort(&na.sort, "Concat", pure_bv_only)?;
                let wb = expect_bv_sort(&nb.sort, "Concat", pure_bv_only)?;
                let sum = if pure_bv_only {
                    wa.checked_add(wb)
                        .filter(|&w| (1..=64).contains(&w))
                        .ok_or(ValidationError::ConcatWidthOverflow {
                            lhs_bits: wa,
                            rhs_bits: wb,
                        })?
                } else {
                    wa.checked_add(wb)
                        .ok_or(ValidationError::ConcatWidthOverflow {
                            lhs_bits: wa,
                            rhs_bits: wb,
                        })?
                };
                let pure = na.is_pure_bv && nb.is_pure_bv && (1..=64).contains(&sum);
                let h = na.height.max(nb.height) + 1;
                (DagOp::Concat(a, b), Sort::BitVector(sum), pure, h)
            }
            SymExpr::ArraySelect { .. } => {
                let array = c0.unwrap();
                let index = c1.unwrap();
                let na = self.node(array);
                let ni = self.node(index);
                let Sort::Array { domain, range } = &na.sort else {
                    return Err(ValidationError::NonArrayTarget {
                        op: "ArraySelect",
                        sort: na.sort.clone(),
                    });
                };
                if ni.sort != **domain {
                    return Err(ValidationError::ArrayIndexSortMismatch {
                        op: "ArraySelect",
                        expected: domain.as_ref().clone(),
                        found: ni.sort.clone(),
                    });
                }
                let h = na.height.max(ni.height) + 1;
                (
                    DagOp::ArraySelect { array, index },
                    range.as_ref().clone(),
                    false,
                    h,
                )
            }
            SymExpr::ArrayStore { .. } => {
                let array = c0.unwrap();
                let index = c1.unwrap();
                let value = c2.unwrap();
                let na = self.node(array);
                let ni = self.node(index);
                let nv = self.node(value);
                let Sort::Array { domain, range } = &na.sort else {
                    return Err(ValidationError::NonArrayTarget {
                        op: "ArrayStore",
                        sort: na.sort.clone(),
                    });
                };
                if ni.sort != **domain {
                    return Err(ValidationError::ArrayIndexSortMismatch {
                        op: "ArrayStore",
                        expected: domain.as_ref().clone(),
                        found: ni.sort.clone(),
                    });
                }
                if nv.sort != **range {
                    return Err(ValidationError::ArrayValueSortMismatch {
                        op: "ArrayStore",
                        expected: range.as_ref().clone(),
                        found: nv.sort.clone(),
                    });
                }
                let h = na.height.max(ni.height).max(nv.height) + 1;
                (
                    DagOp::ArrayStore {
                        array,
                        index,
                        value,
                    },
                    na.sort.clone(),
                    false,
                    h,
                )
            }
            SymExpr::Const { .. } | SymExpr::Var { .. } => {
                unreachable!("leaves are finalized directly on initial entry")
            }
        };

        self.push_interned(op, sort, is_pure_bv, height, budget)
    }
}

fn op_name(expr: &SymExpr) -> &'static str {
    match expr {
        SymExpr::Const { .. } => "Const",
        SymExpr::Var { .. } => "Var",
        SymExpr::Add(..) => "Add",
        SymExpr::Sub(..) => "Sub",
        SymExpr::Mul(..) => "Mul",
        SymExpr::Udiv(..) => "Udiv",
        SymExpr::Urem(..) => "Urem",
        SymExpr::Sdiv(..) => "Sdiv",
        SymExpr::Srem(..) => "Srem",
        SymExpr::Smod(..) => "Smod",
        SymExpr::And(..) => "And",
        SymExpr::Or(..) => "Or",
        SymExpr::Xor(..) => "Xor",
        SymExpr::Shl(..) => "Shl",
        SymExpr::Lshr(..) => "Lshr",
        SymExpr::Ashr(..) => "Ashr",
        SymExpr::Eq(..) => "Eq",
        SymExpr::Neq(..) => "Neq",
        SymExpr::Ult(..) => "Ult",
        SymExpr::Ule(..) => "Ule",
        SymExpr::Slt(..) => "Slt",
        SymExpr::Sle(..) => "Sle",
        SymExpr::Sgt(..) => "Sgt",
        SymExpr::FAdd(..) => "FAdd",
        SymExpr::FSub(..) => "FSub",
        SymExpr::FMul(..) => "FMul",
        SymExpr::FDiv(..) => "FDiv",
        SymExpr::FNeg(..) => "FNeg",
        SymExpr::FAbs(..) => "FAbs",
        SymExpr::FSqrt(..) => "FSqrt",
        SymExpr::FEq(..) => "FEq",
        SymExpr::FNeq(..) => "FNeq",
        SymExpr::FLt(..) => "FLt",
        SymExpr::FLe(..) => "FLe",
        SymExpr::FIsNan(..) => "FIsNan",
        SymExpr::Ite { .. } => "Ite",
        SymExpr::Extract { .. } => "Extract",
        SymExpr::Concat(..) => "Concat",
        SymExpr::ArraySelect { .. } => "ArraySelect",
        SymExpr::ArrayStore { .. } => "ArrayStore",
    }
}

fn build_bin_op(expr: &SymExpr, a: DagNodeId, b: DagNodeId) -> DagOp {
    match expr {
        SymExpr::Add(..) => DagOp::Add(a, b),
        SymExpr::Sub(..) => DagOp::Sub(a, b),
        SymExpr::Mul(..) => DagOp::Mul(a, b),
        SymExpr::Udiv(..) => DagOp::Udiv(a, b),
        SymExpr::Urem(..) => DagOp::Urem(a, b),
        SymExpr::Sdiv(..) => DagOp::Sdiv(a, b),
        SymExpr::Srem(..) => DagOp::Srem(a, b),
        SymExpr::Smod(..) => DagOp::Smod(a, b),
        SymExpr::And(..) => DagOp::And(a, b),
        SymExpr::Or(..) => DagOp::Or(a, b),
        SymExpr::Xor(..) => DagOp::Xor(a, b),
        SymExpr::Shl(..) => DagOp::Shl(a, b),
        SymExpr::Lshr(..) => DagOp::Lshr(a, b),
        SymExpr::Ashr(..) => DagOp::Ashr(a, b),
        SymExpr::Eq(..) => DagOp::Eq(a, b),
        SymExpr::Neq(..) => DagOp::Neq(a, b),
        SymExpr::Ult(..) => DagOp::Ult(a, b),
        SymExpr::Ule(..) => DagOp::Ule(a, b),
        SymExpr::Slt(..) => DagOp::Slt(a, b),
        SymExpr::Sle(..) => DagOp::Sle(a, b),
        SymExpr::Sgt(..) => DagOp::Sgt(a, b),
        SymExpr::FAdd(..) => DagOp::FAdd(a, b),
        SymExpr::FSub(..) => DagOp::FSub(a, b),
        SymExpr::FMul(..) => DagOp::FMul(a, b),
        SymExpr::FDiv(..) => DagOp::FDiv(a, b),
        SymExpr::FEq(..) => DagOp::FEq(a, b),
        SymExpr::FNeq(..) => DagOp::FNeq(a, b),
        SymExpr::FLt(..) => DagOp::FLt(a, b),
        SymExpr::FLe(..) => DagOp::FLe(a, b),
        _ => unreachable!("only called for binary SymExpr variants"),
    }
}

fn expect_bv_sort(
    sort: &Sort,
    op: &'static str,
    pure_bv_only: bool,
) -> Result<u32, ValidationError> {
    match sort {
        Sort::BitVector(bits) => {
            if pure_bv_only && !(1..=64).contains(bits) {
                return Err(ValidationError::ZeroWidth {
                    context: "unsupported pure-BV width",
                });
            }
            Ok(*bits)
        }
        Sort::Float(_) => Err(ValidationError::FloatOperandInBitVectorOp {
            op,
            sort: sort.clone(),
        }),
        Sort::Array { .. } => Err(ValidationError::NonScalarOperand {
            op,
            sort: sort.clone(),
        }),
    }
}

fn expect_float_like_node(node: &DagNode, op: &'static str) -> Result<u32, ValidationError> {
    match &node.sort {
        Sort::Float(bits) => Ok(*bits),
        Sort::BitVector(bits) if matches!(node.op, DagOp::Const { .. }) => Ok(*bits),
        Sort::BitVector(_) => Err(ValidationError::InvalidFloatOperand {
            op,
            sort: node.sort.clone(),
        }),
        Sort::Array { .. } => Err(ValidationError::NonScalarOperand {
            op,
            sort: node.sort.clone(),
        }),
    }
}

fn validate_eq_neq_nodes(
    op: &'static str,
    na: &DagNode,
    nb: &DagNode,
) -> Result<(), ValidationError> {
    let sa = &na.sort;
    let sb = &nb.sort;
    match (sa, sb) {
        (Sort::BitVector(_), Sort::BitVector(_)) => Ok(()),
        (Sort::Float(wa), Sort::Float(wb)) if wa == wb => Ok(()),
        (Sort::Float(wa), Sort::BitVector(wb))
            if wa == wb && matches!(nb.op, DagOp::Const { .. }) =>
        {
            Ok(())
        }
        (Sort::BitVector(wa), Sort::Float(wb))
            if wa == wb && matches!(na.op, DagOp::Const { .. }) =>
        {
            Ok(())
        }
        (Sort::Float(wa), Sort::Float(wb))
        | (Sort::Float(wa), Sort::BitVector(wb))
        | (Sort::BitVector(wa), Sort::Float(wb))
            if wa != wb =>
        {
            Err(ValidationError::MismatchedOperandWidths {
                op,
                lhs_bits: *wa,
                rhs_bits: *wb,
            })
        }
        (Sort::Float(_), _) => Err(ValidationError::FloatOperandInBitVectorOp {
            op,
            sort: sa.clone(),
        }),
        (_, Sort::Float(_)) => Err(ValidationError::FloatOperandInBitVectorOp {
            op,
            sort: sb.clone(),
        }),
        (Sort::Array { .. }, _) => Err(ValidationError::NonScalarOperand {
            op,
            sort: sa.clone(),
        }),
        (_, Sort::Array { .. }) => Err(ValidationError::NonScalarOperand {
            op,
            sort: sb.clone(),
        }),
    }
}

/// Validate `expr` standalone (with a fresh [`VarSortEnv`]), returning its elaborated sort.
pub fn validate_expr(expr: &SymExpr) -> Result<Sort, ValidationError> {
    let mut env = VarSortEnv::new();
    validate_expr_in_env(expr, &mut env)
}

/// Validate `expr` against an existing [`VarSortEnv`] using an explicit
/// [`ValidationBudget`], returning both its elaborated sort and DAG build metrics.
pub fn validate_expr_with_budget(
    expr: &SymExpr,
    env: &mut VarSortEnv,
    budget: ValidationBudget,
) -> Result<(Sort, DagBuildStats), ValidationError> {
    let mut dag = ValidatedDag::new();
    let root = dag.intern_validated(expr, env, budget)?;
    Ok((dag.sort(root).clone(), dag.stats()))
}

/// Validate `expr` against an existing [`VarSortEnv`], checking sort rules and
/// variable ID consistency across all visited nodes.
pub fn validate_expr_in_env(expr: &SymExpr, env: &mut VarSortEnv) -> Result<Sort, ValidationError> {
    let (sort, _) = validate_expr_with_budget(expr, env, ValidationBudget::default())?;
    Ok(sort)
}

/// Validate that `expr` elaborates to `Sort::BitVector(1)` in `env`.
pub fn validate_assertion_in_env(
    expr: &SymExpr,
    env: &mut VarSortEnv,
) -> Result<(), ValidationError> {
    let mut dag = ValidatedDag::new();
    let root = dag.intern_validated(expr, env, ValidationBudget::default())?;
    let sort = dag.sort(root);
    if *sort != Sort::BitVector(1) {
        return Err(ValidationError::InvalidAssertionSort { sort: sort.clone() });
    }
    Ok(())
}

/// Validate all assertions in one shared [`VarSortEnv`], sharing the hash-consed
/// [`ValidatedDag`] across all assertions in the query.
pub fn validate_assertions(assertions: &[SymExpr]) -> Result<ValidatedQuery, ValidationError> {
    let mut env = VarSortEnv::new();
    let mut dag = ValidatedDag::new();
    for assertion in assertions {
        let root = dag.intern_validated(assertion, &mut env, ValidationBudget::default())?;
        let sort = dag.sort(root);
        if *sort != Sort::BitVector(1) {
            return Err(ValidationError::InvalidAssertionSort { sort: sort.clone() });
        }
    }
    Ok(env.into_summary())
}

/// Validate registered `SymExpr::Var` declarations in `nodes` together with
/// `assertions` and `extra` assumptions in one shared [`VarSortEnv`] and
/// hash-consed [`ValidatedDag`].
pub fn validate_query_with_nodes(
    assertions: &[SymExpr],
    extra: &[SymExpr],
    nodes: &HashMap<SymNodeId, SymExpr>,
) -> Result<ValidatedQuery, ValidationError> {
    let mut env = VarSortEnv::new();
    let mut ids: Vec<SymNodeId> = nodes.keys().copied().collect();
    ids.sort_unstable();
    for reg_id in ids {
        if let Some(SymExpr::Var { id, sort, .. }) = nodes.get(&reg_id) {
            env.bind_var(*id, sort)?;
        }
    }
    let mut dag = ValidatedDag::new();
    for assertion in assertions {
        let root = dag.intern_validated(assertion, &mut env, ValidationBudget::default())?;
        let sort = dag.sort(root);
        if *sort != Sort::BitVector(1) {
            return Err(ValidationError::InvalidAssertionSort { sort: sort.clone() });
        }
    }
    for assumption in extra {
        let root = dag.intern_validated(assumption, &mut env, ValidationBudget::default())?;
        let sort = dag.sort(root);
        if *sort != Sort::BitVector(1) {
            return Err(ValidationError::InvalidAssertionSort { sort: sort.clone() });
        }
    }
    Ok(env.into_summary())
}

/// Verify that `expr` is well-formed in `env` and strictly within the pure
/// `1..=64`-bit bitvector subset (no floats, no arrays, no >64-bit BVs).
///
/// Algebraic shortcuts that erase an operand (`a - a => 0`, `a ^ a => 0`,
/// `0 & b => 0`, `a == a => 1`, `a != a => 0`) call this helper so malformed or
/// unmodeled operands are never simplified away before reaching a backend.
pub(crate) fn is_pure_supported_bv(expr: &SymExpr, env: &mut VarSortEnv) -> Result<u32, ()> {
    let mut dag = ValidatedDag::new();
    let root = dag
        .intern_with_mode(expr, env, ValidationBudget::default(), true)
        .map_err(|_| ())?;
    match dag.sort(root) {
        Sort::BitVector(bits) if (1..=64).contains(bits) => Ok(*bits),
        _ => Err(()),
    }
}

/// Helper for iterative array sort checks elsewhere in the crate: collects
/// unique `DagNodeId`s visited for `expr` inside `dag` if needed by callers.
#[allow(dead_code)]
fn _collect_unique_vars(dag: &ValidatedDag, root: DagNodeId, out: &mut HashSet<SymNodeId>) {
    let mut visited = HashSet::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if !visited.insert(id) {
            continue;
        }
        match &dag.node(id).op {
            DagOp::Const { .. } => {}
            DagOp::Var { id: vid, .. } => {
                out.insert(*vid);
            }
            DagOp::FNeg(a)
            | DagOp::FAbs(a)
            | DagOp::FSqrt(a)
            | DagOp::FIsNan(a)
            | DagOp::Extract { expr: a, .. } => stack.push(*a),
            DagOp::Add(a, b)
            | DagOp::Sub(a, b)
            | DagOp::Mul(a, b)
            | DagOp::Udiv(a, b)
            | DagOp::Urem(a, b)
            | DagOp::Sdiv(a, b)
            | DagOp::Srem(a, b)
            | DagOp::Smod(a, b)
            | DagOp::And(a, b)
            | DagOp::Or(a, b)
            | DagOp::Xor(a, b)
            | DagOp::Shl(a, b)
            | DagOp::Lshr(a, b)
            | DagOp::Ashr(a, b)
            | DagOp::Eq(a, b)
            | DagOp::Neq(a, b)
            | DagOp::Ult(a, b)
            | DagOp::Ule(a, b)
            | DagOp::Slt(a, b)
            | DagOp::Sle(a, b)
            | DagOp::Sgt(a, b)
            | DagOp::FAdd(a, b)
            | DagOp::FSub(a, b)
            | DagOp::FMul(a, b)
            | DagOp::FDiv(a, b)
            | DagOp::FEq(a, b)
            | DagOp::FNeq(a, b)
            | DagOp::FLt(a, b)
            | DagOp::FLe(a, b)
            | DagOp::Concat(a, b)
            | DagOp::ArraySelect { array: a, index: b } => {
                stack.push(*a);
                stack.push(*b);
            }
            DagOp::Ite {
                cond: a,
                t: b,
                f: c,
            }
            | DagOp::ArrayStore {
                array: a,
                index: b,
                value: c,
            } => {
                stack.push(*a);
                stack.push(*b);
                stack.push(*c);
            }
        }
    }
}
