//! Retire an assignment whose value is overwritten before anything reads it.
//!
//! [`eliminate_dead_local_clobber_assigns`] turns on a whole-function read
//! count, so it can only retire a name nothing reads *anywhere*. A machine
//! register reused as a scratch home is read somewhere by construction --
//! that is what makes it a register -- so every one of its definitions
//! survives, including the ones a later definition clobbers two statements
//! down:
//!
//! ```text
//! rax = insize;      // dead: overwritten below, nothing reads it between
//! rax = &inbuf;
//! ```
//!
//! Liveness for a *definition* is not liveness for its *name*. This pass
//! asks the narrower question: walking forward from a definition inside one
//! straight-line statement list, is the next thing that touches this name
//! another definition of it? If so the first store cannot be observed and
//! goes.
//!
//! The walk stops -- keeping the definition -- at anything that could read
//! the name along a path this list does not show: a nested body, a label a
//! `goto` could re-enter at, or a transfer out of the list. That makes the
//! pass blind to the clobber-in-both-arms shape (`x = e; if (c) x = a; else
//! x = b;`), which is deliberate: proving it needs both arms to be total,
//! and the straight-line case is where the volume is.
//!
//! Measured on gzip's `get_method` (DecBench `unoptimized`, the single
//! largest GED contributor in that config at the 0.2.1 scoring): 50 of 452
//! assignments in the emitted body are this shape, against 122 excess CFG
//! nodes over the source.

use super::utils::expr_has_side_effects;
use super::utils::retain_unmarked_stmts;
use crate::HashSet;
use crate::prelude::*;
use std::rc::Rc;

/// Names whose storage a write can reach other than by the name itself --
/// stack homes, parameters, slots. A clobber of one of those may still be
/// observable through an aliasing pointer, so none of them are candidates.
fn aliasable_names(func: &PreHirFunction) -> HashSet<String> {
    let mut out: HashSet<String> = func.params.iter().map(|b| b.name.clone()).collect();
    for binding in &func.locals {
        let stack_backed = matches!(
            binding.origin,
            Some(NirBindingOrigin::StackOffset(_))
                | Some(NirBindingOrigin::HomeSlot(_))
                | Some(NirBindingOrigin::OutgoingArgSlot(_))
                | Some(NirBindingOrigin::DerivedFromStackOffset(_))
                | Some(NirBindingOrigin::VaRegion)
        );
        if stack_backed || binding.name.starts_with("slot_") {
            out.insert(binding.name.clone());
        }
        if matches!(binding.ty, NirType::Aggregate { .. }) {
            out.insert(binding.name.clone());
        }
    }
    out
}

pub fn eliminate_overwritten_assigns(func: &mut PreHirFunction) -> bool {
    let blocked = aliasable_names(func);
    eliminate_in_stmts(&mut func.body, &blocked)
}

fn eliminate_in_stmts(stmts: &mut Vec<PreHirStmt>, blocked: &HashSet<String>) -> bool {
    let mut changed = false;

    // Nested bodies first: each is its own straight-line region.
    for stmt in stmts.iter_mut() {
        match stmt {
            PreHirStmt::Block(body)
            | PreHirStmt::While { body, .. }
            | PreHirStmt::DoWhile { body, .. }
            | PreHirStmt::For { body, .. } => {
                changed |= eliminate_in_stmts(Rc::<Vec<PreHirStmt>>::make_mut(body), blocked);
            }
            PreHirStmt::If {
                then_body,
                else_body,
                ..
            } => {
                changed |= eliminate_in_stmts(Rc::<Vec<PreHirStmt>>::make_mut(then_body), blocked);
                changed |= eliminate_in_stmts(Rc::<Vec<PreHirStmt>>::make_mut(else_body), blocked);
            }
            PreHirStmt::Switch { cases, default, .. } => {
                for case in cases {
                    changed |=
                        eliminate_in_stmts(Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body), blocked);
                }
                changed |= eliminate_in_stmts(Rc::<Vec<PreHirStmt>>::make_mut(default), blocked);
            }
            _ => {}
        }
    }

    let mut to_remove = vec![false; stmts.len()];
    for idx in 0..stmts.len() {
        let PreHirStmt::Assign {
            lhs: PreHirLValue::Var(name),
            rhs,
        } = &stmts[idx]
        else {
            continue;
        };
        if blocked.contains(name.as_str()) || expr_has_side_effects(rhs) {
            continue;
        }
        if is_overwritten_before_read(&stmts[idx + 1..], name) {
            to_remove[idx] = true;
        }
    }

    if to_remove.iter().any(|&x| x) {
        retain_unmarked_stmts(stmts, &to_remove);
        changed = true;
    }
    changed
}

