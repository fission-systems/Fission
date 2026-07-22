//! Lowers `DirExpr`/`DirStmt` and `HirExpr`/`HirStmt` to `fission_solver::
//! ast::SymExpr` and asks the real SAT/SMT-style solver
//! ([`fission_solver::Solver`]) to either prove DIR and HIR equivalent
//! (`Unsat` on `dir_result != hir_result`) or produce a genuine
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

use crate::eval::width_of;
use fission_midend_core::ir::NirType;

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

type SymEnv = HashMap<String, SymExpr>;

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

fn sym_const(val: i64, ty: &NirType) -> SymExpr {
    let bits = width_of(ty).clamp(1, 64);
    let mask = if bits >= 64 { u64::MAX } else { (1u64 << bits) - 1 };
    SymExpr::Const {
        val: (val as u64) & mask,
        size: bits,
    }
}

/// Sign- or zero-extend / narrow `v` (currently `from_bits` wide) to
/// `to_bits`, matching [`crate::eval::normalize`]'s semantics for the
/// concrete tier.
fn resize(v: SymExpr, from_bits: u32, to_bits: u32, signed_from: bool) -> SymExpr {
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
                    val: if extra_bits >= 64 { u64::MAX } else { (1u64 << extra_bits) - 1 },
                    size: extra_bits,
                };
                let zeros = SymExpr::Const { val: 0, size: extra_bits };
                SymExpr::Ite {
                    cond: Box::new(is_negative),
                    t: Box::new(ones),
                    f: Box::new(zeros),
                }
            } else {
                SymExpr::Const { val: 0, size: extra_bits }
            };
            SymExpr::Concat(Box::new(high), Box::new(v))
        }
    }
}

