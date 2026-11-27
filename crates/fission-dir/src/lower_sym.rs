//! Lowers `PreHirExpr`/`PreHirStmt` and `HirExpr`/`HirStmt` to `fission_solver::
//! ast::SymExpr` and asks the real SAT/SMT-style solver
//! ([`fission_solver::Solver`]) to either prove PreHIR and HIR equivalent
//! (`Unsat` on `prehir_result != hir_result`) or produce a genuine
//! counterexample (`Sat`) -- for **loop-free** functions only (see this
//! module's scope notes).
//!
//! `fission_solver::ast::SymExpr`'s `size` fields are bit counts, not byte
//! counts, despite `Sort::BitVector`'s doc comment -- confirmed directly
//! against `aig.rs`'s bit-blasting (`Const{val,size}` emits exactly `size`
//! bits, `Extract{lsb,size}` slices `bits[lsb..lsb+size]`), which is the
//! actual behavior `check_sat` runs on. This module uses bit counts
//! throughout to match, the same values [`crate::eval::width_of`] already
//! returns -- no bit/byte conversion needed.
//!
//! # Scope (symbolic tier, v1)
//!
//! Same base expression/statement support as [`crate::eval`] (concrete
//! tier), further restricted to:
//! - **No loops** (`While`/`DoWhile`/`For`): a general symbolic equivalence
//!   proof over loops needs either bounded unrolling (only proves
//!   equivalence up to N iterations) or genuine loop-invariant reasoning,
//!   both out of scope for v1. A function containing a loop is
//!   [`UnsupportedReason::ContainsLoop`] at this tier -- it still gets
//!   concrete-tier coverage via [`crate::diff`]/[`crate::ground_truth`].
//! - **No `Switch`/`Break`/`Continue`**: `If`/`Select`/`Goto`/`Label` cover
//!   the acyclic control-flow shapes this session's real corpus functions
//!   (`clamp`, `max`, ...) actually use; `Switch` is deferred rather than
//!   rushed (its fallthrough/break semantics interacting with path-
//!   condition forking is a real design decision, not free).
//! - **No `Div`/`Mod`**: the concrete tier traps on a zero divisor while the
//!   solver's division is total, so lowering one would prove a function the
//!   concrete tier cannot run. Refused until traps are modelled.
//!
//! # Semantics
//!
//! Every operator, conversion and condition follows the shared scalar
//! contract in [`crate::scalar_semantics`] -- the same one [`crate::eval`]
//! implements concretely, held to it by exhaustive and property tests. In
//! particular shifts use the full unsigned count (no `& 63`), run at the
//! wider of operand and result width, and saturate; and a narrowing
//! conversion into `Bool` is refused as ambiguous.
//!
//! # How it works
//!
//! Symbolic execution by explicit path enumeration: at each `If`, fork into
//! the then/else branches (each with its own cloned environment and the
//! path condition extended by the branch condition), recursively continue
//! processing the rest of the statement list from each surviving branch's
//! own state, and collect every `Return` reached as a `(path_condition,
//! value)` leaf. `Goto`/`Label` resolve exactly like [`crate::eval`]'s
//! concrete interpreter (scan the enclosing statement list for the target
//! label; propagate to the caller's list if not found there) -- the *target*
//! of a goto is always static in this IR, only whether a given goto fires is
//! symbolic. The leaves are combined into one closed-form `SymExpr` via a
//! right-fold of `Ite`s (mutually exclusive by construction, since each
//! fork ANDs in the branch condition or its negation).

use fission_solver::ast::SymExpr;
use std::collections::HashMap;
use std::fmt;

use fission_midend_core::ir::NirType;

/// Structured lowering failure distinguishing unsupported IR types from
/// invalid variable/parameter bindings, cyclic control flow, budget exhaustion,
/// and unsupported control/op constructs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LowerSymError {
    UnsupportedType(&'static str),
    InvalidBinding(String),
    UnsupportedConstruct(String),
    CyclicControlFlow(String),
    BudgetExceeded { steps: usize, limit: usize },
}

impl fmt::Display for LowerSymError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedType(msg) => write!(f, "lower_sym: {msg}"),
            Self::InvalidBinding(msg)
            | Self::UnsupportedConstruct(msg)
            | Self::CyclicControlFlow(msg) => f.write_str(msg),
            Self::BudgetExceeded { steps, limit } => write!(
                f,
                "lower_sym: path-exploration budget exceeded ({steps} > {limit} steps)"
            ),
        }
    }
}

impl std::error::Error for LowerSymError {}

/// Validate a type for exact bitvector lowering at the symbolic tier.
///
/// Returns `(bits, signed)` for `Bool` (`1`-bit unsigned) and `1..=64`-bit
/// integers. Rejects `0`-bit, `>64`-bit, floating-point, aggregate, pointer,
/// and unknown types instead of silently clamping them via [`crate::eval::width_of`].
pub(crate) fn checked_scalar_type(ty: &NirType) -> std::result::Result<(u32, bool), &'static str> {
    match ty {
        NirType::Bool => Ok((1, false)),
        NirType::Int { bits, signed } if (1..=64).contains(bits) => Ok((*bits, *signed)),
        NirType::Int { bits: 0, .. } => Err("symbolic tier rejects 0-bit integer type"),
        NirType::Int { .. } => Err("symbolic tier supports 1..=64-bit integers only"),
        NirType::Float { .. } => Err("symbolic tier does not model floating-point types"),
        NirType::Aggregate { .. } => Err("symbolic tier does not model aggregate types"),
        NirType::Ptr(_) => Err("symbolic tier does not model pointer types"),
        NirType::Unknown => Err("symbolic tier rejects unknown/untyped scalar values"),
    }
}

