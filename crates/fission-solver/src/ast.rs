use std::sync::atomic::{AtomicU32, Ordering};

pub type SymNodeId = u32;

/// A global counter for generating unique variable IDs.
pub(crate) static VAR_COUNTER: AtomicU32 = AtomicU32::new(1);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Sort {
    /// A bitvector whose width is measured in bits.
    BitVector(u32),
    /// IEEE floating-point value stored as a bit-pattern payload; width is 32 or 64 bits.
    Float(u32),
    /// An array mapping a domain sort to a range sort
    Array { domain: Box<Sort>, range: Box<Sort> },
}

impl Sort {
    /// Return the bit width of a bitvector or float sort.
    pub fn expect_bv(&self) -> u32 {
        match self {
            Sort::BitVector(sz) => *sz,
            Sort::Float(sz) => *sz,
            _ => panic!("Expected BitVector/Float sort, got {:?}", self),
        }
    }

    /// Return the number of bytes needed to store a bitvector or float value.
    pub fn byte_size(&self) -> u32 {
        match self {
            Sort::BitVector(bits) | Sort::Float(bits) => bits.saturating_add(7) / 8,
            Sort::Array { range, .. } => range.byte_size(),
        }
    }
}

fn bitvector_mask(width_bits: u32) -> u64 {
    match width_bits {
        0 => 0,
        1..=63 => (1u64 << width_bits) - 1,
        _ => u64::MAX,
    }
}

fn supported_bit_width(width_bits: u32) -> bool {
    (1..=64).contains(&width_bits)
}

fn signed_value(value: u64, width_bits: u32) -> i128 {
    match width_bits {
        0 => 0,
        1..=63 => {
            let mask = bitvector_mask(width_bits);
            let value = value & mask;
            let sign_bit = 1u64 << (width_bits - 1);
            if value & sign_bit == 0 {
                i128::from(value)
            } else {
                i128::from(value) - (1i128 << width_bits)
            }
        }
        64 => i128::from(value as i64),
        // A u64 payload has zeroes above bit 63, so a value wider than 64 bits
        // cannot have its sign bit set through this representation.
        _ => i128::from(value),
    }
}

/// A node in the Symbolic Expression (AST) tree.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SymExpr {
    /// A concrete value with a `size`-bit width. For floats, `val` holds the
    /// IEEE bit pattern and `size` is 32 or 64.
    Const {
        val: u64,
        size: u32,
    },
    /// A symbolic variable (e.g. tainted input byte)
    Var {
        id: SymNodeId,
        name: String,
        sort: Sort,
    },

    // Arithmetic
    Add(Box<SymExpr>, Box<SymExpr>),
    Sub(Box<SymExpr>, Box<SymExpr>),
    Mul(Box<SymExpr>, Box<SymExpr>),
    Udiv(Box<SymExpr>, Box<SymExpr>),
    /// Unsigned remainder. `x % 0` is `x` (SMT-LIB `bvurem`).
    Urem(Box<SymExpr>, Box<SymExpr>),
    /// Signed division, truncating toward zero (SMT-LIB `bvsdiv`).
    Sdiv(Box<SymExpr>, Box<SymExpr>),
    /// Signed remainder; the sign follows the dividend (SMT-LIB `bvsrem`).
    Srem(Box<SymExpr>, Box<SymExpr>),
    /// Signed modulus; the sign follows the divisor (SMT-LIB `bvsmod`).
    Smod(Box<SymExpr>, Box<SymExpr>),

    // Bitwise
    And(Box<SymExpr>, Box<SymExpr>),
    Or(Box<SymExpr>, Box<SymExpr>),
    Xor(Box<SymExpr>, Box<SymExpr>),
    Shl(Box<SymExpr>, Box<SymExpr>),
    Lshr(Box<SymExpr>, Box<SymExpr>),
    /// Arithmetic right shift: vacated bits copy the sign bit.
    Ashr(Box<SymExpr>, Box<SymExpr>),

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
    Ite {
        cond: Box<SymExpr>,
        t: Box<SymExpr>,
        f: Box<SymExpr>,
    },

    // Bit extraction / concat
    Extract {
        expr: Box<SymExpr>,
        lsb: u32,
        size: u32,
    },
    Concat(Box<SymExpr>, Box<SymExpr>),

    // Theory of Arrays
    ArraySelect {
        array: Box<SymExpr>,
        index: Box<SymExpr>,
    },
    ArrayStore {
        array: Box<SymExpr>,
        index: Box<SymExpr>,
        value: Box<SymExpr>,
    },
}

