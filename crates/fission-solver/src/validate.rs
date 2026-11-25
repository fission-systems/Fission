//! Shared symbolic sort and variable-identity elaboration checks.
//!
//! All query entry points (`Solver`, `smtlib`, and algebraic simplification
//! shortcuts on `SymExpr`) elaborate expressions against one set of rules so
//! malformed sorts, conflicting variable IDs, overflowing bit widths, and
//! ill-typed array/ITE terms are rejected uniformly before constant folding,
//! bit-blasting, or SMT-LIB printing.
//!
//! # Mixed-width contract
//!
//! - **Unsigned bitvector operations and ITE branches** (`Add`, `Sub`, `Mul`,
//!   `Udiv`, `Urem`, `And`, `Or`, `Xor`, `Eq`, `Neq`, `Ult`, `Ule`, and
//!   two-BV `Ite`) intentionally accept differing non-zero bitvector widths by
//!   zero-extending the narrower operand to `max(lhs_bits, rhs_bits)`,
//!   matching both [`crate::aig::AigManager`] and [`crate::smtlib`].
//! - **Equal-width operations** (`Sdiv`, `Srem`, `Smod`, `Slt`, `Sle`, `Sgt`,
//!   `Shl`, `Lshr`, `Ashr`, binary float operations, and `Float`-sorted
//!   `Eq`/`Neq`/`Ite`) require matching operand bit widths with no implicit
//!   padding or extension.
//! - **Concrete bit-pattern constants**: `SymExpr::Const { size, .. }` may
//!   supply an IEEE bit-pattern payload to float operations and float
//!   comparisons/ITEs when `size` matches the peer float width.

use std::collections::{BTreeMap, HashMap};
use std::fmt;

use crate::ast::{Sort, SymExpr, SymNodeId};

/// Maximum expression AST and nested array sort recursion depth accepted by
/// elaboration.
pub const MAX_EXPR_DEPTH: usize = 2048;

/// Structured well-formedness error raised during expression, variable, or
/// query elaboration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    DepthExceeded {
        limit: usize,
    },
    ZeroWidth {
        context: &'static str,
    },
    ConflictingVarSort {
        id: SymNodeId,
        existing: Sort,
        found: Sort,
    },
    NonScalarOperand {
        op: &'static str,
        sort: Sort,
    },
    FloatOperandInBitVectorOp {
        op: &'static str,
        sort: Sort,
    },
    InvalidFloatOperand {
        op: &'static str,
        sort: Sort,
    },
    MismatchedOperandWidths {
        op: &'static str,
        lhs_bits: u32,
        rhs_bits: u32,
    },
    InvalidExtract {
        lsb: u32,
        size: u32,
        source_bits: u32,
    },
    ConcatWidthOverflow {
        lhs_bits: u32,
        rhs_bits: u32,
    },
    NonArrayTarget {
        op: &'static str,
        sort: Sort,
    },
    ArrayIndexSortMismatch {
        op: &'static str,
        expected: Sort,
        found: Sort,
    },
    ArrayValueSortMismatch {
        op: &'static str,
        expected: Sort,
        found: Sort,
    },
    InvalidIteCondition {
        sort: Sort,
    },
    IncompatibleIteBranches {
        then_sort: Sort,
        else_sort: Sort,
    },
    InvalidAssertionSort {
        sort: Sort,
    },
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DepthExceeded { limit } => {
                write!(f, "expression nesting exceeds depth limit ({limit})")
            }
            Self::ZeroWidth { context } => write!(f, "zero-width sort in {context}"),
            Self::ConflictingVarSort {
                id,
                existing,
                found,
            } => write!(
                f,
                "variable id {id} declared with conflicting sorts: {existing:?} vs {found:?}"
            ),
            Self::NonScalarOperand { op, sort } => {
                write!(f, "{op} expects scalar operand, got {sort:?}")
            }
            Self::FloatOperandInBitVectorOp { op, sort } => {
                write!(f, "{op} requires bitvector operand, got {sort:?}")
            }
            Self::InvalidFloatOperand { op, sort } => {
                write!(
                    f,
                    "{op} requires float operand or bit-pattern constant, got {sort:?}"
                )
            }
            Self::MismatchedOperandWidths {
                op,
                lhs_bits,
                rhs_bits,
            } => write!(
                f,
                "{op} requires matching operand widths, got {lhs_bits} and {rhs_bits} bits"
            ),
            Self::InvalidExtract {
                lsb,
                size,
                source_bits,
            } => write!(
                f,
                "invalid Extract(lsb={lsb}, size={size}) on {source_bits}-bit value"
            ),
            Self::ConcatWidthOverflow { lhs_bits, rhs_bits } => {
                write!(f, "Concat width overflows u32 ({lhs_bits} + {rhs_bits})")
            }
            Self::NonArrayTarget { op, sort } => {
                write!(f, "{op} expects an array target, got {sort:?}")
            }
            Self::ArrayIndexSortMismatch {
                op,
                expected,
                found,
            } => write!(
                f,
                "{op} index sort mismatch: expected {expected:?}, got {found:?}"
            ),
            Self::ArrayValueSortMismatch {
                op,
                expected,
                found,
            } => write!(
                f,
                "{op} value sort mismatch: expected {expected:?}, got {found:?}"
            ),
            Self::InvalidIteCondition { sort } => {
                write!(f, "Ite condition must be BitVector(1), got {sort:?}")
            }
            Self::IncompatibleIteBranches {
                then_sort,
                else_sort,
            } => write!(
                f,
                "incompatible Ite branch sorts: {then_sort:?} vs {else_sort:?}"
            ),
            Self::InvalidAssertionSort { sort } => {
                write!(f, "assertion must have sort BitVector(1), got {sort:?}")
            }
        }
    }
}

