//! Windows x64 home-slot / variadic-region recovery.
//!
//! This pass promotes builder-classified `HomeSlot` bindings into a semantic
//! `VaStart` marker when a call's tail arguments prove that a `va_list` cursor
//! points into the saved register/home region. The rewrite is intentionally
//! conservative: it only fires when the call already exposes stack-tail
//! arguments and the final argument is provably derived from a recovered home
//! slot.

use crate::HashMap;
use fission_core::CallingConvention;
use fission_midend_core::ir::NirBindingOrigin;
use fission_midend_prehir::{PreHirExpr, PreHirFunction, PreHirStmt};
use std::collections::BTreeSet;

use fission_midend_core::wave_stats::{
    add_abi_slot_recoveries, add_home_slot_promotions, add_va_start_recoveries,
    add_variadic_stack_region_folds,
};

fn home_slot_map(func: &PreHirFunction) -> HashMap<String, i64> {
    func.locals
        .iter()
        .filter_map(|binding| match binding.origin {
            Some(NirBindingOrigin::HomeSlot(offset)) => Some((binding.name.clone(), offset)),
            _ => None,
        })
        .collect()
}

fn expr_uses_home_slot(expr: &PreHirExpr, home_slots: &HashMap<String, i64>) -> bool {
    match expr {
        PreHirExpr::Var(name)
        | PreHirExpr::AddressOfGlobal(name)
        | PreHirExpr::AddressOfLocal(name) => home_slots.contains_key(name),
        PreHirExpr::Load { ptr, .. } => expr_uses_home_slot(ptr, home_slots),
        PreHirExpr::PtrOffset { base, .. } | PreHirExpr::FieldAccess { base, .. } => {
            expr_uses_home_slot(base, home_slots)
        }
        PreHirExpr::Cast { expr, .. } => expr_uses_home_slot(expr, home_slots),
        PreHirExpr::Unary { expr, .. } => expr_uses_home_slot(expr, home_slots),
        PreHirExpr::Binary { lhs, rhs, .. } => {
            expr_uses_home_slot(lhs, home_slots) || expr_uses_home_slot(rhs, home_slots)
        }
        PreHirExpr::Call { args, .. } => {
            args.iter().any(|arg| expr_uses_home_slot(arg, home_slots))
        }
        PreHirExpr::Index { base, index, .. } => {
            expr_uses_home_slot(base, home_slots) || expr_uses_home_slot(index, home_slots)
        }
        PreHirExpr::AggregateCopy { src, .. } => expr_uses_home_slot(src, home_slots),
        PreHirExpr::Select {
            cond,
            then_expr,
            else_expr,
            ..
        } => {
            expr_uses_home_slot(cond, home_slots)
                || expr_uses_home_slot(then_expr, home_slots)
                || expr_uses_home_slot(else_expr, home_slots)
        }
        PreHirExpr::Const(_, _) => false,
    }
}

fn call_tail_uses_home_slot(args: &[PreHirExpr], home_slots: &HashMap<String, i64>) -> bool {
    args.len() > 4
        && args[4..]
            .iter()
            .any(|arg| expr_uses_home_slot(arg, home_slots))
}

fn call_last_arg_is_va_region(args: &[PreHirExpr], home_slots: &HashMap<String, i64>) -> bool {
    let Some(last) = args.last() else {
        return false;
    };
    expr_uses_home_slot(last, home_slots)
}

fn recover_in_stmt(
    stmt: &mut PreHirStmt,
    home_slots: &HashMap<String, i64>,
    last_named_param: Option<&str>,
    folds: &mut usize,
    va_starts: &mut usize,
) -> bool {
    match stmt {
        PreHirStmt::Expr(PreHirExpr::Call { args, .. }) => {
            if call_tail_uses_home_slot(args, home_slots) {
                *folds += 1;
                if last_named_param.is_some() && call_last_arg_is_va_region(args, home_slots) {
                    return true;
                }
            }
            false
        }
        PreHirStmt::VaStart { va_list, .. } => expr_uses_home_slot(va_list, home_slots),
        PreHirStmt::Assign { rhs, .. } => {
            if let PreHirExpr::Call { args, .. } = rhs
                && call_tail_uses_home_slot(args, home_slots)
            {
                *folds += 1;
                if last_named_param.is_some() && call_last_arg_is_va_region(args, home_slots) {
                    return true;
                }
            }
            false
        }
        PreHirStmt::Block(stmts)
        | PreHirStmt::While { body: stmts, .. }
        | PreHirStmt::DoWhile { body: stmts, .. }
        | PreHirStmt::For { body: stmts, .. } => recover_in_stmts(
            std::rc::Rc::<Vec<PreHirStmt>>::make_mut(stmts),
            home_slots,
            last_named_param,
            folds,
            va_starts,
        ),
        PreHirStmt::Switch { cases, default, .. } => {
            let mut changed = false;
            for case in cases {
                changed |= recover_in_stmts(
                    std::rc::Rc::<Vec<PreHirStmt>>::make_mut(&mut case.body),
                    home_slots,
                    last_named_param,
                    folds,
                    va_starts,
                );
            }
            changed |= recover_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(default),
                home_slots,
                last_named_param,
                folds,
                va_starts,
            );
            changed
        }
        PreHirStmt::If {
            then_body,
            else_body,
            ..
        } => {
            recover_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(then_body),
                home_slots,
                last_named_param,
                folds,
                va_starts,
            ) | recover_in_stmts(
                std::rc::Rc::<Vec<PreHirStmt>>::make_mut(else_body),
                home_slots,
                last_named_param,
                folds,
                va_starts,
            )
        }
        PreHirStmt::Label(_)
        | PreHirStmt::Goto(_)
        | PreHirStmt::Return(_)
        | PreHirStmt::Break
        | PreHirStmt::Continue => false,
        _ => false,
    }
}

