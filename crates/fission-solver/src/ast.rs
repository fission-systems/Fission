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

/// The top-level bit-vector widths of a constructor's operands, when both are
/// `1..=64`-bit bit-vectors by [`SymExpr::get_sort`]; `None` disables folding.
///
/// This reads only the operands' top-level sorts (as `get_bit_width` always
/// did), not a validation of each subtree: constructors run on every node an
/// emulator builds, and validating both subtrees made building a chain of `n`
/// nodes cost `O(n^2)` validator walks. It is enough because the only rules
/// that keep a sort-guarded fold are those that keep the operand: a constant
/// fold reads two constants, and `x + 0 => x` keeps `x` -- an ill-formed `x`
/// is still ill-formed afterwards and is rejected where it is used. The rules
/// that *drop* an operand (`a - a`, `a ^ a`, `0 & b`, `a == a`, `a != a`)
/// validate it with [`pair_is_pure_supported_bv`] first.
fn pair_bv_widths(a: &SymExpr, b: &SymExpr) -> Option<(u32, u32)> {
    match (a.get_sort(), b.get_sort()) {
        (Sort::BitVector(wa), Sort::BitVector(wb))
            if supported_bit_width(wa) && supported_bit_width(wb) =>
        {
            Some((wa, wb))
        }
        _ => None,
    }
}

fn pair_is_pure_supported_bv(a: &SymExpr, b: &SymExpr) -> Option<u32> {
    let mut env = crate::validate::VarSortEnv::new();
    let wa = crate::validate::is_pure_supported_bv(a, &mut env).ok()?;
    let wb = crate::validate::is_pure_supported_bv(b, &mut env).ok()?;
    Some(wa.max(wb))
}

/// A sort's bit width as [`SymExpr::get_bit_width`] reports it: the element
/// width of an array, `0` for an array of arrays.
fn sort_bit_width(sort: &Sort) -> u32 {
    match sort {
        Sort::BitVector(bits) | Sort::Float(bits) => *bits,
        Sort::Array { range, .. } => match range.as_ref() {
            Sort::BitVector(bits) | Sort::Float(bits) => *bits,
            Sort::Array { .. } => 0,
        },
    }
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

/// Supported IEEE 754 binary floating-point formats for concrete folding.
///
/// Unsupported widths (such as 16, 80, or 128 bits) are rejected rather than
/// widened or inferred as 64-bit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FloatFormat {
    Binary32,
    Binary64,
}

impl FloatFormat {
    pub const fn from_width_bits(width: u32) -> Option<Self> {
        match width {
            32 => Some(Self::Binary32),
            64 => Some(Self::Binary64),
            _ => None,
        }
    }

    pub const fn width_bits(self) -> u32 {
        match self {
            Self::Binary32 => 32,
            Self::Binary64 => 64,
        }
    }

    pub const fn canonical_qnan_bits(self) -> u64 {
        match self {
            Self::Binary32 => 0x7fc0_0000,
            Self::Binary64 => 0x7ff8_0000_0000_0000,
        }
    }
}

/// Rounding mode assumed when folding concrete IEEE 754 operations.
///
/// Fission currently models only round-to-nearest ties-to-even (native IEEE 754
/// default in Rust `f32`/`f64` arithmetic on IEEE-compliant targets without
/// host-level directed rounding overrides); operations that cannot guarantee
/// this profile must disable folding via [`FpFoldProfile::NO_ARITHMETIC_FOLD`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FloatRoundingMode {
    RoundNearestTiesToEven,
}

/// Policy for NaN inputs and NaN-producing operations during constant folding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NanFoldPolicy {
    /// Leave any expression whose operand or result is NaN unfolded (`FAdd`,
    /// `FDiv`, etc.) so downstream proof consumers preserve bit-pattern
    /// uncertainty instead of proving a spurious equality against one chosen
    /// NaN bit payload.
    PreserveUncertainty,
    /// Fold NaN results to the format's canonical quiet NaN pattern
    /// (`0x7fc0_0000` for Binary32, `0x7ff8_0000_0000_0000` for Binary64).
    /// Only valid when the caller explicitly accepts non-unique NaN payloads.
    CanonicalQuiet,
}

