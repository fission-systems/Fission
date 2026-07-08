use crate::ast::SymExpr;
use crate::theory::{Theory, TheoryStatus};
use crate::cnf::Lit;

use std::collections::{HashMap, HashSet};

pub struct ArrayTheory {
    /// Maps an Array base expression to its Selects
    parent_selects: HashMap<SymExpr, HashSet<SymExpr>>,
    /// Maps an Array base expression to its Stores
    stores: HashMap<SymExpr, HashSet<SymExpr>>,
    /// Accumulated lemmas to be injected into the SAT solver
    pending_lemmas: Vec<SymExpr>,
}

impl ArrayTheory {
    pub fn new() -> Self {
        Self {
            parent_selects: HashMap::new(),
            stores: HashMap::new(),
            pending_lemmas: Vec::new(),
        }
    }
    
    /// Register an expression with the Array Theory.
    /// If it is a Select or Store, we track it and eagerly generate required axioms.
    pub fn add_expr(&mut self, expr: &SymExpr) {
        match expr {
            SymExpr::ArraySelect { array, index } => {
                let base = *array.clone();
                self.parent_selects.entry(base.clone()).or_default().insert(expr.clone());
                
                // Axiom 2: for every known store on this base, if i != j, select(store, j) == select(base, j)
                if let Some(stores) = self.stores.get(&base) {
                    for st in stores {
                        if let SymExpr::ArrayStore { index: i, value: _, .. } = st {
                            // Lemma: i == j || select(store(a, i, v), j) == select(a, j)
                            let eq_idx = SymExpr::Eq(i.clone(), index.clone());
                            let sel_store = SymExpr::ArraySelect { array: Box::new(st.clone()), index: index.clone() };
                            let eq_val = SymExpr::Eq(Box::new(sel_store), Box::new(expr.clone()));
                            let lemma = SymExpr::Or(Box::new(eq_idx), Box::new(eq_val));
                            self.pending_lemmas.push(lemma);
                        }
                    }
                }
            }
            SymExpr::ArrayStore { array, index, value } => {
                let base = *array.clone();
                self.stores.entry(base.clone()).or_default().insert(expr.clone());
                
                // Axiom 1: select(store(a, i, v), i) == v
                let sel = SymExpr::ArraySelect { array: Box::new(expr.clone()), index: index.clone() };
                let lemma1 = SymExpr::Eq(Box::new(sel), value.clone());
                self.pending_lemmas.push(lemma1);

                // Axiom 2: for every known select on this base
                if let Some(selects) = self.parent_selects.get(&base) {
                    for sel_expr in selects {
                        if let SymExpr::ArraySelect { index: j, .. } = sel_expr {
                            let eq_idx = SymExpr::Eq(index.clone(), j.clone());
                            let sel_store = SymExpr::ArraySelect { array: Box::new(expr.clone()), index: j.clone() };
                            let eq_val = SymExpr::Eq(Box::new(sel_store), Box::new(sel_expr.clone()));
                            let lemma = SymExpr::Or(Box::new(eq_idx), Box::new(eq_val));
                            self.pending_lemmas.push(lemma);
                        }
                    }
                }
            }
            _ => {} // Not an array operation
        }
    }

    /// Retrieve and clear pending lemmas.
    pub fn take_lemmas(&mut self) -> Vec<SymExpr> {
        std::mem::take(&mut self.pending_lemmas)
    }
}

impl Theory for ArrayTheory {
    fn check(&mut self, _assignments: &[Lit]) -> TheoryStatus {
        // In a true lazy DPLL(T) solver, we would only instantiate Axiom 2 if the current assignment 
        // implies the indices are distinct. For this scaffolding, we emit them eagerly as AST nodes.
        TheoryStatus::Satisfied
    }
}
