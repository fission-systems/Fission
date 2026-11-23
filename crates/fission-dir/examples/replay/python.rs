// SPDX-License-Identifier: Apache-2.0
use anyhow::{Context, Result, bail, ensure};
use fission_midend_core::ir::{HirBinaryOp, HirExpr, HirFunction, HirStmt, NirType};

pub(super) fn export(function: &HirFunction) -> Result<String> {
    let [HirStmt::Return(Some(value))] = function.body.as_slice() else {
        bail!("Python export requires a single integer return expression");
    };
    ensure!(
        function.locals.is_empty(),
        "Python export does not support locals"
    );
    let mut arguments = Vec::new();
    let mut normalizations = String::new();
    for (index, parameter) in function.params.iter().enumerate() {
        let name = format!("p{index}");
        let normal = word(&name, &parameter.ty)?;
        normalizations.push_str(&format!("    {name} = {normal}\n"));
        arguments.push(name);
    }
    let expression = expression(value, function)?;
    let returned = word(&expression, &function.return_type)?;
    let parameters = arguments.join(", ");
    Ok(format!(
        r"# Generated from Fission's typed HIR.
def _word(value, bits, signed):
    value &= (1 << bits) - 1
    if signed and value & (1 << (bits - 1)):
        return value - (1 << bits)
    return value

def ported({parameters}):
{normalizations}    return {returned}
"
    ))
}

fn word(value: &str, ty: &NirType) -> Result<String> {
    let NirType::Int { bits, signed } = ty else {
        bail!("Python export requires explicit integer types, got {ty:?}");
    };
    ensure!((1..=64).contains(bits), "unsupported integer width {bits}");
    let signed = if *signed { "True" } else { "False" };
    Ok(format!("_word({value}, {bits}, {signed})"))
}

fn expression(value: &HirExpr, function: &HirFunction) -> Result<String> {
    match value {
        HirExpr::Var(name) => {
            let index = function
                .params
                .iter()
                .position(|param| param.name == *name)
                .with_context(|| format!("unbound parameter {name}"))?;
            Ok(format!("p{index}"))
        }
        HirExpr::Const(value, ty) => word(&value.to_string(), ty),
        HirExpr::Cast { ty, expr } => word(&expression(expr, function)?, ty),
        HirExpr::Binary { op, lhs, rhs, ty } => {
            let operation = match op {
                HirBinaryOp::Add => "+",
                HirBinaryOp::Sub => "-",
                HirBinaryOp::Mul => "*",
                HirBinaryOp::And => "&",
                HirBinaryOp::Or => "|",
                HirBinaryOp::Xor => "^",
                HirBinaryOp::Shl if matches!(rhs.as_ref(), HirExpr::Const(amount, _) if (0..64).contains(amount)) => {
                    "<<"
                }
                _ => bail!("Python export does not support {op:?}"),
            };
            let lhs = expression(lhs, function)?;
            let rhs = expression(rhs, function)?;
            word(&format!("({lhs} {operation} {rhs})"), ty)
        }
        _ => bail!("Python export does not support {value:?}"),
    }
}