/// Explicit semantics profile gating concrete floating-point folding.
///
/// Exception/sticky status flags (invalid, divide-by-zero, overflow,
/// underflow, inexact) are not produced by AST nodes under any profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FpFoldProfile {
    pub rounding: FloatRoundingMode,
    pub fold_arithmetic: bool,
    pub fold_predicates: bool,
    pub nan_policy: NanFoldPolicy,
}

impl FpFoldProfile {
    /// Default conservative profile: native target-width IEEE 754 RNE for
    /// finite/zero/subnormal/infinity results, while keeping NaN inputs and
    /// NaN-producing operations symbolic to preserve payload uncertainty.
    pub const STRICT_IEEE754_RNE: Self = Self {
        rounding: FloatRoundingMode::RoundNearestTiesToEven,
        fold_arithmetic: true,
        fold_predicates: true,
        nan_policy: NanFoldPolicy::PreserveUncertainty,
    };

    /// Disable arithmetic folding completely while retaining numeric
    /// comparison folding under IEEE 754 value semantics.
    pub const NO_ARITHMETIC_FOLD: Self = Self {
        rounding: FloatRoundingMode::RoundNearestTiesToEven,
        fold_arithmetic: false,
        fold_predicates: true,
        nan_policy: NanFoldPolicy::PreserveUncertainty,
    };
}

impl Default for FpFoldProfile {
    fn default() -> Self {
        Self::STRICT_IEEE754_RNE
    }
}

#[derive(Clone, Copy, Debug)]
enum ConcreteFloat {
    F32(f32),
    F64(f64),
}

impl ConcreteFloat {
    fn from_expr(e: &SymExpr) -> Option<Self> {
        match e {
            SymExpr::Const { val, size } => match FloatFormat::from_width_bits(*size)? {
                FloatFormat::Binary32 => {
                    Some(Self::F32(f32::from_bits((*val & 0xffff_ffff) as u32)))
                }
                FloatFormat::Binary64 => Some(Self::F64(f64::from_bits(*val))),
            },
            _ => None,
        }
    }

    fn is_nan(self) -> bool {
        match self {
            Self::F32(x) => x.is_nan(),
            Self::F64(x) => x.is_nan(),
        }
    }

    fn format(self) -> FloatFormat {
        match self {
            Self::F32(_) => FloatFormat::Binary32,
            Self::F64(_) => FloatFormat::Binary64,
        }
    }

    fn to_expr(self, nan_policy: NanFoldPolicy) -> Option<SymExpr> {
        if self.is_nan() {
            return match nan_policy {
                NanFoldPolicy::PreserveUncertainty => None,
                NanFoldPolicy::CanonicalQuiet => Some(SymExpr::Const {
                    val: self.format().canonical_qnan_bits(),
                    size: self.format().width_bits(),
                }),
            };
        }
        let (val, size) = match self {
            Self::F32(x) => (u64::from(x.to_bits()), 32),
            Self::F64(x) => (x.to_bits(), 64),
        };
        Some(SymExpr::Const { val, size })
    }
}

/// Format of a float or matching bit-pattern operand, when supported (`Binary32`
/// or `Binary64`); returns `None` rather than inferring `Binary64` on unknown widths.
pub fn checked_float_format(e: &SymExpr) -> Option<FloatFormat> {
    match e.get_sort() {
        Sort::Float(sz) | Sort::BitVector(sz) => FloatFormat::from_width_bits(sz),
        Sort::Array { .. } => None,
    }
}