fn recover_in_stmts(
    stmts: &mut Vec<PreHirStmt>,
    home_slots: &HashMap<String, i64>,
    last_named_param: Option<&str>,
    folds: &mut usize,
    va_starts: &mut usize,
) -> bool {
    let mut changed = false;
    let mut idx = 0usize;
    while idx < stmts.len() {
        let call_last_arg = match &stmts[idx] {
            PreHirStmt::Expr(PreHirExpr::Call { args, .. }) => args.last().cloned(),
            PreHirStmt::Assign {
                rhs: PreHirExpr::Call { args, .. },
                ..
            } => args.last().cloned(),
            _ => None,
        };
        if recover_in_stmt(
            &mut stmts[idx],
            home_slots,
            last_named_param,
            folds,
            va_starts,
        ) && let (Some(last_param), Some(last_arg)) = (last_named_param, call_last_arg)
        {
            let marker = PreHirStmt::VaStart {
                va_list: last_arg,
                last_named_param: last_param.to_string(),
            };
            let already_present = idx > 0
                && matches!(
                    &stmts[idx - 1],
                    PreHirStmt::VaStart {
                        last_named_param,
                        ..
                    } if last_named_param == last_param
                );
            if !already_present {
                stmts.insert(idx, marker);
                *va_starts += 1;
                changed = true;
                idx += 1;
            }
        }
        idx += 1;
    }
    changed
}

pub fn apply_variadic_stack_region_pass(func: &mut PreHirFunction) -> bool {
    if !func.is_64bit || func.calling_convention != CallingConvention::WindowsX64 {
        return false;
    }

    let home_slots = home_slot_map(func);
    if home_slots.is_empty() {
        return false;
    }

    let unique_offsets = home_slots.values().copied().collect::<BTreeSet<_>>();
    add_abi_slot_recoveries(unique_offsets.len());
    add_home_slot_promotions(home_slots.len());

    let last_named_param = func.params.last().map(|param| param.name.as_str());
    let mut folds = 0usize;
    let mut va_starts = 0usize;
    let changed = recover_in_stmts(
        &mut func.body,
        &home_slots,
        last_named_param,
        &mut folds,
        &mut va_starts,
    );
    add_variadic_stack_region_folds(folds);
    add_va_start_recoveries(va_starts);
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;

    #[test]
    fn variadic_stack_region_recovers_va_start_for_assigned_calls() {
        let mut func = PreHirFunction {
            name: "test_va".into(),
            is_64bit: true,
            calling_convention: CallingConvention::WindowsX64,
            params: vec![PreHirBinding {
                name: "fmt".into(),
                ty: NirType::Ptr(Box::new(NirType::Int {
                    bits: 8,
                    signed: false,
                })),
                surface_type_name: None,
                origin: Some(NirBindingOrigin::ParamIndex(0)),
                initializer: None,
            }],
            locals: vec![
                PreHirBinding {
                    name: "home_28".into(),
                    ty: NirType::Int {
                        bits: 64,
                        signed: false,
                    },
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::HomeSlot(0x28)),
                    initializer: None,
                },
                PreHirBinding {
                    name: "len".into(),
                    ty: NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                    surface_type_name: None,
                    origin: Some(NirBindingOrigin::Temp),
                    initializer: None,
                },
            ],
            body: vec![PreHirStmt::Assign {
                lhs: PreHirLValue::Var("len".into()),
                rhs: PreHirExpr::Call {
                    target: "vsnprintf".into(),
                    args: vec![
                        PreHirExpr::Var("buf".into()),
                        PreHirExpr::Const(
                            100,
                            NirType::Int {
                                bits: 64,
                                signed: false,
                            },
                        ),
                        PreHirExpr::Const(
                            0,
                            NirType::Int {
                                bits: 64,
                                signed: false,
                            },
                        ),
                        PreHirExpr::Var("fmt".into()),
                        PreHirExpr::Var("home_28".into()),
                    ],
                    ty: NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                },
            }],
            ..Default::default()
        };

        assert!(apply_variadic_stack_region_pass(&mut func));
        assert!(matches!(
            func.body.first(),
            Some(PreHirStmt::VaStart {
                last_named_param,
                ..
            }) if last_named_param == "fmt"
        ));
    }

    #[test]
    fn call_tail_uses_home_slot_does_not_match_arbitrary_loads() {
        let home_slots = HashMap::default();
        let args = vec![
            PreHirExpr::Var("a".into()),
            PreHirExpr::Var("b".into()),
            PreHirExpr::Var("c".into()),
            PreHirExpr::Var("d".into()),
            PreHirExpr::Load {
                ptr: Box::new(PreHirExpr::Var("ptr".into())),
                ty: NirType::Int {
                    bits: 32,
                    signed: false,
                },
            },
        ];
        assert!(
            !call_tail_uses_home_slot(&args, &home_slots),
            "arbitrary load in 5th argument must not be misclassified as home slot use"
        );
    }
}