/// Generates one symbolic-lowering module for a `Stmt`/`Expr`/`LValue`/
/// `BinaryOp`/`UnaryOp`/`Binding`/`Function` family, mirroring [`crate::eval`]'s
/// `define_interp!` for the same reason: DIR-side and HIR-side lowering
/// logic generated from one macro body can't silently drift.
macro_rules! define_lower_sym {
    (
        $modname:ident, $ir_crate:path,
        $Stmt:ident, $Expr:ident, $LValue:ident, $BinaryOp:ident, $UnaryOp:ident,
        $Binding:ident, $Function:ident
    ) => {
        pub mod $modname {
            use anyhow::{Result, bail};
            use $ir_crate::{$BinaryOp, $Binding, $Expr, $LValue, $Stmt, $UnaryOp};
            use fission_midend_core::ir::NirType;
            use fission_solver::ast::SymExpr;
            use std::collections::HashMap;

            use super::{Leaf, LeafKind, SymEnv, and, resize, sym_const, PATH_EXPLORE_BUDGET};
            use super::super::eval::{is_signed, width_of};

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

            fn infer_ty<'a>(expr: &'a $Expr, env: &'a SymEnv) -> Result<NirType> {
                match expr {
                    $Expr::Var(name) => env
                        .get(name)
                        .map(|e| int_ty_of_size(bit_width(e)))
                        .ok_or_else(|| anyhow::anyhow!("lower_sym: read of undeclared variable '{name}'")),
                    $Expr::Const(_, ty)
                    | $Expr::Cast { ty, .. }
                    | $Expr::Unary { ty, .. }
                    | $Expr::Binary { ty, .. }
                    | $Expr::Select { ty, .. } => Ok(ty.clone()),
                    other => bail!("lower_sym: cannot infer type of unsupported expr {other:?}"),
                }
            }

            fn bit_width(e: &SymExpr) -> u32 {
                use fission_solver::ast::Sort;
                match e.get_sort() {
                    Sort::BitVector(bits) => bits,
                    other => panic!("lower_sym: non-bitvector sort {other:?} (float/array unsupported)"),
                }
            }

            /// A throwaway `NirType::Int` used only to recover a bit width
            /// through [`super::super::eval::width_of`] for a `Var` whose
            /// declared type isn't separately tracked here (the `SymExpr`
            /// itself already carries the width; sign handling for `Var`
            /// reads is unsigned-by-construction since every write already
            /// normalizes via [`resize`] at its own declared type).
            fn int_ty_of_size(bits: u32) -> NirType {
                NirType::Int { bits, signed: false }
            }

            fn eval_expr(expr: &$Expr, env: &SymEnv) -> Result<SymExpr> {
                match expr {
                    $Expr::Var(name) => env
                        .get(name)
                        .cloned()
                        .ok_or_else(|| anyhow::anyhow!("lower_sym: read of undeclared variable '{name}'")),
                    $Expr::Const(v, ty) => Ok(sym_const(*v, ty)),
                    $Expr::Cast { ty, expr } => {
                        let inner = eval_expr(expr, env)?;
                        let from_bits = bit_width(&inner);
                        let from_ty = infer_ty(expr, env)?;
                        Ok(resize(inner, from_bits, width_of(ty).clamp(1, 64), is_signed(&from_ty)))
                    }
                    $Expr::Unary { op, expr, ty } => {
                        let v = eval_expr(expr, env)?;
                        let bits = width_of(ty).clamp(1, 64);
                        Ok(match op {
                            $UnaryOp::Neg => SymExpr::new_sub(SymExpr::Const { val: 0, size: bits }, v),
                            $UnaryOp::Not => {
                                let is_zero = SymExpr::new_eq(v, SymExpr::Const { val: 0, size: bits });
                                resize(is_zero, 1, bits, false)
                            }
                            $UnaryOp::BitNot => SymExpr::new_not(v),
                        })
                    }
                    $Expr::Binary { op, lhs, rhs, ty } => eval_binary(op, lhs, rhs, ty, env),
                    $Expr::Select { cond, then_expr, else_expr, ty } => {
                        let c = eval_expr(cond, env)?;
                        let is_true = SymExpr::new_neq(c, SymExpr::Const { val: 0, size: 1 });
                        let t = eval_expr(then_expr, env)?;
                        let f = eval_expr(else_expr, env)?;
                        let _ = ty;
                        Ok(SymExpr::Ite { cond: Box::new(is_true), t: Box::new(t), f: Box::new(f) })
                    }
                    other => bail!(
                        "lower_sym: unsupported expr {other:?} -- no memory/call/div/mod model at \
                         the symbolic tier, see module docs"
                    ),
                }
            }

            fn eval_binary(
                op: &$BinaryOp,
                lhs: &$Expr,
                rhs: &$Expr,
                ty: &NirType,
                env: &SymEnv,
            ) -> Result<SymExpr> {
                let l = eval_expr(lhs, env)?;
                let r = eval_expr(rhs, env)?;
                let result_bits = width_of(ty).clamp(1, 64);
                Ok(match op {
                    $BinaryOp::Add => SymExpr::new_add(l, r),
                    $BinaryOp::Sub => SymExpr::new_sub(l, r),
                    $BinaryOp::Mul => SymExpr::Mul(Box::new(l), Box::new(r)),
                    $BinaryOp::And => SymExpr::new_and(l, r),
                    $BinaryOp::Or => SymExpr::Or(Box::new(l), Box::new(r)),
                    $BinaryOp::Xor => SymExpr::new_xor(l, r),
                    $BinaryOp::Shl => SymExpr::Shl(Box::new(l), Box::new(r)),
                    $BinaryOp::Shr => SymExpr::Lshr(Box::new(l), Box::new(r)),
                    $BinaryOp::LogicalAnd => and(
                        SymExpr::new_neq(l, SymExpr::Const { val: 0, size: 1 }),
                        SymExpr::new_neq(r, SymExpr::Const { val: 0, size: 1 }),
                    ),
                    $BinaryOp::LogicalOr => {
                        let l_true = SymExpr::new_neq(l, SymExpr::Const { val: 0, size: 1 });
                        let r_true = SymExpr::new_neq(r, SymExpr::Const { val: 0, size: 1 });
                        SymExpr::new_not(and(SymExpr::new_not(l_true), SymExpr::new_not(r_true)))
                    }
                    $BinaryOp::Eq => resize(SymExpr::new_eq(l, r), 1, result_bits.max(1), false),
                    $BinaryOp::Ne => resize(SymExpr::new_neq(l, r), 1, result_bits.max(1), false),
                    $BinaryOp::Lt => resize(SymExpr::new_ult(l, r), 1, result_bits.max(1), false),
                    $BinaryOp::Le => resize(SymExpr::Ule(Box::new(l), Box::new(r)), 1, result_bits.max(1), false),
                    $BinaryOp::Gt => resize(SymExpr::new_ult(r, l), 1, result_bits.max(1), false),
                    $BinaryOp::Ge => resize(SymExpr::Ule(Box::new(r), Box::new(l)), 1, result_bits.max(1), false),
                    $BinaryOp::SLt => resize(SymExpr::new_slt(l, r), 1, result_bits.max(1), false),
                    $BinaryOp::SLe => resize(SymExpr::new_sle(l, r), 1, result_bits.max(1), false),
                    $BinaryOp::SGt => resize(SymExpr::new_sgt(l, r), 1, result_bits.max(1), false),
                    $BinaryOp::SGe => resize(SymExpr::new_not(SymExpr::new_slt(l, r)), 1, result_bits.max(1), false),
                    $BinaryOp::Div | $BinaryOp::Mod | $BinaryOp::Sar => bail!(
                        "lower_sym: Div/Mod/Sar not supported at the symbolic tier -- fission_solver \
                         has no Sdiv/Urem/Srem/Ashr yet, see module docs"
                    ),
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
            ) -> Result<Vec<Leaf>> {
                *budget += 1;
                if *budget > PATH_EXPLORE_BUDGET {
                    bail!("lower_sym: path-exploration budget exceeded (too many branches)");
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
                                other => bail!(
                                    "lower_sym: unsupported assignment target {other:?} -- no \
                                     memory model at the symbolic tier"
                                ),
                            };
                            let v = eval_expr(rhs, &env)?;
                            env.insert(name.clone(), v);
                            idx += 1;
                        }
                        $Stmt::Return(expr) => {
                            let v = match expr {
                                Some(e) => Some(eval_expr(e, &env)?),
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
                            let c = eval_expr(cond, &env)?;
                            let is_true = SymExpr::new_neq(c.clone(), SymExpr::Const { val: 0, size: 1 });
                            let is_false = SymExpr::new_eq(c, SymExpr::Const { val: 0, size: 1 });
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
                            bail!("lower_sym: Switch not supported at the symbolic tier")
                        }
                        $Stmt::While { .. } | $Stmt::DoWhile { .. } | $Stmt::For { .. } => {
                            bail!("lower_sym: loops not supported at the symbolic tier (see contains_loop)")
                        }
                        $Stmt::Break | $Stmt::Continue => {
                            bail!("lower_sym: break/continue not supported at the symbolic tier")
                        }
                        $Stmt::Expr(_) | $Stmt::VaStart { .. } => {
                            bail!("lower_sym: unsupported statement at the symbolic tier")
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
            ) -> Result<Vec<Leaf>> {
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

            /// Lower `body` (with `params` bound to fresh shared `SymExpr::
            /// Var`s, one per parameter -- **not** minted here; the caller
            /// passes them in so DIR- and HIR-side lowering share the exact
            /// same `Var` per parameter index, which is what makes "same
            /// input" a meaningful assertion) to one closed-form `SymExpr`
            /// for the function's return value.
            pub fn lower(
                body: &[$Stmt],
                params: &[$Binding],
                locals: &[$Binding],
                param_vars: &[SymExpr],
            ) -> Result<SymExpr> {
                anyhow::ensure!(
                    param_vars.len() == params.len(),
                    "lower_sym: {} param vars given, function has {} params",
                    param_vars.len(),
                    params.len()
                );
                let mut env: SymEnv = HashMap::new();
                for (p, v) in params.iter().zip(param_vars) {
                    env.insert(p.name.clone(), v.clone());
                }
                for l in locals {
                    let bits = width_of(&l.ty).clamp(1, 64);
                    let v = match &l.initializer {
                        Some(init) => eval_expr(init, &env)?,
                        None => SymExpr::Const { val: 0, size: bits },
                    };
                    env.insert(l.name.clone(), v);
                }
                let mut budget = 0usize;
                let leaves = exec_from(body, 0, SymExpr::Const { val: 1, size: 1 }, env, &mut budget)?;

                let mut result: Option<SymExpr> = None;
                for leaf in leaves.into_iter().rev() {
                    let value = match leaf.kind {
                        LeafKind::Return(Some(v)) => v,
                        LeafKind::Fallthrough(_) | LeafKind::Return(None) => {
                            // Falls off the end / bare `return;` without a
                            // value on this path -- can't contribute a
                            // return-value leaf; skip rather than fail the
                            // whole function (matches the concrete tier's
                            // `Flow::Normal => Ok(None)` treatment: a real
                            // divergence here would already show up as a
                            // `diff_dir_hir`/`check_ground_truth` finding
                            // on the concrete tier).
                            continue;
                        }
                        LeafKind::UnresolvedGoto(label, _) => {
                            bail!("lower_sym: goto '{label}' never resolved to a label in this function")
                        }
                    };
                    result = Some(match result {
                        None => value,
                        Some(acc) => SymExpr::Ite { cond: Box::new(leaf.path_cond), t: Box::new(value), f: Box::new(acc) },
                    });
                }
                result.ok_or_else(|| anyhow::anyhow!("lower_sym: no path through the function returns a value"))
            }
        }
    };
}

define_lower_sym!(
    dir, fission_midend_dir,
    DirStmt, DirExpr, DirLValue, DirBinaryOp, DirUnaryOp, DirBinding, DirFunction
);
define_lower_sym!(
    hir, fission_midend_core::ir,
    HirStmt, HirExpr, HirLValue, HirBinaryOp, HirUnaryOp, NirBinding, HirFunction
);
