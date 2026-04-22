use anyhow::{anyhow, bail, Result};
use fission_pcode::arch::x86::{
    X86_EFLAGS_BASE, X86_REG_BASE, X86_SEG_BASE, X86_XMM_BASE, X86_YMM_BASE,
};
use fission_pcode::{PcodeOp, PcodeOpcode, Varnode};
use iced_x86::{Decoder, DecoderOptions, Instruction, Mnemonic, OpKind, Register};

use super::{RuntimeSleighError, UNIQUE_SPACE_ID};

pub(super) fn decode_and_lift(bytes: &[u8], address: u64) -> Result<(Vec<PcodeOp>, u64)> {
    let mut decoder = Decoder::with_ip(64, bytes, address, DecoderOptions::NONE);
    let instr = decoder.decode();
    let decoded_len = instr.len() as u64;
    if decoded_len == 0 || decoded_len as usize > bytes.len() || instr.is_invalid() {
        return Err(RuntimeSleighError::DecodeNoMatch {
            language: "x86-64".to_string(),
            address,
        }
        .into());
    }

    let mut emitter = X86RuntimeEmitter::new(address);
    emitter.emit_instruction(&instr)?;
    Ok((emitter.finish(), decoded_len))
}

#[derive(Debug, Clone)]
struct X86RuntimeEmitter {
    address: u64,
    seq: u32,
    next_tmp: u64,
    ops: Vec<PcodeOp>,
}

impl X86RuntimeEmitter {
    fn new(address: u64) -> Self {
        Self {
            address,
            seq: 0,
            next_tmp: 0xE200_0000_0000_0000u64.wrapping_add(address.wrapping_shl(6)),
            ops: Vec::new(),
        }
    }

    fn finish(self) -> Vec<PcodeOp> {
        self.ops
    }

    fn emit_instruction(&mut self, instr: &Instruction) -> Result<()> {
        match instr.mnemonic() {
            Mnemonic::Nop | Mnemonic::Pause => Ok(()),
            Mnemonic::Ret => {
                self.push(PcodeOpcode::Return, None, Vec::new(), instr);
                Ok(())
            }
            Mnemonic::Call => self.emit_call(instr),
            Mnemonic::Jmp => self.emit_jmp(instr),
            mnemonic if jcc_condition(mnemonic).is_some() => self.emit_jcc(instr, mnemonic),
            Mnemonic::Mov => self.emit_mov(instr),
            Mnemonic::Lea => self.emit_lea(instr),
            Mnemonic::Push => self.emit_push(instr),
            Mnemonic::Pop => self.emit_pop(instr),
            Mnemonic::Leave => self.emit_leave(instr),
            Mnemonic::Add => self.emit_binary(instr, PcodeOpcode::IntAdd),
            Mnemonic::Sub => self.emit_binary(instr, PcodeOpcode::IntSub),
            Mnemonic::And => self.emit_binary(instr, PcodeOpcode::IntAnd),
            Mnemonic::Or => self.emit_binary(instr, PcodeOpcode::IntOr),
            Mnemonic::Xor => self.emit_binary(instr, PcodeOpcode::IntXor),
            Mnemonic::Imul | Mnemonic::Mul => self.emit_binary(instr, PcodeOpcode::IntMult),
            Mnemonic::Shl | Mnemonic::Sal => self.emit_binary(instr, PcodeOpcode::IntLeft),
            Mnemonic::Shr => self.emit_binary(instr, PcodeOpcode::IntRight),
            Mnemonic::Sar => self.emit_binary(instr, PcodeOpcode::IntSRight),
            Mnemonic::Inc => self.emit_unary_delta(instr, 1),
            Mnemonic::Dec => self.emit_unary_delta(instr, -1),
            Mnemonic::Cmp => self.emit_compare(instr, false),
            Mnemonic::Test => self.emit_compare(instr, true),
            Mnemonic::Movzx => self.emit_extend(instr, PcodeOpcode::IntZExt),
            Mnemonic::Movsx | Mnemonic::Movsxd => self.emit_extend(instr, PcodeOpcode::IntSExt),
            Mnemonic::Seto
            | Mnemonic::Setno
            | Mnemonic::Setb
            | Mnemonic::Setae
            | Mnemonic::Sete
            | Mnemonic::Setne
            | Mnemonic::Setbe
            | Mnemonic::Seta
            | Mnemonic::Sets
            | Mnemonic::Setns
            | Mnemonic::Setp
            | Mnemonic::Setnp
            | Mnemonic::Setl
            | Mnemonic::Setge
            | Mnemonic::Setle
            | Mnemonic::Setg => self.emit_setcc(instr),
            Mnemonic::Cdqe | Mnemonic::Cwde | Mnemonic::Cbw => self.emit_accumulator_extend(instr),
            _ => Err(RuntimeSleighError::UnsupportedPcodeTemplate {
                language: "x86-64".to_string(),
                reason: format!("unsupported x86-64 mnemonic {:?}", instr.mnemonic()),
            }
            .into()),
        }
    }

