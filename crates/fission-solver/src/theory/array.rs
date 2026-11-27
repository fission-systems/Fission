use crate::ast::{Sort, SymExpr};
use crate::cnf::Lit;
use crate::theory::{Theory, TheoryStatus};

use std::collections::{HashMap, HashSet, VecDeque};

/// Default upper bound on array-theory lemmas emitted per [`ArrayTheory`] instance.
pub const DEFAULT_MAX_ARRAY_LEMMAS: usize = 100_000;

/// Compact interned identifier for array/select/store expressions stored inside
/// [`ArrayTheory`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ArrayTermId(u32);

/// Instrumentation counters for [`ArrayTheory`] term interning and lemma generation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ArrayTheoryStats {
    pub terms_interned: u64,
    pub term_cache_hits: u64,
    pub lemmas_emitted: u64,
    pub expr_visits: u64,
    pub budget_exceeded: bool,
}

#[derive(Clone)]
pub struct ArrayTheory {
    /// Interned term arena so maps, sets, and worklists store compact IDs instead
    /// of cloning whole `SymExpr` subtrees repeatedly.
    terms: Vec<SymExpr>,
    term_to_id: HashMap<SymExpr, ArrayTermId>,
    /// Maps an interned Array base expression to its interned Selects.
    parent_selects: HashMap<ArrayTermId, Vec<ArrayTermId>>,
    /// Every select in `parent_selects`, in the order first seen. The map's
    /// own iteration order is the hasher's, and the memory CEGAR loop walks
    /// the selects to emit lemmas -- so models varied from run to run.
    select_order: Vec<ArrayTermId>,
    /// Maps an interned Array base expression to its interned Stores.
    stores: HashMap<ArrayTermId, Vec<ArrayTermId>>,
    /// Select/Store terms already processed so repeated calls do not re-emit axioms.
    processed_terms: HashSet<ArrayTermId>,
    /// Memoized `has_supported_scalar_array_sort` check per interned array base ID.
    supported_scalar_cache: HashMap<ArrayTermId, bool>,
    /// Deduplicates emitted AST lemmas.
    emitted_lemmas: HashSet<SymExpr>,
    /// Accumulated AST lemmas to be asserted into the bitvector solver.
    pending_lemmas: Vec<SymExpr>,
    /// CNF-level conflict/propagation clauses emitted via [`Theory::check`].
    pending_cnf_lemmas: Vec<Vec<Lit>>,
    max_lemmas: usize,
    stats: ArrayTheoryStats,
}

impl Default for ArrayTheory {
    fn default() -> Self {
        Self::new()
    }
}

/// Iterative helper to resolve the `Sort` of an array expression spine without
/// recursive `SymExpr::get_sort` calls over long `ArrayStore` chains.
fn resolve_array_base_sort(mut array: &SymExpr) -> Option<&Sort> {
    let mut steps = 0usize;
    loop {
        steps += 1;
        if steps > crate::validate::MAX_EXPR_DEPTH {
            return None;
        }
        match array {
            SymExpr::Var { sort, .. } => return Some(sort),
            SymExpr::ArrayStore { array: inner, .. } => array = inner.as_ref(),
            _ => return None,
        }
    }
}

/// Whether `array` maps 1-to-64-bit indices to 1-to-64-bit elements: the
/// only arrays [`ArrayTheory`] writes axioms for.
pub(crate) fn has_supported_scalar_array_sort(array: &SymExpr) -> bool {
    let sort_buf;
    let sort = match resolve_array_base_sort(array) {
        Some(s) => s,
        None => {
            sort_buf = array.get_sort();
            &sort_buf
        }
    };
    matches!(
        sort,
        Sort::Array { domain, range }
            if matches!(domain.as_ref(), Sort::BitVector(w) | Sort::Float(w) if (1..=64).contains(w))
                && matches!(range.as_ref(), Sort::BitVector(w) | Sort::Float(w) if (1..=64).contains(w))
    )
}

