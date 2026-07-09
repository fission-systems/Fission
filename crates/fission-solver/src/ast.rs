use std::sync::atomic::{AtomicU32, Ordering};

pub type SymNodeId = u32;

/// A global counter for generating unique variable IDs.
pub(crate) static VAR_COUNTER: AtomicU32 = AtomicU32::new(1);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Sort {
    /// A bitvector of a specific size in bytes
    BitVector(u32),
    /// IEEE floating-point value stored as bit-pattern payload; `size` is 4 or 8.
    Float(u32),
    /// An array mapping a domain sort to a range sort
    Array { domain: Box<Sort>, range: Box<Sort> },
}

impl Sort {
    pub fn expect_bv(&self) -> u32 {
        match self {
            Sort::BitVector(sz) => *sz,
            Sort::Float(sz) => *sz, // bit-pattern width
            _ => panic!("Expected BitVector/Float sort, got {:?}", self),
        }
    }

    pub fn byte_size(&self) -> u32 {
        match self {
            Sort::BitVector(sz) | Sort::Float(sz) => *sz,
            Sort::Array { range, .. } => range.byte_size(),
        }
    }
}

/// A node in the Symbolic Expression (AST) tree.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SymExpr {
    /// A concrete value (constant) — for floats, `val` holds the IEEE bit pattern.
    Const { val: u64, size: u32 },
    /// A symbolic variable (e.g. tainted input byte)
    Var { id: SymNodeId, name: String, sort: Sort },
    
    // Arithmetic
    Add(Box<SymExpr>, Box<SymExpr>),
    Sub(Box<SymExpr>, Box<SymExpr>),
    Mul(Box<SymExpr>, Box<SymExpr>),
    Udiv(Box<SymExpr>, Box<SymExpr>),
    
    // Bitwise
    And(Box<SymExpr>, Box<SymExpr>),
    Or(Box<SymExpr>, Box<SymExpr>),
    Xor(Box<SymExpr>, Box<SymExpr>),
    Shl(Box<SymExpr>, Box<SymExpr>),
    Lshr(Box<SymExpr>, Box<SymExpr>),
    
    // Boolean / Comparison (returns 1-bit boolean expression)
    Eq(Box<SymExpr>, Box<SymExpr>),
    Neq(Box<SymExpr>, Box<SymExpr>),
    Ult(Box<SymExpr>, Box<SymExpr>),
    Ule(Box<SymExpr>, Box<SymExpr>),
    /// Signed less-than (e.g. x86 JLESS, SF ≠ OF)
    Slt(Box<SymExpr>, Box<SymExpr>),
    /// Signed less-than-or-equal
    Sle(Box<SymExpr>, Box<SymExpr>),
    /// Signed greater-than
    Sgt(Box<SymExpr>, Box<SymExpr>),

    // IEEE float theory (payloads are bit-patterns / Float-sorted nodes)
    FAdd(Box<SymExpr>, Box<SymExpr>),
    FSub(Box<SymExpr>, Box<SymExpr>),
    FMul(Box<SymExpr>, Box<SymExpr>),
    FDiv(Box<SymExpr>, Box<SymExpr>),
    FNeg(Box<SymExpr>),
    FAbs(Box<SymExpr>),
    FSqrt(Box<SymExpr>),
    /// Ordered float comparisons → 1-bit boolean
    FEq(Box<SymExpr>, Box<SymExpr>),
    FNeq(Box<SymExpr>, Box<SymExpr>),
    FLt(Box<SymExpr>, Box<SymExpr>),
    FLe(Box<SymExpr>, Box<SymExpr>),
    FIsNan(Box<SymExpr>),
    
    // Control Flow
    Ite { cond: Box<SymExpr>, t: Box<SymExpr>, f: Box<SymExpr> },
    
    // Bit extraction / concat
    Extract { expr: Box<SymExpr>, lsb: u32, size: u32 },
    Concat(Box<SymExpr>, Box<SymExpr>),
    
    // Theory of Arrays
    ArraySelect { array: Box<SymExpr>, index: Box<SymExpr> },
    ArrayStore { array: Box<SymExpr>, index: Box<SymExpr>, value: Box<SymExpr> },
}

impl SymExpr {
    pub fn new_var(name: &str, size: u32) -> Self {
        let id = VAR_COUNTER.fetch_add(1, Ordering::SeqCst);
        Self::Var { id, name: name.to_string(), sort: Sort::BitVector(size) }
    }
    
