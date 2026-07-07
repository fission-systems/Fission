use crate::ast::SymExpr;
use std::collections::HashMap;

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

/// An And-Inverter Graph manager for converting ASTs.
pub struct AigManager {
    nodes: Vec<AigNode>,
    /// Structural hashing to deduplicate AND nodes
    strash: HashMap<(AigLit, AigLit), u32>,
    /// Maps AST Node ID to its vector of AIG literals (one per bit)
    var_map: HashMap<u32, Vec<AigLit>>,
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
        }
    }

    /// Create a new variable of `size` bits.
    pub fn add_var(&mut self, ast_id: u32, size: u32) -> Vec<AigLit> {
        let mut bits = Vec::with_capacity(size as usize);
        for _ in 0..size {
            let idx = self.nodes.len() as u32 + 1;
            self.nodes.push(AigNode::Var(idx));
            bits.push(AigLit::new(idx, false));
        }
        self.var_map.insert(ast_id, bits.clone());
        bits
    }

    /// Return the AIG literal bits for a previously registered AST variable, if known.
    pub fn get_var_bits(&self, ast_id: u32) -> Option<&Vec<AigLit>> {
        self.var_map.get(&ast_id)
    }

    /// Add an AND gate, returning the literal. Applies structural hashing.
    pub fn add_and(&mut self, mut a: AigLit, mut b: AigLit) -> AigLit {
        if a == AigLit::FALSE || b == AigLit::FALSE {
            return AigLit::FALSE;
        }
        if a == AigLit::TRUE { return b; }
        if b == AigLit::TRUE { return a; }
        if a == b { return a; }
        if a == b.not() { return AigLit::FALSE; }

        // Canonicalize ordering
        if a.0 > b.0 {
            std::mem::swap(&mut a, &mut b);
        }

        let key = (a, b);
        if let Some(&idx) = self.strash.get(&key) {
            return AigLit::new(idx, false);
        }

        let idx = self.nodes.len() as u32 + 1;
        self.nodes.push(AigNode::And(a, b));
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

    pub fn add_ripple_carry_adder(&mut self, a_bits: &[AigLit], b_bits: &[AigLit], cin: AigLit) -> Vec<AigLit> {
        let len = std::cmp::max(a_bits.len(), b_bits.len());
        let mut sum_bits = Vec::with_capacity(len);
        let mut carry = cin;
        for i in 0..len {
            let a = a_bits.get(i).copied().unwrap_or(AigLit::FALSE);
            let b = b_bits.get(i).copied().unwrap_or(AigLit::FALSE);
            let (sum, next_carry) = self.add_full_adder(a, b, carry);
            sum_bits.push(sum);
            carry = next_carry;
        }
        sum_bits
    }

    /// Lower a SymExpr into a vector of AigLits (one per bit, LSB first).
    pub fn lower_expr(&mut self, expr: &SymExpr) -> Vec<AigLit> {
        match expr {
            SymExpr::Const { val, size } => {
                let mut bits = Vec::with_capacity(*size as usize);
                for i in 0..*size {
                    bits.push(if (val & (1 << i)) != 0 { AigLit::TRUE } else { AigLit::FALSE });
                }
                bits
            }
            SymExpr::Var { id, size, .. } => {
                if let Some(bits) = self.var_map.get(id) {
                    bits.clone()
                } else {
                    self.add_var(*id, *size)
                }
            }
            SymExpr::And(a, b) => {
                let a_bits = self.lower_expr(a);
                let b_bits = self.lower_expr(b);
                let len = std::cmp::max(a_bits.len(), b_bits.len());
                let mut out = Vec::with_capacity(len);
                for i in 0..len {
                    let ax = a_bits.get(i).copied().unwrap_or(AigLit::FALSE);
                    let bx = b_bits.get(i).copied().unwrap_or(AigLit::FALSE);
                    out.push(self.add_and(ax, bx));
                }
                out
            }
            SymExpr::Or(a, b) => {
                let a_bits = self.lower_expr(a);
                let b_bits = self.lower_expr(b);
                let len = std::cmp::max(a_bits.len(), b_bits.len());
                let mut out = Vec::with_capacity(len);
                for i in 0..len {
                    let ax = a_bits.get(i).copied().unwrap_or(AigLit::FALSE);
                    let bx = b_bits.get(i).copied().unwrap_or(AigLit::FALSE);
                    out.push(self.add_or(ax, bx));
                }
                out
            }
            SymExpr::Xor(a, b) => {
                let a_bits = self.lower_expr(a);
                let b_bits = self.lower_expr(b);
                let len = std::cmp::max(a_bits.len(), b_bits.len());
                let mut out = Vec::with_capacity(len);
                for i in 0..len {
                    let ax = a_bits.get(i).copied().unwrap_or(AigLit::FALSE);
                    let bx = b_bits.get(i).copied().unwrap_or(AigLit::FALSE);
                    out.push(self.add_xor(ax, bx));
                }
                out
            }
            SymExpr::Add(a, b) => {
                let a_bits = self.lower_expr(a);
                let b_bits = self.lower_expr(b);
                self.add_ripple_carry_adder(&a_bits, &b_bits, AigLit::FALSE)
            }
            SymExpr::Sub(a, b) => {
                let a_bits = self.lower_expr(a);
                let b_bits = self.lower_expr(b);
                let b_inv: Vec<AigLit> = b_bits.into_iter().map(|lit| lit.not()).collect();
                // A - B = A + (!B) + 1
                self.add_ripple_carry_adder(&a_bits, &b_inv, AigLit::TRUE)
            }
            SymExpr::Eq(a, b) => {
                let a_bits = self.lower_expr(a);
                let b_bits = self.lower_expr(b);
                vec![self.add_eq(&a_bits, &b_bits)]
            }
            SymExpr::Neq(a, b) => {
                let a_bits = self.lower_expr(a);
                let b_bits = self.lower_expr(b);
                vec![self.add_neq(&a_bits, &b_bits)]
            }
            // For other operations, we'd add multipliers etc.
            // Scaffolding handles basic bitwise and arithmetic.
            _ => {
                tracing::warn!("Unsupported AIG lowering for {:?}", expr);
                vec![AigLit::FALSE; expr.get_size() as usize]
            }
        }
    }

    /// Converts the entire AIG into a CNF formula.
    pub fn to_cnf(&self, cnf: &mut crate::cnf::CnfBuilder) {
        for (i, node) in self.nodes.iter().enumerate() {
            let idx = (i + 1) as u32;
            if let AigNode::And(a, b) = node {
                cnf.add_and_gate(idx, *a, *b);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::SymExpr;
    use crate::cnf::CnfBuilder;
    use crate::sat::SatSolver;

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
        let six  = SymExpr::new_const(6, 8);
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
        let x    = SymExpr::new_var("x", 8);
        let five = SymExpr::new_const(5, 8);
        let ten  = SymExpr::new_const(10, 8);
        let eq   = SymExpr::new_eq(SymExpr::new_add(x, five), ten);
        assert!(check_sat(eq));
    }

    #[test]
    fn test_sub_eq_sat() {
        // x - 3 == 7  =>  SAT (x = 10)
        let x     = SymExpr::new_var("x", 8);
        let three = SymExpr::new_const(3, 8);
        let seven = SymExpr::new_const(7, 8);
        let eq    = SymExpr::new_eq(SymExpr::new_sub(x, three), seven);
        assert!(check_sat(eq));
    }

    #[test]
    fn test_add_const_unsat() {
        // 5 + 5 == 6  =>  constant-folds to 10 == 6 => Const{val:0} => UNSAT
        let five_a = SymExpr::new_const(5, 8);
        let five_b = SymExpr::new_const(5, 8);
        let six    = SymExpr::new_const(6, 8);
        let sum    = SymExpr::new_add(five_a, five_b);
        let eq     = SymExpr::new_eq(sum, six);
        assert!(!check_sat(eq));
    }

    #[test]
    fn test_neq_sat() {
        // x != y  =>  SAT (assign x=0, y=1)
        let x = SymExpr::new_var("x", 8);
        let y = SymExpr::new_var("y", 8);
        assert!(check_sat(SymExpr::new_neq(x, y)));
    }
}