fn fold_float_binop(
    a: &SymExpr,
    b: &SymExpr,
    profile: FpFoldProfile,
    f32_op: fn(f32, f32) -> f32,
    f64_op: fn(f64, f64) -> f64,
) -> Option<SymExpr> {
    if !profile.fold_arithmetic {
        return None;
    }
    let _ = profile.rounding;
    let res = match (ConcreteFloat::from_expr(a)?, ConcreteFloat::from_expr(b)?) {
        (ConcreteFloat::F32(x), ConcreteFloat::F32(y)) => {
            if profile.nan_policy == NanFoldPolicy::PreserveUncertainty
                && (x.is_nan() || y.is_nan())
            {
                return None;
            }
            ConcreteFloat::F32(f32_op(x, y))
        }
        (ConcreteFloat::F64(x), ConcreteFloat::F64(y)) => {
            if profile.nan_policy == NanFoldPolicy::PreserveUncertainty
                && (x.is_nan() || y.is_nan())
            {
                return None;
            }
            ConcreteFloat::F64(f64_op(x, y))
        }
        _ => return None,
    };
    res.to_expr(profile.nan_policy)
}

fn fold_float_pred(
    a: &SymExpr,
    b: &SymExpr,
    profile: FpFoldProfile,
    f32_pred: fn(f32, f32) -> bool,
    f64_pred: fn(f64, f64) -> bool,
) -> Option<SymExpr> {
    if !profile.fold_predicates {
        return None;
    }
    let _ = profile.rounding;
    let flag = match (ConcreteFloat::from_expr(a)?, ConcreteFloat::from_expr(b)?) {
        (ConcreteFloat::F32(x), ConcreteFloat::F32(y)) => f32_pred(x, y),
        (ConcreteFloat::F64(x), ConcreteFloat::F64(y)) => f64_pred(x, y),
        _ => return None,
    };
    Some(SymExpr::Const {
        val: u64::from(flag),
        size: 1,
    })
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
        let Some((wa, wb)) = pair_bv_widths(&a, &b) else {
            return Self::Add(Box::new(a), Box::new(b));
        };
        let width_bits = wa.max(wb);
        match (&a, &b) {
            (Self::Const { val: v1, size: s1 }, Self::Const { val: v2, size: s2 }) => {
                // The payload as lowering reads it: `size` bits and no more.
                let (v1, v2) = (v1 & bitvector_mask(*s1), v2 & bitvector_mask(*s2));
                Self::Const {
                    val: v1.wrapping_add(v2) & bitvector_mask(width_bits),
                    size: width_bits,
                }
            }
            (Self::Const { val: 0, size }, _) if *size <= wb => b,
            (_, Self::Const { val: 0, size }) if *size <= wa => a,
            _ => Self::Add(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_sub(a: SymExpr, b: SymExpr) -> Self {
        let Some((wa, _wb)) = pair_bv_widths(&a, &b) else {
            return Self::Sub(Box::new(a), Box::new(b));
        };
        match (&a, &b) {
            (Self::Const { val: v1, size: s1 }, Self::Const { val: v2, size: s2 }) => {
                let width_bits = (*s1).max(*s2);
                // The payload as lowering reads it: `size` bits and no more.
                let (v1, v2) = (v1 & bitvector_mask(*s1), v2 & bitvector_mask(*s2));
                Self::Const {
                    val: v1.wrapping_sub(v2) & bitvector_mask(width_bits),
                    size: width_bits,
                }
            }
            (_, Self::Const { val: 0, size }) if *size <= wa => a,
            (a_expr, b_expr) if a_expr == b_expr => {
                if let Some(width_bits) = pair_is_pure_supported_bv(a_expr, b_expr) {
                    Self::Const {
                        val: 0,
                        size: width_bits,
                    }
                } else {
                    Self::Sub(Box::new(a), Box::new(b))
                }
            }
            _ => Self::Sub(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_and(a: SymExpr, b: SymExpr) -> Self {
        let Some((wa, wb)) = pair_bv_widths(&a, &b) else {
            return Self::And(Box::new(a), Box::new(b));
        };
        let width_bits = wa.max(wb);
        match (&a, &b) {
            (Self::Const { val: v1, size: s1 }, Self::Const { val: v2, size: s2 }) => {
                // The payload as lowering reads it: `size` bits and no more.
                let (v1, v2) = (v1 & bitvector_mask(*s1), v2 & bitvector_mask(*s2));
                Self::Const {
                    val: v1 & v2 & bitvector_mask(width_bits),
                    size: width_bits,
                }
            }
            (Self::Const { val: 0, .. }, _) | (_, Self::Const { val: 0, .. }) => {
                if let Some(pure_width) = pair_is_pure_supported_bv(&a, &b) {
                    Self::Const {
                        val: 0,
                        size: pure_width,
                    }
                } else {
                    Self::And(Box::new(a), Box::new(b))
                }
            }
            (a_expr, b_expr) if a_expr == b_expr => a.clone(),
            _ => Self::And(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_xor(a: SymExpr, b: SymExpr) -> Self {
        let Some((wa, wb)) = pair_bv_widths(&a, &b) else {
            return Self::Xor(Box::new(a), Box::new(b));
        };
        let width_bits = wa.max(wb);
        match (&a, &b) {
            (Self::Const { val: v1, size: s1 }, Self::Const { val: v2, size: s2 }) => {
                // The payload as lowering reads it: `size` bits and no more.
                let (v1, v2) = (v1 & bitvector_mask(*s1), v2 & bitvector_mask(*s2));
                Self::Const {
                    val: (v1 ^ v2) & bitvector_mask(width_bits),
                    size: width_bits,
                }
            }
            (Self::Const { val: 0, size }, _) if *size <= wb => b,
            (_, Self::Const { val: 0, size }) if *size <= wa => a,
            (a_expr, b_expr) if a_expr == b_expr => {
                if let Some(pure_width) = pair_is_pure_supported_bv(a_expr, b_expr) {
                    Self::Const {
                        val: 0,
                        size: pure_width,
                    }
                } else {
                    Self::Xor(Box::new(a), Box::new(b))
                }
            }
            _ => Self::Xor(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_not(a: SymExpr) -> Self {
        // Keeps `a`, so the top-level sort is all a fold needs (see
        // `pair_bv_widths`).
        let Sort::BitVector(width_bits) = a.get_sort() else {
            let w = a.get_bit_width();
            return Self::Xor(
                Box::new(a),
                Box::new(Self::Const {
                    val: bitvector_mask(w),
                    size: w,
                }),
            );
        };
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
                let mask = bitvector_mask(width_bits);
                Self::new_xor(
                    a,
                    Self::Const {
                        val: mask,
                        size: width_bits,
                    },
                )
            }
        }
    }

    pub fn new_eq(a: SymExpr, b: SymExpr) -> Self {
        if pair_bv_widths(&a, &b).is_none() {
            return Self::Eq(Box::new(a), Box::new(b));
        }
        match (&a, &b) {
            (Self::Const { val: v1, size: s1 }, Self::Const { val: v2, size: s2 }) => {
                // The payload as lowering reads it: `size` bits and no more.
                let (v1, v2) = (v1 & bitvector_mask(*s1), v2 & bitvector_mask(*s2));
                Self::Const {
                    val: if v1 == v2 { 1 } else { 0 },
                    size: 1,
                }
            }
            (a_expr, b_expr) if a_expr == b_expr => {
                if pair_is_pure_supported_bv(a_expr, b_expr).is_some() {
                    Self::Const { val: 1, size: 1 }
                } else {
                    Self::Eq(Box::new(a), Box::new(b))
                }
            }
            _ => Self::Eq(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_neq(a: SymExpr, b: SymExpr) -> Self {
        if pair_bv_widths(&a, &b).is_none() {
            return Self::Neq(Box::new(a), Box::new(b));
        }
        match (&a, &b) {
            (Self::Const { val: v1, size: s1 }, Self::Const { val: v2, size: s2 }) => {
                // The payload as lowering reads it: `size` bits and no more.
                let (v1, v2) = (v1 & bitvector_mask(*s1), v2 & bitvector_mask(*s2));
                Self::Const {
                    val: if v1 != v2 { 1 } else { 0 },
                    size: 1,
                }
            }
            (a_expr, b_expr) if a_expr == b_expr => {
                if pair_is_pure_supported_bv(a_expr, b_expr).is_some() {
                    Self::Const { val: 0, size: 1 }
                } else {
                    Self::Neq(Box::new(a), Box::new(b))
                }
            }
            _ => Self::Neq(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_ult(a: SymExpr, b: SymExpr) -> Self {
        if pair_bv_widths(&a, &b).is_none() {
            return Self::Ult(Box::new(a), Box::new(b));
        }
        match (&a, &b) {
            (Self::Const { val: v1, size: s1 }, Self::Const { val: v2, size: s2 }) => {
                // The payload as lowering reads it: `size` bits and no more.
                let (v1, v2) = (v1 & bitvector_mask(*s1), v2 & bitvector_mask(*s2));
                Self::Const {
                    val: if v1 < v2 { 1 } else { 0 },
                    size: 1,
                }
            }
            _ => Self::Ult(Box::new(a), Box::new(b)),
        }
    }

    /// Signed less-than: interpret both sides as two's-complement signed integers.
    pub fn new_slt(a: SymExpr, b: SymExpr) -> Self {
        let Some((wa, wb)) = pair_bv_widths(&a, &b) else {
            return Self::Slt(Box::new(a), Box::new(b));
        };
        if wa != wb {
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
        let Some((wa, wb)) = pair_bv_widths(&a, &b) else {
            return Self::Sle(Box::new(a), Box::new(b));
        };
        if wa != wb {
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
    /// 32- or 64-bit payload; other widths are preserved cleanly in `get_sort()`
    /// and rejected by [`checked_float_format`] / folding constructors.
    pub fn new_float_var(name: &str, width_bits: u32) -> Self {
        let id = VAR_COUNTER.fetch_add(1, Ordering::SeqCst);
        Self::Var {
            id,
            name: name.to_string(),
            sort: Sort::Float(width_bits),
        }
    }

    pub fn new_fadd_with_profile(a: SymExpr, b: SymExpr, profile: FpFoldProfile) -> Self {
        fold_float_binop(&a, &b, profile, |x, y| x + y, |x, y| x + y)
            .unwrap_or_else(|| Self::FAdd(Box::new(a), Box::new(b)))
    }

    pub fn new_fadd(a: SymExpr, b: SymExpr) -> Self {
        Self::new_fadd_with_profile(a, b, FpFoldProfile::STRICT_IEEE754_RNE)
    }

    pub fn new_fsub_with_profile(a: SymExpr, b: SymExpr, profile: FpFoldProfile) -> Self {
        fold_float_binop(&a, &b, profile, |x, y| x - y, |x, y| x - y)
            .unwrap_or_else(|| Self::FSub(Box::new(a), Box::new(b)))
    }

    pub fn new_fsub(a: SymExpr, b: SymExpr) -> Self {
        Self::new_fsub_with_profile(a, b, FpFoldProfile::STRICT_IEEE754_RNE)
    }

    pub fn new_fmul_with_profile(a: SymExpr, b: SymExpr, profile: FpFoldProfile) -> Self {
        fold_float_binop(&a, &b, profile, |x, y| x * y, |x, y| x * y)
            .unwrap_or_else(|| Self::FMul(Box::new(a), Box::new(b)))
    }

    pub fn new_fmul(a: SymExpr, b: SymExpr) -> Self {
        Self::new_fmul_with_profile(a, b, FpFoldProfile::STRICT_IEEE754_RNE)
    }

    pub fn new_fdiv_with_profile(a: SymExpr, b: SymExpr, profile: FpFoldProfile) -> Self {
        fold_float_binop(&a, &b, profile, |x, y| x / y, |x, y| x / y)
            .unwrap_or_else(|| Self::FDiv(Box::new(a), Box::new(b)))
    }

    pub fn new_fdiv(a: SymExpr, b: SymExpr) -> Self {
        Self::new_fdiv_with_profile(a, b, FpFoldProfile::STRICT_IEEE754_RNE)
    }

    /// Negation flips the sign bit and touches nothing else -- on the bit
    /// pattern, as lowering does, preserving sNaN/qNaN payloads intact.
    pub fn new_fneg(a: SymExpr) -> Self {
        match a {
            Self::Const {
                val,
                size: size @ (32 | 64),
            } => Self::Const {
                val: (val ^ (1u64 << (size - 1))) & bitvector_mask(size),
                size,
            },
            _ => Self::FNeg(Box::new(a)),
        }
    }

    /// Absolute value clears the sign bit; see [`Self::new_fneg`].
    pub fn new_fabs(a: SymExpr) -> Self {
        match a {
            Self::Const {
                val,
                size: size @ (32 | 64),
            } => Self::Const {
                val: val & !(1u64 << (size - 1)) & bitvector_mask(size),
                size,
            },
            _ => Self::FAbs(Box::new(a)),
        }
    }

    pub fn new_fsqrt_with_profile(a: SymExpr, profile: FpFoldProfile) -> Self {
        if profile.fold_arithmetic {
            let _ = profile.rounding;
            if let Some(cf) = ConcreteFloat::from_expr(&a) {
                let folded = match cf {
                    ConcreteFloat::F32(x)
                        if !(profile.nan_policy == NanFoldPolicy::PreserveUncertainty
                            && x.is_nan()) =>
                    {
                        ConcreteFloat::F32(x.sqrt()).to_expr(profile.nan_policy)
                    }
                    ConcreteFloat::F64(x)
                        if !(profile.nan_policy == NanFoldPolicy::PreserveUncertainty
                            && x.is_nan()) =>
                    {
                        ConcreteFloat::F64(x.sqrt()).to_expr(profile.nan_policy)
                    }
                    _ => None,
                };
                if let Some(expr) = folded {
                    return expr;
                }
            }
        }
        Self::FSqrt(Box::new(a))
    }

    pub fn new_fsqrt(a: SymExpr) -> Self {
        Self::new_fsqrt_with_profile(a, FpFoldProfile::STRICT_IEEE754_RNE)
    }

    /// IEEE 754 numeric equality (`+0.0 == -0.0`, `NaN != NaN`), distinct from
    /// raw bitvector equality (`Self::new_eq`).
    pub fn new_feq_with_profile(a: SymExpr, b: SymExpr, profile: FpFoldProfile) -> Self {
        fold_float_pred(&a, &b, profile, |x, y| x == y, |x, y| x == y)
            .unwrap_or_else(|| Self::FEq(Box::new(a), Box::new(b)))
    }

    pub fn new_feq(a: SymExpr, b: SymExpr) -> Self {
        Self::new_feq_with_profile(a, b, FpFoldProfile::STRICT_IEEE754_RNE)
    }

    /// IEEE 754 numeric inequality (`NaN != NaN` is true).
    pub fn new_fneq_with_profile(a: SymExpr, b: SymExpr, profile: FpFoldProfile) -> Self {
        fold_float_pred(&a, &b, profile, |x, y| x != y, |x, y| x != y)
            .unwrap_or_else(|| Self::FNeq(Box::new(a), Box::new(b)))
    }

    pub fn new_fneq(a: SymExpr, b: SymExpr) -> Self {
        Self::new_fneq_with_profile(a, b, FpFoldProfile::STRICT_IEEE754_RNE)
    }

    pub fn new_flt_with_profile(a: SymExpr, b: SymExpr, profile: FpFoldProfile) -> Self {
        fold_float_pred(&a, &b, profile, |x, y| x < y, |x, y| x < y)
            .unwrap_or_else(|| Self::FLt(Box::new(a), Box::new(b)))
    }

    pub fn new_flt(a: SymExpr, b: SymExpr) -> Self {
        Self::new_flt_with_profile(a, b, FpFoldProfile::STRICT_IEEE754_RNE)
    }

    pub fn new_fle_with_profile(a: SymExpr, b: SymExpr, profile: FpFoldProfile) -> Self {
        fold_float_pred(&a, &b, profile, |x, y| x <= y, |x, y| x <= y)
            .unwrap_or_else(|| Self::FLe(Box::new(a), Box::new(b)))
    }

    pub fn new_fle(a: SymExpr, b: SymExpr) -> Self {
        Self::new_fle_with_profile(a, b, FpFoldProfile::STRICT_IEEE754_RNE)
    }

    pub fn new_fisnan_with_profile(a: SymExpr, profile: FpFoldProfile) -> Self {
        if profile.fold_predicates {
            if let Some(cf) = ConcreteFloat::from_expr(&a) {
                return Self::Const {
                    val: u64::from(cf.is_nan()),
                    size: 1,
                };
            }
        }
        Self::FIsNan(Box::new(a))
    }

    pub fn new_fisnan(a: SymExpr) -> Self {
        Self::new_fisnan_with_profile(a, FpFoldProfile::STRICT_IEEE754_RNE)
    }

    /// Elaborate and validate `self` standalone, returning its checked sort.
    pub fn checked_sort(&self) -> Result<Sort, crate::validate::ValidationError> {
        crate::validate::validate_expr(self)
    }

    /// Elaborate and validate `self` standalone, returning its scalar bit width
    /// (or array element width for well-formed arrays).
    pub fn checked_bit_width(&self) -> Result<u32, crate::validate::ValidationError> {
        match self.checked_sort()? {
            Sort::BitVector(bits) | Sort::Float(bits) => Ok(bits),
            Sort::Array { range, .. } => match range.as_ref() {
                Sort::BitVector(bits) | Sort::Float(bits) => Ok(*bits),
                other @ Sort::Array { .. } => {
                    Err(crate::validate::ValidationError::NonScalarOperand {
                        op: "bit_width",
                        sort: other.clone(),
                    })
                }
            },
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
            | Self::FSqrt(a) => match a.get_sort() {
                Sort::Float(w) | Sort::BitVector(w) => Sort::Float(w),
                Sort::Array { .. } => Sort::Float(0),
            },
            Self::FEq(_, _)
            | Self::FNeq(_, _)
            | Self::FLt(_, _)
            | Self::FLe(_, _)
            | Self::FIsNan(_) => Sort::BitVector(1),
            Self::Ite { t, f, .. } => {
                // Each branch's sort is computed once: asking for the widths
                // again on a mismatch walked both branches twice per level,
                // exponential in the depth of a mixed-width `Ite` chain.
                let t_sort = t.get_sort();
                let f_sort = f.get_sort();
                match (&t_sort, &f_sort) {
                    _ if t_sort == f_sort => t_sort,
                    // A float branch and a same-width bit-pattern branch: the
                    // validator types this `Float`, and so must this, or a
                    // fold guarded on `BitVector` could accept it.
                    (Sort::Float(wt), Sort::BitVector(wf))
                    | (Sort::BitVector(wf), Sort::Float(wt))
                        if wt == wf =>
                    {
                        Sort::Float(*wt)
                    }
                    _ => Sort::BitVector(sort_bit_width(&t_sort).max(sort_bit_width(&f_sort))),
                }
            }
            Self::Extract { size, .. } => Sort::BitVector(*size),
            Self::Concat(a, b) => {
                Sort::BitVector(a.get_bit_width().saturating_add(b.get_bit_width()))
            }
            Self::ArraySelect { array, .. } => match array.get_sort() {
                Sort::Array { range, .. } => *range,
                _ => Sort::BitVector(0),
            },
            Self::ArrayStore { array, .. } => array.get_sort(),
        }
    }

    /// Return this expression's bit width. For arrays, this is the element width
    /// (or `0` for malformed/nested non-scalar terms).
    pub fn get_bit_width(&self) -> u32 {
        sort_bit_width(&self.get_sort())
    }

    /// Legacy name retained for source compatibility; the returned unit is bits.
    pub fn get_size(&self) -> u32 {
        self.get_bit_width()
    }

    /// Invoke `visit` once per node in preorder across `self` and all subexpressions.
    pub fn visit_preorder(&self, visit: &mut impl FnMut(&SymExpr)) {
        visit(self);
        match self {
            Self::Const { .. } | Self::Var { .. } => {}
            Self::FNeg(a)
            | Self::FAbs(a)
            | Self::FSqrt(a)
            | Self::FIsNan(a)
            | Self::Extract { expr: a, .. } => a.visit_preorder(visit),
            Self::Add(a, b)
            | Self::Sub(a, b)
            | Self::Mul(a, b)
            | Self::Udiv(a, b)
            | Self::Urem(a, b)
            | Self::Sdiv(a, b)
            | Self::Srem(a, b)
            | Self::Smod(a, b)
            | Self::And(a, b)
            | Self::Or(a, b)
            | Self::Xor(a, b)
            | Self::Shl(a, b)
            | Self::Lshr(a, b)
            | Self::Ashr(a, b)
            | Self::Eq(a, b)
            | Self::Neq(a, b)
            | Self::Ult(a, b)
            | Self::Ule(a, b)
            | Self::Slt(a, b)
            | Self::Sle(a, b)
            | Self::Sgt(a, b)
            | Self::FAdd(a, b)
            | Self::FSub(a, b)
            | Self::FMul(a, b)
            | Self::FDiv(a, b)
            | Self::FEq(a, b)
            | Self::FNeq(a, b)
            | Self::FLt(a, b)
            | Self::FLe(a, b)
            | Self::Concat(a, b)
            | Self::ArraySelect { array: a, index: b } => {
                a.visit_preorder(visit);
                b.visit_preorder(visit);
            }
            Self::Ite {
                cond: a,
                t: b,
                f: c,
            }
            | Self::ArrayStore {
                array: a,
                index: b,
                value: c,
            } => {
                a.visit_preorder(visit);
                b.visit_preorder(visit);
                c.visit_preorder(visit);
            }
        }
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

#[cfg(test)]
mod width_convention_tests {
    use super::*;

    /// `Sort::BitVector` and every `Const.size` are in bits. #293 said
    /// `new_slt` / `new_sle` multiplied by eight and shifted by 511 on a
    /// 64-bit constant; they fold at the stated width.
    #[test]
    fn signed_comparisons_fold_at_the_stated_bit_width() {
        let k = |val, size| SymExpr::Const { val, size };
        for (width, minus_one) in [(8u32, 0xffu64), (32, 0xffff_ffff), (64, u64::MAX)] {
            let lt = SymExpr::new_slt(k(minus_one, width), k(1, width));
            assert_eq!(lt, k(1, 1), "-1 <s 1 at {width} bits");
            let le = SymExpr::new_sle(k(1, width), k(minus_one, width));
            assert_eq!(le, k(0, 1), "1 <=s -1 at {width} bits");
        }
        assert_eq!(Sort::BitVector(8).byte_size(), 1);
        assert_eq!(Sort::BitVector(9).byte_size(), 2);
    }
}