impl std::error::Error for ValidationError {}

/// Elaborated variable-sort summary produced by validating a query.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ValidatedQuery {
    pub vars: BTreeMap<SymNodeId, Sort>,
    pub has_arrays: bool,
}

/// Variable-sort consistency environment accumulated across one query, script,
/// or expression pair.
#[derive(Debug, Clone, Default)]
pub struct VarSortEnv {
    vars: BTreeMap<SymNodeId, Sort>,
    has_arrays: bool,
}

impl VarSortEnv {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bind_var(&mut self, id: SymNodeId, sort: &Sort) -> Result<(), ValidationError> {
        validate_sort_depth(sort, 0)?;
        if matches!(sort, Sort::Array { .. }) {
            self.has_arrays = true;
        }
        if let Some(existing) = self.vars.get(&id) {
            if existing != sort {
                return Err(ValidationError::ConflictingVarSort {
                    id,
                    existing: existing.clone(),
                    found: sort.clone(),
                });
            }
        } else {
            self.vars.insert(id, sort.clone());
        }
        Ok(())
    }

    #[must_use]
    pub fn into_summary(self) -> ValidatedQuery {
        ValidatedQuery {
            vars: self.vars,
            has_arrays: self.has_arrays,
        }
    }
}

fn validate_sort_depth(sort: &Sort, depth: usize) -> Result<(), ValidationError> {
    if depth > MAX_EXPR_DEPTH {
        return Err(ValidationError::DepthExceeded {
            limit: MAX_EXPR_DEPTH,
        });
    }
    match sort {
        Sort::BitVector(0) => Err(ValidationError::ZeroWidth {
            context: "BitVector sort",
        }),
        Sort::Float(0) => Err(ValidationError::ZeroWidth {
            context: "Float sort",
        }),
        Sort::BitVector(_) | Sort::Float(_) => Ok(()),
        Sort::Array { domain, range } => {
            validate_sort_depth(domain, depth + 1)?;
            validate_sort_depth(range, depth + 1)
        }
    }
}