pub(crate) fn checked_scalar_width(ty: &NirType) -> std::result::Result<u32, &'static str> {
    checked_scalar_type(ty).map(|(bits, _)| bits)
}

/// Number of recursive `exec_from` calls before bailing -- guards against
/// pathological branch-count blowup (2^branches paths) the same way
/// `SESE_REGION_PROOF_BUDGET_CALLS` guards structuring's own region-proof
/// search: a call-count budget, not a wall-clock one.
pub const PATH_EXPLORE_BUDGET: usize = 4096;

/// How control left an explored symbolic path prior to ITE folding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolicExitKind {
    /// Normal value-returning `Return(Some(expr))`.
    ReturnValue(SymExpr),
    /// Void `Return(None)` without a modeled return value.
    VoidReturn,
    /// Control fell off the end of a body without returning.
    Fallthrough,
    /// `Goto(label)` whose target label does not exist in scope.
    UnresolvedGoto(String),
    /// `Goto` revisited a label already active along this path (goto cycle).
    CyclicGoto(String),
    /// Call to a non-returning callee reached on this path (not total value return).
    NonReturning(String),
    /// Path reached a modeled trap/fault rather than returning cleanly.
    Fault(String),
    /// Path reached an unmodeled construct instead of returning cleanly.
    Unsupported(String),
}

/// One symbolically explored execution path before folding into a closed-form ITE.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolicPathLeaf {
    pub path_cond: SymExpr,
    pub exit: SymbolicExitKind,
}

/// Pre-ITE lowered path set for one function body, preserving all path predicates
/// and exit kinds so coverage, exclusivity, and exit completeness can be checked
/// explicitly prior to equivalence solving.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredFunctionPaths {
    pub leaves: Vec<SymbolicPathLeaf>,
    pub explored_steps: usize,
    pub budget_limit: usize,
    pub return_bits: Option<u32>,
}

impl LoweredFunctionPaths {
    /// Return all leaves that terminate with a scalar `ReturnValue`.
    #[must_use]
    pub fn return_leaves(&self) -> Vec<(&SymExpr, &SymExpr)> {
        self.leaves
            .iter()
            .filter_map(|leaf| match &leaf.exit {
                SymbolicExitKind::ReturnValue(val) => Some((&leaf.path_cond, val)),
                _ => None,
            })
            .collect()
    }

    /// Return all leaves that do not terminate with a scalar `ReturnValue`.
    #[must_use]
    pub fn non_return_leaves(&self) -> Vec<&SymbolicPathLeaf> {
        self.leaves
            .iter()
            .filter(|leaf| !matches!(leaf.exit, SymbolicExitKind::ReturnValue(_)))
            .collect()
    }

    /// True if any path ended in fallthrough, void return, goto cycle,
    /// unresolved goto, fault, non-returning call, or unmodeled exit.
    #[must_use]
    pub fn has_non_return_leaves(&self) -> bool {
        self.leaves
            .iter()
            .any(|leaf| !matches!(leaf.exit, SymbolicExitKind::ReturnValue(_)))
    }

    /// Fold return leaves into a single right-nested ITE once exit completeness
    /// and width consistency hold.
    pub(crate) fn fold_return_ite(&self) -> std::result::Result<SymExpr, LowerSymError> {
        if let Some(label) = self.leaves.iter().find_map(|leaf| match &leaf.exit {
            SymbolicExitKind::CyclicGoto(label) => Some(label),
            _ => None,
        }) {
            return Err(LowerSymError::CyclicControlFlow(format!(
                "lower_sym: cyclic goto control flow detected at label '{label}'"
            )));
        }
        if let Some(label) = self.leaves.iter().find_map(|leaf| match &leaf.exit {
            SymbolicExitKind::UnresolvedGoto(label) => Some(label),
            _ => None,
        }) {
            return Err(LowerSymError::UnsupportedConstruct(format!(
                "lower_sym: goto '{label}' never resolved to a label in this function"
            )));
        }
        if let Some(msg) = self.leaves.iter().find_map(|leaf| match &leaf.exit {
            SymbolicExitKind::NonReturning(msg)
            | SymbolicExitKind::Fault(msg)
            | SymbolicExitKind::Unsupported(msg) => Some(msg),
            _ => None,
        }) {
            return Err(LowerSymError::UnsupportedConstruct(msg.clone()));
        }

        let return_leaves = self.return_leaves();
        if return_leaves.is_empty() {
            return Err(LowerSymError::UnsupportedConstruct(
                "lower_sym: no path through the function returns a value".to_string(),
            ));
        }
        if self.has_non_return_leaves() {
            return Err(LowerSymError::UnsupportedConstruct(
                "lower_sym: function has incomplete return paths (fallthrough or void return)"
                    .to_string(),
            ));
        }

        let mut iter = return_leaves.into_iter().rev();
        let (_, first_val) = iter.next().expect("non-empty return_leaves");
        let mut acc = first_val.clone();
        let ret_bits = first_val.get_bit_width();
        for (cond, value) in iter {
            let branch_bits = value.get_bit_width();
            if branch_bits != ret_bits {
                return Err(LowerSymError::UnsupportedConstruct(format!(
                    "lower_sym: return branches produce mismatched widths ({branch_bits}-bit vs {ret_bits}-bit)"
                )));
            }
            acc = SymExpr::Ite {
                cond: Box::new(cond.clone()),
                t: Box::new(value.clone()),
                f: Box::new(acc),
            };
        }
        Ok(acc)
    }
}