    fn emit_call(&mut self, instr: &Instruction) -> Result<()> {
        let input = if is_near_branch(instr.op_kind(0)) {
            const_u64(instr.near_branch_target(), 8)
        } else {
            self.read_operand(instr, 0, 8)?
        };
        self.push(PcodeOpcode::Call, None, vec![input], instr);
        Ok(())
    }

    fn emit_jmp(&mut self, instr: &Instruction) -> Result<()> {
        if is_near_branch(instr.op_kind(0)) {
            self.push(
                PcodeOpcode::Branch,
                None,
                vec![const_u64(instr.near_branch_target(), 8)],
                instr,
            );
        } else {
            let target = self.read_operand(instr, 0, 8)?;
            self.push(PcodeOpcode::BranchInd, None, vec![target], instr);
        }
        Ok(())
    }

    fn emit_jcc(&mut self, instr: &Instruction, mnemonic: Mnemonic) -> Result<()> {
        let cond = self.condition_varnode(mnemonic)?;
        self.push(
            PcodeOpcode::CBranch,
            None,
            vec![const_u64(instr.near_branch_target(), 8), cond],
            instr,
        );
        Ok(())
    }

    fn emit_mov(&mut self, instr: &Instruction) -> Result<()> {
        let size = self.dest_size(instr, 0)?;
        let value = self.read_operand(instr, 1, size)?;
        self.write_operand(instr, 0, value, size)?;
        Ok(())
    }

    fn emit_lea(&mut self, instr: &Instruction) -> Result<()> {
        let size = self.dest_size(instr, 0)?.max(8);
        let addr = self.effective_address(instr)?;
        self.write_operand(instr, 0, addr, size)?;
        Ok(())
    }

    fn emit_push(&mut self, instr: &Instruction) -> Result<()> {
        let value = self.read_operand(instr, 0, 8)?;
        let rsp = gpr(4, 8);
        let new_rsp = self.tmp(8);
        self.push(
            PcodeOpcode::IntSub,
            Some(new_rsp.clone()),
            vec![rsp.clone(), const_u64(8, 8)],
            instr,
        );
        self.push(PcodeOpcode::Copy, Some(rsp), vec![new_rsp.clone()], instr);
        self.push(
            PcodeOpcode::Store,
            None,
            vec![const_u64(0, 8), new_rsp, value],
            instr,
        );
        Ok(())
    }

    fn emit_pop(&mut self, instr: &Instruction) -> Result<()> {
        let rsp = gpr(4, 8);
        let value = self.tmp(8);
        self.push(
            PcodeOpcode::Load,
            Some(value.clone()),
            vec![const_u64(0, 8), rsp.clone()],
            instr,
        );
        self.write_operand(instr, 0, value, self.dest_size(instr, 0)?.max(8))?;
        self.push(
            PcodeOpcode::IntAdd,
            Some(rsp.clone()),
            vec![rsp, const_u64(8, 8)],
            instr,
        );
        Ok(())
    }

    fn emit_leave(&mut self, instr: &Instruction) -> Result<()> {
        let rsp = gpr(4, 8);
        let rbp = gpr(5, 8);
        self.push(PcodeOpcode::Copy, Some(rsp.clone()), vec![rbp], instr);
        let value = self.tmp(8);
        self.push(
            PcodeOpcode::Load,
            Some(value.clone()),
            vec![const_u64(0, 8), rsp.clone()],
            instr,
        );
        self.push(PcodeOpcode::Copy, Some(gpr(5, 8)), vec![value], instr);
        self.push(
            PcodeOpcode::IntAdd,
            Some(rsp.clone()),
            vec![rsp, const_u64(8, 8)],
            instr,
        );
        Ok(())
    }

