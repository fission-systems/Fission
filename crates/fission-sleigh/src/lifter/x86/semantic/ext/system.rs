use super::*;

// x87 FPU 추가 정책 ID
const X86_FLDCW_POLICY_ID: u64 = 0xD9_05;
const X86_FNSTCW_POLICY_ID: u64 = 0xD9_07;
const X86_FLDENV_POLICY_ID: u64 = 0xD9_04;
const X86_FNSTENV_POLICY_ID: u64 = 0xD9_06;
const X86_FINIT_POLICY_ID: u64 = 0xDB_E3;
const X86_FSIN_POLICY_ID: u64 = 0xD9_FE;
const X86_FCOS_POLICY_ID: u64 = 0xD9_FF;
const X86_FPTAN_POLICY_ID: u64 = 0xD9_F2;
const X86_FPATAN_POLICY_ID: u64 = 0xD9_F3;
const X86_F2XM1_POLICY_ID: u64 = 0xD9_F0;
const X86_FYL2X_POLICY_ID: u64 = 0xD9_F1;
const X86_FYL2XP1_POLICY_ID: u64 = 0xD9_F9;
const X86_FXTRACT_POLICY_ID: u64 = 0xD9_F4;
const X86_FPREM_POLICY_ID: u64 = 0xD9_F8;
const X86_FPREM1_POLICY_ID: u64 = 0xD9_F5;
const X86_FSCALE_POLICY_ID: u64 = 0xD9_FD;
const X86_FNOP_POLICY_ID: u64 = 0xD9_D0;
const X86_FCMOV_POLICY_BASE_ID: u64 = 0xDA_C0;

pub(super) fn decode_system_policy(
    address: u64,
    seq: &mut u32,
    policy_id: u64,
    mnemonic: &str,
) -> Vec<PcodeOp> {
    vec![PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: None,
        inputs: vec![const_u64(policy_id, 8)],
        asm_mnemonic: Some(mnemonic.to_string()),
    }]
}

pub(super) fn decode_rdtsc_policy(address: u64, seq: &mut u32) -> Vec<PcodeOp> {
    vec![PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: None,
        inputs: vec![const_u64(X86_RDTSC_POLICY_ID, 8)],
        asm_mnemonic: Some("RDTSC_POLICY".to_string()),
    }]
}

pub(super) fn decode_clflush_policy(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded =
        match decode_modrm_operand(insn, op_idx + 1, prefix, 1, address, temp, &mut ops, seq) {
            Some(v) => v,
            None => return Vec::new(),
        };

    if decoded.reg_field != 7 {
        return Vec::new();
    }

    let addr_vn = match decoded.rm {
        RmOperand::Mem(addr) => addr,
        RmOperand::Reg(_) => return Vec::new(),
    };

    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::CallOther,
        address,
        output: None,
        inputs: vec![const_u64(X86_CLFLUSH_POLICY_ID, 8), addr_vn],
        asm_mnemonic: Some("CLFLUSH_POLICY".to_string()),
    });

    ops
}

// Policy IDs for 0x0F 0xAE group
const X86_FXSAVE_POLICY_ID: u64 = 0x0FAE00;
const X86_FXRSTOR_POLICY_ID: u64 = 0x0FAE01;
const X86_LDMXCSR_POLICY_ID: u64 = 0x0FAE02;
const X86_STMXCSR_POLICY_ID: u64 = 0x0FAE03;
const X86_XSAVE_POLICY_ID: u64 = 0x0FAE04;
const X86_XRSTOR_POLICY_ID: u64 = 0x0FAE05;
const X86_XSAVEOPT_POLICY_ID: u64 = 0x0FAE06;
const X86_LFENCE_POLICY_ID: u64 = 0x0FAE_E8;
const X86_MFENCE_POLICY_ID: u64 = 0x0FAE_F0;
const X86_SFENCE_POLICY_ID: u64 = 0x0FAE_F8;