fn push_children<'a>(expr: &'a SymExpr, stack: &mut Vec<&'a SymExpr>) {
    match expr {
        SymExpr::Const { .. } | SymExpr::Var { .. } => {}
        SymExpr::FNeg(a)
        | SymExpr::FAbs(a)
        | SymExpr::FSqrt(a)
        | SymExpr::FIsNan(a)
        | SymExpr::Extract { expr: a, .. } => stack.push(a.as_ref()),
        SymExpr::Add(a, b)
        | SymExpr::Sub(a, b)
        | SymExpr::Mul(a, b)
        | SymExpr::Udiv(a, b)
        | SymExpr::Urem(a, b)
        | SymExpr::Sdiv(a, b)
        | SymExpr::Srem(a, b)
        | SymExpr::Smod(a, b)
        | SymExpr::And(a, b)
        | SymExpr::Or(a, b)
        | SymExpr::Xor(a, b)
        | SymExpr::Shl(a, b)
        | SymExpr::Lshr(a, b)
        | SymExpr::Ashr(a, b)
        | SymExpr::Eq(a, b)
        | SymExpr::Neq(a, b)
        | SymExpr::Ult(a, b)
        | SymExpr::Ule(a, b)
        | SymExpr::Slt(a, b)
        | SymExpr::Sle(a, b)
        | SymExpr::Sgt(a, b)
        | SymExpr::FAdd(a, b)
        | SymExpr::FSub(a, b)
        | SymExpr::FMul(a, b)
        | SymExpr::FDiv(a, b)
        | SymExpr::FEq(a, b)
        | SymExpr::FNeq(a, b)
        | SymExpr::FLt(a, b)
        | SymExpr::FLe(a, b)
        | SymExpr::Concat(a, b)
        | SymExpr::ArraySelect { array: a, index: b } => {
            stack.push(b.as_ref());
            stack.push(a.as_ref());
        }
        SymExpr::Ite {
            cond: a,
            t: b,
            f: c,
        }
        | SymExpr::ArrayStore {
            array: a,
            index: b,
            value: c,
        } => {
            stack.push(c.as_ref());
            stack.push(b.as_ref());
            stack.push(a.as_ref());
        }
    }
}

impl ArrayTheory {
    pub fn new() -> Self {
        Self {
            terms: Vec::new(),
            term_to_id: HashMap::new(),
            parent_selects: HashMap::new(),
            select_order: Vec::new(),
            stores: HashMap::new(),
            processed_terms: HashSet::new(),
            supported_scalar_cache: HashMap::new(),
            emitted_lemmas: HashSet::new(),
            pending_lemmas: Vec::new(),
            pending_cnf_lemmas: Vec::new(),
            max_lemmas: DEFAULT_MAX_ARRAY_LEMMAS,
            stats: ArrayTheoryStats::default(),
        }
    }

    #[must_use]
    pub fn stats(&self) -> ArrayTheoryStats {
        self.stats
    }

    pub fn set_max_lemmas(&mut self, max_lemmas: usize) {
        self.max_lemmas = max_lemmas;
    }

    #[must_use]
    pub fn max_lemmas(&self) -> usize {
        self.max_lemmas
    }

    #[must_use]
    pub fn budget_exceeded(&self) -> bool {
        self.stats.budget_exceeded
    }

    fn intern_owned(&mut self, expr: SymExpr) -> ArrayTermId {
        if let Some(&id) = self.term_to_id.get(&expr) {
            self.stats.term_cache_hits += 1;
            return id;
        }
        let id = ArrayTermId(u32::try_from(self.terms.len()).unwrap_or(u32::MAX));
        self.terms.push(expr.clone());
        self.term_to_id.insert(expr, id);
        self.stats.terms_interned += 1;
        id
    }

    fn intern_ref(&mut self, expr: &SymExpr) -> ArrayTermId {
        if let Some(&id) = self.term_to_id.get(expr) {
            self.stats.term_cache_hits += 1;
            return id;
        }
        self.intern_owned(expr.clone())
    }

    fn is_supported_scalar_base(&mut self, base_id: ArrayTermId) -> bool {
        if let Some(&cached) = self.supported_scalar_cache.get(&base_id) {
            return cached;
        }
        let ok = has_supported_scalar_array_sort(&self.terms[base_id.0 as usize]);
        self.supported_scalar_cache.insert(base_id, ok);
        ok
    }