    fn emit_binary(&mut self, instr: &Instruction, opcode: PcodeOpcode) -> Result<()> {
        let size = self.dest_size(instr, 0)?;
        let lhs = self.read_operand(instr, 0, size)?;
        let rhs = self.read_operand(instr, 1, size)?;
        let result = self.tmp(size);
        self.push(opcode, Some(result.clone()), vec![lhs, rhs], instr);
        self.write_operand(instr, 0, result.clone(), size)?;
        self.emit_basic_result_flags(result, size, instr);
        Ok(())
    }

    fn emit_unary_delta(&mut self, instr: &Instruction, delta: i64) -> Result<()> {
        let size = self.dest_size(instr, 0)?;
        let lhs = self.read_operand(instr, 0, size)?;
        let result = self.tmp(size);
        let (opcode, rhs) = if delta >= 0 {
            (PcodeOpcode::IntAdd, const_u64(delta as u64, size))
        } else {
            (PcodeOpcode::IntSub, const_u64(delta.unsigned_abs(), size))
        };
        self.push(opcode, Some(result.clone()), vec![lhs, rhs], instr);
        self.write_operand(instr, 0, result.clone(), size)?;
        self.emit_basic_result_flags(result, size, instr);
        Ok(())
    }

    fn emit_compare(&mut self, instr: &Instruction, bitwise: bool) -> Result<()> {
        let size = self
            .operand_size(instr, 0)?
            .max(self.operand_size(instr, 1)?);
        let lhs = self.read_operand(instr, 0, size)?;
        let rhs = self.read_operand(instr, 1, size)?;
        let result = self.tmp(size);
        self.push(
            if bitwise {
                PcodeOpcode::IntAnd
            } else {
                PcodeOpcode::IntSub
            },
            Some(result.clone()),
            vec![lhs.clone(), rhs.clone()],
            instr,
        );
        self.emit_basic_result_flags(result, size, instr);
        let cf_value = if bitwise {
            const_u64(0, 1)
        } else {
            let cf = self.tmp(1);
            self.push(
                PcodeOpcode::IntLess,
                Some(cf.clone()),
                vec![lhs, rhs],
                instr,
            );
            cf
        };
        self.push(PcodeOpcode::Copy, Some(flag(0)), vec![cf_value], instr);
        Ok(())
    }

    fn emit_extend(&mut self, instr: &Instruction, opcode: PcodeOpcode) -> Result<()> {
        let dst_size = self.dest_size(instr, 0)?;
        let src_size = self.operand_size(instr, 1)?;
        let src = self.read_operand(instr, 1, src_size)?;
        let out = self.tmp(dst_size);
        self.push(opcode, Some(out.clone()), vec![src], instr);
        self.write_operand(instr, 0, out, dst_size)?;
        Ok(())
    }

    fn emit_setcc(&mut self, instr: &Instruction) -> Result<()> {
        let cond = self.condition_varnode(setcc_to_jcc(instr.mnemonic())?)?;
        self.write_operand(instr, 0, cond, 1)?;
        Ok(())
    }

    fn emit_accumulator_extend(&mut self, instr: &Instruction) -> Result<()> {
        let (src, dst, opcode) = match instr.mnemonic() {
            Mnemonic::Cbw => (gpr(0, 1), gpr(0, 2), PcodeOpcode::IntSExt),
            Mnemonic::Cwde => (gpr(0, 2), gpr(0, 4), PcodeOpcode::IntSExt),
            Mnemonic::Cdqe => (gpr(0, 4), gpr(0, 8), PcodeOpcode::IntSExt),
            _ => bail!("unsupported accumulator extension {:?}", instr.mnemonic()),
        };
        self.push(opcode, Some(dst), vec![src], instr);
        Ok(())
    }