/// Validate `expr` standalone (with a fresh [`VarSortEnv`]), returning its elaborated sort.
pub fn validate_expr(expr: &SymExpr) -> Result<Sort, ValidationError> {
    let mut env = VarSortEnv::new();
    validate_expr_in_env(expr, &mut env)
}

/// Validate `expr` against an existing [`VarSortEnv`], checking sort rules and
/// variable ID consistency across all visited nodes.
pub fn validate_expr_in_env(expr: &SymExpr, env: &mut VarSortEnv) -> Result<Sort, ValidationError> {
    validate_expr_depth(expr, env, 0)
}

/// Validate that `expr` elaborates to `Sort::BitVector(1)` in `env`.
pub fn validate_assertion_in_env(
    expr: &SymExpr,
    env: &mut VarSortEnv,
) -> Result<(), ValidationError> {
    let sort = validate_expr_depth(expr, env, 0)?;
    if sort != Sort::BitVector(1) {
        return Err(ValidationError::InvalidAssertionSort { sort });
    }
    Ok(())
}

/// Validate all assertions in one shared [`VarSortEnv`].
pub fn validate_assertions(assertions: &[SymExpr]) -> Result<ValidatedQuery, ValidationError> {
    let mut env = VarSortEnv::new();
    for assertion in assertions {
        validate_assertion_in_env(assertion, &mut env)?;
    }
    Ok(env.into_summary())
}

/// Validate registered `SymExpr::Var` declarations in `nodes` together with
/// `assertions` and `extra` assumptions in one shared [`VarSortEnv`].
pub fn validate_query_with_nodes(
    assertions: &[SymExpr],
    extra: &[SymExpr],
    nodes: &HashMap<SymNodeId, SymExpr>,
) -> Result<ValidatedQuery, ValidationError> {
    let mut env = VarSortEnv::new();
    let mut ids: Vec<SymNodeId> = nodes.keys().copied().collect();
    ids.sort_unstable();
    for reg_id in ids {
        if let Some(SymExpr::Var { id, sort, .. }) = nodes.get(&reg_id) {
            env.bind_var(*id, sort)?;
        }
    }
    for assertion in assertions {
        validate_assertion_in_env(assertion, &mut env)?;
    }
    for assumption in extra {
        validate_assertion_in_env(assumption, &mut env)?;
    }
    Ok(env.into_summary())
}

/// Verify that `expr` is well-formed in `env` and strictly within the pure
/// `1..=64`-bit bitvector subset (no floats, no arrays, no >64-bit BVs).
///
/// Algebraic shortcuts that erase an operand (`a - a => 0`, `a ^ a => 0`,
/// `0 & b => 0`, `a == a => 1`, `a != a => 0`) call this helper so malformed or
/// unmodeled operands are never simplified away before reaching a backend.
pub(crate) fn is_pure_supported_bv(expr: &SymExpr, env: &mut VarSortEnv) -> Result<u32, ()> {
    let sort = validate_expr_depth_pure_bv(expr, env, 0).map_err(|_| ())?;
    match sort {
        Sort::BitVector(bits) if (1..=64).contains(&bits) => Ok(bits),
        _ => Err(()),
    }
}

