use crate::cnf::{Clause, Lit};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LBool {
    True,
    False,
    Undef,
}

impl LBool {
    pub fn not(self) -> Self {
        match self {
            LBool::True => LBool::False,
            LBool::False => LBool::True,
            LBool::Undef => LBool::Undef,
        }
    }
}

#[derive(Debug, Clone)]
struct VarData {
    reason: Option<usize>, // index of the clause that forced this assignment
    level: u32,
}

#[derive(Debug, Clone)]
pub struct Watcher {
    pub clause_idx: usize,
    pub blocker: Lit, // used for fast check without accessing the clause
}

/// A Conflict-Driven Clause Learning (CDCL) SAT Solver core.
pub struct SatSolver {
    /// The formula clauses (original + learned)
    pub clauses: Vec<Clause>,
    
    /// Value assignment for each variable (1-indexed)
    assigns: Vec<LBool>,
    /// Meta-data for each variable (reason, level)
    vardata: Vec<VarData>,
    
    /// Watch lists: lit.index() -> list of clauses watching this literal.
    /// A clause watches 2 literals. If one becomes false, we must find another or propagate.
    watches: Vec<Vec<Watcher>>,
    
    /// Assignment trail (stack of assigned literals)
    trail: Vec<Lit>,
    /// Indices into the trail marking the start of each decision level
    trail_lim: Vec<usize>,
    
    /// Index in `trail` of the next literal to propagate
    qhead: usize,
    
    /// VSIDS Variable Activity (for decision heuristic)
    activity: Vec<f64>,
    var_inc: f64,
}

impl Default for SatSolver {
    fn default() -> Self {
        Self::new()
    }
}

impl SatSolver {
    pub fn new() -> Self {
        Self {
            clauses: vec![],
            assigns: vec![LBool::Undef], // 0 is unused
            vardata: vec![VarData { reason: None, level: 0 }],
            watches: vec![],
            trail: vec![],
            trail_lim: vec![],
            qhead: 0,
            activity: vec![0.0],
            var_inc: 1.0,
        }
    }

    /// Ensure internal structures are large enough for the given variable.
    fn ensure_var(&mut self, var: u32) {
        let var = var as usize;
        while self.assigns.len() <= var {
            self.assigns.push(LBool::Undef);
            self.vardata.push(VarData { reason: None, level: 0 });
            self.activity.push(0.0);
            
            // Watch lists for var and !var
            self.watches.push(vec![]);
            self.watches.push(vec![]);
        }
    }

    pub fn value_lit(&self, lit: Lit) -> LBool {
        let val = self.assigns[lit.var() as usize];
        if lit.0 < 0 {
            val.not()
        } else {
            val
        }
    }

    pub fn decision_level(&self) -> u32 {
        self.trail_lim.len() as u32
    }

    fn enqueue(&mut self, lit: Lit, reason: Option<usize>) -> bool {
        let val = self.value_lit(lit);
        if val != LBool::Undef {
            return val == LBool::True;
        }

        let var = lit.var() as usize;
        self.assigns[var] = if lit.0 > 0 { LBool::True } else { LBool::False };
        self.vardata[var] = VarData {
            reason,
            level: self.decision_level(),
        };
        self.trail.push(lit);
        true
    }

    /// Add a clause to the solver. Returns false if the formula becomes trivially UNSAT.
    pub fn add_clause(&mut self, mut lits: Vec<Lit>) -> bool {
        for lit in &lits {
            self.ensure_var(lit.var());
        }

        if self.decision_level() != 0 {
            tracing::warn!("Adding clauses during search is not fully supported yet.");
        }

        // Simplify clause (remove duplicates, handle True/False if any)
        lits.sort_by_key(|l| l.0);
        lits.dedup();
        
        // Check for tautology (contains lit and !lit)
        for i in 0..lits.len() {
            for j in (i+1)..lits.len() {
                if lits[i].0 == -lits[j].0 {
                    return true; // Trivial true
                }
            }
        }

        if lits.is_empty() {
            return false;
        } else if lits.len() == 1 {
            return self.enqueue(lits[0], None);
        }

        let c_idx = self.clauses.len();
        let lit0 = lits[0];
        let lit1 = lits[1];
        
        self.clauses.push(Clause(lits));
        
        self.watches[lit0.not().index()].push(Watcher { clause_idx: c_idx, blocker: lit1 });
        self.watches[lit1.not().index()].push(Watcher { clause_idx: c_idx, blocker: lit0 });
        
        true
    }

