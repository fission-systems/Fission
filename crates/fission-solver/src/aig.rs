use crate::ast::{Sort, SymExpr, SymNodeId};
use crate::validate::{
    sort_bit_width, validate_expr, DagNodeId, DagOp, ValidationError, MAX_EXPR_DEPTH,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::time::Instant;

/// Default maximum AST visits allowed during [`AigManager::lower_expr`].
pub const DEFAULT_MAX_LOWERING_EXPR_VISITS: usize = 1_000_000;

/// Default maximum unique hash-consed DAG nodes interned by an [`AigManager`].
pub const DEFAULT_MAX_LOWERING_UNIQUE_NODES: usize = 250_000;

/// Default maximum AIG nodes (`Var` + `And`) materialized by an [`AigManager`].
pub const DEFAULT_MAX_LOWERING_AIG_NODES: usize = 2_000_000;

/// Maximum leaf terms tracked in one [`ModularAffineForm`].
const MAX_AFFINE_TERMS: usize = 32;

/// Maximum AST/DAG operations inspected while assembling a [`ModularAffineForm`].
const MAX_AFFINE_OPS: usize = 256;

/// Which pre-SAT lowering resource bound was reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoweringBudgetKind {
    Depth { limit: usize },
    ExprVisits { limit: usize },
    UniqueNodes { limit: usize },
    AigNodes { limit: usize },
}

impl fmt::Display for LoweringBudgetKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Depth { limit } => write!(f, "expression nesting depth limit ({limit})"),
            Self::ExprVisits { limit } => write!(f, "expression visit limit ({limit})"),
            Self::UniqueNodes { limit } => write!(f, "unique DAG node limit ({limit})"),
            Self::AigNodes { limit } => write!(f, "AIG gate/variable limit ({limit})"),
        }
    }
}

/// Explicit pre-SAT traversal and circuit-size bounds for [`AigManager`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoweringBudget {
    pub max_depth: usize,
    pub max_expr_visits: usize,
    pub max_unique_nodes: usize,
    pub max_aig_nodes: usize,
}

impl Default for LoweringBudget {
    fn default() -> Self {
        Self {
            max_depth: MAX_EXPR_DEPTH,
            max_expr_visits: DEFAULT_MAX_LOWERING_EXPR_VISITS,
            max_unique_nodes: DEFAULT_MAX_LOWERING_UNIQUE_NODES,
            max_aig_nodes: DEFAULT_MAX_LOWERING_AIG_NODES,
        }
    }
}

impl LoweringBudget {
    /// Configuration with maximal limits for callers that opt out of bounding.
    #[must_use]
    pub const fn unlimited() -> Self {
        Self {
            max_depth: usize::MAX,
            max_expr_visits: usize::MAX,
            max_unique_nodes: usize::MAX,
            max_aig_nodes: usize::MAX,
        }
    }
}

/// Pre-SAT symbolic lowering telemetry accumulated on an [`AigManager`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LoweringStats {
    /// Total AST node occurrences visited before DAG deduplication.
    pub tree_occurrences: u64,
    /// Distinct hash-consed DAG subexpressions interned across calls.
    pub unique_subexpressions: u64,
    /// Number of times DAG interning reused an existing node ID.
    pub dag_cache_hits: u64,
    /// Number of times bit-blasting reused already-lowered DAG node bits.
    pub lowered_cache_hits: u64,
    /// Unique nodes whose sort/width was computed and memoized.
    pub width_inference_visits: u64,
    /// Distinct AIG nodes (`Var` + `And`) pushed into `self.nodes`.
    pub aig_nodes_created: u64,
    /// Structural-hashing hits inside `add_and`.
    pub strash_hits: u64,
    /// Number of nodes lowered via modular affine normalization.
    pub modular_affine_rewrites: u64,
    /// First lowering budget exhausted, if any.
    pub budget_exceeded: Option<LoweringBudgetKind>,
    /// Cumulative pre-SAT lowering wall time in nanoseconds.
    pub lowering_time_ns: u64,
}

/// Canonical width-preserving modular affine normal form:
/// `(constant + sum_i (coeff_i * term_i)) mod 2^width`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModularAffineForm {
    pub width: u32,
    pub constant: u64,
    pub terms: BTreeMap<DagNodeId, u64>,
}

#[derive(Debug, Clone)]
struct AigDagNode {
    op: DagOp,
    sort: Sort,
    is_pure_bv: bool,
    height: usize,
}

#[derive(Debug, Clone)]
struct LoweredDagEntry {
    bits: Vec<AigLit>,
    unsupported: Vec<String>,
}

#[derive(Debug, Clone)]
enum ExtractedAffine {
    Constant(u64),
    Term {
        form: ModularAffineForm,
        cancelled_vars: Vec<(SymNodeId, Sort)>,
        simplified: bool,
    },
}

/// An ID representing a node in the AIG.
/// The LSB is the sign bit (1 = inverted, 0 = non-inverted).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AigLit(pub u32);

impl AigLit {
    pub const FALSE: AigLit = AigLit(0);
    pub const TRUE: AigLit = AigLit(1);

    pub fn new(index: u32, inverted: bool) -> Self {
        Self((index << 1) | (inverted as u32))
    }

    pub fn index(self) -> u32 {
        self.0 >> 1
    }

    pub fn is_inverted(self) -> bool {
        (self.0 & 1) != 0
    }

    pub fn not(self) -> Self {
        Self(self.0 ^ 1)
    }
}

/// A node in the And-Inverter Graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AigNode {
    /// External variable / input
    Var(u32),
    /// AND gate of two literals
    And(AigLit, AigLit),
}

/// Which way a shift goes, and what fills the bits it vacates.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ShiftKind {
    Left,
    LogicalRight,
    ArithmeticRight,
}

/// The three signed division results, which share one unsigned divider.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SignedDivision {
    Quotient,
    Remainder,
    Modulus,
}

/// An And-Inverter Graph manager for converting ASTs.
pub struct AigManager {
    nodes: Vec<AigNode>,
    /// Structural hashing to deduplicate AND nodes
    strash: HashMap<(AigLit, AigLit), u32>,
    /// Maps AST Node ID to its vector of AIG literals (one per bit)
    var_map: HashMap<u32, Vec<AigLit>>,
    /// Sort recorded for each AST variable ID to catch conflicting ID/sort aliasing.
    var_sorts: HashMap<u32, Sort>,
    /// Maps ArraySelect nodes to their vector of AIG literals
    array_select_map: HashMap<SymExpr, Vec<AigLit>>,
    /// Query/manager-local hash-consed symbolic DAG nodes (never shared across solvers).
    dag_nodes: Vec<AigDagNode>,
    dag_intern: HashMap<DagOp, DagNodeId>,
    /// Memoized AIG bit-vectors + unsupported notes keyed by [`DagNodeId`].
    dag_lowered: HashMap<DagNodeId, LoweredDagEntry>,
    /// Explicit resource budgets for traversal, interning, and AIG construction.
    budget: LoweringBudget,
    /// Cumulative instrumentation metrics for pre-SAT lowering.
    stats: LoweringStats,
    /// Budget exceeded during the active `lower_expr` call.
    call_budget_exceeded: Option<LoweringBudgetKind>,
    /// Whether width-preserving modular affine normalization is active.
    modular_affine_enabled: bool,
    pub last_cnf_node: usize,
    /// When true, [`Solver`](crate::solver::Solver) supplies `ArrayTheory` axioms for
    /// `ArraySelect` over `Var` / `ArrayStore` / `Ite` chains.
    array_axioms_enabled: bool,
    /// Operations met during lowering that have no circuit here. Any entry
    /// makes an answer about this problem untrustworthy, and the solver
    /// reports `Unknown` rather than a verdict.
    unsupported: Vec<String>,
}

impl Default for AigManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AigManager {
    pub fn new() -> Self {
        Self {
            nodes: vec![], // Index 0 is reserved (constant 0)
            strash: HashMap::new(),
            var_map: HashMap::new(),
            var_sorts: HashMap::new(),
            array_select_map: HashMap::new(),
            dag_nodes: Vec::new(),
            dag_intern: HashMap::new(),
            dag_lowered: HashMap::new(),
            budget: LoweringBudget::default(),
            stats: LoweringStats::default(),
            call_budget_exceeded: None,
            modular_affine_enabled: true,
            last_cnf_node: 0,
            array_axioms_enabled: false,
            unsupported: Vec::new(),
        }
    }

    pub fn set_array_axioms_enabled(&mut self, enabled: bool) {
        if self.array_axioms_enabled != enabled {
            self.array_axioms_enabled = enabled;
            // Cached ArraySelect unsupported notes depend on whether axioms are attached.
            self.dag_lowered.clear();
        }
    }

    /// Configure explicit pre-SAT lowering budgets for subsequent operations.
    pub fn set_lowering_budget(&mut self, budget: LoweringBudget) {
        self.budget = budget;
        self.call_budget_exceeded = None;
    }

    #[must_use]
    pub fn lowering_budget(&self) -> LoweringBudget {
        self.budget
    }

    #[must_use]
    pub fn lowering_stats(&self) -> LoweringStats {
        self.stats
    }

    pub fn reset_lowering_stats(&mut self) {
        self.stats = LoweringStats::default();
    }

    #[must_use]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    #[must_use]
    pub fn dag_node_count(&self) -> usize {
        self.dag_nodes.len()
    }

    /// Enable or disable the width-preserving modular affine normalization pilot.
    pub fn set_modular_affine_enabled(&mut self, enabled: bool) {
        if self.modular_affine_enabled != enabled {
            self.modular_affine_enabled = enabled;
            self.dag_lowered.clear();
        }
    }

    #[must_use]
    pub fn modular_affine_enabled(&self) -> bool {
        self.modular_affine_enabled
    }

    fn record_budget_exceeded(&mut self, kind: LoweringBudgetKind) {
        if self.call_budget_exceeded.is_none() {
            self.call_budget_exceeded = Some(kind);
        }
        if self.stats.budget_exceeded.is_none() {
            self.stats.budget_exceeded = Some(kind);
        }
        self.note_unsupported(format!("lowering budget exceeded: {kind}"));
    }

    fn allocate_raw_var(&mut self) -> Option<AigLit> {
        if self.call_budget_exceeded.is_some() {
            return None;
        }
        if self.nodes.len() >= self.budget.max_aig_nodes {
            self.record_budget_exceeded(LoweringBudgetKind::AigNodes {
                limit: self.budget.max_aig_nodes,
            });
            return None;
        }
        let idx = self.nodes.len() as u32 + 1;
        self.nodes.push(AigNode::Var(idx));
        self.stats.aig_nodes_created += 1;
        Some(AigLit::new(idx, false))
    }

    /// Why `ArrayTheory`'s axioms do not describe a read of `array`, if they
    /// do not (iterative over the DAG so deep `ArrayStore`/`Ite` trees cannot overflow).
    ///
    /// A store whose value is wider than the array's element is one such
    /// reason. The axiom for it is `select(store(a, i, v), i) == v`, and
    /// equality zero-extends the narrower side -- the element -- so the
    /// axiom asserts that `v`'s high bits are zero. With `v == 0x1234`
    /// stored into a byte array that made the whole problem UNSAT, though no
    /// assertion said anything of the kind. SMT-LIB has no such store to
    /// take a meaning from, so the read is unsupported rather than given one.
    fn array_axiom_gap_dag(&self, array_id: DagNodeId) -> Option<String> {
        let mut visited = HashSet::new();
        let mut stack = vec![array_id];
        while let Some(id) = stack.pop() {
            if !visited.insert(id) {
                continue;
            }
            let node = &self.dag_nodes[id.0 as usize];
            match &node.op {
                // `ArrayTheory` skips an array whose index or element is not a
                // 1-to-64-bit scalar, and a read it wrote no axioms for is a
                // free value: two reads at the same index could differ.
                DagOp::Var { sort, .. } => {
                    let supported = matches!(
                        sort,
                        Sort::Array { domain, range }
                            if matches!(domain.as_ref(), Sort::BitVector(w) | Sort::Float(w) if (1..=64).contains(w))
                                && matches!(range.as_ref(), Sort::BitVector(w) | Sort::Float(w) if (1..=64).contains(w))
                    );
                    if !supported {
                        return Some(format!("ArraySelect over an array of sort {sort:?}"));
                    }
                }
                DagOp::ArrayStore {
                    array: inner,
                    value,
                    ..
                } => {
                    let element = sort_bit_width(&node.sort);
                    let stored = sort_bit_width(&self.dag_nodes[value.0 as usize].sort);
                    if stored > element {
                        return Some(format!(
                            "ArrayStore of a {stored}-bit value into {element}-bit elements"
                        ));
                    }
                    stack.push(*inner);
                }
                DagOp::Ite { t, f, .. } => {
                    stack.push(*f);
                    stack.push(*t);
                }
                _ => return Some("ArraySelect over a term without array axioms".to_string()),
            }
        }
        None
    }

