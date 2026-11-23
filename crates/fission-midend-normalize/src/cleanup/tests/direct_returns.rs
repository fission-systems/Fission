use super::*;

#[test]
fn collapse_trivial_assign_returns_skips_preserved_temp() {
    let mut stmts = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("uVar0".to_string()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::Sub,
                lhs: Box::new(PreHirExpr::Var("eax".to_string())),
                rhs: Box::new(PreHirExpr::Var("ecx".to_string())),
                ty: int(32),
            },
        },
        PreHirStmt::Return(Some(PreHirExpr::Var("uVar0".to_string()))),
    ];

    assert!(!collapse_trivial_assign_returns(
        &mut stmts,
        &["uVar0"].into_iter().collect::<HashSet<_>>(),
    ));
    assert!(matches!(stmts[0], PreHirStmt::Assign { .. }));
    assert!(matches!(
        stmts[1],
        PreHirStmt::Return(Some(PreHirExpr::Var(_)))
    ));
}

#[test]
fn collapse_trivial_assign_returns_folds_eax_const() {
    use utils::is_abi_return_register_name;
    assert!(is_abi_return_register_name("eax"));
    let mut stmts = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("eax".to_string()),
            rhs: PreHirExpr::Const(7, int(32)),
        },
        PreHirStmt::Return(Some(PreHirExpr::Var("eax".to_string()))),
    ];
    assert!(collapse_trivial_assign_returns(
        &mut stmts,
        &HashSet::default()
    ));
    assert_eq!(stmts.len(), 1);
    assert!(matches!(
        &stmts[0],
        PreHirStmt::Return(Some(PreHirExpr::Const(7, _)))
    ));
}

#[test]
fn collapse_trivial_assign_returns_folds_eax_const_even_if_preserved() {
    let mut stmts = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("eax".to_string()),
            rhs: PreHirExpr::Const(7, int(32)),
        },
        PreHirStmt::Return(Some(PreHirExpr::Var("eax".to_string()))),
    ];
    assert!(collapse_trivial_assign_returns(
        &mut stmts,
        &["eax"].into_iter().collect::<HashSet<_>>(),
    ));
    assert_eq!(stmts.len(), 1);
    assert!(matches!(
        &stmts[0],
        PreHirStmt::Return(Some(PreHirExpr::Const(7, _)))
    ));
}

#[test]
fn collapse_trivial_assign_returns_folds_rax_param_add() {
    use fission_midend_core::ir::NirType;
    use fission_midend_prehir::{PreHirBinaryOp, PreHirExpr, PreHirLValue, PreHirStmt};
    let mut stmts = vec![
        PreHirStmt::Assign {
            lhs: PreHirLValue::Var("rax".to_string()),
            rhs: PreHirExpr::Binary {
                op: PreHirBinaryOp::Add,
                lhs: Box::new(PreHirExpr::Var("param_1".to_string())),
                rhs: Box::new(PreHirExpr::Const(
                    5,
                    NirType::Int {
                        bits: 32,
                        signed: true,
                    },
                )),
                ty: NirType::Int {
                    bits: 32,
                    signed: true,
                },
            },
        },
        PreHirStmt::Return(Some(PreHirExpr::Var("rax".to_string()))),
    ];
    assert!(collapse_trivial_assign_returns(
        &mut stmts,
        &crate::HashSet::default()
    ));
    assert_eq!(stmts.len(), 1);
    match &stmts[0] {
        PreHirStmt::Return(Some(PreHirExpr::Binary {
            op: PreHirBinaryOp::Add,
            ..
        })) => {}
        other => panic!("expected folded return add, got {other:?}"),
    }
}