    pub fn new_array_var(name: &str, domain: u32, range: u32) -> Self {
        let id = VAR_COUNTER.fetch_add(1, Ordering::SeqCst);
        Self::Var { 
            id, 
            name: name.to_string(), 
            sort: Sort::Array { 
                domain: Box::new(Sort::BitVector(domain)), 
                range: Box::new(Sort::BitVector(range)) 
            } 
        }
    }

    pub fn new_const(val: u64, size: u32) -> Self {
        Self::Const { val, size }
    }

    pub fn new_add(a: SymExpr, b: SymExpr) -> Self {
        match (&a, &b) {
            (Self::Const { val: v1, size }, Self::Const { val: v2, .. }) => {
                let mask = if *size == 64 { u64::MAX } else { (1 << size) - 1 };
                Self::Const { val: (v1.wrapping_add(*v2)) & mask, size: *size }
            },
            (Self::Const { val: 0, .. }, _) => b,
            (_, Self::Const { val: 0, .. }) => a,
            _ => Self::Add(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_sub(a: SymExpr, b: SymExpr) -> Self {
        match (&a, &b) {
            (Self::Const { val: v1, size }, Self::Const { val: v2, .. }) => {
                let mask = if *size == 64 { u64::MAX } else { (1 << size) - 1 };
                Self::Const { val: (v1.wrapping_sub(*v2)) & mask, size: *size }
            },
            (_, Self::Const { val: 0, .. }) => a,
            (a_expr, b_expr) if a_expr == b_expr => Self::Const { val: 0, size: a.get_size() },
            _ => Self::Sub(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_and(a: SymExpr, b: SymExpr) -> Self {
        match (&a, &b) {
            (Self::Const { val: v1, size }, Self::Const { val: v2, .. }) => Self::Const { val: v1 & v2, size: *size },
            (Self::Const { val: 0, size }, _) => Self::Const { val: 0, size: *size },
            (_, Self::Const { val: 0, size }) => Self::Const { val: 0, size: *size },
            (a, b) if a == b => a.clone(),
            _ => Self::And(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_xor(a: SymExpr, b: SymExpr) -> Self {
        match (&a, &b) {
            (Self::Const { val: v1, size }, Self::Const { val: v2, .. }) => Self::Const { val: v1 ^ v2, size: *size },
            (Self::Const { val: 0, .. }, _) => b,
            (_, Self::Const { val: 0, .. }) => a,
            (a, b) if a == b => Self::Const { val: 0, size: a.get_size() },
            _ => Self::Xor(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_not(a: SymExpr) -> Self {
        match &a {
            Self::Const { val, size } => {
                let mask = if *size == 64 { u64::MAX } else { (1 << size) - 1 };
                Self::Const { val: (!val) & mask, size: *size }
            },
            _ => {
                let size = a.get_size();
                let mask = if size == 64 { u64::MAX } else { (1 << size) - 1 };
                Self::new_xor(a, Self::Const { val: mask, size })
            }
        }
    }

    pub fn new_eq(a: SymExpr, b: SymExpr) -> Self {
        match (&a, &b) {
            (Self::Const { val: v1, .. }, Self::Const { val: v2, .. }) => Self::Const { val: if v1 == v2 { 1 } else { 0 }, size: 1 },
            (a_expr, b_expr) if a_expr == b_expr => Self::Const { val: 1, size: 1 },
            _ => Self::Eq(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_neq(a: SymExpr, b: SymExpr) -> Self {
        match (&a, &b) {
            (Self::Const { val: v1, .. }, Self::Const { val: v2, .. }) => Self::Const { val: if v1 != v2 { 1 } else { 0 }, size: 1 },
            (a_expr, b_expr) if a_expr == b_expr => Self::Const { val: 0, size: 1 },
            _ => Self::Neq(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_ult(a: SymExpr, b: SymExpr) -> Self {
        match (&a, &b) {
            (Self::Const { val: v1, .. }, Self::Const { val: v2, .. }) => Self::Const { val: if v1 < v2 { 1 } else { 0 }, size: 1 },
            _ => Self::Ult(Box::new(a), Box::new(b)),
        }
    }

    /// Signed less-than: interpret both sides as two's-complement signed integers.
    pub fn new_slt(a: SymExpr, b: SymExpr) -> Self {
        match (&a, &b) {
            (Self::Const { val: v1, size }, Self::Const { val: v2, .. }) => {
                let bits = *size * 8;
                let sign_bit = 1u64 << (bits.saturating_sub(1));
                let a_signed = if v1 & sign_bit != 0 { (v1.wrapping_sub(1u64 << bits)) as i64 } else { *v1 as i64 };
                let b_signed = if v2 & sign_bit != 0 { (v2.wrapping_sub(1u64 << bits)) as i64 } else { *v2 as i64 };
                Self::Const { val: if a_signed < b_signed { 1 } else { 0 }, size: 1 }
            },
            _ => Self::Slt(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_sle(a: SymExpr, b: SymExpr) -> Self {
        match (&a, &b) {
            (Self::Const { val: v1, size }, Self::Const { val: v2, .. }) => {
                let bits = *size * 8;
                let sign_bit = 1u64 << (bits.saturating_sub(1));
                let a_signed = if v1 & sign_bit != 0 { (v1.wrapping_sub(1u64 << bits)) as i64 } else { *v1 as i64 };
                let b_signed = if v2 & sign_bit != 0 { (v2.wrapping_sub(1u64 << bits)) as i64 } else { *v2 as i64 };
                Self::Const { val: if a_signed <= b_signed { 1 } else { 0 }, size: 1 }
            },
            _ => Self::Sle(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_sgt(a: SymExpr, b: SymExpr) -> Self {
        // a > b (signed) ≡ b < a (signed)
        Self::new_slt(b, a)
    }

    /// Construct a float-sorted variable (IEEE bits live in the concrete domain).
    pub fn new_float_var(name: &str, size: u32) -> Self {
        let id = VAR_COUNTER.fetch_add(1, Ordering::SeqCst);
        let size = if size == 4 || size == 8 { size } else { 8 };
        Self::Var {
            id,
            name: name.to_string(),
            sort: Sort::Float(size),
        }
    }

    fn float_size_of(e: &SymExpr) -> u32 {
        match e.get_sort() {
            Sort::Float(sz) | Sort::BitVector(sz) => {
                if sz == 4 || sz == 8 {
                    sz
                } else {
                    8
                }
            }
            _ => 8,
        }
    }

    fn as_f64_bits(e: &SymExpr) -> Option<(f64, u32)> {
        match e {
            Self::Const { val, size } => {
                let sz = if *size == 4 { 4 } else { 8 };
                let f = if sz == 4 {
                    f32::from_bits(*val as u32) as f64
                } else {
                    f64::from_bits(*val)
                };
                Some((f, sz))
            }
            _ => None,
        }
    }

    fn fconst(val: f64, size: u32) -> Self {
        let bits = if size == 4 {
            (val as f32).to_bits() as u64
        } else {
            val.to_bits()
        };
        Self::Const { val: bits, size }
    }

    pub fn new_fadd(a: SymExpr, b: SymExpr) -> Self {
        match (Self::as_f64_bits(&a), Self::as_f64_bits(&b)) {
            (Some((x, sz)), Some((y, _))) => Self::fconst(x + y, sz),
            _ => Self::FAdd(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_fsub(a: SymExpr, b: SymExpr) -> Self {
        match (Self::as_f64_bits(&a), Self::as_f64_bits(&b)) {
            (Some((x, sz)), Some((y, _))) => Self::fconst(x - y, sz),
            _ => Self::FSub(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_fmul(a: SymExpr, b: SymExpr) -> Self {
        match (Self::as_f64_bits(&a), Self::as_f64_bits(&b)) {
            (Some((x, sz)), Some((y, _))) => Self::fconst(x * y, sz),
            _ => Self::FMul(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_fdiv(a: SymExpr, b: SymExpr) -> Self {
        match (Self::as_f64_bits(&a), Self::as_f64_bits(&b)) {
            (Some((x, sz)), Some((y, _))) => Self::fconst(x / y, sz),
            _ => Self::FDiv(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_fneg(a: SymExpr) -> Self {
        match Self::as_f64_bits(&a) {
            Some((x, sz)) => Self::fconst(-x, sz),
            None => Self::FNeg(Box::new(a)),
        }
    }

    pub fn new_fabs(a: SymExpr) -> Self {
        match Self::as_f64_bits(&a) {
            Some((x, sz)) => Self::fconst(x.abs(), sz),
            None => Self::FAbs(Box::new(a)),
        }
    }

    pub fn new_fsqrt(a: SymExpr) -> Self {
        match Self::as_f64_bits(&a) {
            Some((x, sz)) => Self::fconst(x.sqrt(), sz),
            None => Self::FSqrt(Box::new(a)),
        }
    }

    pub fn new_feq(a: SymExpr, b: SymExpr) -> Self {
        match (Self::as_f64_bits(&a), Self::as_f64_bits(&b)) {
            (Some((x, _)), Some((y, _))) => Self::Const {
                val: u64::from(x == y),
                size: 1,
            },
            _ => Self::FEq(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_fneq(a: SymExpr, b: SymExpr) -> Self {
        match (Self::as_f64_bits(&a), Self::as_f64_bits(&b)) {
            (Some((x, _)), Some((y, _))) => Self::Const {
                val: u64::from(x != y),
                size: 1,
            },
            _ => Self::FNeq(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_flt(a: SymExpr, b: SymExpr) -> Self {
        match (Self::as_f64_bits(&a), Self::as_f64_bits(&b)) {
            (Some((x, _)), Some((y, _))) => Self::Const {
                val: u64::from(x < y),
                size: 1,
            },
            _ => Self::FLt(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_fle(a: SymExpr, b: SymExpr) -> Self {
        match (Self::as_f64_bits(&a), Self::as_f64_bits(&b)) {
            (Some((x, _)), Some((y, _))) => Self::Const {
                val: u64::from(x <= y),
                size: 1,
            },
            _ => Self::FLe(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_fisnan(a: SymExpr) -> Self {
        match Self::as_f64_bits(&a) {
            Some((x, _)) => Self::Const {
                val: u64::from(x.is_nan()),
                size: 1,
            },
            None => Self::FIsNan(Box::new(a)),
        }
    }

    pub fn get_sort(&self) -> Sort {
        match self {
            Self::Const { size, .. } => Sort::BitVector(*size),
            Self::Var { sort, .. } => sort.clone(),
            Self::Add(a, _) | Self::Sub(a, _) | Self::Mul(a, _) | Self::Udiv(a, _) => a.get_sort(),
            Self::And(a, _) | Self::Or(a, _) | Self::Xor(a, _) | Self::Shl(a, _) | Self::Lshr(a, _) => a.get_sort(),
            Self::Eq(_, _) | Self::Neq(_, _) | Self::Ult(_, _) | Self::Ule(_, _)
            | Self::Slt(_, _) | Self::Sle(_, _) | Self::Sgt(_, _) => Sort::BitVector(1),
            Self::FAdd(a, _) | Self::FSub(a, _) | Self::FMul(a, _) | Self::FDiv(a, _)
            | Self::FNeg(a) | Self::FAbs(a) | Self::FSqrt(a) => {
                Sort::Float(Self::float_size_of(a))
            }
            Self::FEq(_, _) | Self::FNeq(_, _) | Self::FLt(_, _) | Self::FLe(_, _)
            | Self::FIsNan(_) => Sort::BitVector(1),
            Self::Ite { t, .. } => t.get_sort(),
            Self::Extract { size, .. } => Sort::BitVector(*size),
            Self::Concat(a, b) => Sort::BitVector(a.get_size() + b.get_size()),
            Self::ArraySelect { array, .. } => {
                if let Sort::Array { range, .. } = array.get_sort() {
                    *range
                } else {
                    panic!("ArraySelect on non-array")
                }
            }
            Self::ArrayStore { array, .. } => array.get_sort(),
        }
    }

    pub fn get_size(&self) -> u32 {
        self.get_sort().byte_size()
    }
}

#[cfg(test)]
mod float_tests {
    use super::*;

    #[test]
    fn fadd_folds_concrete() {
        let a = SymExpr::Const {
            val: 1.5f64.to_bits(),
            size: 8,
        };
        let b = SymExpr::Const {
            val: 2.25f64.to_bits(),
            size: 8,
        };
        let r = SymExpr::new_fadd(a, b);
        match r {
            SymExpr::Const { val, size } => {
                assert_eq!(size, 8);
                assert!((f64::from_bits(val) - 3.75).abs() < 1e-9);
            }
            other => panic!("expected folded const, got {other:?}"),
        }
    }

    #[test]
    fn fadd_symbolic_builds_node() {
        let a = SymExpr::new_float_var("x", 8);
        let b = SymExpr::Const {
            val: 1.0f64.to_bits(),
            size: 8,
        };
        let r = SymExpr::new_fadd(a, b);
        assert!(matches!(r, SymExpr::FAdd(_, _)));
    }
}