    /// Unit propagation (Boolean Constraint Propagation).
    /// Returns Some(clause_idx) if a conflict is found, otherwise None.
    fn propagate(&mut self) -> Option<usize> {
        while self.qhead < self.trail.len() {
            let p = self.trail[self.qhead];
            self.qhead += 1;
            
            // `p` became true, so `!p` became false. Clauses watching `!p` need to find a new watcher.
            let false_lit = p.not();
            let mut i = 0;
            
            // We use standard index trickery to remove elements while iterating
            // Since borrow checker prevents mutable access to self.watches and self.clauses simultaneously,
            // we will drain the list and re-add.
            let mut ws = std::mem::take(&mut self.watches[false_lit.index()]);
            let mut j = 0;
            let mut conflict = None;
            
            while i < ws.len() {
                let w = ws[i].clone();
                i += 1;
                
                if self.value_lit(w.blocker) == LBool::True {
                    ws[j] = w;
                    j += 1;
                    continue;
                }
                
                let c_idx = w.clause_idx;
                let mut c0 = self.clauses[c_idx].0[0];
                let mut c1 = self.clauses[c_idx].0[1];
                
                // Ensure false_lit is at index 1
                if c0 == false_lit {
                    c0 = c1;
                    c1 = false_lit;
                    self.clauses[c_idx].0[0] = c0;
                    self.clauses[c_idx].0[1] = c1;
                }
                
                // If c0 is true, blocker was just wrong, update it and continue.
                if self.value_lit(c0) == LBool::True {
                    let mut new_w = w.clone();
                    new_w.blocker = c0;
                    ws[j] = new_w;
                    j += 1;
                    continue;
                }
                
                // Look for a new literal to watch
                let mut found_new_watch = false;
                let c_len = self.clauses[c_idx].0.len();
                for k in 2..c_len {
                    let ck = self.clauses[c_idx].0[k];
                    if self.value_lit(ck) != LBool::False {
                        // Found new watch!
                        self.clauses[c_idx].0[1] = ck;
                        self.clauses[c_idx].0[k] = false_lit;
                        self.watches[ck.not().index()].push(Watcher {
                            clause_idx: c_idx,
                            blocker: c0,
                        });
                        found_new_watch = true;
                        break;
                    }
                }
                
                if found_new_watch {
                    continue; // we didn't keep it in the current list
                }
                
                // Could not find a new watch. This means clause is unit or empty (conflict).
                ws[j] = w.clone();
                j += 1;
                
                if self.value_lit(c0) == LBool::False {
                    // CONFLICT
                    conflict = Some(w.clause_idx);
                    
                    // Copy remaining watches back
                    while i < ws.len() {
                        ws[j] = ws[i].clone();
                        j += 1;
                        i += 1;
                    }
                    break;
                } else {
                    // UNIT: Enqueue the first literal
                    self.enqueue(c0, Some(w.clause_idx));
                }
            }
            
            ws.truncate(j);
            self.watches[false_lit.index()] = ws;
            
            if conflict.is_some() {
                return conflict;
            }
        }
        None
    }

    /// Analyze a conflict to find a learned clause and a backtrack level (1UIP).
    fn analyze(&mut self, mut confl: usize) -> (Vec<Lit>, u32) {
        let mut learned = vec![Lit(0)]; // Placeholder for the asserting literal
        let mut path_c = 0;
        let mut seen = vec![false; self.assigns.len()];
        let mut p = Lit(0);
        let mut idx = self.trail.len() - 1;
        let mut backtrack_level = 0;

        loop {
            let lits = self.clauses[confl].0.clone();
            // Iterate all literals in the reason clause except the one that is true (if p!=0)
            let start = if p.0 == 0 { 0 } else { 1 };
            
            for j in start..lits.len() {
                let q = lits[j];
                let v = q.var() as usize;
                
                if !seen[v] && self.vardata[v].level > 0 {
                    self.var_bump_activity(v);
                    seen[v] = true;
                    if self.vardata[v].level >= self.decision_level() {
                        path_c += 1;
                    } else {
                        learned.push(q);
                        if self.vardata[v].level > backtrack_level {
                            backtrack_level = self.vardata[v].level;
                        }
                    }
                }
            }

            // Select next literal to look at
            loop {
                p = self.trail[idx];
                idx -= 1;
                if seen[p.var() as usize] {
                    break;
                }
            }

            seen[p.var() as usize] = false;
            path_c -= 1;
            
            if path_c == 0 {
                break;
            }
            confl = self.vardata[p.var() as usize].reason.unwrap();
        }

        learned[0] = p.not();
        
        // Decay activities
        self.var_decay_activity();

        if learned.len() == 1 {
            backtrack_level = 0;
        }

        (learned, backtrack_level)
    }