    fn condition_varnode(&mut self, mnemonic: Mnemonic) -> Result<Varnode> {
        let Some(cond) = jcc_condition(mnemonic) else {
            bail!("not a condition mnemonic: {:?}", mnemonic);
        };
        Ok(match cond {
            Cond::O => flag(11),
            Cond::No => self.bool_not(flag(11), "JNO_PRED"),
            Cond::B => flag(0),
            Cond::Ae => self.bool_not(flag(0), "JAE_PRED"),
            Cond::E => flag(6),
            Cond::Ne => self.bool_not(flag(6), "JNE_PRED"),
            Cond::Be => self.bool_or(flag(0), flag(6), "JBE_PRED"),
            Cond::A => {
                let ncf = self.bool_not(flag(0), "JA_NCF");
                let nzf = self.bool_not(flag(6), "JA_NZF");
                self.bool_and(ncf, nzf, "JA_PRED")
            }
            Cond::S => flag(7),
            Cond::Ns => self.bool_not(flag(7), "JNS_PRED"),
            Cond::P => flag(2),
            Cond::Np => self.bool_not(flag(2), "JNP_PRED"),
            Cond::L => self.bool_ne(flag(7), flag(11), "JL_PRED"),
            Cond::Ge => self.bool_eq(flag(7), flag(11), "JGE_PRED"),
            Cond::Le => {
                let lt = self.bool_ne(flag(7), flag(11), "JLE_LT_CORE");
                self.bool_or(flag(6), lt, "JLE_PRED")
            }
            Cond::G => {
                let ge = self.bool_eq(flag(7), flag(11), "JG_GE_CORE");
                let nz = self.bool_not(flag(6), "JG_NZ");
                self.bool_and(ge, nz, "JG_PRED")
            }
        })
    }

    fn emit_basic_result_flags(&mut self, result: Varnode, size: u32, instr: &Instruction) {
        let zf = self.tmp(1);
        self.push(
            PcodeOpcode::IntEqual,
            Some(zf.clone()),
            vec![result.clone(), const_u64(0, size)],
            instr,
        );
        self.push(PcodeOpcode::Copy, Some(flag(6)), vec![zf], instr);

        let shift = size.saturating_mul(8).saturating_sub(1);
        let sf = self.tmp(1);
        self.push(
            PcodeOpcode::IntRight,
            Some(sf.clone()),
            vec![result, const_u64(u64::from(shift), size)],
            instr,
        );
        self.push(PcodeOpcode::Copy, Some(flag(7)), vec![sf], instr);
    }

    fn read_operand(&mut self, instr: &Instruction, index: u32, size: u32) -> Result<Varnode> {
        match instr.op_kind(index) {
            OpKind::Register => register_varnode(instr.op_register(index))
                .ok_or_else(|| anyhow!("unsupported x86 register {:?}", instr.op_register(index))),
            OpKind::Memory => {
                let addr = self.effective_address(instr)?;
                let out = self.tmp(size);
                self.push(
                    PcodeOpcode::Load,
                    Some(out.clone()),
                    vec![const_u64(0, 8), addr],
                    instr,
                );
                Ok(out)
            }
            kind if is_near_branch(kind) => Ok(const_u64(instr.near_branch_target(), 8)),
            kind if immediate_value(instr, index, kind).is_some() => Ok(const_u64(
                immediate_value(instr, index, kind).unwrap(),
                size,
            )),
            kind => Err(anyhow!("unsupported x86 operand kind {:?}", kind)),
        }
    }

    fn write_operand(
        &mut self,
        instr: &Instruction,
        index: u32,
        value: Varnode,
        _size: u32,
    ) -> Result<()> {
        match instr.op_kind(index) {
            OpKind::Register => {
                let dst = register_varnode(instr.op_register(index)).ok_or_else(|| {
                    anyhow!("unsupported x86 register {:?}", instr.op_register(index))
                })?;
                self.push(PcodeOpcode::Copy, Some(dst), vec![value], instr);
                Ok(())
            }
            OpKind::Memory => {
                let addr = self.effective_address(instr)?;
                self.push(
                    PcodeOpcode::Store,
                    None,
                    vec![const_u64(0, 8), addr, value],
                    instr,
                );
                Ok(())
            }
            kind => Err(anyhow!("unsupported x86 write operand kind {:?}", kind)),
        }
    }