/// Walk forward over one statement list. `true` only when a plain
/// re-definition of `name` is reached with no read of it in between and no
/// statement whose control flow this list does not fully describe.
fn is_overwritten_before_read(rest: &[PreHirStmt], name: &str) -> bool {
    for stmt in rest {
        match stmt {
            PreHirStmt::Assign { lhs, rhs } => {
                // A read anywhere in this statement keeps the earlier store,
                // including a read through the destination (`*x = ...`).
                if lvalue_reads_var(lhs, name) || expr_mentions_var(rhs, name) {
                    return false;
                }
                if matches!(lhs, PreHirLValue::Var(dst) if dst == name) {
                    return true;
                }
                // A store through a pointer may land on this name's storage
                // only if the name is aliasable, and those never get here.
            }
            PreHirStmt::Expr(expr) => {
                if expr_mentions_var(expr, name) {
                    return false;
                }
            }
            // Anything below either reads `name` along a path this list does
            // not show, or lets control re-enter above this point.
            _ => return false,
        }
    }
    false
}

fn lvalue_reads_var(lhs: &PreHirLValue, name: &str) -> bool {
    match lhs {
        PreHirLValue::Var(_) => false,
        PreHirLValue::Deref { ptr, .. } => expr_mentions_var(ptr, name),
        PreHirLValue::Index { base, index, .. } => {
            expr_mentions_var(base, name) || expr_mentions_var(index, name)
        }
        PreHirLValue::FieldAccess { base, .. } => expr_mentions_var(base, name),
    }
}

fn expr_mentions_var(expr: &PreHirExpr, name: &str) -> bool {
    let mut found = false;
    walk(expr, &mut |e| {
        if matches!(e, PreHirExpr::Var(v) if v == name) {
            found = true;
        }
    });
    found
}

