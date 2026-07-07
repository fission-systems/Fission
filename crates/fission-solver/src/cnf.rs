use crate::aig::AigLit;

/// A Literal in CNF is a signed integer. Positive means non-inverted, negative means inverted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Lit(pub i32);

impl Lit {
    pub fn new(var: u32, inverted: bool) -> Self {
        if inverted {
            Lit(-(var as i32))
        } else {
            Lit(var as i32)
        }
    }

    pub fn not(self) -> Self {
        Lit(-self.0)
    }

    pub fn var(self) -> u32 {
        self.0.unsigned_abs()
    }

    /// Returns a 0-indexed integer for this literal: 2 * (var - 1) + sign
    pub fn index(self) -> usize {
        let v = self.var();
        debug_assert!(v > 0);
        let idx = (v - 1) * 2;
        if self.0 < 0 {
            (idx + 1) as usize
        } else {
            idx as usize
        }
    }
}

/// A Clause is a disjunction (OR) of literals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clause(pub Vec<Lit>);

/// Converts AIG to CNF using the Tseitin transformation.
pub struct CnfBuilder {
    pub clauses: Vec<Clause>,
    /// Maps AigLit variable indices to CNF variable indices.
    var_map: Vec<u32>,
    next_var: u32,
}

impl Default for CnfBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl CnfBuilder {
    pub fn new() -> Self {
        Self {
            clauses: vec![],
            var_map: vec![],
            next_var: 1, // 0 is unused in DIMACS CNF
        }
    }

    fn get_cnf_var(&mut self, aig_idx: u32) -> u32 {
        while self.var_map.len() <= aig_idx as usize {
            self.var_map.push(0);
        }
        if self.var_map[aig_idx as usize] == 0 {
            self.var_map[aig_idx as usize] = self.next_var;
            self.next_var += 1;
        }
        self.var_map[aig_idx as usize]
    }

    fn get_lit(&mut self, aig_lit: AigLit) -> Lit {
        let var = self.get_cnf_var(aig_lit.index());
        Lit::new(var, aig_lit.is_inverted())
    }

    /// Add a unit clause asserting a literal is true.
    pub fn assert_lit(&mut self, lit: AigLit) {
        if lit == AigLit::TRUE { return; }
        if lit == AigLit::FALSE {
            // Trivially UNSAT: add an empty clause (∅ is always false)
            self.clauses.push(Clause(vec![]));
            return;
        }
        let cnf_lit = self.get_lit(lit);
        self.clauses.push(Clause(vec![cnf_lit]));
    }

    /// Adds constraints for an AND gate: out = a AND b
    /// Tseitin transformation:
    /// (out => a) AND (out => b) AND (a AND b => out)
    /// = (!out OR a) AND (!out OR b) AND (!a OR !b OR out)
    pub fn add_and_gate(&mut self, out_idx: u32, a_lit: AigLit, b_lit: AigLit) {
        let out = Lit::new(self.get_cnf_var(out_idx), false);
        let a = self.get_lit(a_lit);
        let b = self.get_lit(b_lit);

        self.clauses.push(Clause(vec![out.not(), a]));
        self.clauses.push(Clause(vec![out.not(), b]));
        self.clauses.push(Clause(vec![a.not(), b.not(), out]));
    }
}