    fn emit_lemma(&mut self, lemma: SymExpr) {
        if self.emitted_lemmas.contains(&lemma) {
            return;
        }
        if self.stats.lemmas_emitted as usize >= self.max_lemmas {
            self.stats.budget_exceeded = true;
            return;
        }
        self.emitted_lemmas.insert(lemma.clone());
        self.pending_lemmas.push(lemma);
        self.stats.lemmas_emitted += 1;
    }

    /// Enqueue a CNF clause to be surfaced by [`Theory::check`].
    pub fn push_cnf_lemma(&mut self, clause: Vec<Lit>) {
        self.pending_cnf_lemmas.push(clause);
    }

    /// All unique `ArraySelect` terms observed (including intermediate selects
    /// synthesized while unwinding store chains).
    pub fn select_terms(&self) -> impl Iterator<Item = &SymExpr> {
        self.select_order
            .iter()
            .map(|id| &self.terms[id.0 as usize])
    }

    pub(crate) fn has_new_terms(&self, expr: &SymExpr) -> bool {
        let mut seen_ptrs = HashSet::new();
        let mut stack = vec![expr];
        while let Some(node) = stack.pop() {
            if !seen_ptrs.insert(std::ptr::from_ref(node)) {
                continue;
            }
            if matches!(
                node,
                SymExpr::ArraySelect { .. } | SymExpr::ArrayStore { .. }
            ) {
                let processed = self
                    .term_to_id
                    .get(node)
                    .is_some_and(|id| self.processed_terms.contains(id));
                if !processed {
                    return true;
                }
            }
            push_children(node, &mut stack);
        }
        false
    }