type LabelVisitKey = (usize, usize);

enum LeafKind {
    Return(Option<SymExpr>),
    Fallthrough(SymEnv, Vec<LabelVisitKey>),
    UnresolvedGoto(String, SymEnv, Vec<LabelVisitKey>),
    CyclicGoto(String),
}

struct Leaf {
    path_cond: SymExpr,
    kind: LeafKind,
}

#[derive(Clone, Default)]
struct SymEnv {
    values: HashMap<String, SymExpr>,
    types: HashMap<String, NirType>,
}

fn and(a: SymExpr, b: SymExpr) -> SymExpr {
    // No `new_and_bool`/short-circuit-aware helper on `SymExpr` for boolean
    // (size-1) conjunction specifically -- `Ite` is exact and simplifies
    // trivially for concrete conditions, unlike bitwise `And` which would
    // be equivalent here (both operands are always size-1) but reads as
    // arithmetic rather than logical conjunction.
    SymExpr::Ite {
        cond: Box::new(a),
        t: Box::new(b),
        f: Box::new(SymExpr::Const { val: 0, size: 1 }),
    }
}

fn sym_const(val: i64, ty: &NirType) -> std::result::Result<SymExpr, LowerSymError> {
    let bits = checked_scalar_width(ty).map_err(LowerSymError::UnsupportedType)?;
    let mask = if bits >= 64 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    Ok(SymExpr::Const {
        val: (val as u64) & mask,
        size: bits,
    })
}

/// Sign- or zero-extend / narrow `v` (currently `from_bits` wide) to
/// `to_bits`, matching [`crate::eval::normalize`]'s semantics for the
/// concrete tier.
pub(crate) fn resize(v: SymExpr, from_bits: u32, to_bits: u32, signed_from: bool) -> SymExpr {
    use std::cmp::Ordering;
    match to_bits.cmp(&from_bits) {
        Ordering::Equal => v,
        Ordering::Less => SymExpr::Extract {
            expr: Box::new(v),
            lsb: 0,
            size: to_bits,
        },
        Ordering::Greater => {
            let extra_bits = to_bits - from_bits;
            let high = if signed_from {
                let sign_bit_mask = 1u64 << (from_bits - 1);
                let sign_bit = SymExpr::new_and(
                    v.clone(),
                    SymExpr::Const {
                        val: sign_bit_mask,
                        size: from_bits,
                    },
                );
                let is_negative = SymExpr::new_neq(
                    sign_bit,
                    SymExpr::Const {
                        val: 0,
                        size: from_bits,
                    },
                );
                let ones = SymExpr::Const {
                    val: if extra_bits >= 64 {
                        u64::MAX
                    } else {
                        (1u64 << extra_bits) - 1
                    },
                    size: extra_bits,
                };
                let zeros = SymExpr::Const {
                    val: 0,
                    size: extra_bits,
                };
                SymExpr::Ite {
                    cond: Box::new(is_negative),
                    t: Box::new(ones),
                    f: Box::new(zeros),
                }
            } else {
                SymExpr::Const {
                    val: 0,
                    size: extra_bits,
                }
            };
            SymExpr::Concat(Box::new(high), Box::new(v))
        }
    }
}

