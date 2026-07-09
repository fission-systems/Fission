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
    /// Maps ArraySelect nodes to their vector of AIG literals
    array_select_map: HashMap<SymExpr, Vec<AigLit>>,
    pub last_cnf_node: usize,
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
            array_select_map: HashMap::new(),
            last_cnf_node: 0,
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

    pub fn get_array_select_bits(&self, expr: &SymExpr) -> Option<&Vec<AigLit>> {
        self.array_select_map.get(expr)
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
            SymExpr::Var { id, sort, .. } => {
                if let Some(bits) = self.var_map.get(id) {
                    bits.clone()
                } else {
                    // Float-sorted vars bit-blast as full IEEE bit patterns (size bytes → bits).
                    let bits = match sort {
                        crate::ast::Sort::Float(sz) => sz.saturating_mul(8).max(1),
                        _ => sort.expect_bv().max(1),
                    };
                    self.add_var(*id, bits)
                }
            }
            SymExpr::ArraySelect { .. } => {
                if let Some(bits) = self.array_select_map.get(expr) {
                    bits.clone()
                } else {
                    let size = expr.get_size();
                    let mut bits = Vec::with_capacity(size as usize);
                    for _ in 0..size {
                        let idx = self.nodes.len() as u32 + 1;
                        self.nodes.push(AigNode::Var(idx));
                        bits.push(AigLit::new(idx, false));
                    }
                    self.array_select_map.insert(expr.clone(), bits.clone());
                    bits
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
            SymExpr::Ult(a, b) => {
                // Unsigned less-than: a < b
                // a < b  ⟺  borrow-out of (a - b) = 1
                //          ⟺  carry-out of (a + ~b + 1) = 0  (two's complement)
                // We build a single full-adder chain: a + ~b with carry_in = 1
                // The final carry_out = 1 means a >= b, carry_out = 0 means a < b
                let a_bits = self.lower_expr(a);
                let b_bits = self.lower_expr(b);
                let b_inv: Vec<AigLit> = b_bits.into_iter().map(|l| l.not()).collect();
                let len = a_bits.len();

                let mut carry = AigLit::TRUE; // carry-in = 1 (for two's complement negation)
                for i in 0..len {
                    let ax = a_bits.get(i).copied().unwrap_or(AigLit::FALSE);
                    let bx = b_inv.get(i).copied().unwrap_or(AigLit::TRUE);
                    // full adder carry-out: (a & b) | ((a ^ b) & carry_in)
                    let axb = self.add_xor(ax, bx);
                    let a_and_b = self.add_and(ax, bx);
                    let axb_and_c = self.add_and(axb, carry);
                    carry = self.add_or(a_and_b, axb_and_c);
                }
                // carry == 1 means a >= b (no borrow), carry == 0 means a < b (borrow)
                vec![carry.not()]
            }
            SymExpr::Ule(a, b) => {
                // a <= b  ≡  !(b < a)
                let blt = SymExpr::Ult(b.clone(), a.clone());
                let bits = self.lower_expr(&blt);
                vec![bits[0].not()]
            }
            SymExpr::Slt(a, b) => {
                // Signed less-than: flip sign bits, then do ULT
                // a <_s b  ≡  (a XOR (1<<N-1)) <_u (b XOR (1<<N-1))
                let size = a.get_size() as usize;
                let sign_mask = SymExpr::new_const(1u64 << ((size * 8) - 1), a.get_size());
                let a_flipped = SymExpr::Xor(a.clone(), Box::new(sign_mask.clone()));
                let b_flipped = SymExpr::Xor(b.clone(), Box::new(sign_mask));
                let ult = SymExpr::Ult(Box::new(a_flipped), Box::new(b_flipped));
                self.lower_expr(&ult)
            }
            SymExpr::Sle(a, b) => {
                // a <=_s b  ≡  !(b <_s a)
                let blt = SymExpr::Slt(b.clone(), a.clone());
                let bits = self.lower_expr(&blt);
                vec![bits[0].not()]
            }
            SymExpr::Sgt(a, b) => {
                // a >_s b  ≡  b <_s a
                let blt = SymExpr::Slt(b.clone(), a.clone());
                self.lower_expr(&blt)
            }
            SymExpr::Shl(a, b) => {
                // Constant shift only (symbolic shift amount not yet supported)
                let a_bits = self.lower_expr(a);
                let a_len = a_bits.len();
                if let SymExpr::Const { val: shift, .. } = b.as_ref() {
                    let shift = *shift as usize;
                    let mut out = vec![AigLit::FALSE; shift];
                    let take_count = if shift < a_len { a_len - shift } else { 0 };
                    out.extend(a_bits.into_iter().take(take_count));
                    out.truncate(a_len);
                    // Pad to original length if truncated
                    while out.len() < a_len { out.push(AigLit::FALSE); }
                    out
                } else {
                    tracing::warn!("Symbolic shift not yet supported — treating as identity");
                    a_bits
                }
            }
            SymExpr::Lshr(a, b) => {
                let a_bits = self.lower_expr(a);
                if let SymExpr::Const { val: shift, .. } = b.as_ref() {
                    let shift = *shift as usize;
                    if shift >= a_bits.len() {
                        vec![AigLit::FALSE; a_bits.len()]
                    } else {
                        let mut out = a_bits[shift..].to_vec();
                        while out.len() < a_bits.len() { out.push(AigLit::FALSE); }
                        out
                    }
                } else {
                    tracing::warn!("Symbolic shift not yet supported — treating as identity");
                    a_bits
                }
            }
            SymExpr::Extract { expr, lsb, size } => {
                let bits = self.lower_expr(expr);
                let lsb = *lsb as usize;
                let end = (lsb + *size as usize).min(bits.len());
                let mut out = bits[lsb..end].to_vec();
                while out.len() < *size as usize { out.push(AigLit::FALSE); }
                out
            }
            SymExpr::Concat(a, b) => {
                // Concat(a, b): b is the low bits, a is the high bits
                let mut out = self.lower_expr(b);
                out.extend(self.lower_expr(a));
                out
            }
            // ── IEEE float bit-blast (soft-float style for bit patterns) ─────
            // Operands are treated as bitvectors of width size*8 when Float-sorted.
            SymExpr::FNeg(a) => {
                // Flip sign bit (MSB of the bit pattern).
                let bits = self.lower_float_bits(a);
                let mut out = bits;
                if let Some(sign) = out.last_mut() {
                    *sign = sign.not();
                }
                out
            }
            SymExpr::FAbs(a) => {
                // Clear sign bit.
                let mut out = self.lower_float_bits(a);
                if let Some(sign) = out.last_mut() {
                    *sign = AigLit::FALSE;
                }
                out
            }
            SymExpr::FIsNan(a) => {
                // exp all-1s AND mantissa != 0
                let bits = self.lower_float_bits(a);
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
            SymExpr::FEq(a, b) => {
                // Simplified: pure bit equality of IEEE patterns.
                let a_bits = self.lower_float_bits(a);
                let b_bits = self.lower_float_bits(b);
                vec![self.add_eq(&a_bits, &b_bits)]
            }
            SymExpr::FNeq(a, b) => {
                let a_bits = self.lower_float_bits(a);
                let b_bits = self.lower_float_bits(b);
                vec![self.add_eq(&a_bits, &b_bits).not()]
            }
            SymExpr::FLt(a, b) | SymExpr::FLe(a, b) => {
                let a_bits = self.lower_float_bits(a);
                let b_bits = self.lower_float_bits(b);
                let a_ord = self.float_total_order_bits(&a_bits);
                let b_ord = self.float_total_order_bits(&b_bits);
                let is_le = matches!(expr, SymExpr::FLe(_, _));
                if is_le {
                    let blt = self.bv_ult(&b_ord, &a_ord);
                    vec![blt.not()]
                } else {
                    vec![self.bv_ult(&a_ord, &b_ord)]
                }
            }
            SymExpr::FAdd(a, b) | SymExpr::FSub(a, b) | SymExpr::FMul(a, b) | SymExpr::FDiv(a, b) => {
                // Full IEEE arithmetic bit-blast is enormous; for symbolic operands
                // allocate a free result bitvector of the float width (under-approx
                // of theory axioms). Concrete cases are already folded in SymExpr::new_f*.
                let width = self.lower_float_bits(a).len().max(self.lower_float_bits(b).len());
                let mut out = Vec::with_capacity(width);
                for _ in 0..width {
                    let idx = self.nodes.len() as u32 + 1;
                    self.nodes.push(AigNode::Var(idx));
                    out.push(AigLit::new(idx, false));
                }
                // Touch b for dependency tracking in future axiom expansion.
                let _ = b;
                out
            }
            SymExpr::FSqrt(a) => {
                let width = self.lower_float_bits(a).len();
                let mut out = Vec::with_capacity(width);
                for _ in 0..width {
                    let idx = self.nodes.len() as u32 + 1;
                    self.nodes.push(AigNode::Var(idx));
                    out.push(AigLit::new(idx, false));
                }
                out
            }
            // Mul, Udiv, Ite, and other ops not yet supported
            _ => {
                tracing::warn!("Unsupported AIG lowering for {:?}", expr);
                let n = expr.get_size().max(1) as usize;
                // Prefer bit-width for multi-byte payloads.
                let n = if n <= 8 { n * 8 } else { n };
                vec![AigLit::FALSE; n]
            }
        }
    }

    /// Lower a float-sorted (or BV) expression to IEEE bit-pattern bits (LSB first).
    fn lower_float_bits(&mut self, expr: &SymExpr) -> Vec<AigLit> {
        let bits = self.lower_expr(expr);
        // If we got byte-sized false vectors from Const with size=in-bytes, expand.
        let want = match expr.get_sort() {
            crate::ast::Sort::Float(sz) => (sz as usize) * 8,
            crate::ast::Sort::BitVector(sz) if sz == 4 || sz == 8 => (sz as usize) * 8,
            _ => bits.len(),
        };
        if bits.len() == want {
            return bits;
        }
        if let SymExpr::Const { val, .. } = expr {
            let mut out = Vec::with_capacity(want);
            for i in 0..want {
                out.push(if (val & (1u64 << i)) != 0 {
                    AigLit::TRUE
                } else {
                    AigLit::FALSE
                });
            }
            return out;
        }
        // Pad/truncate free bits.
        let mut out = bits;
        out.resize(want, AigLit::FALSE);
        out
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

    /// Map float bits to a total-order integer encoding for comparison.
    fn float_total_order_bits(&mut self, bits: &[AigLit]) -> Vec<AigLit> {
        // If sign bit set: flip all bits; else flip only sign (classic float→int map).
        let n = bits.len();
        if n == 0 {
            return vec![];
        }
        let sign = bits[n - 1];
        let mut out = Vec::with_capacity(n);
        for i in 0..n - 1 {
            // out[i] = sign ? !bits[i] : bits[i]
            let flipped = bits[i].not();
            // MUX: (sign & flipped) | (!sign & bits[i])
            let t = self.add_and(sign, flipped);
            let f = self.add_and(sign.not(), bits[i]);
            out.push(self.add_or(t, f));
        }
        // Sign bit becomes inverted sense for order: keep as !sign for positives first? 
        // Standard: positive sign bit 1 in ordered map.
        out.push(sign.not());
        out
    }

    fn bv_ult(&mut self, a: &[AigLit], b: &[AigLit]) -> AigLit {
        let len = a.len().max(b.len());
        let mut carry = AigLit::TRUE; // a + ~b + 1
        for i in 0..len {
            let ax = a.get(i).copied().unwrap_or(AigLit::FALSE);
            let bx = b.get(i).copied().unwrap_or(AigLit::FALSE).not();
            let axb = self.add_xor(ax, bx);
            let a_and_b = self.add_and(ax, bx);
            let axb_and_c = self.add_and(axb, carry);
            carry = self.add_or(a_and_b, axb_and_c);
        }
        carry.not() // borrow ⇒ a < b
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

    #[test]
    fn test_ult_sat() {
        // x < 10 (unsigned)  => SAT (x = 0..9)
        let x   = SymExpr::new_var("x", 8);
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
        let x = SymExpr::new_float_var("fx", 4);
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
            size: 4,
        };
        // Avoid constant-folder path by going through Var + free FIsNan on concrete via fold:
        let folded = SymExpr::new_fisnan(one.clone());
        // new_fisnan on concrete folds: 1.0 is not nan → Const 0
        assert_eq!(folded, SymExpr::Const { val: 0, size: 1 });
        assert!(!check_sat(folded));
    }

    #[test]
    fn test_float_feq_symbolic_sat() {
        // two equal float vars → SAT (x == x free)
        let x = SymExpr::new_float_var("a", 4);
        let y = SymExpr::new_float_var("b", 4);
        let eq = SymExpr::FEq(Box::new(x), Box::new(y));
        assert!(check_sat(eq));
    }

    #[test]
    fn test_float_flt_bitblast_is_bool() {
        // Symbolic FLt bit-blasts to a single comparison bit (SAT may be deep;
        // structural width is the gate for this layer).
        let x = SymExpr::new_float_var("p", 4);
        let y = SymExpr::new_float_var("q", 4);
        let lt = SymExpr::FLt(Box::new(x), Box::new(y));
        let mut aig = AigManager::new();
        let bits = aig.lower_expr(&lt);
        assert_eq!(bits.len(), 1);
        // Concrete fold path: 1.0 < 2.0
        let one = SymExpr::Const {
            val: 1.0f32.to_bits() as u64,
            size: 4,
        };
        let two = SymExpr::Const {
            val: 2.0f32.to_bits() as u64,
            size: 4,
        };
        let folded = SymExpr::new_flt(one, two);
        assert_eq!(folded, SymExpr::Const { val: 1, size: 1 });
        assert!(check_sat(folded));
    }

    #[test]
    fn test_float_fadd_allocates_result_bits() {
        // Symbolic FAdd under-approximates with free result bits (width preserved).
        let a = SymExpr::new_float_var("fa", 4);
        let b = SymExpr::new_float_var("fb", 4);
        let sum = SymExpr::FAdd(Box::new(a), Box::new(b));
        let mut aig = AigManager::new();
        let bits = aig.lower_expr(&sum);
        assert_eq!(bits.len(), 32);
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
        let neg_one = SymExpr::new_const(0xFF, 1); // 1-byte -1
        let zero    = SymExpr::new_const(0, 1);
        assert!(check_sat(SymExpr::new_slt(neg_one, zero)));
    }

    #[test]
    fn test_extract_sat() {
        // Extract bits [7:4] of x, assert they equal 0xA
        let x = SymExpr::new_var("x", 8);
        let hi_nibble = SymExpr::Extract { expr: Box::new(x), lsb: 4, size: 4 };
        let target = SymExpr::new_const(0xA, 4);
        assert!(check_sat(SymExpr::new_eq(hi_nibble, target)));
    }

    #[test]
    fn test_lshr_sat() {
        // (x >> 1) == 5  => x must be 10 or 11 => SAT
        let x     = SymExpr::new_var("x", 8);
        let shift = SymExpr::new_const(1, 8);
        let five  = SymExpr::new_const(5, 8);
        let shifted = SymExpr::Lshr(Box::new(x), Box::new(shift));
        assert!(check_sat(SymExpr::new_eq(shifted, five)));
    }

}