    fn effective_address(&mut self, instr: &Instruction) -> Result<Varnode> {
        let mut terms = Vec::new();
        let base = instr.memory_base();
        if base != Register::None {
            if matches!(base, Register::RIP | Register::EIP) {
                terms.push(const_u64(instr.memory_displacement64(), 8));
            } else if let Some(vn) = register_varnode(base) {
                terms.push(vn);
            }
        }

        let index = instr.memory_index();
        if index != Register::None {
            let idx = register_varnode(index)
                .ok_or_else(|| anyhow!("unsupported x86 memory index {:?}", index))?;
            let scale = instr.memory_index_scale();
            if scale > 1 {
                let scaled = self.tmp(8);
                self.push(
                    PcodeOpcode::IntMult,
                    Some(scaled.clone()),
                    vec![idx, const_u64(u64::from(scale), 8)],
                    instr,
                );
                terms.push(scaled);
            } else {
                terms.push(idx);
            }
        }

        let disp = instr.memory_displacement64();
        if disp != 0 && !matches!(base, Register::RIP | Register::EIP) {
            terms.push(const_u64(disp, 8));
        }

        let mut iter = terms.into_iter();
        let Some(mut acc) = iter.next() else {
            return Ok(const_u64(0, 8));
        };
        for term in iter {
            let next = self.tmp(8);
            self.push(
                PcodeOpcode::IntAdd,
                Some(next.clone()),
                vec![acc, term],
                instr,
            );
            acc = next;
        }
        Ok(acc)
    }

    fn operand_size(&self, instr: &Instruction, index: u32) -> Result<u32> {
        match instr.op_kind(index) {
            OpKind::Register => register_size(instr.op_register(index))
                .ok_or_else(|| anyhow!("unsupported x86 register {:?}", instr.op_register(index))),
            OpKind::Memory => {
                let size = instr.memory_size().size() as u32;
                Ok(size.max(1))
            }
            kind if immediate_value(instr, index, kind).is_some() => Ok(match kind {
                OpKind::Immediate8 | OpKind::Immediate8to16 | OpKind::Immediate8to32 => 1,
                OpKind::Immediate16 => 2,
                OpKind::Immediate32 | OpKind::Immediate32to64 => 4,
                OpKind::Immediate64 => 8,
                _ => 8,
            }),
            kind if is_near_branch(kind) => Ok(8),
            kind => Err(anyhow!("unsupported x86 operand size kind {:?}", kind)),
        }
    }

    fn dest_size(&self, instr: &Instruction, index: u32) -> Result<u32> {
        self.operand_size(instr, index)
    }

    fn tmp(&mut self, size: u32) -> Varnode {
        let vn = Varnode {
            space_id: UNIQUE_SPACE_ID,
            offset: self.next_tmp,
            size,
            is_constant: false,
            constant_val: 0,
        };
        self.next_tmp = self.next_tmp.wrapping_add(8);
        vn
    }

    fn bool_not(&mut self, input: Varnode, tag: &str) -> Varnode {
        let out = self.tmp(1);
        self.push_with_mnemonic(PcodeOpcode::BoolNegate, Some(out.clone()), vec![input], tag);
        out
    }

    fn bool_and(&mut self, lhs: Varnode, rhs: Varnode, tag: &str) -> Varnode {
        let out = self.tmp(1);
        self.push_with_mnemonic(PcodeOpcode::BoolAnd, Some(out.clone()), vec![lhs, rhs], tag);
        out
    }

    fn bool_or(&mut self, lhs: Varnode, rhs: Varnode, tag: &str) -> Varnode {
        let out = self.tmp(1);
        self.push_with_mnemonic(PcodeOpcode::BoolOr, Some(out.clone()), vec![lhs, rhs], tag);
        out
    }

    fn bool_eq(&mut self, lhs: Varnode, rhs: Varnode, tag: &str) -> Varnode {
        let out = self.tmp(1);
        self.push_with_mnemonic(
            PcodeOpcode::IntEqual,
            Some(out.clone()),
            vec![lhs, rhs],
            tag,
        );
        out
    }

    fn bool_ne(&mut self, lhs: Varnode, rhs: Varnode, tag: &str) -> Varnode {
        let out = self.tmp(1);
        self.push_with_mnemonic(
            PcodeOpcode::IntNotEqual,
            Some(out.clone()),
            vec![lhs, rhs],
            tag,
        );
        out
    }

    fn push(
        &mut self,
        opcode: PcodeOpcode,
        output: Option<Varnode>,
        inputs: Vec<Varnode>,
        instr: &Instruction,
    ) {
        self.push_with_mnemonic(opcode, output, inputs, &format!("{:?}", instr.mnemonic()));
    }

