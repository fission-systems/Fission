use anyhow::{Context, Result};
use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};
use sleigh_rs::execution::{
    Binary, Execution, Expr, ExprElement, ExprValue,
};
use sleigh_rs::Sleigh;

use super::IRConverter;

impl IRConverter {
    pub(super) fn lower_expr(
        &mut self,
        expr: &Expr,
        current_address: u64,
        sleigh: &Sleigh,
        execution: &Execution,
        emitted: &mut Vec<PcodeOp>,
    ) -> Result<Varnode> {
        match expr {
            Expr::Value(element) => {
                self.lower_expr_element(element, current_address, sleigh, execution, emitted)
            }
            Expr::Op(bin) => {
                let mut lhs = self.lower_expr(
                    &bin.left,
                    current_address,
                    sleigh,
                    execution,
                    emitted,
                )?;
                let mut rhs = self.lower_expr(
                    &bin.right,
                    current_address,
                    sleigh,
                    execution,
                    emitted,
                )?;

                let (opcode, swap) = self.map_binary_opcode(bin.op)?;
                if swap {
                    std::mem::swap(&mut lhs, &mut rhs);
                }

                let out_size = Self::bits_to_bytes(bin.len_bits.get())
                    .context("Invalid output size in binary expr")?;
                let out = self.make_temp_varnode(self.next_seq, out_size);
                emitted.push(PcodeOp {
                    seq_num: self.take_seq(),
                    opcode,
                    address: current_address,
                    output: Some(out.clone()),
                    inputs: vec![lhs, rhs],
                    asm_mnemonic: Some("EXPR_BIN".to_string()),
                });
                Ok(out)
            }
        }
    }

    pub(super) fn lower_expr_element(
        &mut self,
        element: &ExprElement,
        current_address: u64,
        sleigh: &Sleigh,
        execution: &Execution,
        emitted: &mut Vec<PcodeOp>,
    ) -> Result<Varnode> {
        match element {
            ExprElement::Value { value, .. } => {
                self.lower_expr_value(value, current_address, sleigh, execution)
            }
            ExprElement::Op(unary) => {
                let input = self.lower_expr(
                    &unary.input,
                    current_address,
                    sleigh,
                    execution,
                    emitted,
                )?;
                self.lower_unary(
                    unary.op.clone(),
                    input,
                    current_address,
                    emitted,
                )
            }
            ExprElement::Reference(reference) => {
                let out_size = Self::bits_to_bytes(reference.len_bits.get())
                    .context("Invalid reference len")?;
                let out = self.make_temp_varnode(self.next_seq, out_size);
                let src = match &reference.value {
                    sleigh_rs::execution::ReferencedValue::InstStart(_) => {
                        Varnode::constant(current_address as i64, out_size)
                    }
                    sleigh_rs::execution::ReferencedValue::InstNext(_) => {
                        Varnode::constant(current_address as i64, out_size)
                    }
                    _ => anyhow::bail!(
                        "Unsupported reference kind in converter MVP: {:?}",
                        reference.value
                    ),
                };
                emitted.push(PcodeOp {
                    seq_num: self.take_seq(),
                    opcode: PcodeOpcode::Copy,
                    address: current_address,
                    output: Some(out.clone()),
                    inputs: vec![src],
                    asm_mnemonic: Some("REF".to_string()),
                });
                Ok(out)
            }
            _ => anyhow::bail!("Unsupported expr element in converter MVP: {:?}", element),
        }
    }

