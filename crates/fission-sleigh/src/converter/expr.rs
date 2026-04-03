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
        next_address: u64,
        next2_address: u64,
        sleigh: &Sleigh,
        execution: &Execution,
        emitted: &mut Vec<PcodeOp>,
    ) -> Result<Varnode> {
        match expr {
            Expr::Value(element) => {
                self.lower_expr_element(
                    element,
                    current_address,
                    next_address,
                    next2_address,
                    sleigh,
                    execution,
                    emitted,
                )
            }
            Expr::Op(bin) => {
                let mut lhs = self.lower_expr(
                    &bin.left,
                    current_address,
                    next_address,
                    next2_address,
                    sleigh,
                    execution,
                    emitted,
                )?;
                let mut rhs = self.lower_expr(
                    &bin.right,
                    current_address,
                    next_address,
                    next2_address,
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
        next_address: u64,
        next2_address: u64,
        sleigh: &Sleigh,
        execution: &Execution,
        emitted: &mut Vec<PcodeOp>,
    ) -> Result<Varnode> {
        match element {
            ExprElement::Value { value, .. } => {
                self.lower_expr_value(
                    value,
                    current_address,
                    next_address,
                    next2_address,
                    sleigh,
                    execution,
                    emitted,
                )
            }
            ExprElement::Op(unary) => {
                let input = self.lower_expr(
                    &unary.input,
                    current_address,
                    next_address,
                    next2_address,
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
                    sleigh_rs::execution::ReferencedValue::TokenField(token_field) => {
                        let raw = self.token_field_raw_value(sleigh, token_field.id)?;
                        let clamped = if raw > i64::MAX as u64 {
                            i64::MAX
                        } else {
                            raw as i64
                        };
                        Varnode::constant(clamped, out_size)
                    }
                    sleigh_rs::execution::ReferencedValue::InstStart(_) => {
                        Varnode::constant(current_address as i64, out_size)
                    }
                    sleigh_rs::execution::ReferencedValue::InstNext(_) => {
                        Varnode::constant(next_address as i64, out_size)
                    }
                    sleigh_rs::execution::ReferencedValue::Table(table) => {
                        let table_value = self.ensure_table_export_slot(
                            table.id,
                            current_address,
                            sleigh,
                            emitted,
                        )?;
                        self.resize_varnode_for_expr(
                            table_value,
                            out_size,
                            current_address,
                            emitted,
                            "REF_TABLE_RESIZE",
                        )
                    }
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
            ExprElement::New(new_expr) => {
                self.lower_new_expr(
                    new_expr,
                    current_address,
                    next_address,
                    next2_address,
                    sleigh,
                    execution,
                    emitted,
                )
            }
            ExprElement::CPool(cpool) => {
                self.lower_cpool_expr(
                    cpool,
                    current_address,
                    next_address,
                    next2_address,
                    sleigh,
                    execution,
                    emitted,
                )
            }
            ExprElement::UserCall(user_call) => {
                let mut inputs = Vec::with_capacity(user_call.params.len() + 1);
                let userop_id = i64::try_from(user_call.function.0)
                    .context("User function id does not fit i64")?;
                inputs.push(Varnode::constant(userop_id, 4));

                for param in &user_call.params {
                    let lowered = self.lower_expr(
                        param,
                        current_address,
                        next_address,
                        next2_address,
                        sleigh,
                        execution,
                        emitted,
                    )?;
                    inputs.push(lowered);
                }

                let out_size = inputs.get(1).map(|vn| vn.size).unwrap_or(1);
                let out = self.make_temp_varnode(self.next_seq, out_size);
                let userop_name = sleigh.user_function(user_call.function).name();
                emitted.push(PcodeOp {
                    seq_num: self.take_seq(),
                    opcode: PcodeOpcode::CallOther,
                    address: current_address,
                    output: Some(out.clone()),
                    inputs,
                    asm_mnemonic: Some(format!("USERCALL_{userop_name}")),
                });
                Ok(out)
            }
        }
    }

    fn resize_varnode_for_expr(
        &mut self,
        input: Varnode,
        out_size: u32,
        current_address: u64,
        emitted: &mut Vec<PcodeOp>,
        mnemonic: &str,
    ) -> Varnode {
        if input.size == out_size {
            return input;
        }

        let out = self.make_temp_varnode(self.next_seq, out_size);
        let (opcode, inputs) = if input.size < out_size {
            (PcodeOpcode::IntZExt, vec![input])
        } else {
            (PcodeOpcode::SubPiece, vec![input, Varnode::constant(0, 4)])
        };

        emitted.push(PcodeOp {
            seq_num: self.take_seq(),
            opcode,
            address: current_address,
            output: Some(out.clone()),
            inputs,
            asm_mnemonic: Some(mnemonic.to_string()),
        });
        out
    }

    fn lower_new_expr(
        &mut self,
        new_expr: &sleigh_rs::execution::ExprNew,
        current_address: u64,
        next_address: u64,
        next2_address: u64,
        sleigh: &Sleigh,
        execution: &Execution,
        emitted: &mut Vec<PcodeOp>,
    ) -> Result<Varnode> {
        let mut inputs = Vec::with_capacity(2);
        inputs.push(self.lower_expr(
            &new_expr.first,
            current_address,
            next_address,
            next2_address,
            sleigh,
            execution,
            emitted,
        )?);

        if let Some(second) = &new_expr.second {
            inputs.push(self.lower_expr(
                second,
                current_address,
                next_address,
                next2_address,
                sleigh,
                execution,
                emitted,
            )?);
        }

        let out_size = sleigh.addr_bytes().get() as u32;
        let out = self.make_temp_varnode(self.next_seq, out_size);
        emitted.push(PcodeOp {
            seq_num: self.take_seq(),
            opcode: PcodeOpcode::New,
            address: current_address,
            output: Some(out.clone()),
            inputs,
            asm_mnemonic: Some("NEWOBJECT".to_string()),
        });
        Ok(out)
    }

    fn lower_cpool_expr(
        &mut self,
        cpool: &sleigh_rs::execution::ExprCPool,
        current_address: u64,
        next_address: u64,
        next2_address: u64,
        sleigh: &Sleigh,
        execution: &Execution,
        emitted: &mut Vec<PcodeOp>,
    ) -> Result<Varnode> {
        if cpool.params.len() < 2 {
            anyhow::bail!("cpool requires at least two parameters");
        }

        let mut inputs = Vec::with_capacity(cpool.params.len());
        for param in &cpool.params {
            inputs.push(self.lower_expr(
                param,
                current_address,
                next_address,
                next2_address,
                sleigh,
                execution,
                emitted,
            )?);
        }

        let out_size = sleigh.addr_bytes().get() as u32;
        let out = self.make_temp_varnode(self.next_seq, out_size);
        emitted.push(PcodeOp {
            seq_num: self.take_seq(),
            opcode: PcodeOpcode::CPoolRef,
            address: current_address,
            output: Some(out.clone()),
            inputs,
            asm_mnemonic: Some("CPOOLREF".to_string()),
        });
        Ok(out)
    }

    pub(super) fn lower_expr_value(
        &mut self,
        value: &ExprValue,
        current_address: u64,
        next_address: u64,
        _next2_address: u64,
        sleigh: &Sleigh,
        execution: &Execution,
        emitted: &mut Vec<PcodeOp>,
    ) -> Result<Varnode> {
        match value {
            ExprValue::Int(num) => {
                let size = Self::bits_to_bytes(num.size.get())
                    .context("Invalid integer constant size")?;
                let const_val = Self::number_to_i64(num.number);
                Ok(Varnode::constant(const_val, size))
            }
            ExprValue::Varnode(id) => Ok(self.varnode_from_sleigh(sleigh, *id)?),
            ExprValue::VarnodeDynamic(dyn_vn) => {
                let attach = sleigh.attach_varnode(dyn_vn.attach_id);
                if attach.0.len() == 1 {
                    return self.varnode_from_sleigh(sleigh, attach.0[0].1);
                }

                let dynamic_index = self
                    .resolve_dynamic_value_index(dyn_vn.attach_value, sleigh)?;
                if let Some(varnode_id) = attach.find_value(dynamic_index) {
                    return self.varnode_from_sleigh(sleigh, varnode_id);
                }

                anyhow::bail!(
                    "Unable to resolve dynamic varnode expr: {:?}, index={}",
                    dyn_vn,
                    dynamic_index
                )
            }
            ExprValue::TokenField(token_field) => {
                let bits = token_field.size.get();
                let size = Self::bits_to_bytes(bits)
                    .context("Invalid token-field size")?;
                let raw = self.token_field_raw_value(sleigh, token_field.id)?;
                let clamped = if raw > i64::MAX as u64 {
                    i64::MAX
                } else {
                    raw as i64
                };
                Ok(Varnode::constant(clamped, size))
            }
            ExprValue::Context(context) => {
                let bits = context.size.get();
                let size = Self::bits_to_bytes(bits)
                    .context("Invalid context value size")?;
                let raw = self.context_raw_value(sleigh, context.id)?;
                let clamped = if raw > i64::MAX as u64 {
                    i64::MAX
                } else {
                    raw as i64
                };
                Ok(Varnode::constant(clamped, size))
            }
            ExprValue::IntDynamic(dynamic_int) => {
                let bits = dynamic_int.bits.get();
                let size = Self::bits_to_bytes(bits)
                    .context("Invalid dynamic-int size")?;
                let dynamic_index = self
                    .resolve_dynamic_value_index(dynamic_int.attach_value, sleigh)?;
                let number = sleigh
                    .attach_number(dynamic_int.attach_id)
                    .find_value(dynamic_index)
                    .context("Missing attach-number value for dynamic index")?;
                Ok(Varnode::constant(Self::number_to_i64(number), size))
            }
            ExprValue::Bitrange(bitrange) => {
                let info = sleigh.bitrange(bitrange.id);
                let source = self.varnode_from_sleigh(sleigh, info.varnode)?;
                let start = info.bits.start();
                let bits = info.bits.len().get();
                self.extract_bit_range(
                    source,
                    start..(start + bits),
                    bits,
                    current_address,
                    emitted,
                )
            }
            ExprValue::ExeVar(id) => {
                let bits = execution.variable(*id).len_bits.get();
                let size = Self::bits_to_bytes(bits)
                    .context("Invalid execution variable size")?;
                Ok(self.execution_varnode(*id, size))
            }
            ExprValue::Table(table_id) => {
                let slot = self.ensure_table_export_slot(
                    *table_id,
                    current_address,
                    sleigh,
                    emitted,
                )?;
                Ok(slot)
            }
            ExprValue::InstStart(_) => {
                let size = sleigh.addr_bytes().get() as u32;
                Ok(Varnode::constant(current_address as i64, size))
            }
            ExprValue::InstNext(_) => {
                let size = sleigh.addr_bytes().get() as u32;
                Ok(Varnode::constant(next_address as i64, size))
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