fn validate_expr_depth_pure_bv(
    expr: &SymExpr,
    env: &mut VarSortEnv,
    depth: usize,
) -> Result<Sort, ValidationError> {
    if depth > MAX_EXPR_DEPTH {
        return Err(ValidationError::DepthExceeded {
            limit: MAX_EXPR_DEPTH,
        });
    }
    let sort = match expr {
        SymExpr::Const { size, .. } => {
            if !(1..=64).contains(size) {
                return Err(ValidationError::ZeroWidth {
                    context: "unsupported pure-BV constant width",
                });
            }
            Sort::BitVector(*size)
        }
        SymExpr::Var { id, sort, .. } => {
            let Sort::BitVector(bits) = sort else {
                return Err(ValidationError::NonScalarOperand {
                    op: "pure-BV Var",
                    sort: sort.clone(),
                });
            };
            if !(1..=64).contains(bits) {
                return Err(ValidationError::ZeroWidth {
                    context: "unsupported pure-BV variable width",
                });
            }
            env.bind_var(*id, sort)?;
            sort.clone()
        }
        SymExpr::Add(a, b)
        | SymExpr::Sub(a, b)
        | SymExpr::Mul(a, b)
        | SymExpr::Udiv(a, b)
        | SymExpr::Urem(a, b)
        | SymExpr::And(a, b)
        | SymExpr::Or(a, b)
        | SymExpr::Xor(a, b) => {
            let wa = require_pure_bv(a, "pure-BV op", env, depth + 1)?;
            let wb = require_pure_bv(b, "pure-BV op", env, depth + 1)?;
            Sort::BitVector(wa.max(wb))
        }
        SymExpr::Sdiv(a, b)
        | SymExpr::Srem(a, b)
        | SymExpr::Smod(a, b)
        | SymExpr::Shl(a, b)
        | SymExpr::Lshr(a, b)
        | SymExpr::Ashr(a, b) => {
            let wa = require_pure_bv(a, "pure-BV op", env, depth + 1)?;
            let wb = require_pure_bv(b, "pure-BV op", env, depth + 1)?;
            if wa != wb {
                return Err(ValidationError::MismatchedOperandWidths {
                    op: "pure-BV op",
                    lhs_bits: wa,
                    rhs_bits: wb,
                });
            }
            Sort::BitVector(wa)
        }
        SymExpr::Eq(a, b) | SymExpr::Neq(a, b) | SymExpr::Ult(a, b) | SymExpr::Ule(a, b) => {
            let _ = require_pure_bv(a, "pure-BV cmp", env, depth + 1)?;
            let _ = require_pure_bv(b, "pure-BV cmp", env, depth + 1)?;
            Sort::BitVector(1)
        }
        SymExpr::Slt(a, b) | SymExpr::Sle(a, b) | SymExpr::Sgt(a, b) => {
            let wa = require_pure_bv(a, "pure-BV signed cmp", env, depth + 1)?;
            let wb = require_pure_bv(b, "pure-BV signed cmp", env, depth + 1)?;
            if wa != wb {
                return Err(ValidationError::MismatchedOperandWidths {
                    op: "pure-BV signed cmp",
                    lhs_bits: wa,
                    rhs_bits: wb,
                });
            }
            Sort::BitVector(1)
        }
        SymExpr::Ite { cond, t, f } => {
            let wc = require_pure_bv(cond, "Ite", env, depth + 1)?;
            if wc != 1 {
                return Err(ValidationError::InvalidIteCondition {
                    sort: Sort::BitVector(wc),
                });
            }
            let wt = require_pure_bv(t, "Ite", env, depth + 1)?;
            let wf = require_pure_bv(f, "Ite", env, depth + 1)?;
            Sort::BitVector(wt.max(wf))
        }
        SymExpr::Extract { expr, lsb, size } => {
            let src_bits = require_pure_bv(expr, "Extract", env, depth + 1)?;
            if *size == 0 || lsb.checked_add(*size).is_none_or(|end| end > src_bits) {
                return Err(ValidationError::InvalidExtract {
                    lsb: *lsb,
                    size: *size,
                    source_bits: src_bits,
                });
            }
            Sort::BitVector(*size)
        }
        SymExpr::Concat(a, b) => {
            let wa = require_pure_bv(a, "Concat", env, depth + 1)?;
            let wb = require_pure_bv(b, "Concat", env, depth + 1)?;
            let sum = wa
                .checked_add(wb)
                .filter(|&w| (1..=64).contains(&w))
                .ok_or(ValidationError::ConcatWidthOverflow {
                    lhs_bits: wa,
                    rhs_bits: wb,
                })?;
            Sort::BitVector(sum)
        }
        SymExpr::FAdd(..)
        | SymExpr::FSub(..)
        | SymExpr::FMul(..)
        | SymExpr::FDiv(..)
        | SymExpr::FNeg(..)
        | SymExpr::FAbs(..)
        | SymExpr::FSqrt(..)
        | SymExpr::FEq(..)
        | SymExpr::FNeq(..)
        | SymExpr::FLt(..)
        | SymExpr::FLe(..)
        | SymExpr::FIsNan(..) => {
            return Err(ValidationError::FloatOperandInBitVectorOp {
                op: "pure-BV",
                sort: Sort::Float(1),
            });
        }
        SymExpr::ArraySelect { .. } | SymExpr::ArrayStore { .. } => {
            return Err(ValidationError::NonScalarOperand {
                op: "pure-BV",
                sort: Sort::Array {
                    domain: Box::new(Sort::BitVector(1)),
                    range: Box::new(Sort::BitVector(1)),
                },
            });
        }
    };
    Ok(sort)
}