/// Full 0x0F 0xAE dispatcher: handles all variants based on reg field and mod.
///
/// reg | mod  | instruction
///  0  | mem  | FXSAVE
///  1  | mem  | FXRSTOR
///  2  | mem  | LDMXCSR
///  3  | mem  | STMXCSR
///  4  | mem  | XSAVE
///  5  | mem  | XRSTOR
///  5  | 11   | LFENCE
///  6  | mem  | XSAVEOPT
///  6  | 11   | MFENCE
///  7  | mem  | CLFLUSH
///  7  | 11   | SFENCE
pub(super) fn decode_0fae_group(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    // The caller (ext.rs) passes op_idx pointing to the 0x0F prefix byte.
    // Thus: insn[op_idx]=0x0F, insn[op_idx+1]=0xAE, insn[op_idx+2]=ModRM.
    let modrm_idx = op_idx + 2;

    // CLFLUSHOPT: 66 0F AE /7 → same layout as CLFLUSH but distinct policy
    if prefix.operand_size_override {
        let mut ops = Vec::new();
        let decoded = match decode_modrm_operand(
            insn,
            modrm_idx - 1,
            prefix,
            1,
            address,
            temp,
            &mut ops,
            seq,
        ) {
            Some(v) => v,
            None => return Vec::new(),
        };
        if decoded.reg_field == 7 {
            if let RmOperand::Mem(addr_vn) = decoded.rm {
                ops.push(PcodeOp {
                    seq_num: next_seq(seq),
                    opcode: PcodeOpcode::CallOther,
                    address,
                    output: None,
                    inputs: vec![const_u64(X86_CLFLUSHOPT_POLICY_ID, 8), addr_vn],
                    asm_mnemonic: Some("CLFLUSHOPT_POLICY".to_string()),
                });
                return ops;
            }
        }
        return ops;
    }

    let modrm = match insn.get(modrm_idx) {
        Some(v) => *v,
        None => return Vec::new(),
    };
    let reg_field = (modrm >> 3) & 0x7;
    let mod_field = (modrm >> 6) & 0x3;

    // Fence instructions: mod=11 (register operand field)
    if mod_field == 0x3 {
        let (policy_id, mnemonic) = match reg_field {
            5 => (X86_LFENCE_POLICY_ID, "LFENCE_POLICY"),
            6 => (X86_MFENCE_POLICY_ID, "MFENCE_POLICY"),
            7 => (X86_SFENCE_POLICY_ID, "SFENCE_POLICY"),
            _ => return Vec::new(),
        };
        return vec![PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::CallOther,
            address,
            output: None,
            inputs: vec![const_u64(policy_id, 8)],
            asm_mnemonic: Some(mnemonic.to_string()),
        }];
    }

    let mut ops = Vec::new();
    let decoded =
        match decode_modrm_operand(insn, modrm_idx - 1, prefix, 8, address, temp, &mut ops, seq) {
            Some(v) => v,
            None => return Vec::new(),
        };

    match reg_field {
        0 => {
            // FXSAVE: save x87/MMX/XMM state to memory
            let addr_vn = match decoded.rm {
                RmOperand::Mem(a) => a,
                RmOperand::Reg(_) => return Vec::new(),
            };
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::CallOther,
                address,
                output: None,
                inputs: vec![const_u64(X86_FXSAVE_POLICY_ID, 8), addr_vn],
                asm_mnemonic: Some("FXSAVE_POLICY".to_string()),
            });
        }
        1 => {
            // FXRSTOR: restore x87/MMX/XMM state from memory
            let addr_vn = match decoded.rm {
                RmOperand::Mem(a) => a,
                RmOperand::Reg(_) => return Vec::new(),
            };
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::CallOther,
                address,
                output: None,
                inputs: vec![const_u64(X86_FXRSTOR_POLICY_ID, 8), addr_vn],
                asm_mnemonic: Some("FXRSTOR_POLICY".to_string()),
            });
        }
        2 => {
            // LDMXCSR: load MXCSR from memory (m32)
            let mem_vn = match decoded.rm {
                RmOperand::Mem(a) => a,
                RmOperand::Reg(_) => return Vec::new(),
            };
            let loaded = temp.alloc(4);
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::Load,
                address,
                output: Some(loaded.clone()),
                inputs: vec![const_u64(0, 4), mem_vn],
                asm_mnemonic: Some("LDMXCSR_LOAD".to_string()),
            });
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::Copy,
                address,
                output: Some(x86_mxcsr()),
                inputs: vec![loaded],
                asm_mnemonic: Some("LDMXCSR_WRITE".to_string()),
            });
        }
        3 => {
            // STMXCSR: store MXCSR to memory (m32)
            let mem_vn = match decoded.rm {
                RmOperand::Mem(a) => a,
                RmOperand::Reg(_) => return Vec::new(),
            };
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::Store,
                address,
                output: None,
                inputs: vec![const_u64(0, 4), mem_vn, x86_mxcsr()],
                asm_mnemonic: Some("STMXCSR_STORE".to_string()),
            });
        }
        4 => {
            // XSAVE: save processor state
            let addr_vn = match decoded.rm {
                RmOperand::Mem(a) => a,
                RmOperand::Reg(_) => return Vec::new(),
            };
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::CallOther,
                address,
                output: None,
                inputs: vec![const_u64(X86_XSAVE_POLICY_ID, 8), addr_vn],
                asm_mnemonic: Some("XSAVE_POLICY".to_string()),
            });
        }
        5 => {
            // XRSTOR: restore processor state
            let addr_vn = match decoded.rm {
                RmOperand::Mem(a) => a,
                RmOperand::Reg(_) => return Vec::new(),
            };
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::CallOther,
                address,
                output: None,
                inputs: vec![const_u64(X86_XRSTOR_POLICY_ID, 8), addr_vn],
                asm_mnemonic: Some("XRSTOR_POLICY".to_string()),
            });
        }
        6 => {
            // XSAVEOPT: save processor state (optimized)
            let addr_vn = match decoded.rm {
                RmOperand::Mem(a) => a,
                RmOperand::Reg(_) => return Vec::new(),
            };
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::CallOther,
                address,
                output: None,
                inputs: vec![const_u64(X86_XSAVEOPT_POLICY_ID, 8), addr_vn],
                asm_mnemonic: Some("XSAVEOPT_POLICY".to_string()),
            });
        }
        7 => {
            // CLFLUSH: cache line flush
            let addr_vn = match decoded.rm {
                RmOperand::Mem(a) => a,
                RmOperand::Reg(_) => return Vec::new(),
            };
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::CallOther,
                address,
                output: None,
                inputs: vec![const_u64(X86_CLFLUSH_POLICY_ID, 8), addr_vn],
                asm_mnemonic: Some("CLFLUSH_POLICY".to_string()),
            });
        }
        _ => {}
    }

    ops
}

pub(super) fn decode_nop_extended(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    size: u32,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Vec<PcodeOp> {
    let mut ops = Vec::new();
    let decoded =
        match decode_modrm_operand(insn, op_idx + 1, prefix, size, address, temp, &mut ops, seq) {
            Some(v) => v,
            None => return Vec::new(),
        };

    if decoded.reg_field != 0 {
        return Vec::new();
    }

    // Treat 0F 1F /0 as a semantic no-op hint; keep address-side decoding deterministic.
    if matches!(decoded.rm, RmOperand::Reg(_)) {
        return Vec::new();
    }

    let hint = temp.alloc(8);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Copy,
        address,
        output: Some(hint),
        inputs: vec![const_u64(0x0F1F, 8)],
        asm_mnemonic: Some("NOP_EXT_HINT".to_string()),
    });

    ops
}

