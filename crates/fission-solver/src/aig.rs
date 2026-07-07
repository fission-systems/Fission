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
                a_bits.into_iter().zip(b_bits).map(|(x, y)| self.add_and(x, y)).collect()
            }
            SymExpr::Xor(a, b) => {
                let a_bits = self.lower_expr(a);
                let b_bits = self.lower_expr(b);
                a_bits.into_iter().zip(b_bits).map(|(x, y)| self.add_xor(x, y)).collect()
            }
            // For other operations, we'd add adders, subtractors, multipliers etc.
            // Scaffolding only handles basic bitwise for now.
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