fn require_pure_bv(
    expr: &SymExpr,
    op: &'static str,
    env: &mut VarSortEnv,
    depth: usize,
) -> Result<u32, ValidationError> {
    match validate_expr_depth_pure_bv(expr, env, depth)? {
        Sort::BitVector(bits) if (1..=64).contains(&bits) => Ok(bits),
        other => Err(ValidationError::NonScalarOperand { op, sort: other }),
    }
}

fn expect_bv(
    expr: &SymExpr,
    op: &'static str,
    env: &mut VarSortEnv,
    depth: usize,
) -> Result<u32, ValidationError> {
    match validate_expr_depth(expr, env, depth)? {
        Sort::BitVector(bits) => Ok(bits),
        sort @ Sort::Float(_) => Err(ValidationError::FloatOperandInBitVectorOp { op, sort }),
        sort @ Sort::Array { .. } => Err(ValidationError::NonScalarOperand { op, sort }),
    }
}

fn expect_float_like(
    expr: &SymExpr,
    op: &'static str,
    env: &mut VarSortEnv,
    depth: usize,
) -> Result<u32, ValidationError> {
    let sort = validate_expr_depth(expr, env, depth)?;
    match sort {
        Sort::Float(bits) => Ok(bits),
        Sort::BitVector(bits) if matches!(expr, SymExpr::Const { .. }) => Ok(bits),
        Sort::BitVector(_) => Err(ValidationError::InvalidFloatOperand { op, sort }),
        Sort::Array { .. } => Err(ValidationError::NonScalarOperand { op, sort }),
    }
}