// --- x87 FPU helpers ---

fn x87_st(n: u8) -> Varnode {
    // ST(0)..ST(7) occupy register indices 16..23 at 10 bytes (80-bit extended precision)
    x86_reg(16 + u32::from(n), 10)
}

/// Load a float from memory and widen to 80-bit (x87 native format).
fn x87_load_float(
    addr_vn: Varnode,
    src_size: u32,
    ops: &mut Vec<PcodeOp>,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Varnode {
    let loaded = temp.alloc(src_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Load,
        address,
        output: Some(loaded.clone()),
        inputs: vec![const_u64(RAM_SPACE_ID, 8), addr_vn],
        asm_mnemonic: Some("X87_LOAD_FLOAT".to_string()),
    });
    if src_size == 10 {
        return loaded;
    }
    let widened = temp.alloc(10);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::FloatFloat2Float,
        address,
        output: Some(widened.clone()),
        inputs: vec![loaded],
        asm_mnemonic: Some("X87_F2F_WIDEN".to_string()),
    });
    widened
}

/// Load an integer from memory and convert to 80-bit float.
fn x87_load_int(
    addr_vn: Varnode,
    int_size: u32,
    ops: &mut Vec<PcodeOp>,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Varnode {
    let loaded = temp.alloc(int_size);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::Load,
        address,
        output: Some(loaded.clone()),
        inputs: vec![const_u64(RAM_SPACE_ID, 8), addr_vn],
        asm_mnemonic: Some("X87_LOAD_INT".to_string()),
    });
    let converted = temp.alloc(10);
    ops.push(PcodeOp {
        seq_num: next_seq(seq),
        opcode: PcodeOpcode::FloatInt2Float,
        address,
        output: Some(converted.clone()),
        inputs: vec![loaded],
        asm_mnemonic: Some("X87_INT2FLOAT".to_string()),
    });
    converted
}

/// Emit arithmetic op for D8-style instruction group.
/// reg_field 0=FADD, 1=FMUL, 2=FCOM, 3=FCOMP, 4=FSUB, 5=FSUBR, 6=FDIV, 7=FDIVR
fn x87_arith(
    reg_field: u8,
    operand: Varnode,
    st0: Varnode,
    ops: &mut Vec<PcodeOp>,
    address: u64,
    seq: &mut u32,
) {
    match reg_field {
        0 => ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::FloatAdd,
            address,
            output: Some(st0.clone()),
            inputs: vec![st0, operand],
            asm_mnemonic: Some("FADD".to_string()),
        }),
        1 => ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::FloatMult,
            address,
            output: Some(st0.clone()),
            inputs: vec![st0, operand],
            asm_mnemonic: Some("FMUL".to_string()),
        }),
        2 => {
            // FCOM: ST(0) vs operand → map C0→CF, C3→ZF for analysis
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::FloatLess,
                address,
                output: Some(x86_flag_cf()),
                inputs: vec![st0.clone(), operand.clone()],
                asm_mnemonic: Some("FCOM_C0".to_string()),
            });
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::FloatEqual,
                address,
                output: Some(x86_flag_zf()),
                inputs: vec![st0, operand],
                asm_mnemonic: Some("FCOM_C3".to_string()),
            });
        }
        3 => {
            // FCOMP: compare + pop (pop not modeled; stack push/pop out of scope for Phase 1)
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::FloatLess,
                address,
                output: Some(x86_flag_cf()),
                inputs: vec![st0.clone(), operand.clone()],
                asm_mnemonic: Some("FCOMP_C0".to_string()),
            });
            ops.push(PcodeOp {
                seq_num: next_seq(seq),
                opcode: PcodeOpcode::FloatEqual,
                address,
                output: Some(x86_flag_zf()),
                inputs: vec![st0, operand],
                asm_mnemonic: Some("FCOMP_C3".to_string()),
            });
        }
        4 => ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::FloatSub,
            address,
            output: Some(st0.clone()),
            inputs: vec![st0, operand],
            asm_mnemonic: Some("FSUB".to_string()),
        }),
        5 => ops.push(PcodeOp {
            // FSUBR: ST(0) = operand - ST(0)
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::FloatSub,
            address,
            output: Some(st0.clone()),
            inputs: vec![operand, st0],
            asm_mnemonic: Some("FSUBR".to_string()),
        }),
        6 => ops.push(PcodeOp {
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::FloatDiv,
            address,
            output: Some(st0.clone()),
            inputs: vec![st0, operand],
            asm_mnemonic: Some("FDIV".to_string()),
        }),
        7 => ops.push(PcodeOp {
            // FDIVR: ST(0) = operand / ST(0)
            seq_num: next_seq(seq),
            opcode: PcodeOpcode::FloatDiv,
            address,
            output: Some(st0.clone()),
            inputs: vec![operand, st0],
            asm_mnemonic: Some("FDIVR".to_string()),
        }),
        _ => {}
    }
}

/// Decode memory effective address for x87 instruction. `op_idx` points at the D8-DF opcode byte.
fn x87_mem_addr(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    mem_size: u32,
    address: u64,
    ops: &mut Vec<PcodeOp>,
    temp: &mut X86TempFactory,
    seq: &mut u32,
) -> Option<Varnode> {
    let decoded = decode_modrm_operand(insn, op_idx, prefix, mem_size, address, temp, ops, seq)?;
    match decoded.rm {
        RmOperand::Mem(a) => Some(a),
        RmOperand::Reg(_) => None,
    }
}