    pub(super) fn lower_expr_value(
        &self,
        value: &ExprValue,
        current_address: u64,
        sleigh: &Sleigh,
        execution: &Execution,
    ) -> Result<Varnode> {
        match value {
            ExprValue::Int(num) => {
                let size = Self::bits_to_bytes(num.size.get())
                    .context("Invalid integer constant size")?;
                let const_val = Self::number_to_i64(num.number);
                Ok(Varnode::constant(const_val, size))
            }
            ExprValue::Varnode(id) => Ok(self.varnode_from_sleigh(sleigh, *id)?),
            ExprValue::ExeVar(id) => {
                let bits = execution.variable(*id).len_bits.get();
                let size = Self::bits_to_bytes(bits)
                    .context("Invalid execution variable size")?;
                Ok(self.execution_varnode(*id, size))
            }
            ExprValue::InstStart(_) => {
                let size = sleigh.addr_bytes().get() as u32;
                Ok(Varnode::constant(current_address as i64, size))
            }
            ExprValue::InstNext(_) => {
                let size = sleigh.addr_bytes().get() as u32;
                Ok(Varnode::constant(current_address as i64, size))
            }
            _ => anyhow::bail!("Unsupported expr value in converter MVP: {:?}", value),
        }
    }

    pub(super) fn map_binary_opcode(&self, binary: Binary) -> Result<(PcodeOpcode, bool)> {
        let mapped = match binary {
            Binary::Add => (PcodeOpcode::IntAdd, false),
            Binary::Sub => (PcodeOpcode::IntSub, false),
            Binary::Mult => (PcodeOpcode::IntMult, false),
            Binary::Div => (PcodeOpcode::IntDiv, false),
            Binary::SigDiv => (PcodeOpcode::IntSDiv, false),
            Binary::Rem => (PcodeOpcode::IntRem, false),
            Binary::SigRem => (PcodeOpcode::IntSRem, false),
            Binary::Lsl => (PcodeOpcode::IntLeft, false),
            Binary::Lsr => (PcodeOpcode::IntRight, false),
            Binary::Asr => (PcodeOpcode::IntSRight, false),
            Binary::BitAnd => (PcodeOpcode::IntAnd, false),
            Binary::BitXor => (PcodeOpcode::IntXor, false),
            Binary::BitOr => (PcodeOpcode::IntOr, false),
            Binary::Eq => (PcodeOpcode::IntEqual, false),
            Binary::Ne => (PcodeOpcode::IntNotEqual, false),
            Binary::Less => (PcodeOpcode::IntLess, false),
            Binary::Greater => (PcodeOpcode::IntLess, true),
            Binary::LessEq => (PcodeOpcode::IntLessEqual, false),
            Binary::GreaterEq => (PcodeOpcode::IntLessEqual, true),
            Binary::SigLess => (PcodeOpcode::IntSLess, false),
            Binary::SigGreater => (PcodeOpcode::IntSLess, true),
            Binary::SigLessEq => (PcodeOpcode::IntSLessEqual, false),
            Binary::SigGreaterEq => (PcodeOpcode::IntSLessEqual, true),
            Binary::And => (PcodeOpcode::BoolAnd, false),
            Binary::Or => (PcodeOpcode::BoolOr, false),
            Binary::Xor => (PcodeOpcode::BoolXor, false),
            Binary::Carry => (PcodeOpcode::IntCarry, false),
            Binary::SCarry => (PcodeOpcode::IntSCarry, false),
            Binary::SBorrow => (PcodeOpcode::IntSBorrow, false),
            Binary::FloatAdd => (PcodeOpcode::FloatAdd, false),
            Binary::FloatSub => (PcodeOpcode::FloatSub, false),
            Binary::FloatMult => (PcodeOpcode::FloatMult, false),
            Binary::FloatDiv => (PcodeOpcode::FloatDiv, false),
            Binary::FloatEq => (PcodeOpcode::FloatEqual, false),
            Binary::FloatNe => (PcodeOpcode::FloatNotEqual, false),
            Binary::FloatLess => (PcodeOpcode::FloatLess, false),
            Binary::FloatGreater => (PcodeOpcode::FloatLess, true),
            Binary::FloatLessEq => (PcodeOpcode::FloatLessEqual, false),
            Binary::FloatGreaterEq => (PcodeOpcode::FloatLessEqual, true),
        };
        Ok(mapped)
    }
}