    fn push_with_mnemonic(
        &mut self,
        opcode: PcodeOpcode,
        output: Option<Varnode>,
        inputs: Vec<Varnode>,
        mnemonic: &str,
    ) {
        self.ops.push(PcodeOp {
            seq_num: self.seq,
            opcode,
            address: self.address,
            output,
            inputs,
            asm_mnemonic: Some(mnemonic.to_ascii_uppercase()),
        });
        self.seq = self.seq.saturating_add(1);
    }
}

fn const_u64(val: u64, size: u32) -> Varnode {
    let masked = if size >= 8 {
        val
    } else {
        let bits = size.saturating_mul(8);
        if bits == 0 {
            0
        } else {
            val & ((1u64 << bits) - 1)
        }
    };
    Varnode::constant(i64::from_ne_bytes(masked.to_ne_bytes()), size)
}

fn gpr(index: u64, size: u32) -> Varnode {
    Varnode {
        space_id: UNIQUE_SPACE_ID,
        offset: X86_REG_BASE + index * 8,
        size,
        is_constant: false,
        constant_val: 0,
    }
}

fn xmm(index: u64, size: u32) -> Varnode {
    Varnode {
        space_id: UNIQUE_SPACE_ID,
        offset: X86_XMM_BASE + index * 16,
        size,
        is_constant: false,
        constant_val: 0,
    }
}

fn ymm(index: u64, size: u32) -> Varnode {
    Varnode {
        space_id: UNIQUE_SPACE_ID,
        offset: X86_YMM_BASE + index * 32,
        size,
        is_constant: false,
        constant_val: 0,
    }
}

fn flag(bit: u64) -> Varnode {
    Varnode {
        space_id: UNIQUE_SPACE_ID,
        offset: X86_EFLAGS_BASE + bit,
        size: 1,
        is_constant: false,
        constant_val: 0,
    }
}

fn register_varnode(reg: Register) -> Option<Varnode> {
    let (idx, size) = match reg {
        Register::AL | Register::AX | Register::EAX | Register::RAX => (0, register_size(reg)?),
        Register::CL | Register::CX | Register::ECX | Register::RCX => (1, register_size(reg)?),
        Register::DL | Register::DX | Register::EDX | Register::RDX => (2, register_size(reg)?),
        Register::BL | Register::BX | Register::EBX | Register::RBX => (3, register_size(reg)?),
        Register::SPL | Register::SP | Register::ESP | Register::RSP => (4, register_size(reg)?),
        Register::BPL | Register::BP | Register::EBP | Register::RBP => (5, register_size(reg)?),
        Register::SIL | Register::SI | Register::ESI | Register::RSI => (6, register_size(reg)?),
        Register::DIL | Register::DI | Register::EDI | Register::RDI => (7, register_size(reg)?),
        Register::R8L | Register::R8W | Register::R8D | Register::R8 => (8, register_size(reg)?),
        Register::R9L | Register::R9W | Register::R9D | Register::R9 => (9, register_size(reg)?),
        Register::R10L | Register::R10W | Register::R10D | Register::R10 => {
            (10, register_size(reg)?)
        }
        Register::R11L | Register::R11W | Register::R11D | Register::R11 => {
            (11, register_size(reg)?)
        }
        Register::R12L | Register::R12W | Register::R12D | Register::R12 => {
            (12, register_size(reg)?)
        }
        Register::R13L | Register::R13W | Register::R13D | Register::R13 => {
            (13, register_size(reg)?)
        }
        Register::R14L | Register::R14W | Register::R14D | Register::R14 => {
            (14, register_size(reg)?)
        }
        Register::R15L | Register::R15W | Register::R15D | Register::R15 => {
            (15, register_size(reg)?)
        }
        Register::AH => (0, 1),
        Register::CH => (1, 1),
        Register::DH => (2, 1),
        Register::BH => (3, 1),
        Register::XMM0
        | Register::XMM1
        | Register::XMM2
        | Register::XMM3
        | Register::XMM4
        | Register::XMM5
        | Register::XMM6
        | Register::XMM7
        | Register::XMM8
        | Register::XMM9
        | Register::XMM10
        | Register::XMM11
        | Register::XMM12
        | Register::XMM13
        | Register::XMM14
        | Register::XMM15 => return Some(xmm(xmm_index(reg)?, 16)),
        Register::YMM0
        | Register::YMM1
        | Register::YMM2
        | Register::YMM3
        | Register::YMM4
        | Register::YMM5
        | Register::YMM6
        | Register::YMM7
        | Register::YMM8
        | Register::YMM9
        | Register::YMM10
        | Register::YMM11
        | Register::YMM12
        | Register::YMM13
        | Register::YMM14
        | Register::YMM15 => return Some(ymm(xmm_index(reg)?, 32)),
        Register::CS => return Some(segment(0)),
        Register::SS => return Some(segment(1)),
        Register::DS => return Some(segment(2)),
        Register::ES => return Some(segment(3)),
        Register::FS => return Some(segment(4)),
        Register::GS => return Some(segment(5)),
        _ => return None,
    };
    Some(gpr(idx, size))
}