fn walk(expr: &PreHirExpr, f: &mut impl FnMut(&PreHirExpr)) {
    f(expr);
    match expr {
        PreHirExpr::Var(_) | PreHirExpr::AddressOfGlobal(_) | PreHirExpr::Const(_, _) => {}
        PreHirExpr::Cast { expr, .. }
        | PreHirExpr::Unary { expr, .. }
        | PreHirExpr::Load { ptr: expr, .. }
        | PreHirExpr::PtrOffset { base: expr, .. }
        | PreHirExpr::AggregateCopy { src: expr, .. }
        | PreHirExpr::FieldAccess { base: expr, .. } => walk(expr, f),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            walk(lhs, f);
            walk(rhs, f);
        }
        PreHirExpr::Index { base, index, .. } => {
            walk(base, f);
            walk(index, f);
        }
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            walk(cond, f);
            walk(then_expr, f);
            walk(else_expr, f);
        }
        PreHirExpr::Call { args, .. } => {
            for arg in args {
                walk(arg, f);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fission_midend_core::{NirBindingOrigin, NirType};
    use fission_midend_prehir::PreHirBinding;

    fn u32_ty() -> NirType {
        NirType::Int {
            bits: 32,
            signed: false,
        }
    }

    fn binding(name: &str, origin: Option<NirBindingOrigin>) -> PreHirBinding {
        PreHirBinding {
            name: name.to_string(),
            ty: u32_ty(),
            surface_type_name: None,
            origin,
            initializer: None,
        }
    }

    fn func(locals: Vec<PreHirBinding>, body: Vec<PreHirStmt>) -> PreHirFunction {
        PreHirFunction {
            name: "t".to_string(),
            int_param_offsets: Vec::new(),
            locals,
            body,
            ..Default::default()
        }
    }

    fn set(dst: &str, rhs: PreHirExpr) -> PreHirStmt {
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var(dst.to_string()),
            rhs,
        }
    }

    fn var(name: &str) -> PreHirExpr {
        PreHirExpr::Var(name.to_string())
    }

    fn konst(v: i64) -> PreHirExpr {
        PreHirExpr::Const(v, u32_ty())
    }

    fn assigned(f: &PreHirFunction) -> Vec<(String, String)> {
        f.body
            .iter()
            .filter_map(|s| match s {
                PreHirStmt::Assign {
                    lhs: PreHirLValue::Var(n),
                    rhs,
                } => Some((n.clone(), format!("{rhs:?}"))),
                _ => None,
            })
            .collect()
    }

    /// The shape a whole-function read count cannot retire: `rax` is read at
    /// the end, so every definition of it survives `eliminate_dead_local_
    /// clobber_assigns`, including the one clobbered on the next line.
    #[test]
    fn retires_a_definition_the_next_statement_overwrites() {
        let mut f = func(
            vec![binding("rax", Some(NirBindingOrigin::Temp))],
            vec![
                set("rax", konst(7)),
                set("rax", konst(9)),
                PreHirStmt::Return(Some(var("rax"))),
            ],
        );
        assert!(eliminate_overwritten_assigns(&mut f));
        assert_eq!(assigned(&f).len(), 1);
        assert_eq!(assigned(&f)[0].1, format!("{:?}", konst(9)));
    }

    /// A read between the two definitions makes the first observable.
    #[test]
    fn keeps_a_definition_read_before_the_overwrite() {
        let mut f = func(
            vec![
                binding("rax", Some(NirBindingOrigin::Temp)),
                binding("other", Some(NirBindingOrigin::Temp)),
            ],
            vec![
                set("rax", konst(7)),
                set("other", var("rax")),
                set("rax", konst(9)),
            ],
        );
        assert!(!eliminate_overwritten_assigns(&mut f));
        assert_eq!(assigned(&f).len(), 3);
    }

    /// The overwriting statement reading the name is itself a read
    /// (`rax = rax + 1`), so the earlier definition stays.
    #[test]
    fn keeps_a_definition_the_overwrite_itself_reads() {
        let mut f = func(
            vec![binding("rax", Some(NirBindingOrigin::Temp))],
            vec![
                set("rax", konst(7)),
                set(
                    "rax",
                    PreHirExpr::Binary {
                        op: fission_midend_prehir::PreHirBinaryOp::Add,
                        lhs: Box::new(var("rax")),
                        rhs: Box::new(konst(1)),
                        ty: u32_ty(),
                    },
                ),
            ],
        );
        assert!(!eliminate_overwritten_assigns(&mut f));
        assert_eq!(assigned(&f).len(), 2);
    }

    /// A label between the two definitions is a re-entry point: a `goto`
    /// landing there reaches the second definition without the first, and a
    /// reader above it would then see the wrong value.
    #[test]
    fn keeps_a_definition_separated_by_a_label() {
        let mut f = func(
            vec![binding("rax", Some(NirBindingOrigin::Temp))],
            vec![
                set("rax", konst(7)),
                PreHirStmt::Label("block_1".to_string()),
                set("rax", konst(9)),
            ],
        );
        assert!(!eliminate_overwritten_assigns(&mut f));
        assert_eq!(assigned(&f).len(), 2);
    }

    /// Removing an unread definition must not remove a call inside it.
    #[test]
    fn keeps_a_clobbered_definition_that_calls() {
        let mut f = func(
            vec![binding("rax", Some(NirBindingOrigin::Temp))],
            vec![
                set(
                    "rax",
                    PreHirExpr::Call {
                        target: "side_effect".to_string(),
                        args: Vec::new(),
                        ty: u32_ty(),
                    },
                ),
                set("rax", konst(9)),
            ],
        );
        assert!(!eliminate_overwritten_assigns(&mut f));
        assert_eq!(assigned(&f).len(), 2);
    }

    /// A stack home's storage is reachable through a pointer, so clobbering
    /// it is not proof the earlier write went unobserved.
    #[test]
    fn keeps_a_clobbered_stack_home() {
        let mut f = func(
            vec![binding("local_8", Some(NirBindingOrigin::StackOffset(-8)))],
            vec![set("local_8", konst(7)), set("local_8", konst(9))],
        );
        assert!(!eliminate_overwritten_assigns(&mut f));
        assert_eq!(assigned(&f).len(), 2);
    }

    /// The clobber must be reached without leaving the statement list: an
    /// `If` in between may read the name in either arm.
    #[test]
    fn keeps_a_definition_separated_by_a_nested_body() {
        let mut f = func(
            vec![binding("rax", Some(NirBindingOrigin::Temp))],
            vec![
                set("rax", konst(7)),
                PreHirStmt::If {
                    cond: konst(1),
                    then_body: Rc::new(vec![PreHirStmt::Return(Some(var("rax")))]),
                    else_body: Rc::new(Vec::new()),
                },
                set("rax", konst(9)),
            ],
        );
        assert!(!eliminate_overwritten_assigns(&mut f));
        assert_eq!(assigned(&f).len(), 2);
    }

    /// Nested bodies are their own straight-line regions and get the same
    /// treatment.
    #[test]
    fn retires_an_overwritten_definition_inside_a_branch() {
        let mut f = func(
            vec![binding("rax", Some(NirBindingOrigin::Temp))],
            vec![PreHirStmt::If {
                cond: konst(1),
                then_body: Rc::new(vec![
                    set("rax", konst(7)),
                    set("rax", konst(9)),
                    PreHirStmt::Return(Some(var("rax"))),
                ]),
                else_body: Rc::new(Vec::new()),
            }],
        );
        assert!(eliminate_overwritten_assigns(&mut f));
        let PreHirStmt::If { then_body, .. } = &f.body[0] else {
            panic!("shape changed");
        };
        assert_eq!(then_body.len(), 2, "{then_body:?}");
    }
}