    /// Recursively (iteratively via worklist) register every `ArraySelect` and
    /// `ArrayStore` within `expr` and materialize read-over-write and
    /// functional-consistency lemmas using interned term IDs.
    pub fn add_expr(&mut self, expr: &SymExpr) {
        let mut worklist = VecDeque::new();
        let mut seen_ptrs = HashSet::new();
        let mut stack = vec![expr];
        while let Some(node) = stack.pop() {
            if !seen_ptrs.insert(std::ptr::from_ref(node)) {
                continue;
            }
            self.stats.expr_visits += 1;
            if matches!(
                node,
                SymExpr::ArraySelect { .. } | SymExpr::ArrayStore { .. }
            ) {
                let id = self.intern_ref(node);
                worklist.push_back(id);
            }
            push_children(node, &mut stack);
        }

        while let Some(term_id) = worklist.pop_front() {
            if self.stats.budget_exceeded {
                break;
            }
            if !self.processed_terms.insert(term_id) {
                continue;
            }
            let term = self.terms[term_id.0 as usize].clone();
            match &term {
                SymExpr::ArraySelect { array, index } => {
                    let base_id = self.intern_ref(array);
                    if !self.is_supported_scalar_base(base_id) {
                        continue;
                    }
                    let existing_selects = self
                        .parent_selects
                        .get(&base_id)
                        .cloned()
                        .unwrap_or_default();
                    for other_id in existing_selects {
                        let other_sel = self.terms[other_id.0 as usize].clone();
                        if let SymExpr::ArraySelect {
                            index: other_idx, ..
                        } = &other_sel
                        {
                            // Functional consistency (Ackermann): index == other_idx => sel == other_sel
                            let neq_idx = SymExpr::Neq(index.clone(), other_idx.clone());
                            let eq_val = SymExpr::Eq(Box::new(term.clone()), Box::new(other_sel));
                            self.emit_lemma(SymExpr::Or(Box::new(neq_idx), Box::new(eq_val)));
                        }
                    }
                    let selects = self.parent_selects.entry(base_id).or_default();
                    if !selects.contains(&term_id) {
                        selects.push(term_id);
                        self.select_order.push(term_id);
                    }

                    // Direct Read-over-Write unwinding when reading from an ArrayStore or Ite.
                    match array.as_ref() {
                        SymExpr::ArrayStore {
                            array: inner,
                            index: store_idx,
                            value,
                        } => {
                            let neq_idx = SymExpr::Neq(index.clone(), store_idx.clone());
                            let eq_store_val = SymExpr::Eq(Box::new(term.clone()), value.clone());
                            self.emit_lemma(SymExpr::Or(Box::new(neq_idx), Box::new(eq_store_val)));

                            let inner_sel = SymExpr::ArraySelect {
                                array: inner.clone(),
                                index: index.clone(),
                            };
                            let eq_idx = SymExpr::Eq(index.clone(), store_idx.clone());
                            let eq_inner =
                                SymExpr::Eq(Box::new(term.clone()), Box::new(inner_sel.clone()));
                            self.emit_lemma(SymExpr::Or(Box::new(eq_idx), Box::new(eq_inner)));
                            let inner_sel_id = self.intern_owned(inner_sel);
                            worklist.push_back(inner_sel_id);
                        }
                        SymExpr::Ite { cond, t, f } => {
                            let sel_t = SymExpr::ArraySelect {
                                array: t.clone(),
                                index: index.clone(),
                            };
                            let sel_f = SymExpr::ArraySelect {
                                array: f.clone(),
                                index: index.clone(),
                            };
                            let ite_val = SymExpr::Ite {
                                cond: cond.clone(),
                                t: Box::new(sel_t.clone()),
                                f: Box::new(sel_f.clone()),
                            };
                            self.emit_lemma(SymExpr::Eq(Box::new(term.clone()), Box::new(ite_val)));
                            let sel_t_id = self.intern_owned(sel_t);
                            let sel_f_id = self.intern_owned(sel_f);
                            worklist.push_back(sel_t_id);
                            worklist.push_back(sel_f_id);
                        }
                        _ => {}
                    }

                    // Also instantiate Read-over-Write against any Store recorded on this base.
                    if let Some(stores) = self.stores.get(&base_id).cloned() {
                        for st_id in stores {
                            let st = self.terms[st_id.0 as usize].clone();
                            if let SymExpr::ArrayStore {
                                index: store_idx, ..
                            } = &st
                            {
                                let eq_idx = SymExpr::Eq(store_idx.clone(), index.clone());
                                let sel_store = SymExpr::ArraySelect {
                                    array: Box::new(st),
                                    index: index.clone(),
                                };
                                let eq_val = SymExpr::Eq(
                                    Box::new(sel_store.clone()),
                                    Box::new(term.clone()),
                                );
                                self.emit_lemma(SymExpr::Or(Box::new(eq_idx), Box::new(eq_val)));
                                let sel_store_id = self.intern_owned(sel_store);
                                worklist.push_back(sel_store_id);
                            }
                        }
                    }
                }
                SymExpr::ArrayStore {
                    array,
                    index,
                    value,
                } => {
                    let base_id = self.intern_ref(array);
                    if !self.is_supported_scalar_base(base_id) {
                        continue;
                    }
                    let stores = self.stores.entry(base_id).or_default();
                    if !stores.contains(&term_id) {
                        stores.push(term_id);
                    }

                    // Axiom 1: select(store(a, i, v), i) == v
                    let sel = SymExpr::ArraySelect {
                        array: Box::new(term.clone()),
                        index: index.clone(),
                    };
                    self.emit_lemma(SymExpr::Eq(Box::new(sel), value.clone()));

                    // Axiom 2: for every known select on this base, preserve untouched slots.
                    if let Some(selects) = self.parent_selects.get(&base_id).cloned() {
                        for sel_id in selects {
                            let sel_expr = self.terms[sel_id.0 as usize].clone();
                            if let SymExpr::ArraySelect { index: j, .. } = &sel_expr {
                                let eq_idx = SymExpr::Eq(index.clone(), j.clone());
                                let sel_store = SymExpr::ArraySelect {
                                    array: Box::new(term.clone()),
                                    index: j.clone(),
                                };
                                let eq_val = SymExpr::Eq(Box::new(sel_store), Box::new(sel_expr));
                                self.emit_lemma(SymExpr::Or(Box::new(eq_idx), Box::new(eq_val)));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Retrieve and clear pending lemmas.
    pub fn take_lemmas(&mut self) -> Vec<SymExpr> {
        std::mem::take(&mut self.pending_lemmas)
    }
}

impl Theory for ArrayTheory {
    fn check(&mut self, _assignments: &[Lit]) -> TheoryStatus {
        if self.pending_cnf_lemmas.is_empty() {
            TheoryStatus::Satisfied
        } else {
            TheoryStatus::Lemmas(std::mem::take(&mut self.pending_cnf_lemmas))
        }
    }
}