    /// Create a new variable of `size` bits.
    pub fn add_var(&mut self, ast_id: u32, size: u32) -> Vec<AigLit> {
        let mut bits = Vec::with_capacity(size as usize);
        for _ in 0..size {
            if let Some(lit) = self.allocate_raw_var() {
                bits.push(lit);
            } else {
                bits.push(AigLit::FALSE);
            }
        }
        if self.call_budget_exceeded.is_none() {
            self.var_map.insert(ast_id, bits.clone());
        }
        bits
    }

    /// Return the AIG literal bits for a previously registered AST variable, if known.
    pub fn get_var_bits(&self, ast_id: u32) -> Option<&Vec<AigLit>> {
        self.var_map.get(&ast_id)
    }

    pub fn get_array_select_bits(&self, expr: &SymExpr) -> Option<&Vec<AigLit>> {
        self.array_select_map.get(expr)
    }

    /// Add an AND gate, returning the literal. Applies structural hashing.
    pub fn add_and(&mut self, mut a: AigLit, mut b: AigLit) -> AigLit {
        if a == AigLit::FALSE || b == AigLit::FALSE {
            return AigLit::FALSE;
        }
        if a == AigLit::TRUE {
            return b;
        }
        if b == AigLit::TRUE {
            return a;
        }
        if a == b {
            return a;
        }
        if a == b.not() {
            return AigLit::FALSE;
        }
        if self.call_budget_exceeded.is_some() {
            return AigLit::FALSE;
        }

        // Canonicalize ordering
        if a.0 > b.0 {
            std::mem::swap(&mut a, &mut b);
        }

        let key = (a, b);
        if let Some(&idx) = self.strash.get(&key) {
            self.stats.strash_hits += 1;
            return AigLit::new(idx, false);
        }
        if self.nodes.len() >= self.budget.max_aig_nodes {
            self.record_budget_exceeded(LoweringBudgetKind::AigNodes {
                limit: self.budget.max_aig_nodes,
            });
            return AigLit::FALSE;
        }

        let idx = self.nodes.len() as u32 + 1;
        self.nodes.push(AigNode::And(a, b));
        self.stats.aig_nodes_created += 1;
        self.strash.insert(key, idx);
        AigLit::new(idx, false)
    }

    /// Add an XOR gate using AND and NOT.
    pub fn add_xor(&mut self, a: AigLit, b: AigLit) -> AigLit {
        // a XOR b = (a AND !b) OR (!a AND b)
        // = !(!(a AND !b) AND !(!a AND b))
        let t1 = self.add_and(a, b.not());
        let t2 = self.add_and(a.not(), b);
        self.add_and(t1.not(), t2.not()).not()
    }

    pub fn add_or(&mut self, a: AigLit, b: AigLit) -> AigLit {
        // a OR b = !(!a AND !b)
        self.add_and(a.not(), b.not()).not()
    }

    pub fn add_eq(&mut self, a_bits: &[AigLit], b_bits: &[AigLit]) -> AigLit {
        let len = std::cmp::max(a_bits.len(), b_bits.len());
        let mut eq = AigLit::TRUE;
        for i in 0..len {
            if self.call_budget_exceeded.is_some() {
                return AigLit::FALSE;
            }
            let a = a_bits.get(i).copied().unwrap_or(AigLit::FALSE);
            let b = b_bits.get(i).copied().unwrap_or(AigLit::FALSE);
            let xor = self.add_xor(a, b);
            let xnor = xor.not();
            eq = self.add_and(eq, xnor);
        }
        eq
    }

    pub fn add_neq(&mut self, a_bits: &[AigLit], b_bits: &[AigLit]) -> AigLit {
        self.add_eq(a_bits, b_bits).not()
    }

    /// What lowering met and could not encode, in the order it was met.
    pub fn unsupported(&self) -> &[String] {
        &self.unsupported
    }

    pub(crate) fn take_unsupported(&mut self) -> Vec<String> {
        std::mem::take(&mut self.unsupported)
    }

    /// Drains any unsupported notes recorded at or beyond `prior_len`,
    /// preserving earlier persistent notes (for example from permanent
    /// assertions) across a query-local transaction.
    pub(crate) fn drain_unsupported_since(&mut self, prior_len: usize) -> Vec<String> {
        if prior_len >= self.unsupported.len() {
            Vec::new()
        } else {
            self.unsupported.drain(prior_len..).collect()
        }
    }

    pub fn note_unsupported(&mut self, what: String) {
        if !self.unsupported.contains(&what) {
            self.unsupported.push(what);
        }
    }