fn validate_expr_depth(
    expr: &SymExpr,
    env: &mut VarSortEnv,
    depth: usize,
) -> Result<Sort, ValidationError> {
    if depth > MAX_EXPR_DEPTH {
        return Err(ValidationError::DepthExceeded {
            limit: MAX_EXPR_DEPTH,
        });
    }
    match expr {
        SymExpr::Const { size, .. } => {
            if *size == 0 {
                return Err(ValidationError::ZeroWidth {
                    context: "Const size",
                });
            }
            Ok(Sort::BitVector(*size))
        }
        SymExpr::Var { id, sort, .. } => {
            env.bind_var(*id, sort)?;
            Ok(sort.clone())
        }
        SymExpr::Add(a, b) => validate_mixed_bv("Add", a, b, env, depth),
        SymExpr::Sub(a, b) => validate_mixed_bv("Sub", a, b, env, depth),
        SymExpr::Mul(a, b) => validate_mixed_bv("Mul", a, b, env, depth),
        SymExpr::Udiv(a, b) => validate_mixed_bv("Udiv", a, b, env, depth),
        SymExpr::Urem(a, b) => validate_mixed_bv("Urem", a, b, env, depth),
        SymExpr::And(a, b) => validate_mixed_bv("And", a, b, env, depth),
        SymExpr::Or(a, b) => validate_mixed_bv("Or", a, b, env, depth),
        SymExpr::Xor(a, b) => validate_mixed_bv("Xor", a, b, env, depth),
        SymExpr::Sdiv(a, b) => validate_equal_bv("Sdiv", a, b, env, depth),
        SymExpr::Srem(a, b) => validate_equal_bv("Srem", a, b, env, depth),
        SymExpr::Smod(a, b) => validate_equal_bv("Smod", a, b, env, depth),
        SymExpr::Shl(a, b) => validate_equal_bv("Shl", a, b, env, depth),
        SymExpr::Lshr(a, b) => validate_equal_bv("Lshr", a, b, env, depth),
        SymExpr::Ashr(a, b) => validate_equal_bv("Ashr", a, b, env, depth),
        SymExpr::Eq(a, b) => validate_eq_neq("Eq", a, b, env, depth),
        SymExpr::Neq(a, b) => validate_eq_neq("Neq", a, b, env, depth),
        SymExpr::Ult(a, b) => {
            let _ = expect_bv(a, "Ult", env, depth + 1)?;
            let _ = expect_bv(b, "Ult", env, depth + 1)?;
            Ok(Sort::BitVector(1))
        }
        SymExpr::Ule(a, b) => {
            let _ = expect_bv(a, "Ule", env, depth + 1)?;
            let _ = expect_bv(b, "Ule", env, depth + 1)?;
            Ok(Sort::BitVector(1))
        }
        SymExpr::Slt(a, b) => {
            let wa = expect_bv(a, "Slt", env, depth + 1)?;
            let wb = expect_bv(b, "Slt", env, depth + 1)?;
            if wa != wb {
                return Err(ValidationError::MismatchedOperandWidths {
                    op: "Slt",
                    lhs_bits: wa,
                    rhs_bits: wb,
                });
            }
            Ok(Sort::BitVector(1))
        }
        SymExpr::Sle(a, b) => {
            let wa = expect_bv(a, "Sle", env, depth + 1)?;
            let wb = expect_bv(b, "Sle", env, depth + 1)?;
            if wa != wb {
                return Err(ValidationError::MismatchedOperandWidths {
                    op: "Sle",
                    lhs_bits: wa,
                    rhs_bits: wb,
                });
            }
            Ok(Sort::BitVector(1))
        }
        SymExpr::Sgt(a, b) => {
            let wa = expect_bv(a, "Sgt", env, depth + 1)?;
            let wb = expect_bv(b, "Sgt", env, depth + 1)?;
            if wa != wb {
                return Err(ValidationError::MismatchedOperandWidths {
                    op: "Sgt",
                    lhs_bits: wa,
                    rhs_bits: wb,
                });
            }
            Ok(Sort::BitVector(1))
        }
        SymExpr::FNeg(a) => {
            let w = expect_float_like(a, "FNeg", env, depth + 1)?;
            Ok(Sort::Float(w))
        }
        SymExpr::FAbs(a) => {
            let w = expect_float_like(a, "FAbs", env, depth + 1)?;
            Ok(Sort::Float(w))
        }
        SymExpr::FSqrt(a) => {
            let w = expect_float_like(a, "FSqrt", env, depth + 1)?;
            Ok(Sort::Float(w))
        }
        SymExpr::FIsNan(a) => {
            let _ = expect_float_like(a, "FIsNan", env, depth + 1)?;
            Ok(Sort::BitVector(1))
        }
        SymExpr::FAdd(a, b) => validate_float_binop("FAdd", a, b, false, env, depth),
        SymExpr::FSub(a, b) => validate_float_binop("FSub", a, b, false, env, depth),
        SymExpr::FMul(a, b) => validate_float_binop("FMul", a, b, false, env, depth),
        SymExpr::FDiv(a, b) => validate_float_binop("FDiv", a, b, false, env, depth),
        SymExpr::FEq(a, b) => validate_float_binop("FEq", a, b, true, env, depth),
        SymExpr::FNeq(a, b) => validate_float_binop("FNeq", a, b, true, env, depth),
        SymExpr::FLt(a, b) => validate_float_binop("FLt", a, b, true, env, depth),
        SymExpr::FLe(a, b) => validate_float_binop("FLe", a, b, true, env, depth),
        SymExpr::Ite { cond, t, f } => {
            let cond_sort = validate_expr_depth(cond, env, depth + 1)?;
            if cond_sort != Sort::BitVector(1) {
                return Err(ValidationError::InvalidIteCondition { sort: cond_sort });
            }
            let t_sort = validate_expr_depth(t, env, depth + 1)?;
            let f_sort = validate_expr_depth(f, env, depth + 1)?;
            match (&t_sort, &f_sort) {
                (Sort::BitVector(wt), Sort::BitVector(wf)) => Ok(Sort::BitVector((*wt).max(*wf))),
                (Sort::Float(wt), Sort::Float(wf)) if wt == wf => Ok(Sort::Float(*wt)),
                (Sort::Float(wt), Sort::BitVector(wf))
                    if wt == wf && matches!(f.as_ref(), SymExpr::Const { .. }) =>
                {
                    Ok(Sort::Float(*wt))
                }
                (Sort::BitVector(wt), Sort::Float(wf))
                    if wt == wf && matches!(t.as_ref(), SymExpr::Const { .. }) =>
                {
                    Ok(Sort::Float(*wf))
                }
                (Sort::Array { .. }, Sort::Array { .. }) if t_sort == f_sort => Ok(t_sort),
                _ => Err(ValidationError::IncompatibleIteBranches {
                    then_sort: t_sort,
                    else_sort: f_sort,
                }),
            }
        }
        SymExpr::Extract { expr, lsb, size } => {
            let src_bits = expect_bv(expr, "Extract", env, depth + 1)?;
            if *size == 0 || lsb.checked_add(*size).is_none_or(|end| end > src_bits) {
                return Err(ValidationError::InvalidExtract {
                    lsb: *lsb,
                    size: *size,
                    source_bits: src_bits,
                });
            }
            Ok(Sort::BitVector(*size))
        }
        SymExpr::Concat(a, b) => {
            let wa = expect_bv(a, "Concat", env, depth + 1)?;
            let wb = expect_bv(b, "Concat", env, depth + 1)?;
            let sum = wa
                .checked_add(wb)
                .ok_or(ValidationError::ConcatWidthOverflow {
                    lhs_bits: wa,
                    rhs_bits: wb,
                })?;
            Ok(Sort::BitVector(sum))
        }
        SymExpr::ArraySelect { array, index } => {
            let arr_sort = validate_expr_depth(array, env, depth + 1)?;
            let Sort::Array { domain, range } = arr_sort else {
                return Err(ValidationError::NonArrayTarget {
                    op: "ArraySelect",
                    sort: arr_sort,
                });
            };
            let idx_sort = validate_expr_depth(index, env, depth + 1)?;
            if idx_sort != *domain {
                return Err(ValidationError::ArrayIndexSortMismatch {
                    op: "ArraySelect",
                    expected: *domain,
                    found: idx_sort,
                });
            }
            Ok(*range)
        }
        SymExpr::ArrayStore {
            array,
            index,
            value,
        } => {
            let arr_sort = validate_expr_depth(array, env, depth + 1)?;
            let Sort::Array { domain, range } = arr_sort.clone() else {
                return Err(ValidationError::NonArrayTarget {
                    op: "ArrayStore",
                    sort: arr_sort,
                });
            };
            let idx_sort = validate_expr_depth(index, env, depth + 1)?;
            if idx_sort != *domain {
                return Err(ValidationError::ArrayIndexSortMismatch {
                    op: "ArrayStore",
                    expected: *domain,
                    found: idx_sort,
                });
            }
            let val_sort = validate_expr_depth(value, env, depth + 1)?;
            if val_sort != *range {
                return Err(ValidationError::ArrayValueSortMismatch {
                    op: "ArrayStore",
                    expected: *range,
                    found: val_sort,
                });
            }
            Ok(arr_sort)
        }
    }
}

