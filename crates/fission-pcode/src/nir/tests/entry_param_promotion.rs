//! Tests for entry-block `tmp = <param reg>` → `param_k` promotion.

use super::*;
use crate::nir::normalize::normalize_hir_function;
use crate::nir::types::{
    HirExpr, HirFunction, HirLValue, HirStmt, NirBinding, NirBindingOrigin, NirType,
};
use crate::nir::CallingConvention;

fn binding_temp(name: &str) -> NirBinding {
    NirBinding {
        name: name.to_string(),
        ty: NirType::Int {
            bits: 64,
            signed: true,
        },
        surface_type_name: None,
        origin: Some(NirBindingOrigin::Temp),
        initializer: None,
    }
}

#[test]
fn entry_spill_sysv_rsi_becomes_param_2() {
    // System V AMD64: rsi is the second integer parameter register.
    let mut func = HirFunction {
        name: "spill".into(),
        params: vec![],
        locals: vec![binding_temp("tmp_x")],
        return_type: NirType::Unknown,
        surface_return_type_name: None,
        body: vec![
            HirStmt::Assign {
                lhs: HirLValue::Var("tmp_x".into()),
                rhs: HirExpr::Var("rsi".into()),
            },
            HirStmt::Return(Some(HirExpr::Var("tmp_x".into()))),
        ],
        calling_convention: CallingConvention::SystemVAmd64,
        is_64bit: true,
        ..Default::default()
    };
    normalize_hir_function(&mut func);
    let rendered = print_hir_function(&func);
    assert!(
        rendered.contains("param_2"),
        "expected param_2 promotion, got:\n{rendered}"
    );
}