    /// `n` fresh, unconstrained bits.
    fn fresh_bits(&mut self, n: usize) -> Vec<AigLit> {
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            out.push(self.allocate_raw_var().unwrap_or(AigLit::FALSE));
        }
        out
    }

    /// `c ? t : e` for one bit.
    pub fn add_ite(&mut self, c: AigLit, t: AigLit, e: AigLit) -> AigLit {
        let then_side = self.add_and(c, t);
        let else_side = self.add_and(c.not(), e);
        self.add_or(then_side, else_side)
    }

    /// `a - b` and whether it did *not* borrow -- that is, whether `a >= b`.
    fn add_subtracter(&mut self, a: &[AigLit], b: &[AigLit]) -> (Vec<AigLit>, AigLit) {
        let len = a.len().max(b.len());
        let mut diff = Vec::with_capacity(len);
        let mut carry = AigLit::TRUE;
        for i in 0..len {
            if self.call_budget_exceeded.is_some() {
                diff.resize(len, AigLit::FALSE);
                return (diff, AigLit::FALSE);
            }
            let x = a.get(i).copied().unwrap_or(AigLit::FALSE);
            let y = b.get(i).copied().unwrap_or(AigLit::FALSE).not();
            let (sum, next) = self.add_full_adder(x, y, carry);
            diff.push(sum);
            carry = next;
        }
        (diff, carry)
    }

    /// `a * b` modulo `2^width`, shift-and-add.
    fn add_multiplier(&mut self, a: &[AigLit], b: &[AigLit]) -> Vec<AigLit> {
        let width = a.len().max(b.len());
        let bit = |bits: &[AigLit], i: usize| bits.get(i).copied().unwrap_or(AigLit::FALSE);
        let mut product = vec![AigLit::FALSE; width];
        for i in 0..width {
            if self.call_budget_exceeded.is_some() {
                break;
            }
            let multiplier_bit = bit(b, i);
            let mut partial = vec![AigLit::FALSE; width];
            for j in 0..width - i {
                if self.call_budget_exceeded.is_some() {
                    break;
                }
                partial[i + j] = self.add_and(bit(a, j), multiplier_bit);
            }
            product = self.add_ripple_carry_adder(&product, &partial, AigLit::FALSE);
        }
        product
    }

    /// Unsigned `a / b` and `a % b`, by restoring division -- the circuit
    /// Z3 builds in `bit_blaster_tpl_def.h`'s `mk_udiv_urem` (MIT License,
    /// Copyright (c) Microsoft Corporation).
    ///
    /// Division by zero needs no special case and gets the SMT-LIB answer:
    /// every subtraction of zero succeeds, so the quotient is all ones and the
    /// remainder is the dividend.
    fn add_udiv_urem(&mut self, a: &[AigLit], b: &[AigLit]) -> (Vec<AigLit>, Vec<AigLit>) {
        let width = a.len().max(b.len());
        let pad = |bits: &[AigLit]| -> Vec<AigLit> {
            (0..width)
                .map(|i| bits.get(i).copied().unwrap_or(AigLit::FALSE))
                .collect()
        };
        let (a, b) = (pad(a), pad(b));
        let mut partial = vec![AigLit::FALSE; width];
        partial[0] = a[width - 1];
        let mut quotient = vec![AigLit::FALSE; width];
        for step in 0..width {
            if self.call_budget_exceeded.is_some() {
                break;
            }
            let (difference, fits) = self.add_subtracter(&partial, &b);
            quotient[width - 1 - step] = fits;
            if step < width - 1 {
                // Keep the difference if it fitted, then bring the next
                // dividend bit down. The top bit that falls off is always
                // zero: a partial remainder before the last step holds at
                // most `step + 1` bits.
                let mut next = vec![AigLit::FALSE; width];
                for j in (1..width).rev() {
                    if self.call_budget_exceeded.is_some() {
                        break;
                    }
                    next[j] = self.add_ite(fits, difference[j - 1], partial[j - 1]);
                }
                next[0] = a[width - step - 2];
                partial = next;
            } else {
                for j in 0..width {
                    if self.call_budget_exceeded.is_some() {
                        break;
                    }
                    partial[j] = self.add_ite(fits, difference[j], partial[j]);
                }
            }
        }
        (quotient, partial)
    }

    /// Two's-complement negation: invert and add one.
    fn add_neg(&mut self, a: &[AigLit]) -> Vec<AigLit> {
        let inverted: Vec<AigLit> = a.iter().map(|lit| lit.not()).collect();
        self.add_ripple_carry_adder(&inverted, &[], AigLit::TRUE)
    }

    /// `c ? t : e` bit by bit, padding the shorter side with zeros.
    fn mux_bits(&mut self, c: AigLit, t: &[AigLit], e: &[AigLit]) -> Vec<AigLit> {
        let width = t.len().max(e.len());
        (0..width)
            .map(|i| {
                let then_bit = t.get(i).copied().unwrap_or(AigLit::FALSE);
                let else_bit = e.get(i).copied().unwrap_or(AigLit::FALSE);
                self.add_ite(c, then_bit, else_bit)
            })
            .collect()
    }

    /// A shift by a symbolic amount: a barrel shifter, one stage per amount
    /// bit that can still move something, then saturation for everything
    /// larger. SMT-LIB: shifting by the width or more gives zero, or all sign
    /// bits for an arithmetic right shift.
    ///
    /// This used to be the identity -- `x << y` lowered as `x` -- so
    /// `x << 1 == 2` for `x = 1` came back UNSAT.
    fn add_shift(&mut self, a: &[AigLit], amount: &[AigLit], kind: ShiftKind) -> Vec<AigLit> {
        let width = a.len();
        if width == 0 {
            return Vec::new();
        }
        let fill = if kind == ShiftKind::ArithmeticRight {
            a[width - 1]
        } else {
            AigLit::FALSE
        };
        let mut out = a.to_vec();
        let mut stage = 0usize;
        while stage < amount.len() && stage < usize::BITS as usize - 1 && (1usize << stage) < width
        {
            if self.call_budget_exceeded.is_some() {
                break;
            }
            let step = 1usize << stage;
            let shifted: Vec<AigLit> = (0..width)
                .map(|j| match kind {
                    ShiftKind::Left if j >= step => out[j - step],
                    ShiftKind::Left => AigLit::FALSE,
                    _ if j + step < width => out[j + step],
                    _ => fill,
                })
                .collect();
            out = self.mux_bits(amount[stage], &shifted, &out);
            stage += 1;
        }
        // Any higher amount bit set means a shift of at least the width.
        let mut too_far = AigLit::FALSE;
        for &bit in &amount[stage..] {
            if self.call_budget_exceeded.is_some() {
                break;
            }
            too_far = self.add_or(too_far, bit);
        }
        let saturated = vec![fill; width];
        self.mux_bits(too_far, &saturated, &out)
    }

    /// A shift by a constant, without building a shifter -- and without
    /// allocating the shift amount: this used to start from
    /// `vec![FALSE; shift]`, so a large constant allocated that many bits.
    fn shift_by_constant(a: &[AigLit], amount: u64, kind: ShiftKind) -> Vec<AigLit> {
        let width = a.len();
        let fill = match (kind, a.last()) {
            (ShiftKind::ArithmeticRight, Some(&msb)) => msb,
            _ => AigLit::FALSE,
        };
        let shift = usize::try_from(amount).unwrap_or(usize::MAX);
        (0..width)
            .map(|j| match kind {
                ShiftKind::Left => j.checked_sub(shift).map_or(AigLit::FALSE, |from| a[from]),
                _ => j
                    .checked_add(shift)
                    .filter(|&from| from < width)
                    .map_or(fill, |from| a[from]),
            })
            .collect()
    }

    /// Signed quotient, remainder or modulus, straight from the SMT-LIB
    /// definitions: divide the magnitudes once, then fix the sign.
    ///
    /// Z3's `mk_sdiv_srem_smod` builds four unsigned dividers, one per sign
    /// combination, and chooses between them; one divider over absolute
    /// values computes the same functions. Division by zero needs no special
    /// case: it inherits `bvudiv`/`bvurem`'s answers through the formulas.
    fn add_signed_division(
        &mut self,
        a: &[AigLit],
        b: &[AigLit],
        kind: SignedDivision,
    ) -> Vec<AigLit> {
        let width = a.len().max(b.len());
        if width == 0 {
            return Vec::new();
        }
        let pad = |bits: &[AigLit]| -> Vec<AigLit> {
            (0..width)
                .map(|i| bits.get(i).copied().unwrap_or(AigLit::FALSE))
                .collect()
        };
        let (a, b) = (pad(a), pad(b));
        let (a_negative, b_negative) = (a[width - 1], b[width - 1]);

        let neg_a = self.add_neg(&a);
        let neg_b = self.add_neg(&b);
        let abs_a = self.mux_bits(a_negative, &neg_a, &a);
        let abs_b = self.mux_bits(b_negative, &neg_b, &b);
        let (quotient, remainder) = self.add_udiv_urem(&abs_a, &abs_b);

        match kind {
            // Negative exactly when the signs differ.
            SignedDivision::Quotient => {
                let signs_differ = self.add_xor(a_negative, b_negative);
                let negated = self.add_neg(&quotient);
                self.mux_bits(signs_differ, &negated, &quotient)
            }
            // The dividend's sign.
            SignedDivision::Remainder => {
                let negated = self.add_neg(&remainder);
                self.mux_bits(a_negative, &negated, &remainder)
            }
            // The divisor's sign -- and zero stays zero, which is the case a
            // sign fix-up applied unconditionally gets wrong: `-u + t` would
            // turn a zero remainder into `t`.
            SignedDivision::Modulus => {
                let negated = self.add_neg(&remainder);
                let negated_plus_b = self.add_ripple_carry_adder(&negated, &b, AigLit::FALSE);
                let plus_b = self.add_ripple_carry_adder(&remainder, &b, AigLit::FALSE);
                // (a<0, b<0) -> -u ; (a<0, b>=0) -> -u + b
                let when_a_negative = self.mux_bits(b_negative, &negated, &negated_plus_b);
                // (a>=0, b<0) -> u + b ; (a>=0, b>=0) -> u
                let when_a_positive = self.mux_bits(b_negative, &plus_b, &remainder);
                let signed = self.mux_bits(a_negative, &when_a_negative, &when_a_positive);
                let zero = vec![AigLit::FALSE; width];
                let remainder_is_zero = self.add_eq(&remainder, &zero);
                self.mux_bits(remainder_is_zero, &zero, &signed)
            }
        }
    }

    /// `a < b`, unsigned: the borrow out of `a - b`, as a carry chain.
    ///
    /// Both operands at the wider width. This used to size the chain by `a`
    /// alone, so a wider `b` had its high bits ignored.
    fn add_ult(&mut self, a: &[AigLit], b: &[AigLit]) -> AigLit {
        let width = a.len().max(b.len());
        let mut carry = AigLit::TRUE;
        for i in 0..width {
            if self.call_budget_exceeded.is_some() {
                return AigLit::FALSE;
            }
            let x = a.get(i).copied().unwrap_or(AigLit::FALSE);
            let y = b.get(i).copied().unwrap_or(AigLit::FALSE).not();
            let x_xor_y = self.add_xor(x, y);
            let both = self.add_and(x, y);
            let propagated = self.add_and(x_xor_y, carry);
            carry = self.add_or(both, propagated);
        }
        // carry out set: no borrow, a >= b.
        carry.not()
    }

    /// `a < b`, signed: flip both sign bits, then compare unsigned.
    ///
    /// On the bits themselves, with no constant involved. This used to XOR
    /// with the constant `1 << (size * 8 - 1)` -- a size read as *bytes*.
    /// Called with a size in bits, as fission-dir and every test do, a 6-bit
    /// operand got the mask `1 << 47`, which truncates to zero: the flip did
    /// nothing, and every signed comparison was silently unsigned. `27 <s 62`
    /// (62 being -2 in six bits) came back SAT. At 64 bits the mask was
    /// `1 << 511`, which overflows.
    fn add_slt(&mut self, a: &[AigLit], b: &[AigLit]) -> AigLit {
        let width = a.len().max(b.len());
        if width == 0 {
            return AigLit::FALSE;
        }
        let pad = |bits: &[AigLit]| -> Vec<AigLit> {
            (0..width)
                .map(|i| bits.get(i).copied().unwrap_or(AigLit::FALSE))
                .collect()
        };
        let (mut a, mut b) = (pad(a), pad(b));
        a[width - 1] = a[width - 1].not();
        b[width - 1] = b[width - 1].not();
        self.add_ult(&a, &b)
    }

    pub fn add_full_adder(&mut self, a: AigLit, b: AigLit, cin: AigLit) -> (AigLit, AigLit) {
        // sum = a ^ b ^ cin
        let a_xor_b = self.add_xor(a, b);
        let sum = self.add_xor(a_xor_b, cin);
        // cout = (a & b) | (cin & (a ^ b))
        let a_and_b = self.add_and(a, b);
        let cin_and_axorb = self.add_and(cin, a_xor_b);
        let cout = self.add_or(a_and_b, cin_and_axorb);
        (sum, cout)
    }

    pub fn add_ripple_carry_adder(
        &mut self,
        a_bits: &[AigLit],
        b_bits: &[AigLit],
        cin: AigLit,
    ) -> Vec<AigLit> {
        let len = std::cmp::max(a_bits.len(), b_bits.len());
        let mut sum_bits = Vec::with_capacity(len);
        let mut carry = cin;
        for i in 0..len {
            if self.call_budget_exceeded.is_some() {
                sum_bits.resize(len, AigLit::FALSE);
                return sum_bits;
            }
            let a = a_bits.get(i).copied().unwrap_or(AigLit::FALSE);
            let b = b_bits.get(i).copied().unwrap_or(AigLit::FALSE);
            let (sum, next_carry) = self.add_full_adder(a, b, carry);
            sum_bits.push(sum);
            carry = next_carry;
        }
        sum_bits
    }

    /// Lower a `SymExpr` into a vector of `AigLit`s (one per bit, LSB first)
    /// using iterative DAG interning, memoized width/sort inference, explicit
    /// resource budgets, and optional width-preserving modular affine
    /// normalization.
    pub fn lower_expr(&mut self, expr: &SymExpr) -> Vec<AigLit> {
        let started = Instant::now();
        self.call_budget_exceeded = None;

        let mut select_exprs: HashMap<DagNodeId, &SymExpr> = HashMap::new();
        let root_id = match self.intern_expr_iterative(expr, &mut select_exprs) {
            Ok(id) => id,
            Err(kind) => {
                self.record_budget_exceeded(kind);
                self.stats.lowering_time_ns = self
                    .stats
                    .lowering_time_ns
                    .saturating_add(started.elapsed().as_nanos() as u64);
                let safe_width = match expr {
                    SymExpr::Const { size, .. } => (*size).clamp(1, 64) as usize,
                    SymExpr::Var { sort, .. } => sort_bit_width(sort).clamp(1, 64) as usize,
                    _ => 1,
                };
                return vec![AigLit::FALSE; safe_width];
            }
        };

        let result = self.lower_dag_iterative(root_id, &select_exprs);
        self.stats.lowering_time_ns = self
            .stats
            .lowering_time_ns
            .saturating_add(started.elapsed().as_nanos() as u64);
        result
    }

    fn push_dag_node(
        &mut self,
        op: DagOp,
        sort: Sort,
        is_pure_bv: bool,
        height: usize,
    ) -> Result<DagNodeId, LoweringBudgetKind> {
        if let Some(&id) = self.dag_intern.get(&op) {
            self.stats.dag_cache_hits += 1;
            return Ok(id);
        }
        if self.dag_nodes.len() >= self.budget.max_unique_nodes {
            return Err(LoweringBudgetKind::UniqueNodes {
                limit: self.budget.max_unique_nodes,
            });
        }
        let id = DagNodeId(u32::try_from(self.dag_nodes.len()).unwrap_or(u32::MAX));
        self.dag_nodes.push(AigDagNode {
            op: op.clone(),
            sort,
            is_pure_bv,
            height,
        });
        self.dag_intern.insert(op, id);
        self.stats.unique_subexpressions += 1;
        self.stats.width_inference_visits += 1;
        Ok(id)
    }

    fn intern_expr_iterative<'a>(
        &mut self,
        root: &'a SymExpr,
        select_exprs: &mut HashMap<DagNodeId, &'a SymExpr>,
    ) -> Result<DagNodeId, LoweringBudgetKind> {
        struct Frame<'a> {
            expr: &'a SymExpr,
            depth: usize,
            next_child: u8,
            children: [Option<DagNodeId>; 3],
        }

        let mut visits = 0usize;
        let mut ptr_cache: HashMap<*const SymExpr, DagNodeId> = HashMap::new();
        let mut local_var_sorts: HashMap<u32, Sort> = HashMap::new();
        let mut stack = vec![Frame {
            expr: root,
            depth: 0,
            next_child: 0,
            children: [None, None, None],
        }];

        while let Some(frame) = stack.last_mut() {
            let cur = frame.expr;
            let depth = frame.depth;

            if frame.next_child == 0 {
                if depth > self.budget.max_depth {
                    return Err(LoweringBudgetKind::Depth {
                        limit: self.budget.max_depth,
                    });
                }
                visits += 1;
                self.stats.tree_occurrences += 1;
                if visits > self.budget.max_expr_visits {
                    return Err(LoweringBudgetKind::ExprVisits {
                        limit: self.budget.max_expr_visits,
                    });
                }

                let ptr = std::ptr::from_ref(cur);
                if let Some(&cached_id) = ptr_cache.get(&ptr) {
                    let deepest = depth.saturating_add(self.dag_nodes[cached_id.0 as usize].height);
                    if deepest > self.budget.max_depth {
                        return Err(LoweringBudgetKind::Depth {
                            limit: self.budget.max_depth,
                        });
                    }
                    self.stats.dag_cache_hits += 1;
                    stack.pop();
                    if let Some(parent) = stack.last_mut() {
                        let slot = (parent.next_child - 1) as usize;
                        parent.children[slot] = Some(cached_id);
                    } else {
                        return Ok(cached_id);
                    }
                    continue;
                }

                match cur {
                    SymExpr::Const { val, size } => {
                        let pure = (1..=64).contains(size);
                        let id = self.push_dag_node(
                            DagOp::Const {
                                val: *val,
                                size: *size,
                            },
                            Sort::BitVector(*size),
                            pure,
                            0,
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
                        let conflicts_global = self
                            .var_sorts
                            .get(var_id)
                            .is_some_and(|existing| existing != sort);
                        let conflicts_local = local_var_sorts
                            .get(var_id)
                            .is_some_and(|existing| existing != sort);
                        if !conflicts_local {
                            local_var_sorts
                                .entry(*var_id)
                                .or_insert_with(|| sort.clone());
                        }
                        let pure = !conflicts_global
                            && !conflicts_local
                            && matches!(sort, Sort::BitVector(bits) if (1..=64).contains(bits));
                        let node_id = self.push_dag_node(
                            DagOp::Var {
                                id: *var_id,
                                name: name.clone(),
                                sort: sort.clone(),
                            },
                            sort.clone(),
                            pure,
                            0,
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
                    _ => {}
                }
            }

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
                | (SymExpr::ArrayStore { index: b, .. }, 1) => Some(b.as_ref()),
                (SymExpr::Ite { f: c, .. }, 2) | (SymExpr::ArrayStore { value: c, .. }, 2) => {
                    Some(c.as_ref())
                }
                _ => None,
            };

            if let Some(child_expr) = next_child_expr {
                frame.next_child += 1;
                stack.push(Frame {
                    expr: child_expr,
                    depth: depth + 1,
                    next_child: 0,
                    children: [None, None, None],
                });
                continue;
            }

            let done = stack.pop().expect("active frame");
            let c0 = done.children[0];
            let c1 = done.children[1];
            let c2 = done.children[2];
            let (op, sort, pure, height) = self.build_dag_node_fields(done.expr, c0, c1, c2);
            let node_id = self.push_dag_node(op, sort, pure, height)?;
            if matches!(done.expr, SymExpr::ArraySelect { .. }) {
                select_exprs.entry(node_id).or_insert(done.expr);
            }
            ptr_cache.insert(std::ptr::from_ref(done.expr), node_id);
            if let Some(parent) = stack.last_mut() {
                let slot = (parent.next_child - 1) as usize;
                parent.children[slot] = Some(node_id);
            } else {
                return Ok(node_id);
            }
        }

        unreachable!("intern loop returns on root completion")
    }

    fn build_dag_node_fields(
        &self,
        expr: &SymExpr,
        c0: Option<DagNodeId>,
        c1: Option<DagNodeId>,
        c2: Option<DagNodeId>,
    ) -> (DagOp, Sort, bool, usize) {
        let bv_ok = |s: &Sort| matches!(s, Sort::BitVector(w) if (1..=64).contains(w));
        match expr {
            SymExpr::Add(..)
            | SymExpr::Sub(..)
            | SymExpr::Mul(..)
            | SymExpr::Udiv(..)
            | SymExpr::Urem(..)
            | SymExpr::And(..)
            | SymExpr::Or(..)
            | SymExpr::Xor(..) => {
                let (a, b) = (c0.unwrap(), c1.unwrap());
                let na = &self.dag_nodes[a.0 as usize];
                let nb = &self.dag_nodes[b.0 as usize];
                let w = sort_bit_width(&na.sort).max(sort_bit_width(&nb.sort));
                let pure = na.is_pure_bv && nb.is_pure_bv && bv_ok(&na.sort) && bv_ok(&nb.sort);
                let h = na.height.max(nb.height) + 1;
                (dag_bin_op(expr, a, b), Sort::BitVector(w), pure, h)
            }
            SymExpr::Sdiv(..)
            | SymExpr::Srem(..)
            | SymExpr::Smod(..)
            | SymExpr::Shl(..)
            | SymExpr::Lshr(..)
            | SymExpr::Ashr(..) => {
                let (a, b) = (c0.unwrap(), c1.unwrap());
                let na = &self.dag_nodes[a.0 as usize];
                let nb = &self.dag_nodes[b.0 as usize];
                let pure = na.is_pure_bv && nb.is_pure_bv && bv_ok(&na.sort) && na.sort == nb.sort;
                let h = na.height.max(nb.height) + 1;
                (dag_bin_op(expr, a, b), na.sort.clone(), pure, h)
            }
            SymExpr::Eq(..) | SymExpr::Neq(..) | SymExpr::Ult(..) | SymExpr::Ule(..) => {
                let (a, b) = (c0.unwrap(), c1.unwrap());
                let na = &self.dag_nodes[a.0 as usize];
                let nb = &self.dag_nodes[b.0 as usize];
                let pure = na.is_pure_bv && nb.is_pure_bv && bv_ok(&na.sort) && bv_ok(&nb.sort);
                let h = na.height.max(nb.height) + 1;
                (dag_bin_op(expr, a, b), Sort::BitVector(1), pure, h)
            }
            SymExpr::Slt(..) | SymExpr::Sle(..) | SymExpr::Sgt(..) => {
                let (a, b) = (c0.unwrap(), c1.unwrap());
                let na = &self.dag_nodes[a.0 as usize];
                let nb = &self.dag_nodes[b.0 as usize];
                let pure = na.is_pure_bv && nb.is_pure_bv && bv_ok(&na.sort) && na.sort == nb.sort;
                let h = na.height.max(nb.height) + 1;
                (dag_bin_op(expr, a, b), Sort::BitVector(1), pure, h)
            }
            SymExpr::FAdd(..) | SymExpr::FSub(..) | SymExpr::FMul(..) | SymExpr::FDiv(..) => {
                let (a, b) = (c0.unwrap(), c1.unwrap());
                let na = &self.dag_nodes[a.0 as usize];
                let nb = &self.dag_nodes[b.0 as usize];
                let sort = match &na.sort {
                    Sort::Float(w) | Sort::BitVector(w) => Sort::Float(*w),
                    Sort::Array { .. } => Sort::Float(0),
                };
                let h = na.height.max(nb.height) + 1;
                (dag_bin_op(expr, a, b), sort, false, h)
            }
            SymExpr::FEq(..) | SymExpr::FNeq(..) | SymExpr::FLt(..) | SymExpr::FLe(..) => {
                let (a, b) = (c0.unwrap(), c1.unwrap());
                let na = &self.dag_nodes[a.0 as usize];
                let nb = &self.dag_nodes[b.0 as usize];
                let h = na.height.max(nb.height) + 1;
                (dag_bin_op(expr, a, b), Sort::BitVector(1), false, h)
            }
            SymExpr::FNeg(..) | SymExpr::FAbs(..) | SymExpr::FSqrt(..) => {
                let a = c0.unwrap();
                let na = &self.dag_nodes[a.0 as usize];
                let sort = match &na.sort {
                    Sort::Float(w) | Sort::BitVector(w) => Sort::Float(*w),
                    Sort::Array { .. } => Sort::Float(0),
                };
                let op = match expr {
                    SymExpr::FNeg(..) => DagOp::FNeg(a),
                    SymExpr::FAbs(..) => DagOp::FAbs(a),
                    _ => DagOp::FSqrt(a),
                };
                (op, sort, false, na.height + 1)
            }
            SymExpr::FIsNan(..) => {
                let a = c0.unwrap();
                let na = &self.dag_nodes[a.0 as usize];
                (DagOp::FIsNan(a), Sort::BitVector(1), false, na.height + 1)
            }
            SymExpr::Ite { .. } => {
                let (cond, t, f) = (c0.unwrap(), c1.unwrap(), c2.unwrap());
                let nc = &self.dag_nodes[cond.0 as usize];
                let nt = &self.dag_nodes[t.0 as usize];
                let nf = &self.dag_nodes[f.0 as usize];
                let sort = if nt.sort == nf.sort {
                    nt.sort.clone()
                } else {
                    match (&nt.sort, &nf.sort) {
                        (Sort::Float(wt), Sort::BitVector(wf))
                        | (Sort::BitVector(wf), Sort::Float(wt))
                            if wt == wf =>
                        {
                            Sort::Float(*wt)
                        }
                        _ => {
                            Sort::BitVector(sort_bit_width(&nt.sort).max(sort_bit_width(&nf.sort)))
                        }
                    }
                };
                let pure = nc.is_pure_bv
                    && nt.is_pure_bv
                    && nf.is_pure_bv
                    && nc.sort == Sort::BitVector(1)
                    && bv_ok(&nt.sort)
                    && bv_ok(&nf.sort);
                let h = nc.height.max(nt.height).max(nf.height) + 1;
                (DagOp::Ite { cond, t, f }, sort, pure, h)
            }
            SymExpr::Extract { lsb, size, .. } => {
                let a = c0.unwrap();
                let na = &self.dag_nodes[a.0 as usize];
                let pure = na.is_pure_bv
                    && bv_ok(&na.sort)
                    && (1..=64).contains(size)
                    && lsb
                        .checked_add(*size)
                        .is_some_and(|end| end <= sort_bit_width(&na.sort));
                (
                    DagOp::Extract {
                        expr: a,
                        lsb: *lsb,
                        size: *size,
                    },
                    Sort::BitVector(*size),
                    pure,
                    na.height + 1,
                )
            }
            SymExpr::Concat(..) => {
                let (a, b) = (c0.unwrap(), c1.unwrap());
                let na = &self.dag_nodes[a.0 as usize];
                let nb = &self.dag_nodes[b.0 as usize];
                let sum = sort_bit_width(&na.sort).saturating_add(sort_bit_width(&nb.sort));
                let pure = na.is_pure_bv
                    && nb.is_pure_bv
                    && bv_ok(&na.sort)
                    && bv_ok(&nb.sort)
                    && (1..=64).contains(&sum);
                let h = na.height.max(nb.height) + 1;
                (DagOp::Concat(a, b), Sort::BitVector(sum), pure, h)
            }
            SymExpr::ArraySelect { .. } => {
                let (array, index) = (c0.unwrap(), c1.unwrap());
                let na = &self.dag_nodes[array.0 as usize];
                let ni = &self.dag_nodes[index.0 as usize];
                let sort = match &na.sort {
                    Sort::Array { range, .. } => range.as_ref().clone(),
                    _ => Sort::BitVector(0),
                };
                let h = na.height.max(ni.height) + 1;
                (DagOp::ArraySelect { array, index }, sort, false, h)
            }
            SymExpr::ArrayStore { .. } => {
                let (array, index, value) = (c0.unwrap(), c1.unwrap(), c2.unwrap());
                let na = &self.dag_nodes[array.0 as usize];
                let ni = &self.dag_nodes[index.0 as usize];
                let nv = &self.dag_nodes[value.0 as usize];
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
                unreachable!("leaves handled directly during interning")
            }
        }
    }

    fn record_dag_entry(&mut self, id: DagNodeId, bits: Vec<AigLit>, prior_unsupported: usize) {
        if self.call_budget_exceeded.is_none() {
            let unsupported = self.unsupported[prior_unsupported..].to_vec();
            self.dag_lowered
                .insert(id, LoweredDagEntry { bits, unsupported });
        }
    }

    fn enqueue_dag_child(&mut self, child: DagNodeId) -> bool {
        if let Some(entry) = self.dag_lowered.get(&child) {
            self.stats.lowered_cache_hits += 1;
            if !entry.unsupported.is_empty() {
                let notes = entry.unsupported.clone();
                for msg in notes {
                    self.note_unsupported(msg);
                }
            }
            false
        } else {
            true
        }
    }

    fn lower_dag_iterative<'a>(
        &mut self,
        root_id: DagNodeId,
        select_exprs: &HashMap<DagNodeId, &'a SymExpr>,
    ) -> Vec<AigLit> {
        enum Phase<'a> {
            Enter {
                id: DagNodeId,
            },
            Finish {
                id: DagNodeId,
                prior_unsupported: usize,
            },
            FinishAffine {
                id: DagNodeId,
                prior_unsupported: usize,
                form: ModularAffineForm,
                select_exprs_unused: std::marker::PhantomData<&'a SymExpr>,
            },
        }

        let mut stack = vec![Phase::Enter { id: root_id }];
        let mut root_bits: Option<Vec<AigLit>> = None;
        let max_phase2_steps = self.budget.max_unique_nodes.saturating_mul(8).max(4096);
        let mut phase2_steps = 0usize;

        while let Some(frame) = stack.pop() {
            phase2_steps += 1;
            if phase2_steps > max_phase2_steps {
                self.record_budget_exceeded(LoweringBudgetKind::ExprVisits {
                    limit: max_phase2_steps,
                });
            }
            if self.call_budget_exceeded.is_some() {
                let width =
                    sort_bit_width(&self.dag_nodes[root_id.0 as usize].sort).clamp(1, 64) as usize;
                return vec![AigLit::FALSE; width];
            }

            match frame {
                Phase::Enter { id } => {
                    if let Some(entry) = self.dag_lowered.get(&id).cloned() {
                        for msg in entry.unsupported {
                            self.note_unsupported(msg);
                        }
                        self.stats.lowered_cache_hits += 1;
                        if id == root_id && stack.is_empty() {
                            root_bits = Some(entry.bits);
                        }
                        continue;
                    }

                    let prior_unsupported = self.unsupported.len();
                    let node_sort = self.dag_nodes[id.0 as usize].sort.clone();
                    let is_pure_bv = self.dag_nodes[id.0 as usize].is_pure_bv;

                    // Top-of-node scalar/width guards matching prior recursive contract.
                    match &node_sort {
                        Sort::Array { range, .. } => {
                            self.note_unsupported("array expression used as a scalar".to_string());
                            let w = match range.as_ref() {
                                Sort::BitVector(width) | Sort::Float(width) => {
                                    (*width).clamp(1, 64)
                                }
                                Sort::Array { .. } => 1,
                            };
                            let bits = self.fresh_bits(w as usize);
                            self.record_dag_entry(id, bits.clone(), prior_unsupported);
                            if id == root_id && stack.is_empty() {
                                root_bits = Some(bits);
                            }
                            continue;
                        }
                        Sort::BitVector(width) | Sort::Float(width)
                            if *width == 0 || *width > 64 =>
                        {
                            self.note_unsupported(format!(
                                "bitvector width {width} is outside the supported 1..=64-bit range"
                            ));
                            let bits = self.fresh_bits((*width).clamp(1, 64) as usize);
                            self.record_dag_entry(id, bits.clone(), prior_unsupported);
                            if id == root_id && stack.is_empty() {
                                root_bits = Some(bits);
                            }
                            continue;
                        }
                        _ => {}
                    }

                    // Width-preserving modular affine normalization pilot (only after validation).
                    let op = self.dag_nodes[id.0 as usize].op.clone();
                    if self.modular_affine_enabled
                        && is_pure_bv
                        && matches!(op, DagOp::Add(..) | DagOp::Sub(..) | DagOp::Mul(..))
                    {
                        if let Some(extracted) = self.try_extract_modular_affine(id) {
                            match extracted {
                                ExtractedAffine::Constant(val) => {
                                    let w = sort_bit_width(&node_sort);
                                    let bits = const_to_bits(val, w);
                                    self.stats.modular_affine_rewrites += 1;
                                    self.record_dag_entry(id, bits.clone(), prior_unsupported);
                                    if id == root_id && stack.is_empty() {
                                        root_bits = Some(bits);
                                    }
                                    continue;
                                }
                                ExtractedAffine::Term {
                                    form,
                                    cancelled_vars,
                                    simplified,
                                } => {
                                    if simplified {
                                        let mut conflict = false;
                                        for (vid, vsort) in &cancelled_vars {
                                            if let Some(existing) = self.var_sorts.get(vid) {
                                                if existing != vsort {
                                                    conflict = true;
                                                    break;
                                                }
                                            }
                                        }
                                        if !conflict && form.terms.keys().all(|tid| tid.0 < id.0) {
                                            for (vid, vsort) in cancelled_vars {
                                                self.var_sorts.entry(vid).or_insert(vsort);
                                            }
                                            let mut missing = Vec::new();
                                            for &tid in form.terms.keys() {
                                                if self.enqueue_dag_child(tid) {
                                                    missing.push(tid);
                                                }
                                            }
                                            stack.push(Phase::FinishAffine {
                                                id,
                                                prior_unsupported,
                                                form,
                                                select_exprs_unused: std::marker::PhantomData,
                                            });
                                            for tid in missing.into_iter().rev() {
                                                stack.push(Phase::Enter { id: tid });
                                            }
                                            continue;
                                        }
                                    }
                                }
                            }
                        }
                    }

                    match op {
                        DagOp::Const { val, size } => {
                            let bits = const_to_bits(val, size);
                            self.record_dag_entry(id, bits.clone(), prior_unsupported);
                            if id == root_id && stack.is_empty() {
                                root_bits = Some(bits);
                            }
                        }
                        DagOp::Var {
                            id: var_id, sort, ..
                        } => {
                            let bits = self.lower_var_node(var_id, &sort);
                            self.record_dag_entry(id, bits.clone(), prior_unsupported);
                            if id == root_id && stack.is_empty() {
                                root_bits = Some(bits);
                            }
                        }
                        DagOp::FEq(..) => {
                            self.note_unsupported("FEq IEEE semantics".to_string());
                            let bits = self.fresh_bits(1);
                            self.record_dag_entry(id, bits.clone(), prior_unsupported);
                            if id == root_id && stack.is_empty() {
                                root_bits = Some(bits);
                            }
                        }
                        DagOp::FNeq(..) => {
                            self.note_unsupported("FNeq IEEE semantics".to_string());
                            let bits = self.fresh_bits(1);
                            self.record_dag_entry(id, bits.clone(), prior_unsupported);
                            if id == root_id && stack.is_empty() {
                                root_bits = Some(bits);
                            }
                        }
                        DagOp::FLt(..) | DagOp::FLe(..) => {
                            self.note_unsupported(
                                "ordered float comparison IEEE semantics".to_string(),
                            );
                            let bits = self.fresh_bits(1);
                            self.record_dag_entry(id, bits.clone(), prior_unsupported);
                            if id == root_id && stack.is_empty() {
                                root_bits = Some(bits);
                            }
                        }
                        DagOp::ArraySelect { index, .. } => {
                            stack.push(Phase::Finish {
                                id,
                                prior_unsupported,
                            });
                            if self.enqueue_dag_child(index) {
                                stack.push(Phase::Enter { id: index });
                            }
                        }
                        DagOp::Shl(a, b) | DagOp::Lshr(a, b) | DagOp::Ashr(a, b) => {
                            let a_w = sort_bit_width(&self.dag_nodes[a.0 as usize].sort);
                            let b_w = sort_bit_width(&self.dag_nodes[b.0 as usize].sort);
                            let b_is_const =
                                matches!(self.dag_nodes[b.0 as usize].op, DagOp::Const { .. });
                            stack.push(Phase::Finish {
                                id,
                                prior_unsupported,
                            });
                            if a_w == b_w && !b_is_const && self.enqueue_dag_child(b) {
                                stack.push(Phase::Enter { id: b });
                            }
                            if self.enqueue_dag_child(a) {
                                stack.push(Phase::Enter { id: a });
                            }
                        }
                        DagOp::FNeg(a)
                        | DagOp::FAbs(a)
                        | DagOp::FSqrt(a)
                        | DagOp::FIsNan(a)
                        | DagOp::Extract { expr: a, .. } => {
                            stack.push(Phase::Finish {
                                id,
                                prior_unsupported,
                            });
                            if self.enqueue_dag_child(a) {
                                stack.push(Phase::Enter { id: a });
                            }
                        }
                        DagOp::Ule(a, b) | DagOp::Sle(a, b) | DagOp::Sgt(a, b) => {
                            // Matches lowering in reversed order `b <_ a` without cloning SymExpr.
                            stack.push(Phase::Finish {
                                id,
                                prior_unsupported,
                            });
                            if self.enqueue_dag_child(a) {
                                stack.push(Phase::Enter { id: a });
                            }
                            if self.enqueue_dag_child(b) {
                                stack.push(Phase::Enter { id: b });
                            }
                        }
                        DagOp::Concat(a, b) => {
                            // Concat lowers low bits (b) first, then high bits (a).
                            stack.push(Phase::Finish {
                                id,
                                prior_unsupported,
                            });
                            if self.enqueue_dag_child(a) {
                                stack.push(Phase::Enter { id: a });
                            }
                            if self.enqueue_dag_child(b) {
                                stack.push(Phase::Enter { id: b });
                            }
                        }
                        DagOp::And(a, b)
                        | DagOp::Or(a, b)
                        | DagOp::Xor(a, b)
                        | DagOp::Add(a, b)
                        | DagOp::Sub(a, b)
                        | DagOp::Mul(a, b)
                        | DagOp::Udiv(a, b)
                        | DagOp::Urem(a, b)
                        | DagOp::Sdiv(a, b)
                        | DagOp::Srem(a, b)
                        | DagOp::Smod(a, b)
                        | DagOp::Eq(a, b)
                        | DagOp::Neq(a, b)
                        | DagOp::Ult(a, b)
                        | DagOp::Slt(a, b)
                        | DagOp::FAdd(a, b)
                        | DagOp::FSub(a, b)
                        | DagOp::FMul(a, b)
                        | DagOp::FDiv(a, b) => {
                            stack.push(Phase::Finish {
                                id,
                                prior_unsupported,
                            });
                            if self.enqueue_dag_child(b) {
                                stack.push(Phase::Enter { id: b });
                            }
                            if self.enqueue_dag_child(a) {
                                stack.push(Phase::Enter { id: a });
                            }
                        }
                        DagOp::Ite { cond, t, f } => {
                            stack.push(Phase::Finish {
                                id,
                                prior_unsupported,
                            });
                            if self.enqueue_dag_child(f) {
                                stack.push(Phase::Enter { id: f });
                            }
                            if self.enqueue_dag_child(t) {
                                stack.push(Phase::Enter { id: t });
                            }
                            if self.enqueue_dag_child(cond) {
                                stack.push(Phase::Enter { id: cond });
                            }
                        }
                        DagOp::ArrayStore { .. } => {
                            unreachable!("ArrayStore handled by Array sort guard above")
                        }
                    }
                }
                Phase::FinishAffine {
                    id,
                    prior_unsupported,
                    form,
                    ..
                } => {
                    let bits = self.lower_modular_affine_form(&form);
                    self.stats.modular_affine_rewrites += 1;
                    self.record_dag_entry(id, bits.clone(), prior_unsupported);
                    if id == root_id && stack.is_empty() {
                        root_bits = Some(bits);
                    }
                }
                Phase::Finish {
                    id,
                    prior_unsupported,
                } => {
                    if let Some(entry) = self.dag_lowered.get(&id).cloned() {
                        if id == root_id && stack.is_empty() {
                            root_bits = Some(entry.bits);
                        }
                        continue;
                    }
                    let bits = self.finish_dag_node(id, select_exprs);
                    self.record_dag_entry(id, bits.clone(), prior_unsupported);
                    if id == root_id && stack.is_empty() {
                        root_bits = Some(bits);
                    }
                }
            }
        }

        if let Some(bits) = root_bits {
            return bits;
        }
        if let Some(entry) = self.dag_lowered.get(&root_id) {
            return entry.bits.clone();
        }
        let width = sort_bit_width(&self.dag_nodes[root_id.0 as usize].sort).clamp(1, 64) as usize;
        vec![AigLit::FALSE; width]
    }

    fn lower_var_node(&mut self, var_id: u32, sort: &Sort) -> Vec<AigLit> {
        if let Some(existing) = self.var_sorts.get(&var_id) {
            if existing != sort {
                let existing = existing.clone();
                self.note_unsupported(format!(
                    "variable id {var_id} aliases conflicting sorts: {existing:?} vs {sort:?}"
                ));
                let safe_w = sort_bit_width(sort).clamp(1, 64) as usize;
                return self.fresh_bits(safe_w);
            }
        }
        if let Some(bits) = self.var_map.get(&var_id) {
            bits.clone()
        } else {
            let bits = match sort {
                Sort::BitVector(sz) | Sort::Float(sz) => (*sz).max(1),
                Sort::Array { .. } => {
                    self.note_unsupported(format!(
                        "array variable id {var_id} lowered without scalar projection"
                    ));
                    return self.fresh_bits(1);
                }
            };
            self.var_sorts.insert(var_id, sort.clone());
            self.add_var(var_id, bits)
        }
    }

    fn finish_dag_node(
        &mut self,
        id: DagNodeId,
        select_exprs: &HashMap<DagNodeId, &SymExpr>,
    ) -> Vec<AigLit> {
        let op = self.dag_nodes[id.0 as usize].op.clone();
        let bits_of =
            |dag_lowered: &HashMap<DagNodeId, LoweredDagEntry>, child: DagNodeId| -> Vec<AigLit> {
                dag_lowered
                    .get(&child)
                    .map(|entry| entry.bits.clone())
                    .unwrap_or_default()
            };

        match op {
            DagOp::ArraySelect { array, index } => {
                let select_expr = select_exprs.get(&id).copied();
                if let Some(sel) = select_expr {
                    let SymExpr::ArraySelect {
                        array: arr_expr,
                        index: idx_expr,
                    } = sel
                    else {
                        unreachable!("select_exprs only holds ArraySelect nodes");
                    };
                    let err: Option<ValidationError> = validate_expr(arr_expr)
                        .err()
                        .or_else(|| validate_expr(idx_expr).err());
                    if let Some(err) = err {
                        self.note_unsupported(err.to_string());
                        return self.fresh_bits(1);
                    }
                }
                let arr_sort = self.dag_nodes[array.0 as usize].sort.clone();
                let idx_sort = self.dag_nodes[index.0 as usize].sort.clone();
                match (&arr_sort, &idx_sort) {
                    (Sort::Array { domain, range }, idx_sort) => {
                        if idx_sort != domain.as_ref() {
                            self.note_unsupported(format!(
                                "ArraySelect index sort mismatch: expected {domain:?}, got {idx_sort:?}"
                            ));
                            let w = match range.as_ref() {
                                Sort::BitVector(b) | Sort::Float(b) => (*b).clamp(1, 64),
                                Sort::Array { .. } => 1,
                            };
                            return self.fresh_bits(w as usize);
                        }
                    }
                    (other, _) => {
                        self.note_unsupported(format!(
                            "ArraySelect target is not an array: {other:?}"
                        ));
                        return self.fresh_bits(1);
                    }
                }
                let oracle_backed = matches!(
                    &self.dag_nodes[array.0 as usize].op,
                    DagOp::Var { name, .. } if name.starts_with("space_")
                );
                if !oracle_backed {
                    let gap = if self.array_axioms_enabled {
                        self.array_axiom_gap_dag(array)
                    } else {
                        Some("ArraySelect over a term without array axioms".to_string())
                    };
                    if let Some(gap) = gap {
                        self.note_unsupported(gap);
                    }
                }
                if let Some(sel) = select_expr {
                    if let Some(bits) = self.array_select_map.get(sel) {
                        return bits.clone();
                    }
                    let size = sort_bit_width(&self.dag_nodes[id.0 as usize].sort).clamp(1, 64);
                    let bits = self.fresh_bits(size as usize);
                    if self.call_budget_exceeded.is_none() {
                        self.array_select_map.insert(sel.clone(), bits.clone());
                    }
                    bits
                } else {
                    let size = sort_bit_width(&self.dag_nodes[id.0 as usize].sort).clamp(1, 64);
                    self.fresh_bits(size as usize)
                }
            }
            DagOp::And(a, b) => {
                let a_bits = bits_of(&self.dag_lowered, a);
                let b_bits = bits_of(&self.dag_lowered, b);
                let len = std::cmp::max(a_bits.len(), b_bits.len());
                let mut out = Vec::with_capacity(len);
                for i in 0..len {
                    if self.call_budget_exceeded.is_some() {
                        out.resize(len, AigLit::FALSE);
                        return out;
                    }
                    let ax = a_bits.get(i).copied().unwrap_or(AigLit::FALSE);
                    let bx = b_bits.get(i).copied().unwrap_or(AigLit::FALSE);
                    out.push(self.add_and(ax, bx));
                }
                out
            }
            DagOp::Or(a, b) => {
                let a_bits = bits_of(&self.dag_lowered, a);
                let b_bits = bits_of(&self.dag_lowered, b);
                let len = std::cmp::max(a_bits.len(), b_bits.len());
                let mut out = Vec::with_capacity(len);
                for i in 0..len {
                    if self.call_budget_exceeded.is_some() {
                        out.resize(len, AigLit::FALSE);
                        return out;
                    }
                    let ax = a_bits.get(i).copied().unwrap_or(AigLit::FALSE);
                    let bx = b_bits.get(i).copied().unwrap_or(AigLit::FALSE);
                    out.push(self.add_or(ax, bx));
                }
                out
            }
            DagOp::Xor(a, b) => {
                let a_bits = bits_of(&self.dag_lowered, a);
                let b_bits = bits_of(&self.dag_lowered, b);
                let len = std::cmp::max(a_bits.len(), b_bits.len());
                let mut out = Vec::with_capacity(len);
                for i in 0..len {
                    if self.call_budget_exceeded.is_some() {
                        out.resize(len, AigLit::FALSE);
                        return out;
                    }
                    let ax = a_bits.get(i).copied().unwrap_or(AigLit::FALSE);
                    let bx = b_bits.get(i).copied().unwrap_or(AigLit::FALSE);
                    out.push(self.add_xor(ax, bx));
                }
                out
            }
            DagOp::Add(a, b) => {
                let a_bits = bits_of(&self.dag_lowered, a);
                let b_bits = bits_of(&self.dag_lowered, b);
                self.add_ripple_carry_adder(&a_bits, &b_bits, AigLit::FALSE)
            }
            DagOp::Sub(a, b) => {
                let a_bits = bits_of(&self.dag_lowered, a);
                let b_bits = bits_of(&self.dag_lowered, b);
                // Extend B before complementing it: zero-extending !B would
                // turn the implicit high one bits of two's complement into zero.
                let width = a_bits.len().max(b_bits.len());
                let b_inv: Vec<AigLit> = (0..width)
                    .map(|i| b_bits.get(i).copied().unwrap_or(AigLit::FALSE).not())
                    .collect();
                // A - B = A + (!B) + 1
                self.add_ripple_carry_adder(&a_bits, &b_inv, AigLit::TRUE)
            }
            DagOp::Eq(a, b) => {
                let a_bits = bits_of(&self.dag_lowered, a);
                let b_bits = bits_of(&self.dag_lowered, b);
                vec![self.add_eq(&a_bits, &b_bits)]
            }
            DagOp::Neq(a, b) => {
                let a_bits = bits_of(&self.dag_lowered, a);
                let b_bits = bits_of(&self.dag_lowered, b);
                vec![self.add_neq(&a_bits, &b_bits)]
            }
            DagOp::Ult(a, b) => {
                let a_bits = bits_of(&self.dag_lowered, a);
                let b_bits = bits_of(&self.dag_lowered, b);
                vec![self.add_ult(&a_bits, &b_bits)]
            }
            DagOp::Ule(a, b) => {
                // a <= b  ≡  !(b < a)
                let b_bits = bits_of(&self.dag_lowered, b);
                let a_bits = bits_of(&self.dag_lowered, a);
                vec![self.add_ult(&b_bits, &a_bits).not()]
            }
            DagOp::Slt(a, b) => {
                let a_bits = bits_of(&self.dag_lowered, a);
                let b_bits = bits_of(&self.dag_lowered, b);
                if a_bits.len() != b_bits.len() {
                    self.note_unsupported(format!(
                        "Slt operands have different widths: {} and {} bits",
                        a_bits.len(),
                        b_bits.len()
                    ));
                    self.fresh_bits(1)
                } else {
                    vec![self.add_slt(&a_bits, &b_bits)]
                }
            }
            DagOp::Sle(a, b) => {
                // a <=_s b  ≡  !(b <_s a)
                let b_bits = bits_of(&self.dag_lowered, b);
                let a_bits = bits_of(&self.dag_lowered, a);
                if b_bits.len() != a_bits.len() {
                    self.note_unsupported(format!(
                        "Slt operands have different widths: {} and {} bits",
                        b_bits.len(),
                        a_bits.len()
                    ));
                    let fresh = self.fresh_bits(1);
                    vec![fresh[0].not()]
                } else {
                    vec![self.add_slt(&b_bits, &a_bits).not()]
                }
            }
            DagOp::Sgt(a, b) => {
                // a >_s b  ≡  b <_s a
                let b_bits = bits_of(&self.dag_lowered, b);
                let a_bits = bits_of(&self.dag_lowered, a);
                if b_bits.len() != a_bits.len() {
                    self.note_unsupported(format!(
                        "Slt operands have different widths: {} and {} bits",
                        b_bits.len(),
                        a_bits.len()
                    ));
                    self.fresh_bits(1)
                } else {
                    vec![self.add_slt(&b_bits, &a_bits)]
                }
            }
            DagOp::Shl(a, b) | DagOp::Lshr(a, b) | DagOp::Ashr(a, b) => {
                let kind = match op {
                    DagOp::Shl(..) => ShiftKind::Left,
                    DagOp::Lshr(..) => ShiftKind::LogicalRight,
                    _ => ShiftKind::ArithmeticRight,
                };
                let a_bits = bits_of(&self.dag_lowered, a);
                let b_width = sort_bit_width(&self.dag_nodes[b.0 as usize].sort);
                if a_bits.len() != b_width as usize {
                    self.note_unsupported(format!(
                        "shift amount width {b_width} differs from value width {}",
                        a_bits.len()
                    ));
                    self.fresh_bits(a_bits.len().max(1))
                } else if let DagOp::Const { val, size } = self.dag_nodes[b.0 as usize].op {
                    let amount = if size >= 64 {
                        val
                    } else {
                        val & ((1u64 << size) - 1)
                    };
                    Self::shift_by_constant(&a_bits, amount, kind)
                } else {
                    let amount = bits_of(&self.dag_lowered, b);
                    self.add_shift(&a_bits, &amount, kind)
                }
            }
            DagOp::Extract {
                expr: inner,
                lsb,
                size,
            } => {
                let bits = bits_of(&self.dag_lowered, inner);
                let Some(end) = lsb.checked_add(size) else {
                    self.note_unsupported("Extract width overflow".to_string());
                    return self.fresh_bits(size.max(1) as usize);
                };
                if size == 0 || end as usize > bits.len() {
                    self.note_unsupported("Extract outside source bit width".to_string());
                    return self.fresh_bits(size.max(1) as usize);
                }
                bits[lsb as usize..end as usize].to_vec()
            }
            DagOp::Concat(a, b) => {
                // Concat(a, b): b is the low bits, a is the high bits
                let mut out = bits_of(&self.dag_lowered, b);
                out.extend(bits_of(&self.dag_lowered, a));
                out
            }
            DagOp::FNeg(a) => {
                let a_bits = bits_of(&self.dag_lowered, a);
                let mut out = self.adjust_float_bits(a, a_bits);
                if let Some(sign) = out.last_mut() {
                    *sign = sign.not();
                }
                out
            }
            DagOp::FAbs(a) => {
                let a_bits = bits_of(&self.dag_lowered, a);
                let mut out = self.adjust_float_bits(a, a_bits);
                if let Some(sign) = out.last_mut() {
                    *sign = AigLit::FALSE;
                }
                out
            }
            DagOp::FIsNan(a) => {
                let a_bits = bits_of(&self.dag_lowered, a);
                let bits = self.adjust_float_bits(a, a_bits);
                let (exp, mant) = Self::float_fields(&bits);
                let mut exp_all1 = AigLit::TRUE;
                for b in exp {
                    exp_all1 = self.add_and(exp_all1, b);
                }
                let mut mant_nz = AigLit::FALSE;
                for b in mant {
                    mant_nz = self.add_or(mant_nz, b);
                }
                vec![self.add_and(exp_all1, mant_nz)]
            }
            DagOp::FAdd(a, b) | DagOp::FSub(a, b) | DagOp::FMul(a, b) | DagOp::FDiv(a, b) => {
                let a_bits = bits_of(&self.dag_lowered, a);
                let a_len = self.adjust_float_bits(a, a_bits).len();
                let b_bits = bits_of(&self.dag_lowered, b);
                let b_len = self.adjust_float_bits(b, b_bits).len();
                let width = a_len.max(b_len);
                self.note_unsupported("symbolic float arithmetic".to_string());
                self.fresh_bits(width.max(1))
            }
            DagOp::FSqrt(a) => {
                let a_bits = bits_of(&self.dag_lowered, a);
                let width = self.adjust_float_bits(a, a_bits).len();
                self.note_unsupported("symbolic FSqrt".to_string());
                self.fresh_bits(width.max(1))
            }
            DagOp::Mul(a, b) => {
                let a_bits = bits_of(&self.dag_lowered, a);
                let b_bits = bits_of(&self.dag_lowered, b);
                self.add_multiplier(&a_bits, &b_bits)
            }
            DagOp::Udiv(a, b) => {
                let a_bits = bits_of(&self.dag_lowered, a);
                let b_bits = bits_of(&self.dag_lowered, b);
                self.add_udiv_urem(&a_bits, &b_bits).0
            }
            DagOp::Urem(a, b) => {
                let a_bits = bits_of(&self.dag_lowered, a);
                let b_bits = bits_of(&self.dag_lowered, b);
                self.add_udiv_urem(&a_bits, &b_bits).1
            }
            DagOp::Sdiv(a, b) | DagOp::Srem(a, b) | DagOp::Smod(a, b) => {
                let kind = match op {
                    DagOp::Sdiv(..) => SignedDivision::Quotient,
                    DagOp::Srem(..) => SignedDivision::Remainder,
                    _ => SignedDivision::Modulus,
                };
                let a_bits = bits_of(&self.dag_lowered, a);
                let b_bits = bits_of(&self.dag_lowered, b);
                if a_bits.len() != b_bits.len() {
                    self.note_unsupported(format!(
                        "signed division operands have different widths: {} and {} bits",
                        a_bits.len(),
                        b_bits.len()
                    ));
                    self.fresh_bits(a_bits.len().max(b_bits.len()).max(1))
                } else {
                    self.add_signed_division(&a_bits, &b_bits, kind)
                }
            }
            DagOp::Ite { cond, t, f } => {
                let c_bits = bits_of(&self.dag_lowered, cond);
                let t_bits = bits_of(&self.dag_lowered, t);
                let f_bits = bits_of(&self.dag_lowered, f);
                if c_bits.len() != 1 {
                    self.note_unsupported(format!("Ite with a {}-bit condition", c_bits.len()));
                }
                let c = c_bits.first().copied().unwrap_or(AigLit::FALSE);
                let width = t_bits.len().max(f_bits.len());
                (0..width)
                    .map(|i| {
                        let then_bit = t_bits.get(i).copied().unwrap_or(AigLit::FALSE);
                        let else_bit = f_bits.get(i).copied().unwrap_or(AigLit::FALSE);
                        self.add_ite(c, then_bit, else_bit)
                    })
                    .collect()
            }
            DagOp::Const { .. }
            | DagOp::Var { .. }
            | DagOp::FEq(..)
            | DagOp::FNeq(..)
            | DagOp::FLt(..)
            | DagOp::FLe(..)
            | DagOp::ArrayStore { .. } => {
                unreachable!("leaf/short-circuit nodes finish directly in Phase::Enter")
            }
        }
    }

    fn adjust_float_bits(&mut self, child_id: DagNodeId, bits: Vec<AigLit>) -> Vec<AigLit> {
        let want = match &self.dag_nodes[child_id.0 as usize].sort {
            Sort::Float(sz) | Sort::BitVector(sz) => *sz as usize,
            _ => bits.len(),
        };
        let const_val = match self.dag_nodes[child_id.0 as usize].op {
            DagOp::Const { val, .. } => Some(val),
            _ => None,
        };
        if want != 32 && want != 64 {
            self.note_unsupported(format!(
                "IEEE bit-pattern width {want} is not 32 or 64 bits"
            ));
        }
        if bits.len() == want {
            return bits;
        }
        if let Some(val) = const_val {
            let mut out = Vec::with_capacity(want);
            for i in 0..want {
                out.push(if i < 64 && (val & (1u64 << i)) != 0 {
                    AigLit::TRUE
                } else {
                    AigLit::FALSE
                });
            }
            return out;
        }
        let mut out = bits;
        out.resize(want, AigLit::FALSE);
        out
    }

    /// Attempt to extract a width-preserving [`ModularAffineForm`] from a
    /// validated `1..=64`-bit `Add`/`Sub`/`Mul` node.
    ///
    /// Returns `None` if the subtree exceeds term/op caps, or contains an
    /// unmodeled shape; never crosses a mixed-width zero-extension boundary.
    fn try_extract_modular_affine(&self, root_id: DagNodeId) -> Option<ExtractedAffine> {
        let root_node = &self.dag_nodes[root_id.0 as usize];
        if !root_node.is_pure_bv {
            return None;
        }
        let width = match root_node.sort {
            Sort::BitVector(w) if (1..=64).contains(&w) => w,
            _ => return None,
        };
        let mask = bitvector_mask(width);

        let mut work = vec![(root_id, 1u64)];
        let mut constant = 0u64;
        let mut terms: BTreeMap<DagNodeId, u64> = BTreeMap::new();
        let mut dropped_roots: Vec<DagNodeId> = Vec::new();
        let mut ops = 0usize;
        let mut const_hits = 0usize;
        let mut term_hits = 0usize;
        let mut unfolded_mul_const = false;
        let mut saw_zero_scale = false;

        while let Some((id, coeff)) = work.pop() {
            ops += 1;
            if ops > MAX_AFFINE_OPS {
                return None;
            }
            let coeff = coeff & mask;
            if coeff == 0 {
                saw_zero_scale = true;
                dropped_roots.push(id);
                continue;
            }

            let node = &self.dag_nodes[id.0 as usize];
            let node_width = sort_bit_width(&node.sort);
            match &node.op {
                DagOp::Const { val, size } if (1..=64).contains(size) => {
                    let k = (*val & bitvector_mask(*size)) & mask;
                    constant = constant.wrapping_add(k.wrapping_mul(coeff)) & mask;
                    const_hits += 1;
                    continue;
                }
                DagOp::Add(a, b) if node_width == width && node.is_pure_bv => {
                    work.push((*b, coeff));
                    work.push((*a, coeff));
                    continue;
                }
                DagOp::Sub(a, b) if node_width == width && node.is_pure_bv => {
                    let neg_coeff = (0u64.wrapping_sub(coeff)) & mask;
                    work.push((*b, neg_coeff));
                    work.push((*a, coeff));
                    continue;
                }
                DagOp::Mul(a, b) if node_width == width && node.is_pure_bv => {
                    let na = &self.dag_nodes[a.0 as usize];
                    let nb = &self.dag_nodes[b.0 as usize];
                    match (&na.op, &nb.op) {
                        (DagOp::Const { val, size }, _) if (1..=64).contains(size) => {
                            let k = (*val & bitvector_mask(*size)) & mask;
                            let next_coeff = coeff.wrapping_mul(k) & mask;
                            unfolded_mul_const = true;
                            work.push((*b, next_coeff));
                            continue;
                        }
                        (_, DagOp::Const { val, size }) if (1..=64).contains(size) => {
                            let k = (*val & bitvector_mask(*size)) & mask;
                            let next_coeff = coeff.wrapping_mul(k) & mask;
                            unfolded_mul_const = true;
                            work.push((*a, next_coeff));
                            continue;
                        }
                        _ => {}
                    }
                }
                _ => {}
            }

            if id == root_id {
                return None;
            }
            debug_assert!(id.0 < root_id.0, "affine child must strictly precede root");

            term_hits += 1;
            let prev = terms.get(&id).copied().unwrap_or(0);
            let next = prev.wrapping_add(coeff) & mask;
            if next == 0 {
                terms.remove(&id);
                dropped_roots.push(id);
            } else {
                terms.insert(id, next);
                if terms.len() > MAX_AFFINE_TERMS {
                    return None;
                }
            }
        }

        // Collect every variable in cancelled/zeroed subexpressions so we never
        // drop a variable declaration's sort binding when simplifying to 0.
        let mut cancelled_vars = Vec::new();
        let mut seen = HashSet::new();
        while let Some(drop_id) = dropped_roots.pop() {
            if !seen.insert(drop_id) {
                continue;
            }
            if seen.len() > MAX_AFFINE_OPS {
                return None;
            }
            match &self.dag_nodes[drop_id.0 as usize].op {
                DagOp::Const { .. } => {}
                DagOp::Var {
                    id: var_id, sort, ..
                } => cancelled_vars.push((*var_id, sort.clone())),
                DagOp::Add(a, b) | DagOp::Sub(a, b) | DagOp::Mul(a, b) => {
                    dropped_roots.push(*a);
                    dropped_roots.push(*b);
                }
                _ => {}
            }
        }

        let simplified = terms.is_empty()
            || (terms.len() == 1 && constant == 0 && terms.values().next().copied() == Some(1))
            || unfolded_mul_const
            || saw_zero_scale
            || const_hits >= 2
            || (const_hits == 1 && constant == 0)
            || term_hits > terms.len();

        if terms.is_empty() && cancelled_vars.is_empty() {
            return Some(ExtractedAffine::Constant(constant));
        }

        if terms.keys().any(|tid| tid.0 >= root_id.0) {
            return None;
        }

        Some(ExtractedAffine::Term {
            form: ModularAffineForm {
                width,
                constant,
                terms,
            },
            cancelled_vars,
            simplified,
        })
    }

    fn scale_bits_by_const(&mut self, bits: &[AigLit], coeff: u64, width: u32) -> Vec<AigLit> {
        let w = width as usize;
        let padded: Vec<AigLit> = (0..w)
            .map(|i| bits.get(i).copied().unwrap_or(AigLit::FALSE))
            .collect();
        let mask = bitvector_mask(width);
        let coeff = coeff & mask;
        if coeff == 0 {
            return vec![AigLit::FALSE; w];
        }
        if coeff == 1 {
            return padded;
        }
        if coeff == mask {
            return self.add_neg(&padded);
        }
        if coeff.is_power_of_two() {
            return Self::shift_by_constant(
                &padded,
                u64::from(coeff.trailing_zeros()),
                ShiftKind::Left,
            );
        }

        let mut acc: Option<Vec<AigLit>> = None;
        for shift in 0..width {
            if self.call_budget_exceeded.is_some() {
                return vec![AigLit::FALSE; w];
            }
            if (coeff & (1u64 << shift)) != 0 {
                let shifted = Self::shift_by_constant(&padded, u64::from(shift), ShiftKind::Left);
                acc = Some(match acc {
                    None => shifted,
                    Some(prev) => self.add_ripple_carry_adder(&prev, &shifted, AigLit::FALSE),
                });
            }
        }
        acc.unwrap_or_else(|| vec![AigLit::FALSE; w])
    }

    fn lower_modular_affine_form(&mut self, form: &ModularAffineForm) -> Vec<AigLit> {
        let w = form.width as usize;
        let mask = bitvector_mask(form.width);
        let constant = form.constant & mask;

        if form.terms.is_empty() {
            return const_to_bits(constant, form.width);
        }

        let mut acc: Option<Vec<AigLit>> = if constant != 0 {
            Some(const_to_bits(constant, form.width))
        } else {
            None
        };

        for (&term_id, &coeff) in &form.terms {
            if self.call_budget_exceeded.is_some() {
                return vec![AigLit::FALSE; w];
            }
            let raw_bits = self
                .dag_lowered
                .get(&term_id)
                .map(|entry| entry.bits.clone())
                .unwrap_or_default();
            let scaled = self.scale_bits_by_const(&raw_bits, coeff, form.width);
            acc = Some(match acc {
                None => scaled,
                Some(prev) => self.add_ripple_carry_adder(&prev, &scaled, AigLit::FALSE),
            });
        }

        acc.unwrap_or_else(|| vec![AigLit::FALSE; w])
    }

    fn float_fields(bits: &[AigLit]) -> (Vec<AigLit>, Vec<AigLit>) {
        // f32: 1 sign + 8 exp + 23 mant; f64: 1 + 11 + 52
        let n = bits.len();
        if n == 32 {
            let mant = bits[0..23].to_vec();
            let exp = bits[23..31].to_vec();
            (exp, mant)
        } else if n >= 64 {
            let mant = bits[0..52].to_vec();
            let exp = bits[52..63].to_vec();
            (exp, mant)
        } else {
            // Fallback: top half exp, bottom mantissa
            let mid = n / 2;
            (bits[mid..].to_vec(), bits[..mid].to_vec())
        }
    }

    /// Converts the entire AIG into a CNF formula.
    pub fn to_cnf(&mut self, cnf: &mut crate::cnf::CnfBuilder) {
        for i in self.last_cnf_node..self.nodes.len() {
            let node = &self.nodes[i];
            let idx = (i + 1) as u32;
            if let AigNode::And(a, b) = node {
                cnf.add_and_gate(idx, *a, *b);
            }
        }
        self.last_cnf_node = self.nodes.len();
    }
}

