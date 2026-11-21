use crate::ast::{Sort, SymExpr};
use crate::cnf::Lit;
use crate::theory::{Theory, TheoryStatus};

use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Clone)]
pub struct ArrayTheory {
    /// Maps an Array base expression to its Selects.
    parent_selects: HashMap<SymExpr, Vec<SymExpr>>,
    /// Every select in `parent_selects`, in the order first seen. The map's
    /// own iteration order is the hasher's, and the memory CEGAR loop walks
    /// the selects to emit lemmas -- so models varied from run to run.
    select_order: Vec<SymExpr>,
    /// Maps an Array base expression to its Stores.
    stores: HashMap<SymExpr, Vec<SymExpr>>,
    /// Select/Store terms already processed so repeated calls do not re-emit axioms.
    processed_terms: HashSet<SymExpr>,
    /// Deduplicates emitted AST lemmas.
    emitted_lemmas: HashSet<SymExpr>,
    /// Accumulated AST lemmas to be asserted into the bitvector solver.
    pending_lemmas: Vec<SymExpr>,
    /// CNF-level conflict/propagation clauses emitted via [`Theory::check`].
    pending_cnf_lemmas: Vec<Vec<Lit>>,
}

impl Default for ArrayTheory {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether `array` maps 1-to-64-bit indices to 1-to-64-bit elements: the
/// only arrays [`ArrayTheory`] writes axioms for.
pub(crate) fn has_supported_scalar_array_sort(array: &SymExpr) -> bool {
    matches!(
        array.get_sort(),
        Sort::Array { domain, range }
            if matches!(domain.as_ref(), Sort::BitVector(w) | Sort::Float(w) if (1..=64).contains(w))
                && matches!(range.as_ref(), Sort::BitVector(w) | Sort::Float(w) if (1..=64).contains(w))
    )
}

impl ArrayTheory {
    pub fn new() -> Self {
        Self {
            parent_selects: HashMap::new(),
            select_order: Vec::new(),
            stores: HashMap::new(),
            processed_terms: HashSet::new(),
            emitted_lemmas: HashSet::new(),
            pending_lemmas: Vec::new(),
            pending_cnf_lemmas: Vec::new(),
        }
    }

    fn emit_lemma(&mut self, lemma: SymExpr) {
        if self.emitted_lemmas.insert(lemma.clone()) {
            self.pending_lemmas.push(lemma);
        }
    }

    /// Enqueue a CNF clause to be surfaced by [`Theory::check`].
    pub fn push_cnf_lemma(&mut self, clause: Vec<Lit>) {
        self.pending_cnf_lemmas.push(clause);
    }

    /// All unique `ArraySelect` terms observed (including intermediate selects
    /// synthesized while unwinding store chains).
    pub fn select_terms(&self) -> impl Iterator<Item = &SymExpr> {
        self.select_order.iter()
    }

    pub(crate) fn has_new_terms(&self, expr: &SymExpr) -> bool {
        let mut found = false;
        expr.visit_preorder(&mut |node| {
            if matches!(
                node,
                SymExpr::ArraySelect { .. } | SymExpr::ArrayStore { .. }
            ) && !self.processed_terms.contains(node)
            {
                found = true;
            }
        });
        found
    }

    /// Recursively register every `ArraySelect` and `ArrayStore` within `expr`
    /// and materialize read-over-write and functional-consistency lemmas.
    pub fn add_expr(&mut self, expr: &SymExpr) {
        let mut worklist = VecDeque::new();
        expr.visit_preorder(&mut |node| {
            if matches!(
                node,
                SymExpr::ArraySelect { .. } | SymExpr::ArrayStore { .. }
            ) {
                worklist.push_back(node.clone());
            }
        });

        while let Some(term) = worklist.pop_front() {
            if !self.processed_terms.insert(term.clone()) {
                continue;
            }
            match &term {
                SymExpr::ArraySelect { array, index } => {
                    if !has_supported_scalar_array_sort(array) {
                        continue;
                    }
                    let base = array.as_ref().clone();
                    let existing_selects =
                        self.parent_selects.get(&base).cloned().unwrap_or_default();
                    for other_sel in existing_selects {
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
                    let selects = self.parent_selects.entry(base.clone()).or_default();
                    if !selects.contains(&term) {
                        selects.push(term.clone());
                        self.select_order.push(term.clone());
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
                            worklist.push_back(inner_sel);
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
                            worklist.push_back(sel_t);
                            worklist.push_back(sel_f);
                        }
                        _ => {}
                    }

                    // Also instantiate Read-over-Write against any Store recorded on this base.
                    if let Some(stores) = self.stores.get(&base) {
                        let stores_snapshot = stores.clone();
                        for st in stores_snapshot {
                            if let SymExpr::ArrayStore {
                                index: store_idx, ..
                            } = &st
                            {
                                let eq_idx = SymExpr::Eq(store_idx.clone(), index.clone());
                                let sel_store = SymExpr::ArraySelect {
                                    array: Box::new(st.clone()),
                                    index: index.clone(),
                                };
                                let eq_val = SymExpr::Eq(
                                    Box::new(sel_store.clone()),
                                    Box::new(term.clone()),
                                );
                                self.emit_lemma(SymExpr::Or(Box::new(eq_idx), Box::new(eq_val)));
                                worklist.push_back(sel_store);
                            }
                        }
                    }
                }
                SymExpr::ArrayStore {
                    array,
                    index,
                    value,
                } => {
                    if !has_supported_scalar_array_sort(array) {
                        continue;
                    }
                    let base = array.as_ref().clone();
                    let stores = self.stores.entry(base.clone()).or_default();
                    if !stores.contains(&term) {
                        stores.push(term.clone());
                    }

                    // Axiom 1: select(store(a, i, v), i) == v
                    let sel = SymExpr::ArraySelect {
                        array: Box::new(term.clone()),
                        index: index.clone(),
                    };
                    self.emit_lemma(SymExpr::Eq(Box::new(sel), value.clone()));

                    // Axiom 2: for every known select on this base, preserve untouched slots.
                    if let Some(selects) = self.parent_selects.get(&base) {
                        let selects_snapshot = selects.clone();
                        for sel_expr in selects_snapshot {
                            if let SymExpr::ArraySelect { index: j, .. } = &sel_expr {
                                let eq_idx = SymExpr::Eq(index.clone(), j.clone());
                                let sel_store = SymExpr::ArraySelect {
                                    array: Box::new(term.clone()),
                                    index: j.clone(),
                                };
                                let eq_val =
                                    SymExpr::Eq(Box::new(sel_store), Box::new(sel_expr.clone()));
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
