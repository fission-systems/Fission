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
    /// Maps ArraySelect nodes to their vector of AIG literals
    array_select_map: HashMap<SymExpr, Vec<AigLit>>,
    pub last_cnf_node: usize,
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
            array_select_map: HashMap::new(),
            last_cnf_node: 0,
            unsupported: Vec::new(),
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

    /// What lowering met and could not encode, in the order it was met.
    pub fn unsupported(&self) -> &[String] {
        &self.unsupported
    }

    pub fn note_unsupported(&mut self, what: String) {
        if !self.unsupported.contains(&what) {
            self.unsupported.push(what);
        }
    }

    /// `n` fresh, unconstrained bits.
    fn fresh_bits(&mut self, n: usize) -> Vec<AigLit> {
        (0..n)
            .map(|_| {
                let idx = self.nodes.len() as u32 + 1;
                self.nodes.push(AigNode::Var(idx));
                AigLit::new(idx, false)
            })
            .collect()
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
            let multiplier_bit = bit(b, i);
            let mut partial = vec![AigLit::FALSE; width];
            for j in 0..width - i {
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
            let (difference, fits) = self.add_subtracter(&partial, &b);
            quotient[width - 1 - step] = fits;
            if step < width - 1 {
                // Keep the difference if it fitted, then bring the next
                // dividend bit down. The top bit that falls off is always
                // zero: a partial remainder before the last step holds at
                // most `step + 1` bits.
                let mut next = vec![AigLit::FALSE; width];
                for j in (1..width).rev() {
                    next[j] = self.add_ite(fits, difference[j - 1], partial[j - 1]);
                }
                next[0] = a[width - step - 2];
                partial = next;
            } else {
                for j in 0..width {
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
                    bits.push(if (val & (1 << i)) != 0 {
                        AigLit::TRUE
                    } else {
                        AigLit::FALSE
                    });
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
                let a_bits = self.lower_expr(a);
                let b_bits = self.lower_expr(b);
                vec![self.add_ult(&a_bits, &b_bits)]
            }
            SymExpr::Ule(a, b) => {
                // a <= b  ≡  !(b < a)
                let blt = SymExpr::Ult(b.clone(), a.clone());
                let bits = self.lower_expr(&blt);
                vec![bits[0].not()]
            }
            SymExpr::Slt(a, b) => {
                let a_bits = self.lower_expr(a);
                let b_bits = self.lower_expr(b);
                vec![self.add_slt(&a_bits, &b_bits)]
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
            SymExpr::Shl(a, b) | SymExpr::Lshr(a, b) | SymExpr::Ashr(a, b) => {
                let kind = match expr {
                    SymExpr::Shl(..) => ShiftKind::Left,
                    SymExpr::Lshr(..) => ShiftKind::LogicalRight,
                    _ => ShiftKind::ArithmeticRight,
                };
                let a_bits = self.lower_expr(a);
                if let SymExpr::Const { val, .. } = b.as_ref() {
                    Self::shift_by_constant(&a_bits, *val, kind)
                } else {
                    let amount = self.lower_expr(b);
                    self.add_shift(&a_bits, &amount, kind)
                }
            }
            SymExpr::Extract { expr, lsb, size } => {
                let bits = self.lower_expr(expr);
                let lsb = *lsb as usize;
                let end = (lsb + *size as usize).min(bits.len());
                let mut out = bits[lsb..end].to_vec();
                while out.len() < *size as usize {
                    out.push(AigLit::FALSE);
                }
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
            SymExpr::FAdd(a, b)
            | SymExpr::FSub(a, b)
            | SymExpr::FMul(a, b)
            | SymExpr::FDiv(a, b) => {
                // Full IEEE arithmetic bit-blast is enormous; for symbolic operands
                // allocate a free result bitvector of the float width (under-approx
                // of theory axioms). Concrete cases are already folded in SymExpr::new_f*.
                let width = self
                    .lower_float_bits(a)
                    .len()
                    .max(self.lower_float_bits(b).len());
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
            SymExpr::Mul(a, b) => {
                let a_bits = self.lower_expr(a);
                let b_bits = self.lower_expr(b);
                self.add_multiplier(&a_bits, &b_bits)
            }
            SymExpr::Udiv(a, b) => {
                let a_bits = self.lower_expr(a);
                let b_bits = self.lower_expr(b);
                self.add_udiv_urem(&a_bits, &b_bits).0
            }
            SymExpr::Urem(a, b) => {
                let a_bits = self.lower_expr(a);
                let b_bits = self.lower_expr(b);
                self.add_udiv_urem(&a_bits, &b_bits).1
            }
            SymExpr::Sdiv(a, b) | SymExpr::Srem(a, b) | SymExpr::Smod(a, b) => {
                let kind = match expr {
                    SymExpr::Sdiv(..) => SignedDivision::Quotient,
                    SymExpr::Srem(..) => SignedDivision::Remainder,
                    _ => SignedDivision::Modulus,
                };
                let a_bits = self.lower_expr(a);
                let b_bits = self.lower_expr(b);
                self.add_signed_division(&a_bits, &b_bits, kind)
            }
            SymExpr::Ite { cond, t, f } => {
                let c_bits = self.lower_expr(cond);
                let t_bits = self.lower_expr(t);
                let f_bits = self.lower_expr(f);
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
            // Everything else. This used to answer all-false bits, so the
            // solver believed `a * b`, `a / b` and `ite(c, x, y)` were always
            // zero: `x * 3 == 6` came back UNSAT, and `x*3 != x*5` came back
            // UNSAT -- a false proof that two different functions are equal,
            // which fission-dir reports as `Equivalent`. Unconstrained bits
            // plus a recorded miss make the answer `Unknown` instead.
            _ => {
                let debug = format!("{expr:?}");
                let name = debug
                    .split(|c: char| !c.is_alphanumeric())
                    .next()
                    .unwrap_or("?")
                    .to_string();
                tracing::warn!("Unsupported AIG lowering for {name}");
                self.note_unsupported(name);
                let n = match expr.get_sort() {
                    crate::ast::Sort::Float(sz) => sz * 8,
                    crate::ast::Sort::BitVector(sz) => sz,
                    crate::ast::Sort::Array { range, .. } => range.byte_size(),
                };
                self.fresh_bits(n.max(1) as usize)
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
        let zero = SymExpr::new_const(0, 1);
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