fn bitvector_mask(width_bits: u32) -> u64 {
    match width_bits {
        0 => 0,
        1..=63 => (1u64 << width_bits) - 1,
        _ => u64::MAX,
    }
}

fn const_to_bits(val: u64, size: u32) -> Vec<AigLit> {
    let mut bits = Vec::with_capacity(size as usize);
    for i in 0..size {
        bits.push(if i < 64 && (val & (1u64 << i)) != 0 {
            AigLit::TRUE
        } else {
            AigLit::FALSE
        });
    }
    bits
}

fn dag_bin_op(expr: &SymExpr, a: DagNodeId, b: DagNodeId) -> DagOp {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::SymExpr;
    use crate::cnf::CnfBuilder;
    use crate::sat::SatSolver;
    use crate::solver::{SatResult, Solver};

    /// Lower a boolean SymExpr (must yield 1 bit) into SAT and check satisfiability.
    fn check_sat(expr: SymExpr) -> bool {
        let mut aig = AigManager::new();
        let bits = aig.lower_expr(&expr);
        assert_eq!(bits.len(), 1, "Expected a 1-bit boolean expression");
        let out_lit = bits[0];

        let mut cnf = CnfBuilder::new();
        aig.to_cnf(&mut cnf);

        // Assert the output literal is TRUE
        cnf.assert_lit(out_lit);

        let mut sat = SatSolver::new();
        for clause in &cnf.clauses {
            // add_clause returns false when adding an empty (trivially UNSAT) clause
            if !sat.add_clause(clause.0.clone()) {
                return false;
            }
        }
        sat.solve()
    }

    #[test]
    fn test_const_eq_sat() {
        // 5 == 5  => should fold to Const{val:1} => trivially SAT
        let five_a = SymExpr::new_const(5, 8);
        let five_b = SymExpr::new_const(5, 8);
        let eq = SymExpr::new_eq(five_a, five_b);
        // Constant fold should yield Const { val: 1, size: 1 }
        assert_eq!(eq, SymExpr::Const { val: 1, size: 1 });
        assert!(check_sat(eq));
    }

    #[test]
    fn test_const_eq_unsat() {
        // 5 == 6  => should fold to Const{val:0} => trivially UNSAT
        let five = SymExpr::new_const(5, 8);
        let six = SymExpr::new_const(6, 8);
        let eq = SymExpr::new_eq(five, six);
        assert_eq!(eq, SymExpr::Const { val: 0, size: 1 });
        assert!(!check_sat(eq));
    }

    #[test]
    fn test_var_eq_sat() {
        // x == y  =>  SAT (assign x = y = 0)
        let x = SymExpr::new_var("x", 8);
        let y = SymExpr::new_var("y", 8);
        assert!(check_sat(SymExpr::new_eq(x, y)));
    }

    #[test]
    fn test_add_eq_sat() {
        // x + 5 == 10  =>  SAT (x = 5)
        let x = SymExpr::new_var("x", 8);
        let five = SymExpr::new_const(5, 8);
        let ten = SymExpr::new_const(10, 8);
        let eq = SymExpr::new_eq(SymExpr::new_add(x, five), ten);
        assert!(check_sat(eq));
    }

    #[test]
    fn test_sub_eq_sat() {
        // x - 3 == 7  =>  SAT (x = 10)
        let x = SymExpr::new_var("x", 8);
        let three = SymExpr::new_const(3, 8);
        let seven = SymExpr::new_const(7, 8);
        let eq = SymExpr::new_eq(SymExpr::new_sub(x, three), seven);
        assert!(check_sat(eq));
    }

    #[test]
    fn test_add_const_unsat() {
        // 5 + 5 == 6  =>  constant-folds to 10 == 6 => Const{val:0} => UNSAT
        let five_a = SymExpr::new_const(5, 8);
        let five_b = SymExpr::new_const(5, 8);
        let six = SymExpr::new_const(6, 8);
        let sum = SymExpr::new_add(five_a, five_b);
        let eq = SymExpr::new_eq(sum, six);
        assert!(!check_sat(eq));
    }

    #[test]
    fn test_neq_sat() {
        // x != y  =>  SAT (assign x=0, y=1)
        let x = SymExpr::new_var("x", 8);
        let y = SymExpr::new_var("y", 8);
        assert!(check_sat(SymExpr::new_neq(x, y)));
    }

    #[test]
    fn test_ult_sat() {
        // x < 10 (unsigned)  => SAT (x = 0..9)
        let x = SymExpr::new_var("x", 8);
        let ten = SymExpr::new_const(10, 8);
        assert!(check_sat(SymExpr::Ult(Box::new(x), Box::new(ten))));
    }

    #[test]
    fn test_ult_unsat() {
        // 10 < 10 => UNSAT (constant fold: false)
        let ten_a = SymExpr::new_const(10, 8);
        let ten_b = SymExpr::new_const(10, 8);
        let ult = SymExpr::new_ult(ten_a, ten_b);
        assert_eq!(ult, SymExpr::Const { val: 0, size: 1 });
        assert!(!check_sat(ult));
    }

    #[test]
    fn test_float_fneg_bitblast_width() {
        // FNeg of f32-sorted var → 32 IEEE bits (sign flip is structural).
        let x = SymExpr::new_float_var("fx", 32);
        let neg = SymExpr::FNeg(Box::new(x));
        let mut aig = AigManager::new();
        let bits = aig.lower_expr(&neg);
        assert_eq!(bits.len(), 32, "f32 FNeg must bit-blast to 32 bits");
    }

    #[test]
    fn test_float_fisnan_concrete_unsat() {
        // 1.0 is not NaN → FIsNan folds or bit-blasts to false → UNSAT when asserted.
        let one = SymExpr::Const {
            val: 1.0f32.to_bits() as u64,
            size: 32,
        };
        // Avoid constant-folder path by going through Var + free FIsNan on concrete via fold:
        let folded = SymExpr::new_fisnan(one.clone());
        // new_fisnan on concrete folds: 1.0 is not nan → Const 0
        assert_eq!(folded, SymExpr::Const { val: 0, size: 1 });
        assert!(!check_sat(folded));
    }

    #[test]
    fn symbolic_float_equality_is_unknown_until_ieee_semantics_are_encoded() {
        let x = SymExpr::new_float_var("a", 32);
        let y = SymExpr::new_float_var("b", 32);
        let eq = SymExpr::FEq(Box::new(x), Box::new(y));
        let mut solver = Solver::new();
        solver.assert(eq);
        assert_eq!(solver.check_sat().expect("check_sat"), SatResult::Unknown);
    }

    #[test]
    fn test_float_flt_bitblast_is_bool() {
        // Unsupported symbolic float comparisons still have a one-bit result
        // shape, but the solver records that it cannot decide their semantics.
        let x = SymExpr::new_float_var("p", 32);
        let y = SymExpr::new_float_var("q", 32);
        let lt = SymExpr::FLt(Box::new(x), Box::new(y));
        let mut aig = AigManager::new();
        let bits = aig.lower_expr(&lt);
        assert_eq!(bits.len(), 1);
        // Concrete fold path: 1.0 < 2.0
        let one = SymExpr::Const {
            val: 1.0f32.to_bits() as u64,
            size: 32,
        };
        let two = SymExpr::Const {
            val: 2.0f32.to_bits() as u64,
            size: 32,
        };
        let folded = SymExpr::new_flt(one, two);
        assert_eq!(folded, SymExpr::Const { val: 1, size: 1 });
        assert!(check_sat(folded));
    }

    #[test]
    fn symbolic_float_arithmetic_is_unknown_until_encoded() {
        let a = SymExpr::new_float_var("fa", 32);
        let b = SymExpr::new_float_var("fb", 32);
        let sum = SymExpr::FAdd(Box::new(a), Box::new(b));
        let differs_from_zero = SymExpr::Neq(Box::new(sum), Box::new(SymExpr::new_const(0, 32)));
        let mut solver = Solver::new();
        solver.assert(differs_from_zero);
        assert_eq!(solver.check_sat().expect("check_sat"), SatResult::Unknown);
    }

    #[test]
    fn test_eq_var_const_sat() {
        let x = SymExpr::new_var("cx", 8);
        let five = SymExpr::new_const(5, 8);
        assert!(check_sat(SymExpr::new_eq(x, five)), "x==5 must be SAT");
    }

    #[test]
    fn test_eq_and_neq_same_const_unsat() {
        // Structural: And(eq, eq.not()) collapses to FALSE in AIG.
        let x = SymExpr::new_var("cx2", 8);
        let five = SymExpr::new_const(5, 8);
        let eq = SymExpr::new_eq(x.clone(), five.clone());
        let ne = SymExpr::new_neq(x, five);
        let both = SymExpr::And(Box::new(eq), Box::new(ne));
        assert!(!check_sat(both), "Eq ∧ Neq same const must be UNSAT");
    }

    #[test]
    fn test_eq_var_two_consts_contradiction() {
        // Watch-list BCP (MiniSat polarity) must force bit conflicts across eqs.
        let x = SymExpr::new_var("cx3", 8);
        let five = SymExpr::new_const(5, 8);
        let six = SymExpr::new_const(6, 8);
        let both = SymExpr::And(
            Box::new(SymExpr::new_eq(x.clone(), five)),
            Box::new(SymExpr::new_eq(x, six)),
        );
        assert!(!check_sat(both), "x==5 ∧ x==6 must be UNSAT");
    }

    #[test]
    fn test_slt_signed_wrap() {
        // -1 <_s 0: i8(-1) = 0xFF, i8(0) = 0x00 => -1 < 0 signed => SAT
        let neg_one = SymExpr::new_const(0xFF, 8);
        let zero = SymExpr::new_const(0, 8);
        assert!(check_sat(SymExpr::new_slt(neg_one, zero)));
    }

    #[test]
    fn test_extract_sat() {
        // Extract bits [7:4] of x, assert they equal 0xA
        let x = SymExpr::new_var("x", 8);
        let hi_nibble = SymExpr::Extract {
            expr: Box::new(x),
            lsb: 4,
            size: 4,
        };
        let target = SymExpr::new_const(0xA, 4);
        assert!(check_sat(SymExpr::new_eq(hi_nibble, target)));
    }

    #[test]
    fn test_lshr_sat() {
        // (x >> 1) == 5  => x must be 10 or 11 => SAT
        let x = SymExpr::new_var("x", 8);
        let shift = SymExpr::new_const(1, 8);
        let five = SymExpr::new_const(5, 8);
        let shifted = SymExpr::Lshr(Box::new(x), Box::new(shift));
        assert!(check_sat(SymExpr::new_eq(shifted, five)));
    }
}