/// Generates one symbolic-lowering module for a `Stmt`/`Expr`/`LValue`/
/// `BinaryOp`/`UnaryOp`/`Binding`/`Function` family, mirroring [`crate::eval`]'s
/// `define_interp!` for the same reason: PreHIR-side and HIR-side lowering
/// logic generated from one macro body can't silently drift.
macro_rules! define_lower_sym {
    (
        $modname:ident, $ir_crate:path,
        $Stmt:ident, $Expr:ident, $LValue:ident, $BinaryOp:ident, $UnaryOp:ident,
        $Binding:ident, $Function:ident
    ) => {
        pub mod $modname {
            use std::collections::HashSet;

            use $ir_crate::{$BinaryOp, $Binding, $Expr, $LValue, $Stmt, $UnaryOp};
            use fission_midend_core::ir::NirType;
            use fission_solver::ast::{Sort, SymExpr};

            use super::{
                LabelVisitKey, Leaf, LeafKind, LowerSymError, LoweredFunctionPaths,
                PATH_EXPLORE_BUDGET, SymEnv, SymbolicExitKind, SymbolicPathLeaf, and,
                checked_scalar_type, checked_scalar_width, resize, sym_const,
            };
            use crate::scalar_semantics::{ShiftKind, shift_sym};
            use super::super::eval::is_signed;

            type LowerResult<T> = std::result::Result<T, LowerSymError>;

            /// Whether `body` contains a loop -- checked before attempting
            /// symbolic lowering at all (see [`crate::report::
            /// UnsupportedReason::ContainsLoop`]).
            pub fn contains_loop(stmts: &[$Stmt]) -> bool {
                stmts.iter().any(stmt_contains_loop)
            }

            fn stmt_contains_loop(stmt: &$Stmt) -> bool {
                match stmt {
                    $Stmt::While { .. } | $Stmt::DoWhile { .. } | $Stmt::For { .. } => true,
                    $Stmt::Block(b) => contains_loop(b),
                    $Stmt::If { then_body, else_body, .. } => {
                        contains_loop(then_body) || contains_loop(else_body)
                    }
                    $Stmt::Switch { cases, default, .. } => {
                        cases.iter().any(|c| contains_loop(&c.body)) || contains_loop(default)
                    }
                    _ => false,
                }
            }

            fn checked_ty(ty: &NirType) -> LowerResult<(u32, bool)> {
                checked_scalar_type(ty).map_err(LowerSymError::UnsupportedType)
            }

            fn checked_width(ty: &NirType) -> LowerResult<u32> {
                checked_scalar_width(ty).map_err(LowerSymError::UnsupportedType)
            }

            fn infer_ty(expr: &$Expr, env: &SymEnv) -> LowerResult<NirType> {
                match expr {
                    $Expr::Var(name) => {
                        let e = env.values.get(name).ok_or_else(|| {
                            LowerSymError::UnsupportedConstruct(format!(
                                "lower_sym: read of undeclared variable '{name}'"
                            ))
                        })?;
                        match env.types.get(name) {
                            Some(ty) => Ok(ty.clone()),
                            None => Ok(int_ty_of_size(bit_width(e)?)),
                        }
                    }
                    $Expr::Const(_, ty)
                    | $Expr::Cast { ty, .. }
                    | $Expr::Unary { ty, .. }
                    | $Expr::Binary { ty, .. }
                    | $Expr::Select { ty, .. } => Ok(ty.clone()),
                    other => Err(LowerSymError::UnsupportedConstruct(format!(
                        "lower_sym: cannot infer type of unsupported expr {other:?}"
                    ))),
                }
            }

            fn bit_width(e: &SymExpr) -> LowerResult<u32> {
                match e.get_sort() {
                    Sort::BitVector(bits) => Ok(bits),
                    other => Err(LowerSymError::UnsupportedType(match other {
                        Sort::Float { .. } => "symbolic tier does not model floating-point types",
                        _ => "symbolic tier requires bitvector sorts only",
                    })),
                }
            }

            /// Fallback unsigned integer type used only when a variable has a
            /// value in `SymEnv` without a separate declared `NirType`.
            fn int_ty_of_size(bits: u32) -> NirType {
                NirType::Int { bits, signed: false }
            }

            /// Convert a `from_ty` value to `to_ty` under the shared contract
            /// (`crate::scalar_semantics`): truncate when narrowing, extend by
            /// the *source* signedness when widening. A narrowing conversion
            /// of a multi-bit value into `Bool` is refused: p-code truncates,
            /// the C the printer writes (`(bool)x`) tests `x != 0`, and a
            /// proof under either reading would certify the other wrongly.
            fn convert(v: SymExpr, from_ty: &NirType, to_ty: &NirType) -> LowerResult<SymExpr> {
                let (from_bits, from_signed) = checked_ty(from_ty)?;
                convert_bits(v, from_bits, from_signed, to_ty)
            }

            fn convert_bits(
                v: SymExpr,
                from_bits: u32,
                from_signed: bool,
                to_ty: &NirType,
            ) -> LowerResult<SymExpr> {
                let to_bits = checked_width(to_ty)?;
                let actual = bit_width(&v)?;
                if actual != from_bits {
                    return Err(LowerSymError::UnsupportedConstruct(format!(
                        "lower_sym: {actual}-bit value converted as a {from_bits}-bit one -- \
                         refusing an inconsistent lowering"
                    )));
                }
                if matches!(to_ty, NirType::Bool) && from_bits > 1 {
                    return Err(LowerSymError::UnsupportedConstruct(format!(
                        "lower_sym: narrowing conversion of a {from_bits}-bit value into Bool is \
                         ambiguous (p-code truncation vs C `!= 0`) -- refused at the symbolic tier"
                    )));
                }
                Ok(resize(v, from_bits, to_bits, from_signed))
            }

            /// `v != 0` at `v`'s own width, as one bit.
            fn truth(v: SymExpr) -> LowerResult<SymExpr> {
                let bits = bit_width(&v)?;
                Ok(SymExpr::new_neq(v, SymExpr::Const { val: 0, size: bits }))
            }

            /// Lower `expr` and return it with its declared type, checking
            /// the invariant every conversion relies on: the lowered value is
            /// exactly as wide as the declared type says.
            fn eval_typed(expr: &$Expr, env: &SymEnv) -> LowerResult<(SymExpr, NirType)> {
                let ty = infer_ty(expr, env)?;
                let bits = checked_width(&ty)?;
                let v = eval_untyped(expr, env)?;
                let got = bit_width(&v)?;
                if got != bits {
                    return Err(LowerSymError::UnsupportedConstruct(format!(
                        "lower_sym: lowered a {got}-bit value for a {bits}-bit declared type -- \
                         refusing an inconsistent lowering"
                    )));
                }
                Ok((v, ty))
            }

            fn eval_untyped(expr: &$Expr, env: &SymEnv) -> LowerResult<SymExpr> {
                match expr {
                    $Expr::Var(name) => {
                        if let Some(declared_ty) = env.types.get(name) {
                            let _ = checked_ty(declared_ty)?;
                        }
                        env.values.get(name).cloned().ok_or_else(|| {
                            LowerSymError::UnsupportedConstruct(format!(
                                "lower_sym: read of undeclared variable '{name}'"
                            ))
                        })
                    }
                    $Expr::Const(v, ty) => sym_const(*v, ty),
                    $Expr::Cast { ty, expr } => {
                        let (inner, from_ty) = eval_typed(expr, env)?;
                        convert(inner, &from_ty, ty)
                    }
                    $Expr::Unary { op, expr, ty } => {
                        let bits = checked_width(ty)?;
                        let (v, v_ty) = eval_typed(expr, env)?;
                        Ok(match op {
                            $UnaryOp::Neg => SymExpr::new_sub(
                                SymExpr::Const { val: 0, size: bits },
                                convert(v, &v_ty, ty)?,
                            ),
                            $UnaryOp::Not => {
                                let is_zero = SymExpr::new_not(truth(v)?);
                                resize(is_zero, 1, bits, false)
                            }
                            $UnaryOp::BitNot => SymExpr::new_not(convert(v, &v_ty, ty)?),
                        })
                    }
                    $Expr::Binary { op, lhs, rhs, ty } => eval_binary(op, lhs, rhs, ty, env),
                    $Expr::Select { cond, then_expr, else_expr, ty } => {
                        let _ = checked_width(ty)?;
                        let (c, _) = eval_typed(cond, env)?;
                        let (t, t_ty) = eval_typed(then_expr, env)?;
                        let (f, f_ty) = eval_typed(else_expr, env)?;
                        Ok(SymExpr::Ite {
                            cond: Box::new(truth(c)?),
                            t: Box::new(convert(t, &t_ty, ty)?),
                            f: Box::new(convert(f, &f_ty, ty)?),
                        })
                    }
                    other => Err(LowerSymError::UnsupportedConstruct(format!(
                        "lower_sym: unsupported expr {other:?} -- no memory/call/div/mod model at \
                         the symbolic tier, see module docs"
                    ))),
                }
            }

            fn eval_binary(
                op: &$BinaryOp,
                lhs: &$Expr,
                rhs: &$Expr,
                ty: &NirType,
                env: &SymEnv,
            ) -> LowerResult<SymExpr> {
                let result_bits = checked_width(ty)?;
                let (l, l_ty) = eval_typed(lhs, env)?;
                let (r, r_ty) = eval_typed(rhs, env)?;
                let (l_bits, l_signed) = checked_ty(&l_ty)?;
                let (r_bits, r_signed) = checked_ty(&r_ty)?;
                // Comparisons meet at the wider operand width, each operand
                // extended by its own signedness (never into `Bool`).
                let cmp_bits = l_bits.max(r_bits);
                let cmp = |v: SymExpr, signed: bool, bits: u32| {
                    convert_bits(v, bits, signed, &NirType::Int { bits: cmp_bits, signed })
                };
                let flag = |b: SymExpr| resize(b, 1, result_bits, false);

                Ok(match op {
                    $BinaryOp::Add => {
                        SymExpr::new_add(convert(l, &l_ty, ty)?, convert(r, &r_ty, ty)?)
                    }
                    $BinaryOp::Sub => {
                        SymExpr::new_sub(convert(l, &l_ty, ty)?, convert(r, &r_ty, ty)?)
                    }
                    $BinaryOp::Mul => SymExpr::Mul(
                        Box::new(convert(l, &l_ty, ty)?),
                        Box::new(convert(r, &r_ty, ty)?),
                    ),
                    $BinaryOp::And => {
                        SymExpr::new_and(convert(l, &l_ty, ty)?, convert(r, &r_ty, ty)?)
                    }
                    $BinaryOp::Or => SymExpr::Or(
                        Box::new(convert(l, &l_ty, ty)?),
                        Box::new(convert(r, &r_ty, ty)?),
                    ),
                    $BinaryOp::Xor => {
                        SymExpr::new_xor(convert(l, &l_ty, ty)?, convert(r, &r_ty, ty)?)
                    }
                    // The shift contract (`crate::scalar_semantics`): shift at
                    // `W = max(operand, result)` with the operand brought to `W`
                    // as the operator says, the count read unsigned at its own
                    // width (never truncated), saturating at `W`; then narrow.
                    $BinaryOp::Shl | $BinaryOp::Shr | $BinaryOp::Sar => {
                        let width = l_bits.max(result_bits);
                        let (kind, extend_signed) = match op {
                            $BinaryOp::Shl => (ShiftKind::Left, l_signed),
                            $BinaryOp::Shr => (ShiftKind::LogicalRight, false),
                            _ => (ShiftKind::ArithmeticRight, true),
                        };
                        let operand = resize(l, l_bits, width, extend_signed);
                        let shifted = shift_sym(kind, operand, width, r, r_bits);
                        convert_bits(shifted, width, false, ty)?
                    }
                    $BinaryOp::LogicalAnd => flag(and(truth(l)?, truth(r)?)),
                    $BinaryOp::LogicalOr => flag(SymExpr::new_not(and(
                        SymExpr::new_not(truth(l)?),
                        SymExpr::new_not(truth(r)?),
                    ))),
                    $BinaryOp::Eq => flag(SymExpr::new_eq(
                        cmp(l, l_signed, l_bits)?,
                        cmp(r, r_signed, r_bits)?,
                    )),
                    $BinaryOp::Ne => flag(SymExpr::new_neq(
                        cmp(l, l_signed, l_bits)?,
                        cmp(r, r_signed, r_bits)?,
                    )),
                    $BinaryOp::Lt => flag(SymExpr::new_ult(
                        cmp(l, l_signed, l_bits)?,
                        cmp(r, r_signed, r_bits)?,
                    )),
                    $BinaryOp::Le => flag(SymExpr::Ule(
                        Box::new(cmp(l, l_signed, l_bits)?),
                        Box::new(cmp(r, r_signed, r_bits)?),
                    )),
                    $BinaryOp::Gt => flag(SymExpr::new_ult(
                        cmp(r, r_signed, r_bits)?,
                        cmp(l, l_signed, l_bits)?,
                    )),
                    $BinaryOp::Ge => flag(SymExpr::Ule(
                        Box::new(cmp(r, r_signed, r_bits)?),
                        Box::new(cmp(l, l_signed, l_bits)?),
                    )),
                    $BinaryOp::SLt => flag(SymExpr::new_slt(
                        cmp(l, l_signed, l_bits)?,
                        cmp(r, r_signed, r_bits)?,
                    )),
                    $BinaryOp::SLe => flag(SymExpr::new_sle(
                        cmp(l, l_signed, l_bits)?,
                        cmp(r, r_signed, r_bits)?,
                    )),
                    $BinaryOp::SGt => flag(SymExpr::new_sgt(
                        cmp(l, l_signed, l_bits)?,
                        cmp(r, r_signed, r_bits)?,
                    )),
                    $BinaryOp::SGe => flag(SymExpr::new_not(SymExpr::new_slt(
                        cmp(l, l_signed, l_bits)?,
                        cmp(r, r_signed, r_bits)?,
                    ))),
                    $BinaryOp::Div | $BinaryOp::Mod => {
                        return Err(LowerSymError::UnsupportedConstruct(
                            "lower_sym: Div/Mod not supported at the symbolic tier -- the \
                             concrete tier traps on a zero divisor and the solver's division is \
                             total, see crate::scalar_semantics"
                                .to_string(),
                        ));
                    }
                })
            }

            fn find_label(stmts: &[$Stmt], label: &str) -> Option<usize> {
                stmts.iter().position(|s| matches!(s, $Stmt::Label(l) if l == label))
            }

            /// Explore every path through `stmts[start..]` under `path_cond`
            /// starting from `env`, returning one [`Leaf`] per way control
            /// can leave this call (a `Return`, a `Goto` this list's own
            /// scope can't resolve, a revisited-label `CyclicGoto`, or
            /// falling off the end).
            fn exec_from(
                stmts: &[$Stmt],
                start: usize,
                path_cond: SymExpr,
                env: SymEnv,
                mut visited_labels: Vec<LabelVisitKey>,
                budget: &mut usize,
            ) -> LowerResult<Vec<Leaf>> {
                *budget += 1;
                if *budget > PATH_EXPLORE_BUDGET {
                    return Err(LowerSymError::BudgetExceeded {
                        steps: *budget,
                        limit: PATH_EXPLORE_BUDGET,
                    });
                }
                let list_key = stmts.as_ptr() as usize;
                let mut idx = start;
                let mut env = env;
                loop {
                    if idx >= stmts.len() {
                        return Ok(vec![Leaf {
                            path_cond,
                            kind: LeafKind::Fallthrough(env, visited_labels),
                        }]);
                    }
                    match &stmts[idx] {
                        $Stmt::Label(label) => {
                            let key = (list_key, idx);
                            if visited_labels.contains(&key) {
                                return Ok(vec![Leaf {
                                    path_cond,
                                    kind: LeafKind::CyclicGoto(label.clone()),
                                }]);
                            }
                            visited_labels.push(key);
                            idx += 1;
                        }
                        $Stmt::Assign { lhs, rhs } => {
                            let name = match lhs {
                                $LValue::Var(name) => name,
                                other => {
                                    return Err(LowerSymError::UnsupportedConstruct(format!(
                                        "lower_sym: unsupported assignment target {other:?} -- no \
                                         memory model at the symbolic tier"
                                    )));
                                }
                            };
                            let (v, rhs_ty) = eval_typed(rhs, &env)?;
                            let target_ty = env
                                .types
                                .get(name)
                                .cloned()
                                .unwrap_or_else(|| rhs_ty.clone());
                            let resized = convert(v, &rhs_ty, &target_ty)?;
                            env.types.insert(name.clone(), target_ty);
                            env.values.insert(name.clone(), resized);
                            idx += 1;
                        }
                        $Stmt::Return(expr) => {
                            let v = match expr {
                                Some(e) => Some(eval_typed(e, &env)?.0),
                                None => None,
                            };
                            return Ok(vec![Leaf { path_cond, kind: LeafKind::Return(v) }]);
                        }
                        $Stmt::Goto(label) => {
                            return Ok(match find_label(stmts, label) {
                                Some(target) => {
                                    let key = (list_key, target);
                                    if visited_labels.contains(&key) {
                                        vec![Leaf {
                                            path_cond,
                                            kind: LeafKind::CyclicGoto(label.clone()),
                                        }]
                                    } else {
                                        visited_labels.push(key);
                                        exec_from(
                                            stmts,
                                            target + 1,
                                            path_cond,
                                            env,
                                            visited_labels,
                                            budget,
                                        )?
                                    }
                                }
                                None => vec![Leaf {
                                    path_cond,
                                    kind: LeafKind::UnresolvedGoto(
                                        label.clone(),
                                        env,
                                        visited_labels,
                                    ),
                                }],
                            });
                        }
                        $Stmt::If { cond, then_body, else_body } => {
                            let (c, _) = eval_typed(cond, &env)?;
                            let is_true = truth(c)?;
                            let is_false = SymExpr::new_not(is_true.clone());
                            let then_leaves = exec_from(
                                then_body,
                                0,
                                and(path_cond.clone(), is_true),
                                env.clone(),
                                visited_labels.clone(),
                                budget,
                            )?;
                            let else_leaves = exec_from(
                                else_body,
                                0,
                                and(path_cond.clone(), is_false),
                                env,
                                visited_labels,
                                budget,
                            )?;
                            let mut out = Vec::new();
                            for leaf in then_leaves.into_iter().chain(else_leaves) {
                                out.extend(continue_leaf(stmts, idx + 1, leaf, budget)?);
                            }
                            return Ok(out);
                        }
                        $Stmt::Block(inner) => {
                            let inner_leaves = exec_from(
                                inner,
                                0,
                                path_cond.clone(),
                                env,
                                visited_labels,
                                budget,
                            )?;
                            let mut out = Vec::new();
                            for leaf in inner_leaves {
                                out.extend(continue_leaf(stmts, idx + 1, leaf, budget)?);
                            }
                            return Ok(out);
                        }
                        $Stmt::Switch { .. } => {
                            return Err(LowerSymError::UnsupportedConstruct(
                                "lower_sym: Switch not supported at the symbolic tier".to_string(),
                            ));
                        }
                        $Stmt::While { .. } | $Stmt::DoWhile { .. } | $Stmt::For { .. } => {
                            return Err(LowerSymError::UnsupportedConstruct(
                                "lower_sym: loops not supported at the symbolic tier (see contains_loop)".to_string(),
                            ));
                        }
                        $Stmt::Break | $Stmt::Continue => {
                            return Err(LowerSymError::UnsupportedConstruct(
                                "lower_sym: break/continue not supported at the symbolic tier".to_string(),
                            ));
                        }
                        $Stmt::Expr(_) | $Stmt::VaStart { .. } => {
                            return Err(LowerSymError::UnsupportedConstruct(
                                "lower_sym: unsupported statement at the symbolic tier".to_string(),
                            ));
                        }
                    }
                }
            }

            /// Resolve one leaf from a nested (`If`/`Block`) scope against
            /// the *enclosing* `stmts` list: a `Return` or `CyclicGoto` is
            /// terminal as-is; a `Fallthrough`/unresolved `Goto` continues
            /// processing `stmts` from `resume_idx` (or from the found label)
            /// using that leaf's own environment, visited-label set, and
            /// accumulated path condition.
            fn continue_leaf(
                stmts: &[$Stmt],
                resume_idx: usize,
                leaf: Leaf,
                budget: &mut usize,
            ) -> LowerResult<Vec<Leaf>> {
                match leaf.kind {
                    LeafKind::Return(v) => {
                        Ok(vec![Leaf { path_cond: leaf.path_cond, kind: LeafKind::Return(v) }])
                    }
                    LeafKind::CyclicGoto(label) => Ok(vec![Leaf {
                        path_cond: leaf.path_cond,
                        kind: LeafKind::CyclicGoto(label),
                    }]),
                    LeafKind::Fallthrough(env, visited_labels) => {
                        exec_from(stmts, resume_idx, leaf.path_cond, env, visited_labels, budget)
                    }
                    LeafKind::UnresolvedGoto(label, env, mut visited_labels) => {
                        match find_label(stmts, &label) {
                            Some(target) => {
                                let key = (stmts.as_ptr() as usize, target);
                                if visited_labels.contains(&key) {
                                    Ok(vec![Leaf {
                                        path_cond: leaf.path_cond,
                                        kind: LeafKind::CyclicGoto(label),
                                    }])
                                } else {
                                    visited_labels.push(key);
                                    exec_from(
                                        stmts,
                                        target + 1,
                                        leaf.path_cond,
                                        env,
                                        visited_labels,
                                        budget,
                                    )
                                }
                            }
                            None => Ok(vec![Leaf {
                                path_cond: leaf.path_cond,
                                kind: LeafKind::UnresolvedGoto(label, env, visited_labels),
                            }]),
                        }
                    }
                }
            }

            pub(crate) fn lower_paths_checked(
                body: &[$Stmt],
                params: &[$Binding],
                locals: &[$Binding],
                param_vars: &[SymExpr],
            ) -> LowerResult<LoweredFunctionPaths> {
                if param_vars.len() != params.len() {
                    return Err(LowerSymError::InvalidBinding(format!(
                        "lower_sym: {} param vars given, function has {} params",
                        param_vars.len(),
                        params.len()
                    )));
                }
                let mut env = SymEnv::default();
                let mut param_names = HashSet::with_capacity(params.len());
                for (idx, (p, v)) in params.iter().zip(param_vars).enumerate() {
                    if p.name.is_empty() {
                        return Err(LowerSymError::InvalidBinding(format!(
                            "lower_sym: parameter at index {idx} has an empty name"
                        )));
                    }
                    if !param_names.insert(p.name.as_str()) {
                        return Err(LowerSymError::InvalidBinding(format!(
                            "lower_sym: duplicate parameter name '{}'",
                            p.name
                        )));
                    }
                    if p.initializer.is_some() {
                        return Err(LowerSymError::InvalidBinding(format!(
                            "lower_sym: parameter '{}' must not carry an initializer",
                            p.name
                        )));
                    }
                    let param_bits = checked_width(&p.ty)?;
                    let var_bits = bit_width(v)?;
                    if !(1..=64).contains(&var_bits) {
                        return Err(LowerSymError::UnsupportedType(
                            "symbolic tier supports 1..=64-bit integers only",
                        ));
                    }
                    if var_bits < param_bits {
                        return Err(LowerSymError::InvalidBinding(format!(
                            "lower_sym: shared symbolic variable for '{}' is narrower ({var_bits}-bit) \
                             than declared parameter width ({param_bits}-bit)",
                            p.name
                        )));
                    }
                    let bound = resize(v.clone(), var_bits, param_bits, is_signed(&p.ty));
                    env.types.insert(p.name.clone(), p.ty.clone());
                    env.values.insert(p.name.clone(), bound);
                }

                let mut local_names = HashSet::with_capacity(locals.len());
                for (idx, l) in locals.iter().enumerate() {
                    if l.name.is_empty() {
                        return Err(LowerSymError::InvalidBinding(format!(
                            "lower_sym: local at index {idx} has an empty name"
                        )));
                    }
                    if param_names.contains(l.name.as_str()) {
                        return Err(LowerSymError::InvalidBinding(format!(
                            "lower_sym: local '{}' collides with parameter name",
                            l.name
                        )));
                    }
                    if !local_names.insert(l.name.as_str()) {
                        return Err(LowerSymError::InvalidBinding(format!(
                            "lower_sym: duplicate local name '{}'",
                            l.name
                        )));
                    }
                    env.types.insert(l.name.clone(), l.ty.clone());
                    if let Some(init) = &l.initializer {
                        let (v, init_ty) = eval_typed(init, &env)?;
                        let converted = convert(v, &init_ty, &l.ty)?;
                        env.values.insert(l.name.clone(), converted);
                    }
                }

                let mut budget = 0usize;
                let raw_leaves = exec_from(
                    body,
                    0,
                    SymExpr::Const { val: 1, size: 1 },
                    env,
                    Vec::new(),
                    &mut budget,
                )?;

                let mut leaves = Vec::with_capacity(raw_leaves.len());
                let mut return_bits = None;
                for leaf in raw_leaves {
                    let exit = match leaf.kind {
                        LeafKind::Return(Some(v)) => {
                            let width = bit_width(&v)?;
                            if let Some(expected) = return_bits {
                                if expected != width {
                                    return Err(LowerSymError::UnsupportedConstruct(format!(
                                        "lower_sym: return branches produce mismatched widths ({width}-bit vs {expected}-bit)"
                                    )));
                                }
                            } else {
                                return_bits = Some(width);
                            }
                            SymbolicExitKind::ReturnValue(v)
                        }
                        LeafKind::Return(None) => SymbolicExitKind::VoidReturn,
                        LeafKind::Fallthrough(_, _) => SymbolicExitKind::Fallthrough,
                        LeafKind::UnresolvedGoto(label, _, _) => {
                            SymbolicExitKind::UnresolvedGoto(label)
                        }
                        LeafKind::CyclicGoto(label) => SymbolicExitKind::CyclicGoto(label),
                    };
                    leaves.push(SymbolicPathLeaf {
                        path_cond: leaf.path_cond,
                        exit,
                    });
                }

                Ok(LoweredFunctionPaths {
                    leaves,
                    explored_steps: budget,
                    budget_limit: PATH_EXPLORE_BUDGET,
                    return_bits,
                })
            }

            /// Public entrypoint exposing pre-ITE symbolic leaves (`path_cond`
            /// and exit kinds) for external callers and tests.
            pub fn lower_paths(
                body: &[$Stmt],
                params: &[$Binding],
                locals: &[$Binding],
                param_vars: &[SymExpr],
            ) -> anyhow::Result<LoweredFunctionPaths> {
                lower_paths_checked(body, params, locals, param_vars).map_err(anyhow::Error::new)
            }

            pub(crate) fn lower_checked(
                body: &[$Stmt],
                params: &[$Binding],
                locals: &[$Binding],
                param_vars: &[SymExpr],
            ) -> LowerResult<SymExpr> {
                lower_paths_checked(body, params, locals, param_vars)?.fold_return_ite()
            }

            /// Lower `body` (with `params` bound to fresh shared `SymExpr::
            /// Var`s, one per parameter -- **not** minted here; the caller
            /// passes them in so PreHIR- and HIR-side lowering share the exact
            /// same `Var` per parameter index, which is what makes "same
            /// input" a meaningful assertion) to one closed-form `SymExpr`
            /// for the function's return value.
            pub fn lower(
                body: &[$Stmt],
                params: &[$Binding],
                locals: &[$Binding],
                param_vars: &[SymExpr],
            ) -> anyhow::Result<SymExpr> {
                lower_checked(body, params, locals, param_vars).map_err(anyhow::Error::new)
            }
        }
    };
}

define_lower_sym!(
    prehir,
    fission_midend_prehir,
    PreHirStmt,
    PreHirExpr,
    PreHirLValue,
    PreHirBinaryOp,
    PreHirUnaryOp,
    PreHirBinding,
    PreHirFunction
);
define_lower_sym!(
    hir,
    fission_midend_core::ir,
    HirStmt,
    HirExpr,
    HirLValue,
    HirBinaryOp,
    HirUnaryOp,
    NirBinding,
    HirFunction
);