impl SymExpr {
    /// Create a bitvector variable. `width_bits` is measured in bits.
    pub fn new_var(name: &str, width_bits: u32) -> Self {
        let id = VAR_COUNTER.fetch_add(1, Ordering::SeqCst);
        Self::Var {
            id,
            name: name.to_string(),
            sort: Sort::BitVector(width_bits),
        }
    }

    /// Create an array variable with bit widths for its address and element.
    pub fn new_array_var(name: &str, domain_bits: u32, range_bits: u32) -> Self {
        let id = VAR_COUNTER.fetch_add(1, Ordering::SeqCst);
        Self::Var {
            id,
            name: name.to_string(),
            sort: Sort::Array {
                domain: Box::new(Sort::BitVector(domain_bits)),
                range: Box::new(Sort::BitVector(range_bits)),
            },
        }
    }

    /// Create a bitvector constant. `width_bits` is measured in bits.
    pub fn new_const(val: u64, width_bits: u32) -> Self {
        Self::Const {
            val: val & bitvector_mask(width_bits),
            size: width_bits,
        }
    }

    pub fn new_add(a: SymExpr, b: SymExpr) -> Self {
        if !supported_bit_width(a.get_bit_width()) || !supported_bit_width(b.get_bit_width()) {
            return Self::Add(Box::new(a), Box::new(b));
        }
        let width_bits = a.get_bit_width().max(b.get_bit_width());
        match (&a, &b) {
            (Self::Const { val: v1, .. }, Self::Const { val: v2, .. }) => Self::Const {
                val: v1.wrapping_add(*v2) & bitvector_mask(width_bits),
                size: width_bits,
            },
            (Self::Const { val: 0, size }, _) if *size <= b.get_bit_width() => b,
            (_, Self::Const { val: 0, size }) if *size <= a.get_bit_width() => a,
            _ => Self::Add(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_sub(a: SymExpr, b: SymExpr) -> Self {
        if !supported_bit_width(a.get_bit_width()) || !supported_bit_width(b.get_bit_width()) {
            return Self::Sub(Box::new(a), Box::new(b));
        }
        let width_bits = a.get_bit_width().max(b.get_bit_width());
        match (&a, &b) {
            (Self::Const { val: v1, .. }, Self::Const { val: v2, .. }) => Self::Const {
                val: v1.wrapping_sub(*v2) & bitvector_mask(width_bits),
                size: width_bits,
            },
            (_, Self::Const { val: 0, size }) if *size <= a.get_bit_width() => a,
            (a_expr, b_expr) if a_expr == b_expr => Self::Const {
                val: 0,
                size: width_bits,
            },
            _ => Self::Sub(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_and(a: SymExpr, b: SymExpr) -> Self {
        if !supported_bit_width(a.get_bit_width()) || !supported_bit_width(b.get_bit_width()) {
            return Self::And(Box::new(a), Box::new(b));
        }
        let width_bits = a.get_bit_width().max(b.get_bit_width());
        match (&a, &b) {
            (Self::Const { val: v1, .. }, Self::Const { val: v2, .. }) => Self::Const {
                val: v1 & v2 & bitvector_mask(width_bits),
                size: width_bits,
            },
            (Self::Const { val: 0, .. }, _) => Self::Const {
                val: 0,
                size: width_bits,
            },
            (_, Self::Const { val: 0, .. }) => Self::Const {
                val: 0,
                size: width_bits,
            },
            (a, b) if a == b => a.clone(),
            _ => Self::And(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_xor(a: SymExpr, b: SymExpr) -> Self {
        if !supported_bit_width(a.get_bit_width()) || !supported_bit_width(b.get_bit_width()) {
            return Self::Xor(Box::new(a), Box::new(b));
        }
        let width_bits = a.get_bit_width().max(b.get_bit_width());
        match (&a, &b) {
            (Self::Const { val: v1, .. }, Self::Const { val: v2, .. }) => Self::Const {
                val: (v1 ^ v2) & bitvector_mask(width_bits),
                size: width_bits,
            },
            (Self::Const { val: 0, size }, _) if *size <= b.get_bit_width() => b,
            (_, Self::Const { val: 0, size }) if *size <= a.get_bit_width() => a,
            (a, b) if a == b => Self::Const {
                val: 0,
                size: width_bits,
            },
            _ => Self::Xor(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_not(a: SymExpr) -> Self {
        let width_bits = a.get_bit_width();
        if !supported_bit_width(width_bits) {
            return Self::Xor(
                Box::new(a),
                Box::new(Self::Const {
                    val: bitvector_mask(width_bits),
                    size: width_bits,
                }),
            );
        }
        match &a {
            Self::Const { val, size } => Self::Const {
                val: (!val) & bitvector_mask(*size),
                size: *size,
            },
            _ => {
                let size = a.get_bit_width();
                let mask = bitvector_mask(size);
                Self::new_xor(a, Self::Const { val: mask, size })
            }
        }
    }

    pub fn new_eq(a: SymExpr, b: SymExpr) -> Self {
        if !supported_bit_width(a.get_bit_width()) || !supported_bit_width(b.get_bit_width()) {
            return Self::Eq(Box::new(a), Box::new(b));
        }
        match (&a, &b) {
            (Self::Const { val: v1, .. }, Self::Const { val: v2, .. }) => Self::Const {
                val: if v1 == v2 { 1 } else { 0 },
                size: 1,
            },
            (a_expr, b_expr) if a_expr == b_expr => Self::Const { val: 1, size: 1 },
            _ => Self::Eq(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_neq(a: SymExpr, b: SymExpr) -> Self {
        if !supported_bit_width(a.get_bit_width()) || !supported_bit_width(b.get_bit_width()) {
            return Self::Neq(Box::new(a), Box::new(b));
        }
        match (&a, &b) {
            (Self::Const { val: v1, .. }, Self::Const { val: v2, .. }) => Self::Const {
                val: if v1 != v2 { 1 } else { 0 },
                size: 1,
            },
            (a_expr, b_expr) if a_expr == b_expr => Self::Const { val: 0, size: 1 },
            _ => Self::Neq(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_ult(a: SymExpr, b: SymExpr) -> Self {
        if !supported_bit_width(a.get_bit_width()) || !supported_bit_width(b.get_bit_width()) {
            return Self::Ult(Box::new(a), Box::new(b));
        }
        match (&a, &b) {
            (Self::Const { val: v1, .. }, Self::Const { val: v2, .. }) => Self::Const {
                val: if v1 < v2 { 1 } else { 0 },
                size: 1,
            },
            _ => Self::Ult(Box::new(a), Box::new(b)),
        }
    }

    /// Signed less-than: interpret both sides as two's-complement signed integers.
    pub fn new_slt(a: SymExpr, b: SymExpr) -> Self {
        if !supported_bit_width(a.get_bit_width()) || !supported_bit_width(b.get_bit_width()) {
            return Self::Slt(Box::new(a), Box::new(b));
        }
        match (&a, &b) {
            (
                Self::Const { val: v1, size },
                Self::Const {
                    val: v2,
                    size: other_size,
                },
            ) if size == other_size => Self::Const {
                val: u64::from(signed_value(*v1, *size) < signed_value(*v2, *size)),
                size: 1,
            },
            _ => Self::Slt(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_sle(a: SymExpr, b: SymExpr) -> Self {
        if !supported_bit_width(a.get_bit_width()) || !supported_bit_width(b.get_bit_width()) {
            return Self::Sle(Box::new(a), Box::new(b));
        }
        match (&a, &b) {
            (
                Self::Const { val: v1, size },
                Self::Const {
                    val: v2,
                    size: other_size,
                },
            ) if size == other_size => Self::Const {
                val: u64::from(signed_value(*v1, *size) <= signed_value(*v2, *size)),
                size: 1,
            },
            _ => Self::Sle(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_sgt(a: SymExpr, b: SymExpr) -> Self {
        // a > b (signed) ≡ b < a (signed)
        Self::new_slt(b, a)
    }

    /// Construct a float-sorted variable. IEEE circuits currently require a
    /// 32- or 64-bit payload; other widths are preserved and lower as Unknown.
    pub fn new_float_var(name: &str, width_bits: u32) -> Self {
        let id = VAR_COUNTER.fetch_add(1, Ordering::SeqCst);
        Self::Var {
            id,
            name: name.to_string(),
            sort: Sort::Float(width_bits),
        }
    }

    fn float_size_of(e: &SymExpr) -> u32 {
        match e.get_sort() {
            Sort::Float(sz) | Sort::BitVector(sz) => {
                if sz == 32 || sz == 64 {
                    sz
                } else {
                    64
                }
            }
            _ => 64,
        }
    }

    fn as_f64_bits(e: &SymExpr) -> Option<(f64, u32)> {
        match e {
            Self::Const {
                val,
                size: width_bits @ (32 | 64),
            } => {
                let f = if *width_bits == 32 {
                    f32::from_bits(*val as u32) as f64
                } else {
                    f64::from_bits(*val)
                };
                Some((f, *width_bits))
            }
            _ => None,
        }
    }

    fn fconst(val: f64, size: u32) -> Self {
        let bits = if size == 32 {
            (val as f32).to_bits() as u64
        } else {
            val.to_bits()
        };
        Self::Const { val: bits, size }
    }

    pub fn new_fadd(a: SymExpr, b: SymExpr) -> Self {
        match (Self::as_f64_bits(&a), Self::as_f64_bits(&b)) {
            (Some((x, sz)), Some((y, other_sz))) if sz == other_sz => Self::fconst(x + y, sz),
            _ => Self::FAdd(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_fsub(a: SymExpr, b: SymExpr) -> Self {
        match (Self::as_f64_bits(&a), Self::as_f64_bits(&b)) {
            (Some((x, sz)), Some((y, other_sz))) if sz == other_sz => Self::fconst(x - y, sz),
            _ => Self::FSub(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_fmul(a: SymExpr, b: SymExpr) -> Self {
        match (Self::as_f64_bits(&a), Self::as_f64_bits(&b)) {
            (Some((x, sz)), Some((y, other_sz))) if sz == other_sz => Self::fconst(x * y, sz),
            _ => Self::FMul(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_fdiv(a: SymExpr, b: SymExpr) -> Self {
        match (Self::as_f64_bits(&a), Self::as_f64_bits(&b)) {
            (Some((x, sz)), Some((y, other_sz))) if sz == other_sz => Self::fconst(x / y, sz),
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
            (Some((x, x_width)), Some((y, y_width))) if x_width == y_width => Self::Const {
                val: u64::from(x == y),
                size: 1,
            },
            _ => Self::FEq(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_fneq(a: SymExpr, b: SymExpr) -> Self {
        match (Self::as_f64_bits(&a), Self::as_f64_bits(&b)) {
            (Some((x, x_width)), Some((y, y_width))) if x_width == y_width => Self::Const {
                val: u64::from(x != y),
                size: 1,
            },
            _ => Self::FNeq(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_flt(a: SymExpr, b: SymExpr) -> Self {
        match (Self::as_f64_bits(&a), Self::as_f64_bits(&b)) {
            (Some((x, x_width)), Some((y, y_width))) if x_width == y_width => Self::Const {
                val: u64::from(x < y),
                size: 1,
            },
            _ => Self::FLt(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_fle(a: SymExpr, b: SymExpr) -> Self {
        match (Self::as_f64_bits(&a), Self::as_f64_bits(&b)) {
            (Some((x, x_width)), Some((y, y_width))) if x_width == y_width => Self::Const {
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
            Self::Add(a, b)
            | Self::Sub(a, b)
            | Self::Mul(a, b)
            | Self::Udiv(a, b)
            | Self::Urem(a, b)
            | Self::And(a, b)
            | Self::Or(a, b)
            | Self::Xor(a, b) => Sort::BitVector(a.get_bit_width().max(b.get_bit_width())),
            Self::Sdiv(a, _)
            | Self::Srem(a, _)
            | Self::Smod(a, _)
            | Self::Shl(a, _)
            | Self::Lshr(a, _)
            | Self::Ashr(a, _) => a.get_sort(),
            Self::Eq(_, _)
            | Self::Neq(_, _)
            | Self::Ult(_, _)
            | Self::Ule(_, _)
            | Self::Slt(_, _)
            | Self::Sle(_, _)
            | Self::Sgt(_, _) => Sort::BitVector(1),
            Self::FAdd(a, _)
            | Self::FSub(a, _)
            | Self::FMul(a, _)
            | Self::FDiv(a, _)
            | Self::FNeg(a)
            | Self::FAbs(a)
            | Self::FSqrt(a) => Sort::Float(Self::float_size_of(a)),
            Self::FEq(_, _)
            | Self::FNeq(_, _)
            | Self::FLt(_, _)
            | Self::FLe(_, _)
            | Self::FIsNan(_) => Sort::BitVector(1),
            Self::Ite { t, f, .. } => {
                let t_sort = t.get_sort();
                let f_sort = f.get_sort();
                if t_sort == f_sort {
                    t_sort
                } else {
                    Sort::BitVector(t.get_bit_width().max(f.get_bit_width()))
                }
            }
            Self::Extract { size, .. } => Sort::BitVector(*size),
            Self::Concat(a, b) => Sort::BitVector(a.get_bit_width() + b.get_bit_width()),
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

    /// Return this expression's bit width. For arrays, this is the element width.
    pub fn get_bit_width(&self) -> u32 {
        match self.get_sort() {
            Sort::BitVector(bits) | Sort::Float(bits) => bits,
            Sort::Array { range, .. } => range.expect_bv(),
        }
    }

    /// Legacy name retained for source compatibility; the returned unit is bits.
    pub fn get_size(&self) -> u32 {
        self.get_bit_width()
    }
}

#[cfg(test)]
mod float_tests {
    use super::*;

    #[test]
    fn fadd_folds_concrete() {
        let a = SymExpr::Const {
            val: 1.5f64.to_bits(),
            size: 64,
        };
        let b = SymExpr::Const {
            val: 2.25f64.to_bits(),
            size: 64,
        };
        let r = SymExpr::new_fadd(a, b);
        match r {
            SymExpr::Const { val, size } => {
                assert_eq!(size, 64);
                assert!((f64::from_bits(val) - 3.75).abs() < 1e-9);
            }
            other => panic!("expected folded const, got {other:?}"),
        }
    }

    #[test]
    fn fadd_symbolic_builds_node() {
        let a = SymExpr::new_float_var("x", 64);
        let b = SymExpr::Const {
            val: 1.0f64.to_bits(),
            size: 64,
        };
        let r = SymExpr::new_fadd(a, b);
        assert!(matches!(r, SymExpr::FAdd(_, _)));
    }
}

#[cfg(test)]
mod bit_width_tests {
    use super::*;

    #[test]
    fn bitvector_widths_and_storage_sizes_use_distinct_units() {
        for (width_bits, storage_bytes) in [(1, 1), (8, 1), (32, 4), (64, 8)] {
            let value = SymExpr::new_var("width", width_bits);
            assert_eq!(value.get_bit_width(), width_bits);
            assert_eq!(value.get_size(), width_bits, "legacy alias remains in bits");
            assert_eq!(value.get_sort().byte_size(), storage_bytes);
        }
    }

    #[test]
    fn floating_point_widths_are_also_measured_in_bits() {
        for (width_bits, storage_bytes) in [(32, 4), (64, 8)] {
            let value = SymExpr::new_float_var("float", width_bits);
            assert_eq!(value.get_bit_width(), width_bits);
            assert_eq!(value.get_sort().byte_size(), storage_bytes);
        }
    }

    #[test]
    fn constants_are_truncated_to_the_declared_bit_width() {
        for (width_bits, value, expected) in [
            (1, 0b11, 0b1),
            (8, 0x1ff, 0xff),
            (32, 0x1_0000_0005, 5),
            (64, u64::MAX, u64::MAX),
        ] {
            assert_eq!(
                SymExpr::new_const(value, width_bits),
                SymExpr::Const {
                    val: expected,
                    size: width_bits,
                }
            );
        }
    }

    #[test]
    fn constant_folding_masks_results_at_the_bit_width() {
        let lhs = SymExpr::new_const(0xff, 8);
        let rhs = SymExpr::new_const(2, 8);
        assert_eq!(
            SymExpr::new_add(lhs.clone(), rhs.clone()),
            SymExpr::new_const(1, 8)
        );
        assert_eq!(
            SymExpr::new_sub(SymExpr::new_const(0, 8), rhs.clone()),
            SymExpr::new_const(0xfe, 8)
        );
        assert_eq!(
            SymExpr::new_and(lhs.clone(), rhs.clone()),
            SymExpr::new_const(2, 8)
        );
        assert_eq!(SymExpr::new_xor(lhs, rhs), SymExpr::new_const(0xfd, 8));
        assert_eq!(
            SymExpr::new_not(SymExpr::new_const(0, 64)),
            SymExpr::new_const(u64::MAX, 64)
        );
    }

    #[test]
    fn signed_constant_comparisons_use_the_declared_bit_width() {
        let cases = [
            (1, 1, 0, true),
            (8, 0x80, 0, true),
            (8, 0x7f, 0, false),
            (32, 0x8000_0000, 0, true),
            (64, 1u64 << 63, 0, true),
        ];
        for (width_bits, lhs, rhs, expected) in cases {
            let lt = SymExpr::new_slt(
                SymExpr::new_const(lhs, width_bits),
                SymExpr::new_const(rhs, width_bits),
            );
            let le = SymExpr::new_sle(
                SymExpr::new_const(lhs, width_bits),
                SymExpr::new_const(rhs, width_bits),
            );
            assert_eq!(lt, SymExpr::new_const(u64::from(expected), 1));
            assert_eq!(le, SymExpr::new_const(u64::from(expected), 1));
        }
    }
}