fn register_size(reg: Register) -> Option<u32> {
    Some(match reg {
        Register::AL
        | Register::CL
        | Register::DL
        | Register::BL
        | Register::AH
        | Register::CH
        | Register::DH
        | Register::BH
        | Register::SPL
        | Register::BPL
        | Register::SIL
        | Register::DIL
        | Register::R8L
        | Register::R9L
        | Register::R10L
        | Register::R11L
        | Register::R12L
        | Register::R13L
        | Register::R14L
        | Register::R15L => 1,
        Register::AX
        | Register::CX
        | Register::DX
        | Register::BX
        | Register::SP
        | Register::BP
        | Register::SI
        | Register::DI
        | Register::R8W
        | Register::R9W
        | Register::R10W
        | Register::R11W
        | Register::R12W
        | Register::R13W
        | Register::R14W
        | Register::R15W => 2,
        Register::EAX
        | Register::ECX
        | Register::EDX
        | Register::EBX
        | Register::ESP
        | Register::EBP
        | Register::ESI
        | Register::EDI
        | Register::R8D
        | Register::R9D
        | Register::R10D
        | Register::R11D
        | Register::R12D
        | Register::R13D
        | Register::R14D
        | Register::R15D => 4,
        Register::RAX
        | Register::RCX
        | Register::RDX
        | Register::RBX
        | Register::RSP
        | Register::RBP
        | Register::RSI
        | Register::RDI
        | Register::R8
        | Register::R9
        | Register::R10
        | Register::R11
        | Register::R12
        | Register::R13
        | Register::R14
        | Register::R15
        | Register::RIP
        | Register::EIP => 8,
        Register::XMM0
        | Register::XMM1
        | Register::XMM2
        | Register::XMM3
        | Register::XMM4
        | Register::XMM5
        | Register::XMM6
        | Register::XMM7
        | Register::XMM8
        | Register::XMM9
        | Register::XMM10
        | Register::XMM11
        | Register::XMM12
        | Register::XMM13
        | Register::XMM14
        | Register::XMM15 => 16,
        Register::YMM0
        | Register::YMM1
        | Register::YMM2
        | Register::YMM3
        | Register::YMM4
        | Register::YMM5
        | Register::YMM6
        | Register::YMM7
        | Register::YMM8
        | Register::YMM9
        | Register::YMM10
        | Register::YMM11
        | Register::YMM12
        | Register::YMM13
        | Register::YMM14
        | Register::YMM15 => 32,
        _ => return None,
    })
}

fn xmm_index(reg: Register) -> Option<u64> {
    Some(match reg {
        Register::XMM0 | Register::YMM0 => 0,
        Register::XMM1 | Register::YMM1 => 1,
        Register::XMM2 | Register::YMM2 => 2,
        Register::XMM3 | Register::YMM3 => 3,
        Register::XMM4 | Register::YMM4 => 4,
        Register::XMM5 | Register::YMM5 => 5,
        Register::XMM6 | Register::YMM6 => 6,
        Register::XMM7 | Register::YMM7 => 7,
        Register::XMM8 | Register::YMM8 => 8,
        Register::XMM9 | Register::YMM9 => 9,
        Register::XMM10 | Register::YMM10 => 10,
        Register::XMM11 | Register::YMM11 => 11,
        Register::XMM12 | Register::YMM12 => 12,
        Register::XMM13 | Register::YMM13 => 13,
        Register::XMM14 | Register::YMM14 => 14,
        Register::XMM15 | Register::YMM15 => 15,
        _ => return None,
    })
}

fn segment(index: u64) -> Varnode {
    Varnode {
        space_id: UNIQUE_SPACE_ID,
        offset: X86_SEG_BASE + index * 8,
        size: 8,
        is_constant: false,
        constant_val: 0,
    }
}