pub(crate) fn decode_x87_policy(
    insn: &[u8],
    op_idx: usize,
    prefix: &PrefixState,
    address: u64,
    temp: &mut X86TempFactory,
    seq: &mut u32,
    ext: u8,
) -> Vec<PcodeOp> {
    // Read raw ModRM byte to distinguish register vs. memory forms without calling
    // decode_modrm_operand (which computes effective addresses we may not need).
    let modrm = match insn.get(op_idx + 1) {
        Some(v) => *v,
        None => return Vec::new(),
    };
    let mod_field = (modrm >> 6) & 3;
    let reg_field = (modrm >> 3) & 7;
    let rm_low = modrm & 7;
    let is_reg = mod_field == 3;

    let st0 = x87_st(0);
    let mut ops = Vec::new();

    match ext {
        // D8: arithmetic ST(0) op= m32fp or ST(i)
        0 => {
            let operand = if is_reg {
                x87_st(rm_low)
            } else {
                let addr = match x87_mem_addr(insn, op_idx, prefix, 4, address, &mut ops, temp, seq)
                {
                    Some(a) => a,
                    None => return Vec::new(),
                };
                x87_load_float(addr, 4, &mut ops, address, temp, seq)
            };
            x87_arith(reg_field, operand, st0, &mut ops, address, seq);
        }

        // D9: FLD/FST/FSTP m32fp + misc register-only operations
        1 => {
            if is_reg {
                match reg_field {
                    0 => {
                        // FLD ST(n): push copy of ST(n) (simplified: ST(0) = ST(n))
                        ops.push(PcodeOp {
                            seq_num: next_seq(seq),
                            opcode: PcodeOpcode::Copy,
                            address,
                            output: Some(st0),
                            inputs: vec![x87_st(rm_low)],
                            asm_mnemonic: Some("FLD_ST".to_string()),
                        });
                    }
                    1 => {
                        // FXCH ST(n): swap ST(0) and ST(n)
                        let tmp = temp.alloc(10);
                        ops.push(PcodeOp {
                            seq_num: next_seq(seq),
                            opcode: PcodeOpcode::Copy,
                            address,
                            output: Some(tmp.clone()),
                            inputs: vec![st0.clone()],
                            asm_mnemonic: Some("FXCH_SAVE".to_string()),
                        });
                        ops.push(PcodeOp {
                            seq_num: next_seq(seq),
                            opcode: PcodeOpcode::Copy,
                            address,
                            output: Some(st0.clone()),
                            inputs: vec![x87_st(rm_low)],
                            asm_mnemonic: Some("FXCH_ST0".to_string()),
                        });
                        ops.push(PcodeOp {
                            seq_num: next_seq(seq),
                            opcode: PcodeOpcode::Copy,
                            address,
                            output: Some(x87_st(rm_low)),
                            inputs: vec![tmp],
                            asm_mnemonic: Some("FXCH_STN".to_string()),
                        });
                    }
                    4 if rm_low == 0 => {
                        // FCHS: ST(0) = -ST(0)
                        ops.push(PcodeOp {
                            seq_num: next_seq(seq),
                            opcode: PcodeOpcode::FloatNeg,
                            address,
                            output: Some(st0.clone()),
                            inputs: vec![st0],
                            asm_mnemonic: Some("FCHS".to_string()),
                        });
                    }
                    4 if rm_low == 1 => {
                        // FABS: ST(0) = |ST(0)|
                        ops.push(PcodeOp {
                            seq_num: next_seq(seq),
                            opcode: PcodeOpcode::FloatAbs,
                            address,
                            output: Some(st0.clone()),
                            inputs: vec![st0],
                            asm_mnemonic: Some("FABS".to_string()),
                        });
                    }
                    // FLD constants: reg_field==5, rm selects constant
                    5 => {
                        let (float_bits, mnem): (u64, &str) = match rm_low {
                            0 => (
                                0x3FFF_8000_0000_0000_0000u128.to_le_bytes()[0..8]
                                    .iter()
                                    .fold(0u64, |a, &b| (a << 8) | u64::from(b)),
                                "FLD1",
                            ),
                            1 => (0, "FLDL2T"), // log2(10) — CallOther
                            2 => (0, "FLDL2E"), // log2(e)  — CallOther
                            3 => (0, "FLDPI"),  // π        — CallOther
                            4 => (0, "FLDLG2"), // log10(2) — CallOther
                            5 => (0, "FLDLN2"), // ln(2)    — CallOther
                            6 => (0, "FLDZ"),   // +0.0     — handled below
                            _ => {
                                ops.push(PcodeOp {
                                    seq_num: next_seq(seq),
                                    opcode: PcodeOpcode::CallOther,
                                    address,
                                    output: None,
                                    inputs: vec![const_u64(X86_FNOP_POLICY_ID, 8)],
                                    asm_mnemonic: Some("FNOP".to_string()),
                                });
                                return ops;
                            }
                        };
                        if rm_low == 0 {
                            // FLD1: ST(0) = 1.0 (80-bit)
                            let one = Varnode::constant(1, 10);
                            ops.push(PcodeOp {
                                seq_num: next_seq(seq),
                                opcode: PcodeOpcode::FloatInt2Float,
                                address,
                                output: Some(st0),
                                inputs: vec![one],
                                asm_mnemonic: Some("FLD1".to_string()),
                            });
                        } else if rm_low == 6 {
                            // FLDZ: ST(0) = 0.0
                            ops.push(PcodeOp {
                                seq_num: next_seq(seq),
                                opcode: PcodeOpcode::Copy,
                                address,
                                output: Some(st0),
                                inputs: vec![const_u64(0, 10)],
                                asm_mnemonic: Some("FLDZ".to_string()),
                            });
                        } else {
                            let _ = float_bits;
                            // Transcendental constants → CallOther
                            let policy_id = match rm_low {
                                1 => X86_FYL2X_POLICY_ID,
                                2 => X86_F2XM1_POLICY_ID,
                                3 => X86_FPTAN_POLICY_ID,
                                4 => X86_FPATAN_POLICY_ID,
                                5 => X86_FYL2XP1_POLICY_ID,
                                _ => X86_FNOP_POLICY_ID,
                            };
                            ops.push(PcodeOp {
                                seq_num: next_seq(seq),
                                opcode: PcodeOpcode::CallOther,
                                address,
                                output: None,
                                inputs: vec![const_u64(policy_id, 8)],
                                asm_mnemonic: Some(mnem.to_string()),
                            });
                        }
                    }
                    // D9 E0-EF: FNOP, FXAM, FTST, trig/transcendental ops → CallOther
                    2 if rm_low == 0 => {
                        // FNOP
                        ops.push(PcodeOp {
                            seq_num: next_seq(seq),
                            opcode: PcodeOpcode::CallOther,
                            address,
                            output: None,
                            inputs: vec![const_u64(X86_FNOP_POLICY_ID, 8)],
                            asm_mnemonic: Some("FNOP".to_string()),
                        });
                    }
                    6 => {
                        // D9 F0-F7: F2XM1, FYL2X, FPTAN, FPATAN, FXTRACT, FPREM1, FDECSTP, FINCSTP
                        let policy_id = match rm_low {
                            0 => X86_F2XM1_POLICY_ID,
                            1 => X86_FYL2X_POLICY_ID,
                            2 => X86_FPTAN_POLICY_ID,
                            3 => X86_FPATAN_POLICY_ID,
                            4 => X86_FXTRACT_POLICY_ID,
                            5 => X86_FPREM1_POLICY_ID,
                            _ => X86_FNOP_POLICY_ID,
                        };
                        let st0_input = x87_st(0);
                        let mnemonic = match rm_low {
                            0 => "F2XM1",
                            1 => "FYL2X",
                            2 => "FPTAN",
                            3 => "FPATAN",
                            4 => "FXTRACT",
                            5 => "FPREM1",
                            6 => "FDECSTP",
                            _ => "FINCSTP",
                        };
                        ops.push(PcodeOp {
                            seq_num: next_seq(seq),
                            opcode: PcodeOpcode::CallOther,
                            address,
                            output: Some(st0.clone()),
                            inputs: vec![const_u64(policy_id, 8), st0_input],
                            asm_mnemonic: Some(mnemonic.to_string()),
                        });
                    }
                    7 => {
                        // D9 F8-FF: FPREM(F8), FYL2XP1(F9), FSQRT(FA), FSINCOS(FB),
                        //           FRNDINT(FC), FSCALE(FD), FSIN(FE), FCOS(FF)
                        if rm_low == 2 {
                            // FSQRT: ST(0) = sqrt(ST(0))
                            ops.push(PcodeOp {
                                seq_num: next_seq(seq),
                                opcode: PcodeOpcode::FloatSqrt,
                                address,
                                output: Some(st0.clone()),
                                inputs: vec![st0],
                                asm_mnemonic: Some("FSQRT".to_string()),
                            });
                        } else {
                            let (policy_id, mnemonic) = match rm_low {
                                0 => (X86_FPREM_POLICY_ID, "FPREM"),
                                1 => (X86_FYL2XP1_POLICY_ID, "FYL2XP1"),
                                4 => (X86_FSCALE_POLICY_ID, "FRNDINT"),
                                5 => (X86_FSCALE_POLICY_ID, "FSCALE"),
                                6 => (X86_FSIN_POLICY_ID, "FSIN"),
                                7 => (X86_FCOS_POLICY_ID, "FCOS"),
                                _ => (X86_FNOP_POLICY_ID, "FSINCOS"),
                            };
                            let st0_input = x87_st(0);
                            ops.push(PcodeOp {
                                seq_num: next_seq(seq),
                                opcode: PcodeOpcode::CallOther,
                                address,
                                output: Some(st0.clone()),
                                inputs: vec![const_u64(policy_id, 8), st0_input],
                                asm_mnemonic: Some(mnemonic.to_string()),
                            });
                        }
                    }
                    _ => {} // remaining (FXAM, FTST etc.) → no-op
                }
            } else {
                let addr = match x87_mem_addr(insn, op_idx, prefix, 4, address, &mut ops, temp, seq)
                {
                    Some(a) => a,
                    None => return Vec::new(),
                };
                match reg_field {
                    0 => {
                        // FLD m32fp → ST(0) = widen(load_f32(addr))
                        let val = x87_load_float(addr, 4, &mut ops, address, temp, seq);
                        ops.push(PcodeOp {
                            seq_num: next_seq(seq),
                            opcode: PcodeOpcode::Copy,
                            address,
                            output: Some(st0),
                            inputs: vec![val],
                            asm_mnemonic: Some("FLD_M32".to_string()),
                        });
                    }
                    2 | 3 => {
                        // FST/FSTP m32fp → store narrow(ST(0)) to mem
                        let narrowed = temp.alloc(4);
                        ops.push(PcodeOp {
                            seq_num: next_seq(seq),
                            opcode: PcodeOpcode::FloatFloat2Float,
                            address,
                            output: Some(narrowed.clone()),
                            inputs: vec![st0],
                            asm_mnemonic: Some("X87_F2F_NARROW32".to_string()),
                        });
                        ops.push(PcodeOp {
                            seq_num: next_seq(seq),
                            opcode: PcodeOpcode::Store,
                            address,
                            output: None,
                            inputs: vec![const_u64(RAM_SPACE_ID, 8), addr, narrowed],
                            asm_mnemonic: Some(
                                if reg_field == 2 {
                                    "FST_M32"
                                } else {
                                    "FSTP_M32"
                                }
                                .to_string(),
                            ),
                        });
                    }
                    // FLDENV (4), FLDCW (5), FNSTENV (6), FNSTCW (7) → CallOther with address
                    4 | 5 | 6 | 7 => {
                        let policy_id = match reg_field {
                            4 => X86_FLDENV_POLICY_ID,
                            5 => X86_FLDCW_POLICY_ID,
                            6 => X86_FNSTENV_POLICY_ID,
                            7 => X86_FNSTCW_POLICY_ID,
                            _ => unreachable!(),
                        };
                        let mnem = match reg_field {
                            4 => "FLDENV_POLICY",
                            5 => "FLDCW_POLICY",
                            6 => "FNSTENV_POLICY",
                            7 => "FNSTCW_POLICY",
                            _ => unreachable!(),
                        };
                        ops.push(PcodeOp {
                            seq_num: next_seq(seq),
                            opcode: PcodeOpcode::CallOther,
                            address,
                            output: None,
                            inputs: vec![const_u64(policy_id, 8), addr],
                            asm_mnemonic: Some(mnem.to_string()),
                        });
                    }
                    _ => {}
                }
            }
        }

        // DA: integer 32-bit arithmetic (memory) or FCMOVcc (register form)
        2 => {
            if !is_reg {
                let addr = match x87_mem_addr(insn, op_idx, prefix, 4, address, &mut ops, temp, seq)
                {
                    Some(a) => a,
                    None => return Vec::new(),
                };
                let operand = x87_load_int(addr, 4, &mut ops, address, temp, seq);
                x87_arith(reg_field, operand, st0, &mut ops, address, seq);
            } else {
                // Register form = FCMOVcc → CallOther (conditional stack move depends on EFLAGS)
                let policy_id =
                    X86_FCMOV_POLICY_BASE_ID + u64::from(reg_field) * 8 + u64::from(rm_low);
                ops.push(PcodeOp {
                    seq_num: next_seq(seq),
                    opcode: PcodeOpcode::CallOther,
                    address,
                    output: None,
                    inputs: vec![const_u64(policy_id, 8)],
                    asm_mnemonic: Some("FCMOV_POLICY".to_string()),
                });
            }
        }

        // DB: FILD/FIST/FISTP m32int or FCOMI/FUCOMI/FCMOV (register form)
        3 => {
            if !is_reg {
                let addr = match x87_mem_addr(insn, op_idx, prefix, 4, address, &mut ops, temp, seq)
                {
                    Some(a) => a,
                    None => return Vec::new(),
                };
                match reg_field {
                    0 => {
                        // FILD m32int → ST(0) = int_to_float(load_i32(addr))
                        let val = x87_load_int(addr, 4, &mut ops, address, temp, seq);
                        ops.push(PcodeOp {
                            seq_num: next_seq(seq),
                            opcode: PcodeOpcode::Copy,
                            address,
                            output: Some(st0),
                            inputs: vec![val],
                            asm_mnemonic: Some("FILD_M32".to_string()),
                        });
                    }
                    2 | 3 => {
                        // FIST/FISTP m32int
                        let truncated = temp.alloc(4);
                        ops.push(PcodeOp {
                            seq_num: next_seq(seq),
                            opcode: PcodeOpcode::FloatTrunc,
                            address,
                            output: Some(truncated.clone()),
                            inputs: vec![st0],
                            asm_mnemonic: Some("X87_TRUNC32".to_string()),
                        });
                        ops.push(PcodeOp {
                            seq_num: next_seq(seq),
                            opcode: PcodeOpcode::Store,
                            address,
                            output: None,
                            inputs: vec![const_u64(RAM_SPACE_ID, 8), addr, truncated],
                            asm_mnemonic: Some(
                                if reg_field == 2 {
                                    "FIST_M32"
                                } else {
                                    "FISTP_M32"
                                }
                                .to_string(),
                            ),
                        });
                    }
                    _ => {}
                }
            } else {
                // Register form: DB E3 = FINIT, DB /6 = FCOMI, DB /7 = FUCOMI, others = FCMOV
                if reg_field == 4 && rm_low == 3 {
                    // FINIT → CallOther
                    ops.push(PcodeOp {
                        seq_num: next_seq(seq),
                        opcode: PcodeOpcode::CallOther,
                        address,
                        output: None,
                        inputs: vec![const_u64(X86_FINIT_POLICY_ID, 8)],
                        asm_mnemonic: Some("FINIT_POLICY".to_string()),
                    });
                } else if reg_field == 6 || reg_field == 7 {
                    // FCOMI / FUCOMI ST(0), ST(i): compare and set CF/ZF/PF
                    let sti = x87_st(rm_low);
                    let mnem = if reg_field == 6 { "FCOMI" } else { "FUCOMI" };
                    // ZF = (ST(0) == ST(i))
                    ops.push(PcodeOp {
                        seq_num: next_seq(seq),
                        opcode: PcodeOpcode::FloatEqual,
                        address,
                        output: Some(x86_flag_zf()),
                        inputs: vec![st0.clone(), sti.clone()],
                        asm_mnemonic: Some(format!("{mnem}_ZF")),
                    });
                    // CF = (ST(0) < ST(i))
                    ops.push(PcodeOp {
                        seq_num: next_seq(seq),
                        opcode: PcodeOpcode::FloatLess,
                        address,
                        output: Some(x86_flag_cf()),
                        inputs: vec![st0, sti],
                        asm_mnemonic: Some(format!("{mnem}_CF")),
                    });
                    // PF = 0 (unordered not modeled; set to 0 for simplicity)
                    ops.push(PcodeOp {
                        seq_num: next_seq(seq),
                        opcode: PcodeOpcode::Copy,
                        address,
                        output: Some(x86_flag_pf()),
                        inputs: vec![const_u64(0, 1)],
                        asm_mnemonic: Some(format!("{mnem}_PF")),
                    });
                } else {
                    // FCMOVcc register form → CallOther
                    let policy_id = X86_FCMOV_POLICY_BASE_ID
                        + 0x100
                        + u64::from(reg_field) * 8
                        + u64::from(rm_low);
                    ops.push(PcodeOp {
                        seq_num: next_seq(seq),
                        opcode: PcodeOpcode::CallOther,
                        address,
                        output: None,
                        inputs: vec![const_u64(policy_id, 8)],
                        asm_mnemonic: Some("FCMOV_DB_POLICY".to_string()),
                    });
                }
            }
        }

        // DC: arithmetic ST(0) op= m64fp or ST(i) [register form reverses operands for non-arith]
        4 => {
            let operand = if is_reg {
                x87_st(rm_low)
            } else {
                let addr = match x87_mem_addr(insn, op_idx, prefix, 8, address, &mut ops, temp, seq)
                {
                    Some(a) => a,
                    None => return Vec::new(),
                };
                x87_load_float(addr, 8, &mut ops, address, temp, seq)
            };
            x87_arith(reg_field, operand, st0, &mut ops, address, seq);
        }

        // DD: FLD/FST/FSTP m64fp + register FST/FSTP ST(n)
        5 => {
            if is_reg {
                match reg_field {
                    2 => {
                        // FST ST(n): ST(n) = ST(0)
                        ops.push(PcodeOp {
                            seq_num: next_seq(seq),
                            opcode: PcodeOpcode::Copy,
                            address,
                            output: Some(x87_st(rm_low)),
                            inputs: vec![st0],
                            asm_mnemonic: Some("FST_ST".to_string()),
                        });
                    }
                    3 => {
                        // FSTP ST(n): ST(n) = ST(0) + pop (pop not modeled)
                        ops.push(PcodeOp {
                            seq_num: next_seq(seq),
                            opcode: PcodeOpcode::Copy,
                            address,
                            output: Some(x87_st(rm_low)),
                            inputs: vec![st0],
                            asm_mnemonic: Some("FSTP_ST".to_string()),
                        });
                    }
                    _ => {} // FFREE ST(n) and others → no-op
                }
            } else {
                let addr = match x87_mem_addr(insn, op_idx, prefix, 8, address, &mut ops, temp, seq)
                {
                    Some(a) => a,
                    None => return Vec::new(),
                };
                match reg_field {
                    0 => {
                        // FLD m64fp → ST(0) = widen(load_f64(addr))
                        let val = x87_load_float(addr, 8, &mut ops, address, temp, seq);
                        ops.push(PcodeOp {
                            seq_num: next_seq(seq),
                            opcode: PcodeOpcode::Copy,
                            address,
                            output: Some(st0),
                            inputs: vec![val],
                            asm_mnemonic: Some("FLD_M64".to_string()),
                        });
                    }
                    2 | 3 => {
                        // FST/FSTP m64fp
                        let narrowed = temp.alloc(8);
                        ops.push(PcodeOp {
                            seq_num: next_seq(seq),
                            opcode: PcodeOpcode::FloatFloat2Float,
                            address,
                            output: Some(narrowed.clone()),
                            inputs: vec![st0],
                            asm_mnemonic: Some("X87_F2F_NARROW64".to_string()),
                        });
                        ops.push(PcodeOp {
                            seq_num: next_seq(seq),
                            opcode: PcodeOpcode::Store,
                            address,
                            output: None,
                            inputs: vec![const_u64(RAM_SPACE_ID, 8), addr, narrowed],
                            asm_mnemonic: Some(
                                if reg_field == 2 {
                                    "FST_M64"
                                } else {
                                    "FSTP_M64"
                                }
                                .to_string(),
                            ),
                        });
                    }
                    _ => {}
                }
            }
        }

        // DE: FADDP/FMULP/etc (register = pop variants) + m16int arithmetic
        6 => {
            if is_reg {
                // Pop-form: operation on ST(n) with ST(0), result in ST(n) + pop
                let st_n = x87_st(rm_low);
                match reg_field {
                    0 => ops.push(PcodeOp {
                        seq_num: next_seq(seq),
                        opcode: PcodeOpcode::FloatAdd,
                        address,
                        output: Some(st_n.clone()),
                        inputs: vec![st_n, st0],
                        asm_mnemonic: Some("FADDP".to_string()),
                    }),
                    1 => ops.push(PcodeOp {
                        seq_num: next_seq(seq),
                        opcode: PcodeOpcode::FloatMult,
                        address,
                        output: Some(st_n.clone()),
                        inputs: vec![st_n, st0],
                        asm_mnemonic: Some("FMULP".to_string()),
                    }),
                    4 => ops.push(PcodeOp {
                        // FSUBRP: ST(n) = ST(0) - ST(n)
                        seq_num: next_seq(seq),
                        opcode: PcodeOpcode::FloatSub,
                        address,
                        output: Some(st_n.clone()),
                        inputs: vec![st0, st_n],
                        asm_mnemonic: Some("FSUBRP".to_string()),
                    }),
                    5 => ops.push(PcodeOp {
                        // FSUBP: ST(n) = ST(n) - ST(0)
                        seq_num: next_seq(seq),
                        opcode: PcodeOpcode::FloatSub,
                        address,
                        output: Some(st_n.clone()),
                        inputs: vec![st_n, st0],
                        asm_mnemonic: Some("FSUBP".to_string()),
                    }),
                    6 => ops.push(PcodeOp {
                        // FDIVRP: ST(n) = ST(0) / ST(n)
                        seq_num: next_seq(seq),
                        opcode: PcodeOpcode::FloatDiv,
                        address,
                        output: Some(st_n.clone()),
                        inputs: vec![st0, st_n],
                        asm_mnemonic: Some("FDIVRP".to_string()),
                    }),
                    7 => ops.push(PcodeOp {
                        // FDIVP: ST(n) = ST(n) / ST(0)
                        seq_num: next_seq(seq),
                        opcode: PcodeOpcode::FloatDiv,
                        address,
                        output: Some(st_n.clone()),
                        inputs: vec![st_n, st0],
                        asm_mnemonic: Some("FDIVP".to_string()),
                    }),
                    _ => {} // FCOMPP at DE/D9 → no-op
                }
            } else {
                // m16int arithmetic
                let addr = match x87_mem_addr(insn, op_idx, prefix, 2, address, &mut ops, temp, seq)
                {
                    Some(a) => a,
                    None => return Vec::new(),
                };
                let operand = x87_load_int(addr, 2, &mut ops, address, temp, seq);
                x87_arith(reg_field, operand, st0, &mut ops, address, seq);
            }
        }

        // DF: FILD/FIST/FISTP m16int; FILD m64int (/5), FISTP m64int (/7)
        7 => {
            if !is_reg {
                let int_size: u32 = if reg_field >= 5 { 8 } else { 2 };
                let addr = match x87_mem_addr(
                    insn, op_idx, prefix, int_size, address, &mut ops, temp, seq,
                ) {
                    Some(a) => a,
                    None => return Vec::new(),
                };
                match reg_field {
                    0 => {
                        // FILD m16int
                        let val = x87_load_int(addr, 2, &mut ops, address, temp, seq);
                        ops.push(PcodeOp {
                            seq_num: next_seq(seq),
                            opcode: PcodeOpcode::Copy,
                            address,
                            output: Some(st0),
                            inputs: vec![val],
                            asm_mnemonic: Some("FILD_M16".to_string()),
                        });
                    }
                    2 | 3 => {
                        // FIST/FISTP m16int
                        let truncated = temp.alloc(2);
                        ops.push(PcodeOp {
                            seq_num: next_seq(seq),
                            opcode: PcodeOpcode::FloatTrunc,
                            address,
                            output: Some(truncated.clone()),
                            inputs: vec![st0],
                            asm_mnemonic: Some("X87_TRUNC16".to_string()),
                        });
                        ops.push(PcodeOp {
                            seq_num: next_seq(seq),
                            opcode: PcodeOpcode::Store,
                            address,
                            output: None,
                            inputs: vec![const_u64(RAM_SPACE_ID, 8), addr, truncated],
                            asm_mnemonic: Some(
                                if reg_field == 2 {
                                    "FIST_M16"
                                } else {
                                    "FISTP_M16"
                                }
                                .to_string(),
                            ),
                        });
                    }
                    5 => {
                        // FILD m64int
                        let val = x87_load_int(addr, 8, &mut ops, address, temp, seq);
                        ops.push(PcodeOp {
                            seq_num: next_seq(seq),
                            opcode: PcodeOpcode::Copy,
                            address,
                            output: Some(st0),
                            inputs: vec![val],
                            asm_mnemonic: Some("FILD_M64".to_string()),
                        });
                    }
                    7 => {
                        // FISTP m64int
                        let truncated = temp.alloc(8);
                        ops.push(PcodeOp {
                            seq_num: next_seq(seq),
                            opcode: PcodeOpcode::FloatTrunc,
                            address,
                            output: Some(truncated.clone()),
                            inputs: vec![st0],
                            asm_mnemonic: Some("X87_TRUNC64".to_string()),
                        });
                        ops.push(PcodeOp {
                            seq_num: next_seq(seq),
                            opcode: PcodeOpcode::Store,
                            address,
                            output: None,
                            inputs: vec![const_u64(RAM_SPACE_ID, 8), addr, truncated],
                            asm_mnemonic: Some("FISTP_M64".to_string()),
                        });
                    }
                    _ => {}
                }
            }
            // DF register form: FUCOMIP (/5) and FCOMIP (/7) → compare + set flags (like FCOMI) + pop
            if is_reg && (reg_field == 5 || reg_field == 7) {
                let st0_new = x87_st(0);
                let sti = x87_st(rm_low);
                let mnem = if reg_field == 5 { "FUCOMIP" } else { "FCOMIP" };
                ops.push(PcodeOp {
                    seq_num: next_seq(seq),
                    opcode: PcodeOpcode::FloatEqual,
                    address,
                    output: Some(x86_flag_zf()),
                    inputs: vec![st0_new.clone(), sti.clone()],
                    asm_mnemonic: Some(format!("{mnem}_ZF")),
                });
                ops.push(PcodeOp {
                    seq_num: next_seq(seq),
                    opcode: PcodeOpcode::FloatLess,
                    address,
                    output: Some(x86_flag_cf()),
                    inputs: vec![st0_new, sti],
                    asm_mnemonic: Some(format!("{mnem}_CF")),
                });
                ops.push(PcodeOp {
                    seq_num: next_seq(seq),
                    opcode: PcodeOpcode::Copy,
                    address,
                    output: Some(x86_flag_pf()),
                    inputs: vec![const_u64(0, 1)],
                    asm_mnemonic: Some(format!("{mnem}_PF")),
                });
            }
        }

        _ => {}
    }

    ops
}