fn validate_mixed_bv(
    op: &'static str,
    a: &SymExpr,
    b: &SymExpr,
    env: &mut VarSortEnv,
    depth: usize,
) -> Result<Sort, ValidationError> {
    let wa = expect_bv(a, op, env, depth + 1)?;
    let wb = expect_bv(b, op, env, depth + 1)?;
    Ok(Sort::BitVector(wa.max(wb)))
}

fn validate_equal_bv(
    op: &'static str,
    a: &SymExpr,
    b: &SymExpr,
    env: &mut VarSortEnv,
    depth: usize,
) -> Result<Sort, ValidationError> {
    let wa = expect_bv(a, op, env, depth + 1)?;
    let wb = expect_bv(b, op, env, depth + 1)?;
    if wa != wb {
        return Err(ValidationError::MismatchedOperandWidths {
            op,
            lhs_bits: wa,
            rhs_bits: wb,
        });
    }
    Ok(Sort::BitVector(wa))
}

fn validate_eq_neq(
    op: &'static str,
    a: &SymExpr,
    b: &SymExpr,
    env: &mut VarSortEnv,
    depth: usize,
) -> Result<Sort, ValidationError> {
    let sa = validate_expr_depth(a, env, depth + 1)?;
    let sb = validate_expr_depth(b, env, depth + 1)?;
    match (&sa, &sb) {
        (Sort::BitVector(_), Sort::BitVector(_)) => Ok(Sort::BitVector(1)),
        (Sort::Float(wa), Sort::Float(wb)) if wa == wb => Ok(Sort::BitVector(1)),
        (Sort::Float(wa), Sort::BitVector(wb))
            if wa == wb && matches!(b, SymExpr::Const { .. }) =>
        {
            Ok(Sort::BitVector(1))
        }
        (Sort::BitVector(wa), Sort::Float(wb))
            if wa == wb && matches!(a, SymExpr::Const { .. }) =>
        {
            Ok(Sort::BitVector(1))
        }
        (Sort::Float(wa), Sort::Float(wb))
        | (Sort::Float(wa), Sort::BitVector(wb))
        | (Sort::BitVector(wa), Sort::Float(wb))
            if wa != wb =>
        {
            Err(ValidationError::MismatchedOperandWidths {
                op,
                lhs_bits: *wa,
                rhs_bits: *wb,
            })
        }
        (Sort::Float(_), _) => Err(ValidationError::FloatOperandInBitVectorOp { op, sort: sa }),
        (_, Sort::Float(_)) => Err(ValidationError::FloatOperandInBitVectorOp { op, sort: sb }),
        (Sort::Array { .. }, _) => Err(ValidationError::NonScalarOperand { op, sort: sa }),
        (_, Sort::Array { .. }) => Err(ValidationError::NonScalarOperand { op, sort: sb }),
    }
}

fn validate_float_binop(
    op: &'static str,
    a: &SymExpr,
    b: &SymExpr,
    returns_bool: bool,
    env: &mut VarSortEnv,
    depth: usize,
) -> Result<Sort, ValidationError> {
    let wa = expect_float_like(a, op, env, depth + 1)?;
    let wb = expect_float_like(b, op, env, depth + 1)?;
    if wa != wb {
        return Err(ValidationError::MismatchedOperandWidths {
            op,
            lhs_bits: wa,
            rhs_bits: wb,
        });
    }
    if returns_bool {
        Ok(Sort::BitVector(1))
    } else {
        Ok(Sort::Float(wa))
    }
}