fn is_near_branch(kind: OpKind) -> bool {
    matches!(
        kind,
        OpKind::NearBranch16 | OpKind::NearBranch32 | OpKind::NearBranch64
    )
}

fn immediate_value(instr: &Instruction, _index: u32, kind: OpKind) -> Option<u64> {
    Some(match kind {
        OpKind::Immediate8 => u64::from(instr.immediate8()),
        OpKind::Immediate8to16 | OpKind::Immediate8to32 | OpKind::Immediate8to64 => {
            instr.immediate8to64() as u64
        }
        OpKind::Immediate16 => u64::from(instr.immediate16()),
        OpKind::Immediate32 => u64::from(instr.immediate32()),
        OpKind::Immediate32to64 => instr.immediate32to64() as u64,
        OpKind::Immediate64 => instr.immediate64(),
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy)]
enum Cond {
    O,
    No,
    B,
    Ae,
    E,
    Ne,
    Be,
    A,
    S,
    Ns,
    P,
    Np,
    L,
    Ge,
    Le,
    G,
}

fn jcc_condition(mnemonic: Mnemonic) -> Option<Cond> {
    Some(match mnemonic {
        Mnemonic::Jo => Cond::O,
        Mnemonic::Jno => Cond::No,
        Mnemonic::Jb => Cond::B,
        Mnemonic::Jae => Cond::Ae,
        Mnemonic::Je => Cond::E,
        Mnemonic::Jne => Cond::Ne,
        Mnemonic::Jbe => Cond::Be,
        Mnemonic::Ja => Cond::A,
        Mnemonic::Js => Cond::S,
        Mnemonic::Jns => Cond::Ns,
        Mnemonic::Jp => Cond::P,
        Mnemonic::Jnp => Cond::Np,
        Mnemonic::Jl => Cond::L,
        Mnemonic::Jge => Cond::Ge,
        Mnemonic::Jle => Cond::Le,
        Mnemonic::Jg => Cond::G,
        _ => return None,
    })
}

fn setcc_to_jcc(mnemonic: Mnemonic) -> Result<Mnemonic> {
    Ok(match mnemonic {
        Mnemonic::Seto => Mnemonic::Jo,
        Mnemonic::Setno => Mnemonic::Jno,
        Mnemonic::Setb => Mnemonic::Jb,
        Mnemonic::Setae => Mnemonic::Jae,
        Mnemonic::Sete => Mnemonic::Je,
        Mnemonic::Setne => Mnemonic::Jne,
        Mnemonic::Setbe => Mnemonic::Jbe,
        Mnemonic::Seta => Mnemonic::Ja,
        Mnemonic::Sets => Mnemonic::Js,
        Mnemonic::Setns => Mnemonic::Jns,
        Mnemonic::Setp => Mnemonic::Jp,
        Mnemonic::Setnp => Mnemonic::Jnp,
        Mnemonic::Setl => Mnemonic::Jl,
        Mnemonic::Setge => Mnemonic::Jge,
        Mnemonic::Setle => Mnemonic::Jle,
        Mnemonic::Setg => Mnemonic::Jg,
        _ => bail!("unsupported setcc mnemonic {:?}", mnemonic),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn x86_64_runtime_decodes_ret() {
        let (ops, len) = decode_and_lift(&[0xC3], 0x1400).expect("ret");
        assert_eq!(len, 1);
        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].opcode, PcodeOpcode::Return);
    }

    #[test]
    fn x86_64_runtime_decodes_mov_imm_and_ret() {
        let (ops, len) = decode_and_lift(&[0xB8, 0x2A, 0, 0, 0], 0x1400).expect("mov eax, 42");
        assert_eq!(len, 5);
        assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::Copy));
        assert_eq!(
            ops.last().and_then(|op| op.output.as_ref()).unwrap().size,
            4
        );
    }

    #[test]
    fn x86_64_runtime_decodes_conditional_branch() {
        let (ops, len) = decode_and_lift(&[0x75, 0x05], 0x1000).expect("jne rel8");
        assert_eq!(len, 2);
        assert_eq!(ops.last().map(|op| op.opcode), Some(PcodeOpcode::CBranch));
        assert_eq!(ops.last().unwrap().inputs[0].constant_val as u64, 0x1007);
    }
}
