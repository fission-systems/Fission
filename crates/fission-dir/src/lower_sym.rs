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
//! - **No `Div`/`Mod`/`Sar`**: `SymExpr` doesn't yet have `Sdiv`/`Urem`/
//!   `Srem`/`Ashr` (only `Udiv`, `Shl`, `Lshr` exist). Extending
//!   `fission-solver`'s bitvector theory to add these is a legitimate,
//!   self-contained follow-up, not bundled into this crate.
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
/// invalid variable/parameter bindings and unsupported control/op constructs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LowerSymError {
    UnsupportedType(&'static str),
    InvalidBinding(String),
    UnsupportedConstruct(String),
}

impl fmt::Display for LowerSymError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedType(msg) => write!(f, "lower_sym: {msg}"),
            Self::InvalidBinding(msg) | Self::UnsupportedConstruct(msg) => f.write_str(msg),
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
const PATH_EXPLORE_BUDGET: usize = 4096;

enum LeafKind {
    Return(Option<SymExpr>),
    Fallthrough(SymEnv),
    UnresolvedGoto(String, SymEnv),
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
                Leaf, LeafKind, LowerSymError, PATH_EXPLORE_BUDGET, SymEnv, and,
                checked_scalar_type, checked_scalar_width, resize, sym_const,
            };
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

            fn align_width(expr: SymExpr, target_bits: u32, signed: bool) -> LowerResult<SymExpr> {
                let from_bits = bit_width(&expr)?;
                Ok(resize(expr, from_bits, target_bits, signed))
            }

            fn eval_expr(expr: &$Expr, env: &SymEnv) -> LowerResult<SymExpr> {
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
                        let from_ty = infer_ty(expr, env)?;
                        let (_, from_signed) = checked_ty(&from_ty)?;
                        let to_bits = checked_width(ty)?;
                        let inner = eval_expr(expr, env)?;
                        let from_bits = bit_width(&inner)?;
                        Ok(resize(inner, from_bits, to_bits, from_signed))
                    }
                    $Expr::Unary { op, expr, ty } => {
                        let expr_ty = infer_ty(expr, env)?;
                        let (_, expr_signed) = checked_ty(&expr_ty)?;
                        let bits = checked_width(ty)?;
                        let v = eval_expr(expr, env)?;
                        let v_bits = bit_width(&v)?;
                        Ok(match op {
                            $UnaryOp::Neg => {
                                let aligned = resize(v, v_bits, bits, expr_signed);
                                SymExpr::new_sub(SymExpr::Const { val: 0, size: bits }, aligned)
                            }
                            $UnaryOp::Not => {
                                let is_zero = SymExpr::new_eq(v, SymExpr::Const { val: 0, size: v_bits });
                                resize(is_zero, 1, bits, false)
                            }
                            $UnaryOp::BitNot => {
                                let aligned = resize(v, v_bits, bits, expr_signed);
                                SymExpr::new_not(aligned)
                            }
                        })
                    }
                    $Expr::Binary { op, lhs, rhs, ty } => eval_binary(op, lhs, rhs, ty, env),
                    $Expr::Select { cond, then_expr, else_expr, ty } => {
                        let target_bits = checked_width(ty)?;
                        let cond_ty = infer_ty(cond, env)?;
                        let _ = checked_ty(&cond_ty)?;
                        let then_ty = infer_ty(then_expr, env)?;
                        let (_, then_signed) = checked_ty(&then_ty)?;
                        let else_ty = infer_ty(else_expr, env)?;
                        let (_, else_signed) = checked_ty(&else_ty)?;
                        let c = eval_expr(cond, env)?;
                        let zero = SymExpr::Const { val: 0, size: bit_width(&c)? };
                        let is_true = SymExpr::new_neq(c, zero);
                        let t = align_width(eval_expr(then_expr, env)?, target_bits, then_signed)?;
                        let f = align_width(eval_expr(else_expr, env)?, target_bits, else_signed)?;
                        Ok(SymExpr::Ite { cond: Box::new(is_true), t: Box::new(t), f: Box::new(f) })
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
                let lhs_ty = infer_ty(lhs, env)?;
                let (_, lhs_signed) = checked_ty(&lhs_ty)?;
                let rhs_ty = infer_ty(rhs, env)?;
                let (_, rhs_signed) = checked_ty(&rhs_ty)?;
                let l = eval_expr(lhs, env)?;
                let r = eval_expr(rhs, env)?;
                let l_bits = bit_width(&l)?;
                let r_bits = bit_width(&r)?;
                let cmp_bits = l_bits.max(r_bits);

                Ok(match op {
                    $BinaryOp::Add => SymExpr::new_add(
                        resize(l, l_bits, result_bits, lhs_signed),
                        resize(r, r_bits, result_bits, rhs_signed),
                    ),
                    $BinaryOp::Sub => SymExpr::new_sub(
                        resize(l, l_bits, result_bits, lhs_signed),
                        resize(r, r_bits, result_bits, rhs_signed),
                    ),
                    $BinaryOp::Mul => SymExpr::Mul(
                        Box::new(resize(l, l_bits, result_bits, lhs_signed)),
                        Box::new(resize(r, r_bits, result_bits, rhs_signed)),
                    ),
                    $BinaryOp::And => SymExpr::new_and(
                        resize(l, l_bits, result_bits, lhs_signed),
                        resize(r, r_bits, result_bits, rhs_signed),
                    ),
                    $BinaryOp::Or => SymExpr::Or(
                        Box::new(resize(l, l_bits, result_bits, lhs_signed)),
                        Box::new(resize(r, r_bits, result_bits, rhs_signed)),
                    ),
                    $BinaryOp::Xor => SymExpr::new_xor(
                        resize(l, l_bits, result_bits, lhs_signed),
                        resize(r, r_bits, result_bits, rhs_signed),
                    ),
                    $BinaryOp::Shl => SymExpr::Shl(
                        Box::new(resize(l, l_bits, result_bits, lhs_signed)),
                        Box::new(resize(r, r_bits, result_bits, false)),
                    ),
                    $BinaryOp::Shr => SymExpr::Lshr(
                        Box::new(resize(l, l_bits, result_bits, false)),
                        Box::new(resize(r, r_bits, result_bits, false)),
                    ),
                    $BinaryOp::LogicalAnd => {
                        let b = and(
                            SymExpr::new_neq(l, SymExpr::Const { val: 0, size: l_bits }),
                            SymExpr::new_neq(r, SymExpr::Const { val: 0, size: r_bits }),
                        );
                        resize(b, 1, result_bits, false)
                    }
                    $BinaryOp::LogicalOr => {
                        let l_true = SymExpr::new_neq(l, SymExpr::Const { val: 0, size: l_bits });
                        let r_true = SymExpr::new_neq(r, SymExpr::Const { val: 0, size: r_bits });
                        let b = SymExpr::new_not(and(SymExpr::new_not(l_true), SymExpr::new_not(r_true)));
                        resize(b, 1, result_bits, false)
                    }
                    $BinaryOp::Eq => resize(
                        SymExpr::new_eq(
                            resize(l, l_bits, cmp_bits, lhs_signed),
                            resize(r, r_bits, cmp_bits, rhs_signed),
                        ),
                        1,
                        result_bits,
                        false,
                    ),
                    $BinaryOp::Ne => resize(
                        SymExpr::new_neq(
                            resize(l, l_bits, cmp_bits, lhs_signed),
                            resize(r, r_bits, cmp_bits, rhs_signed),
                        ),
                        1,
                        result_bits,
                        false,
                    ),
                    $BinaryOp::Lt => resize(
                        SymExpr::new_ult(
                            resize(l, l_bits, cmp_bits, lhs_signed),
                            resize(r, r_bits, cmp_bits, rhs_signed),
                        ),
                        1,
                        result_bits,
                        false,
                    ),
                    $BinaryOp::Le => resize(
                        SymExpr::Ule(
                            Box::new(resize(l, l_bits, cmp_bits, lhs_signed)),
                            Box::new(resize(r, r_bits, cmp_bits, rhs_signed)),
                        ),
                        1,
                        result_bits,
                        false,
                    ),
                    $BinaryOp::Gt => resize(
                        SymExpr::new_ult(
                            resize(r, r_bits, cmp_bits, rhs_signed),
                            resize(l, l_bits, cmp_bits, lhs_signed),
                        ),
                        1,
                        result_bits,
                        false,
                    ),
                    $BinaryOp::Ge => resize(
                        SymExpr::Ule(
                            Box::new(resize(r, r_bits, cmp_bits, rhs_signed)),
                            Box::new(resize(l, l_bits, cmp_bits, lhs_signed)),
                        ),
                        1,
                        result_bits,
                        false,
                    ),
                    $BinaryOp::SLt => resize(
                        SymExpr::new_slt(
                            resize(l, l_bits, cmp_bits, lhs_signed),
                            resize(r, r_bits, cmp_bits, rhs_signed),
                        ),
                        1,
                        result_bits,
                        false,
                    ),
                    $BinaryOp::SLe => resize(
                        SymExpr::new_sle(
                            resize(l, l_bits, cmp_bits, lhs_signed),
                            resize(r, r_bits, cmp_bits, rhs_signed),
                        ),
                        1,
                        result_bits,
                        false,
                    ),
                    $BinaryOp::SGt => resize(
                        SymExpr::new_sgt(
                            resize(l, l_bits, cmp_bits, lhs_signed),
                            resize(r, r_bits, cmp_bits, rhs_signed),
                        ),
                        1,
                        result_bits,
                        false,
                    ),
                    $BinaryOp::SGe => resize(
                        SymExpr::new_not(SymExpr::new_slt(
                            resize(l, l_bits, cmp_bits, lhs_signed),
                            resize(r, r_bits, cmp_bits, rhs_signed),
                        )),
                        1,
                        result_bits,
                        false,
                    ),
                    $BinaryOp::Div | $BinaryOp::Mod | $BinaryOp::Sar => {
                        return Err(LowerSymError::UnsupportedConstruct(
                            "lower_sym: Div/Mod/Sar not supported at the symbolic tier -- \
                             fission_solver has no Sdiv/Urem/Srem/Ashr yet, see module docs"
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
            /// scope can't resolve, or falling off the end).
            fn exec_from(
                stmts: &[$Stmt],
                start: usize,
                path_cond: SymExpr,
                env: SymEnv,
                budget: &mut usize,
            ) -> LowerResult<Vec<Leaf>> {
                *budget += 1;
                if *budget > PATH_EXPLORE_BUDGET {
                    return Err(LowerSymError::UnsupportedConstruct(
                        "lower_sym: path-exploration budget exceeded (too many branches)".to_string(),
                    ));
                }
                let mut idx = start;
                let mut env = env;
                loop {
                    if idx >= stmts.len() {
                        return Ok(vec![Leaf { path_cond, kind: LeafKind::Fallthrough(env) }]);
                    }
                    match &stmts[idx] {
                        $Stmt::Label(_) => {
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
                            let rhs_ty = infer_ty(rhs, &env)?;
                            let (_, rhs_signed) = checked_ty(&rhs_ty)?;
                            let v = eval_expr(rhs, &env)?;
                            let target_ty = env
                                .types
                                .get(name)
                                .cloned()
                                .unwrap_or_else(|| rhs_ty.clone());
                            let target_bits = checked_width(&target_ty)?;
                            let from_bits = bit_width(&v)?;
                            let resized = resize(v, from_bits, target_bits, rhs_signed);
                            env.types.insert(name.clone(), target_ty);
                            env.values.insert(name.clone(), resized);
                            idx += 1;
                        }
                        $Stmt::Return(expr) => {
                            let v = match expr {
                                Some(e) => {
                                    let ret_ty = infer_ty(e, &env)?;
                                    let _ = checked_ty(&ret_ty)?;
                                    Some(eval_expr(e, &env)?)
                                }
                                None => None,
                            };
                            return Ok(vec![Leaf { path_cond, kind: LeafKind::Return(v) }]);
                        }
                        $Stmt::Goto(label) => {
                            return Ok(match find_label(stmts, label) {
                                Some(target) => exec_from(stmts, target + 1, path_cond, env, budget)?,
                                None => vec![Leaf {
                                    path_cond,
                                    kind: LeafKind::UnresolvedGoto(label.clone(), env),
                                }],
                            });
                        }
                        $Stmt::If { cond, then_body, else_body } => {
                            let cond_ty = infer_ty(cond, &env)?;
                            let _ = checked_ty(&cond_ty)?;
                            let c = eval_expr(cond, &env)?;
                            let zero = SymExpr::Const { val: 0, size: bit_width(&c)? };
                            let is_true = SymExpr::new_neq(c.clone(), zero.clone());
                            let is_false = SymExpr::new_eq(c, zero);
                            let then_leaves = exec_from(
                                then_body, 0, and(path_cond.clone(), is_true), env.clone(), budget,
                            )?;
                            let else_leaves = exec_from(
                                else_body, 0, and(path_cond.clone(), is_false), env.clone(), budget,
                            )?;
                            let mut out = Vec::new();
                            for leaf in then_leaves.into_iter().chain(else_leaves) {
                                out.extend(continue_leaf(stmts, idx + 1, leaf, budget)?);
                            }
                            return Ok(out);
                        }
                        $Stmt::Block(inner) => {
                            let inner_leaves = exec_from(inner, 0, path_cond.clone(), env.clone(), budget)?;
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
            /// the *enclosing* `stmts` list: a `Return` is terminal as-is;
            /// a `Fallthrough`/unresolved `Goto` continues processing
            /// `stmts` from `resume_idx` (or from the found label) using
            /// that leaf's own environment and accumulated path condition.
            fn continue_leaf(
                stmts: &[$Stmt],
                resume_idx: usize,
                leaf: Leaf,
                budget: &mut usize,
            ) -> LowerResult<Vec<Leaf>> {
                match leaf.kind {
                    LeafKind::Return(v) => Ok(vec![Leaf { path_cond: leaf.path_cond, kind: LeafKind::Return(v) }]),
                    LeafKind::Fallthrough(env) => exec_from(stmts, resume_idx, leaf.path_cond, env, budget),
                    LeafKind::UnresolvedGoto(label, env) => match find_label(stmts, &label) {
                        Some(target) => exec_from(stmts, target + 1, leaf.path_cond, env, budget),
                        None => Ok(vec![Leaf {
                            path_cond: leaf.path_cond,
                            kind: LeafKind::UnresolvedGoto(label, env),
                        }]),
                    },
                }
            }

            pub(crate) fn lower_checked(
                body: &[$Stmt],
                params: &[$Binding],
                locals: &[$Binding],
                param_vars: &[SymExpr],
            ) -> LowerResult<SymExpr> {
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
                        let target_bits = checked_width(&l.ty)?;
                        let init_ty = infer_ty(init, &env)?;
                        let (_, init_signed) = checked_ty(&init_ty)?;
                        let v = eval_expr(init, &env)?;
                        let from_bits = bit_width(&v)?;
                        let resized = resize(v, from_bits, target_bits, init_signed);
                        env.values.insert(l.name.clone(), resized);
                    }
                }

                let mut budget = 0usize;
                let leaves = exec_from(body, 0, SymExpr::Const { val: 1, size: 1 }, env, &mut budget)?;

                let mut return_leaves: Vec<(SymExpr, SymExpr)> = Vec::new();
                let mut has_incomplete_return = false;
                for leaf in leaves {
                    match leaf.kind {
                        LeafKind::Return(Some(v)) => {
                            return_leaves.push((leaf.path_cond, v));
                        }
                        LeafKind::Fallthrough(_) | LeafKind::Return(None) => {
                            has_incomplete_return = true;
                        }
                        LeafKind::UnresolvedGoto(label, _) => {
                            return Err(LowerSymError::UnsupportedConstruct(format!(
                                "lower_sym: goto '{label}' never resolved to a label in this function"
                            )));
                        }
                    }
                }
                if return_leaves.is_empty() {
                    return Err(LowerSymError::UnsupportedConstruct(
                        "lower_sym: no path through the function returns a value".to_string(),
                    ));
                }
                if has_incomplete_return {
                    return Err(LowerSymError::UnsupportedConstruct(
                        "lower_sym: function has incomplete return paths (fallthrough or void return)"
                            .to_string(),
                    ));
                }
                let mut iter = return_leaves.into_iter().rev();
                let (_, mut acc) = iter.next().expect("non-empty return_leaves");
                let ret_bits = bit_width(&acc)?;
                for (cond, value) in iter {
                    let branch_bits = bit_width(&value)?;
                    if branch_bits != ret_bits {
                        return Err(LowerSymError::UnsupportedConstruct(format!(
                            "lower_sym: return branches produce mismatched widths ({branch_bits}-bit vs {ret_bits}-bit)"
                        )));
                    }
                    acc = SymExpr::Ite {
                        cond: Box::new(cond),
                        t: Box::new(value),
                        f: Box::new(acc),
                    };
                }
                Ok(acc)
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