    fn cancel_until(&mut self, level: u32) {
        if self.decision_level() > level {
            let limit = self.trail_lim[level as usize];
            for c in (limit..self.trail.len()).rev() {
                let v = self.trail[c].var() as usize;
                self.assigns[v] = LBool::Undef;
                self.vardata[v].reason = None;
            }
            self.trail.truncate(limit);
            self.trail_lim.truncate(level as usize);
            self.qhead = self.trail.len();
        }
    }

    fn var_bump_activity(&mut self, var: usize) {
        self.activity[var] += self.var_inc;
        if self.activity[var] > 1e100 {
            // Rescale
            for act in &mut self.activity {
                *act *= 1e-100;
            }
            self.var_inc *= 1e-100;
        }
    }

    fn var_decay_activity(&mut self) {
        self.var_inc *= 1.05; // 1 / 0.95
    }

    fn pick_branch_lit(&self) -> Option<Lit> {
        let mut best_var = 0;
        let mut best_act = -1.0;
        
        // Extremely naive O(N) VSIDS pick for scaffolding.
        // A production solver would use a priority queue (Binary Heap).
        for (v, &val) in self.assigns.iter().enumerate().skip(1) {
            if val == LBool::Undef {
                if self.activity[v] > best_act {
                    best_act = self.activity[v];
                    best_var = v;
                }
            }
        }
        
        if best_var == 0 {
            None
        } else {
            // By default, guess false. (Can be optimized with Phase Saving)
            Some(Lit::new(best_var as u32, true)) // inverted = true -> value False
        }
    }

    pub fn solve(&mut self) -> bool {
        // Initial BCP
        if self.propagate().is_some() {
            return false;
        }

        loop {
            if let Some(confl) = self.propagate() {
                // Conflict
                if self.decision_level() == 0 {
                    return false; // Root level conflict -> UNSAT
                }
                
                let (learned_clause, backtrack_level) = self.analyze(confl);
                self.cancel_until(backtrack_level);
                
                if learned_clause.len() == 1 {
                    self.enqueue(learned_clause[0], None);
                } else {
                    let c_idx = self.clauses.len();
                    
                    // We must watch the first two literals. The first is the asserting literal.
                    // The second must be one with the highest decision level (which is backtrack_level).
                    let mut max_i = 1;
                    let mut max_lvl = self.vardata[learned_clause[1].var() as usize].level;
                    for i in 2..learned_clause.len() {
                        let lvl = self.vardata[learned_clause[i].var() as usize].level;
                        if lvl > max_lvl {
                            max_lvl = lvl;
                            max_i = i;
                        }
                    }
                    
                    let mut final_clause = learned_clause.clone();
                    final_clause.swap(1, max_i);
                    
                    let lit0 = final_clause[0];
                    let lit1 = final_clause[1];
                    
                    self.clauses.push(Clause(final_clause));
                    
                    self.watches[lit0.not().index()].push(Watcher { clause_idx: c_idx, blocker: lit1 });
                    self.watches[lit1.not().index()].push(Watcher { clause_idx: c_idx, blocker: lit0 });
                    
                    // BCP will flip the asserting literal
                    self.enqueue(lit0, Some(c_idx));
                }
            } else {
                // No conflict, make a decision
                if let Some(next_lit) = self.pick_branch_lit() {
                    self.trail_lim.push(self.trail.len());
                    self.enqueue(next_lit, None);
                } else {
                    // All variables assigned -> SAT!
                    return true;
                }
            }
        }
    }
}
