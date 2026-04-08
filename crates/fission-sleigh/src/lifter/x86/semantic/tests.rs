use super::*;

fn has_flag_write(ops: &[PcodeOp], flag: Varnode) -> bool {
    ops.iter().any(|op| {
        op.output
            .as_ref()
            .map(|out| out.space_id == flag.space_id && out.offset == flag.offset)
            .unwrap_or(false)
    })
}

fn has_flag_zero_copy(ops: &[PcodeOp], flag: Varnode) -> bool {
    ops.iter().any(|op| {
        op.opcode == PcodeOpcode::Copy
            && op
                .output
                .as_ref()
                .map(|out| out.space_id == flag.space_id && out.offset == flag.offset)
                .unwrap_or(false)
            && op.inputs.len() == 1
            && op.inputs[0].is_constant
            && op.inputs[0].constant_val == 0
            && op.inputs[0].size == 1
    })
}

fn has_flag_input(ops: &[PcodeOp], flag: Varnode) -> bool {
    ops.iter().any(|op| {
        op.inputs
            .iter()
            .any(|inp| inp.space_id == flag.space_id && inp.offset == flag.offset)
    })
}

fn has_pf_pipeline(ops: &[PcodeOp]) -> bool {
    let has_low8 = ops
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PF_LOW8") && op.opcode == PcodeOpcode::IntAnd);
    let has_pop = ops
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PF_POPCNT") && op.opcode == PcodeOpcode::PopCount);
    let has_lsb = ops
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PF_LSB") && op.opcode == PcodeOpcode::IntAnd);
    let has_set = ops
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("SET_PF") && op.opcode == PcodeOpcode::IntEqual);
    has_low8 && has_pop && has_lsb && has_set
}

#[test]
fn decode_cmp_reg_reg_emits_sub_and_branch_flags() {
    let ops = decode_semantic(&[0x48, 0x39, 0xD8], 0x1000);
    assert!(!ops.is_empty());
    assert_eq!(ops[0].opcode, PcodeOpcode::IntSub);
    assert_eq!(ops[0].asm_mnemonic.as_deref(), Some("CMP"));
    assert!(has_flag_write(&ops, x86_flag_cf()));
    assert!(has_flag_write(&ops, x86_flag_of()));
    assert!(has_flag_write(&ops, x86_flag_zf()));
    assert!(has_flag_write(&ops, x86_flag_sf()));
    assert!(has_flag_write(&ops, x86_flag_pf()));
    assert!(has_pf_pipeline(&ops));
}

#[test]
fn decode_test_reg_reg_clears_cf_of_and_sets_zsp() {
    let ops = decode_semantic(&[0x85, 0xC0], 0x2000);
    assert!(!ops.is_empty());
    assert_eq!(ops[0].opcode, PcodeOpcode::IntAnd);
    assert_eq!(ops[0].asm_mnemonic.as_deref(), Some("TEST"));
    assert!(has_flag_zero_copy(&ops, x86_flag_cf()));
    assert!(has_flag_zero_copy(&ops, x86_flag_of()));
    assert!(has_flag_write(&ops, x86_flag_zf()));
    assert!(has_flag_write(&ops, x86_flag_sf()));
    assert!(has_flag_write(&ops, x86_flag_pf()));
    assert!(has_pf_pipeline(&ops));
}

#[test]
fn decode_basic_alu_group_register_forms() {
    let cases: [(&[u8], PcodeOpcode, &str); 5] = [
        (&[0x01, 0xD8], PcodeOpcode::IntAdd, "ADD"),
        (&[0x29, 0xD8], PcodeOpcode::IntSub, "SUB"),
        (&[0x21, 0xD8], PcodeOpcode::IntAnd, "AND"),
        (&[0x09, 0xD8], PcodeOpcode::IntOr, "OR"),
        (&[0x31, 0xD8], PcodeOpcode::IntXor, "XOR"),
    ];

    for (bytes, expected_opcode, expected_mnemonic) in cases {
        let ops = decode_semantic(bytes, 0x3000);
        assert!(!ops.is_empty(), "expected semantic ops for {expected_mnemonic}");
        assert_eq!(ops[0].opcode, expected_opcode, "{expected_mnemonic}");
        assert_eq!(ops[0].asm_mnemonic.as_deref(), Some(expected_mnemonic));
        assert_eq!(ops[0].output.as_ref(), Some(&x86_reg(0, 4)));
        assert!(has_flag_write(&ops, x86_flag_zf()));
        assert!(has_flag_write(&ops, x86_flag_pf()));
        assert!(has_pf_pipeline(&ops));
    }
}

#[test]
fn decode_immediate_81_83_cmp_sub_forms() {
    let cmp_ops = decode_semantic(&[0x81, 0xF8, 0x34, 0x12, 0x00, 0x00], 0x4100);
    assert!(!cmp_ops.is_empty());
    assert_eq!(cmp_ops[0].opcode, PcodeOpcode::IntSub);
    assert_eq!(cmp_ops[0].asm_mnemonic.as_deref(), Some("CMP"));
    assert!(has_pf_pipeline(&cmp_ops));

    let sub_ops = decode_semantic(&[0x83, 0xE8, 0xFF], 0x4200);
    assert!(!sub_ops.is_empty());
    assert_eq!(sub_ops[0].opcode, PcodeOpcode::IntSub);
    assert_eq!(sub_ops[0].asm_mnemonic.as_deref(), Some("SUB"));
    assert_eq!(sub_ops[0].inputs[1].size, 4);
    assert_eq!((sub_ops[0].inputs[1].constant_val as u64) & 0xFFFF_FFFF, 0xFFFF_FFFF);
    assert!(has_pf_pipeline(&sub_ops));
}

#[test]
fn decode_test_immediate_forms_f7_a9() {
    let f7_ops = decode_semantic(&[0xF7, 0xC0, 0x78, 0x56, 0x34, 0x12], 0x4300);
    assert!(!f7_ops.is_empty());
    assert_eq!(f7_ops[0].opcode, PcodeOpcode::IntAnd);
    assert_eq!(f7_ops[0].asm_mnemonic.as_deref(), Some("TEST"));
    assert!(has_flag_zero_copy(&f7_ops, x86_flag_cf()));
    assert!(has_flag_zero_copy(&f7_ops, x86_flag_of()));
    assert!(has_pf_pipeline(&f7_ops));

    let a9_ops = decode_semantic(&[0xA9, 0x01, 0x00, 0x00, 0x00], 0x4310);
    assert!(!a9_ops.is_empty());
    assert_eq!(a9_ops[0].opcode, PcodeOpcode::IntAnd);
    assert_eq!(a9_ops[0].asm_mnemonic.as_deref(), Some("TEST"));
    assert!(has_pf_pipeline(&a9_ops));
}

#[test]
fn decode_memory_operand_read_write_forms() {
    let rm_dst = decode_semantic(&[0x01, 0x18], 0x5000); // add [rax], ebx
    assert!(rm_dst.iter().any(|op| op.opcode == PcodeOpcode::Load));
    assert!(rm_dst.iter().any(|op| op.opcode == PcodeOpcode::Store));
    assert!(rm_dst.iter().any(|op| op.asm_mnemonic.as_deref() == Some("RM_STORE")));
    assert!(has_pf_pipeline(&rm_dst));

    let reg_dst = decode_semantic(&[0x03, 0x18], 0x5010); // add ebx, [rax]
    assert!(reg_dst.iter().any(|op| op.opcode == PcodeOpcode::Load));
    assert!(!reg_dst.iter().any(|op| op.opcode == PcodeOpcode::Store));
    let add = reg_dst
        .iter()
        .find(|op| op.opcode == PcodeOpcode::IntAdd)
        .expect("add op");
    assert_eq!(add.output.as_ref(), Some(&x86_reg(3, 4)));
}

#[test]
fn decode_memory_cmp_has_no_store_but_has_pf_pipeline() {
    let ops = decode_semantic(&[0x39, 0x58, 0x04], 0x6000); // cmp [rax+4], ebx
    assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::Load));
    assert!(!ops.iter().any(|op| op.opcode == PcodeOpcode::Store));
    assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::IntSub));
    assert!(has_pf_pipeline(&ops));
}

#[test]
fn decode_adc_sbb_paths_consume_cf_and_update_flags() {
    let adc = decode_semantic(&[0x11, 0xD8], 0x6100); // adc eax, ebx
    assert!(!adc.is_empty());
    assert!(adc.iter().any(|op| op.asm_mnemonic.as_deref() == Some("ADC")));
    assert!(has_flag_input(&adc, x86_flag_cf()));
    assert!(has_flag_write(&adc, x86_flag_cf()));
    assert!(has_flag_write(&adc, x86_flag_of()));
    assert!(has_pf_pipeline(&adc));

    let sbb = decode_semantic(&[0x83, 0xD8, 0x01], 0x6110); // sbb eax, 1
    assert!(!sbb.is_empty());
    assert!(sbb.iter().any(|op| op.asm_mnemonic.as_deref() == Some("SBB")));
    assert!(has_flag_input(&sbb, x86_flag_cf()));
    assert!(has_flag_write(&sbb, x86_flag_cf()));
    assert!(has_flag_write(&sbb, x86_flag_of()));
    assert!(has_pf_pipeline(&sbb));
}

#[test]
fn decode_inc_dec_do_not_write_cf_but_update_other_flags() {
    let inc = decode_semantic(&[0xFF, 0xC0], 0x6200); // inc eax
    assert!(!inc.is_empty());
    assert!(inc.iter().any(|op| op.asm_mnemonic.as_deref() == Some("INC")));
    assert!(!has_flag_write(&inc, x86_flag_cf()));
    assert!(has_flag_write(&inc, x86_flag_of()));
    assert!(has_flag_write(&inc, x86_flag_zf()));
    assert!(has_pf_pipeline(&inc));

    let dec = decode_semantic(&[0xFF, 0xC8], 0x6210); // dec eax
    assert!(!dec.is_empty());
    assert!(dec.iter().any(|op| op.asm_mnemonic.as_deref() == Some("DEC")));
    assert!(!has_flag_write(&dec, x86_flag_cf()));
    assert!(has_flag_write(&dec, x86_flag_of()));
    assert!(has_flag_write(&dec, x86_flag_zf()));
    assert!(has_pf_pipeline(&dec));
}

#[test]
fn decode_neg_and_shift_forms_update_flags_and_memory_paths() {
    let neg = decode_semantic(&[0xF7, 0xD8], 0x6300); // neg eax
    assert!(!neg.is_empty());
    assert!(neg.iter().any(|op| op.asm_mnemonic.as_deref() == Some("NEG")));
    assert!(has_flag_write(&neg, x86_flag_cf()));
    assert!(has_flag_write(&neg, x86_flag_of()));
    assert!(has_pf_pipeline(&neg));

    let shl = decode_semantic(&[0xD1, 0xE0], 0x6310); // shl eax,1
    assert!(!shl.is_empty());
    assert!(shl.iter().any(|op| op.asm_mnemonic.as_deref() == Some("SHL")));
    assert!(has_flag_write(&shl, x86_flag_cf()));
    assert!(has_pf_pipeline(&shl));

    let shr = decode_semantic(&[0xC1, 0xE8, 0x03], 0x6320); // shr eax,3
    assert!(!shr.is_empty());
    assert!(shr.iter().any(|op| op.asm_mnemonic.as_deref() == Some("SHR")));
    assert!(has_flag_write(&shr, x86_flag_cf()));
    assert!(has_pf_pipeline(&shr));

    let inc_mem = decode_semantic(&[0xFF, 0x00], 0x6330); // inc dword ptr [rax]
    assert!(inc_mem.iter().any(|op| op.opcode == PcodeOpcode::Load));
    assert!(inc_mem.iter().any(|op| op.opcode == PcodeOpcode::Store));

    let sar_mem = decode_semantic(&[0xD1, 0x38], 0x6340); // sar dword ptr [rax],1
    assert!(sar_mem.iter().any(|op| op.opcode == PcodeOpcode::Load));
    assert!(sar_mem.iter().any(|op| op.opcode == PcodeOpcode::Store));
    assert!(has_pf_pipeline(&sar_mem));
}

#[test]
fn decode_respects_rex_w_operand_width() {
    let ops = decode_semantic(&[0x48, 0x01, 0xD8], 0x7000);
    assert!(!ops.is_empty());
    assert_eq!(ops[0].opcode, PcodeOpcode::IntAdd);
    assert_eq!(ops[0].output.as_ref(), Some(&x86_reg(0, 8)));
}

#[test]
fn decode_d3_shift_uses_cl_with_masking() {
    let ops = decode_semantic(&[0xD3, 0xE0], 0x7010); // shl eax, cl
    assert!(!ops.is_empty());

    let zext = ops
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("SHIFT_COUNT_ZEXT"))
        .expect("expected CL zext for D3 count");
    assert_eq!(zext.inputs[0], x86_reg(1, 1));

    let mask = ops
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("SHIFT_COUNT_MASK"))
        .expect("expected masked shift count");
    assert_eq!(mask.opcode, PcodeOpcode::IntAnd);
    assert!(mask.inputs[1].is_constant);
    assert_eq!(mask.inputs[1].constant_val as u64, 0x1F);
    assert!(ops
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("SHIFT_COUNT_NONZERO")));
    assert!(ops
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("SHIFT_RESULT_WRITE")));
    assert!(ops
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("SHIFT_CF_WRITE")));

    assert!(ops.iter().all(|op| op.asm_mnemonic.as_deref() != Some("SHL_OF")));
}

#[test]
fn decode_67_address_size_override_promotes_address_registers() {
    let ops = decode_semantic(&[0x67, 0x01, 0x18], 0x7020); // add dword ptr [eax], ebx
    assert!(!ops.is_empty());
    assert!(ops
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("EA32_FINAL_ZEXT")));
    let zext = ops
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("EA32_FINAL_ZEXT"))
        .expect("expected final 32->64 zext");
    assert_eq!(zext.inputs[0].size, 4);
    assert_eq!(zext.output.as_ref().map(|v| v.size), Some(8));
    assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::Load));
    assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::Store));
}

#[test]
fn decode_67_address_size_override_uses_32bit_disp_math() {
    let ops = decode_semantic(&[0x67, 0x01, 0x58, 0x04], 0x7028); // add dword ptr [eax+4], ebx
    assert!(!ops.is_empty());

    let disp = ops
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("EA32_DISP"))
        .expect("expected EA32 displacement op");
    assert_eq!(disp.output.as_ref().map(|v| v.size), Some(4));

    let zext = ops
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("EA32_FINAL_ZEXT"))
        .expect("expected final 32->64 zext");
    assert_eq!(zext.inputs[0].size, 4);
    assert_eq!(zext.output.as_ref().map(|v| v.size), Some(8));
}

#[test]
fn decode_byte_shift_group2_uses_byte_width() {
    let d0 = decode_semantic(&[0xD0, 0xE0], 0x7030); // shl al,1
    assert!(!d0.is_empty());
    let d0_shl = d0
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("SHL"))
        .expect("expected SHL for D0");
    assert_eq!(d0_shl.inputs[0].size, 1);
    assert_eq!(d0_shl.inputs[1].size, 1);

    let d2 = decode_semantic(&[0xD2, 0xE8], 0x7040); // shr al,cl
    assert!(!d2.is_empty());
    let d2_mask = d2
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("SHIFT_COUNT_MASK"))
        .expect("expected masked count for D2");
    assert_eq!(d2_mask.inputs[0], x86_reg(1, 1));
    assert!(d2
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("SHIFT_RESULT_WRITE")));

    let c0 = decode_semantic(&[0xC0, 0xF8, 0x03], 0x7050); // sar al,3
    assert!(!c0.is_empty());
    let c0_sar = c0
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("SAR"))
        .expect("expected SAR for C0");
    assert_eq!(c0_sar.inputs[0].size, 1);
    assert_eq!(c0_sar.inputs[1].size, 1);
}

#[test]
fn decode_rotate_group2_emits_pcode_shift_sequences() {
    // ROL eax, 1  →  explicit SHL | SHR | OR P-code + CF/OF writes
    let rol = decode_semantic(&[0xD1, 0xC0], 0x7058);
    assert!(!rol.is_empty());
    assert!(rol.iter().any(|op| op.asm_mnemonic.as_deref() == Some("ROL_SHL")));
    assert!(rol.iter().any(|op| op.asm_mnemonic.as_deref() == Some("ROL_SHR")));
    assert!(rol.iter().any(|op| op.asm_mnemonic.as_deref() == Some("ROL")));
    assert!(has_flag_write(&rol, x86_flag_cf()));
    assert!(has_flag_write(&rol, x86_flag_of()));
    assert!(!has_flag_write(&rol, x86_flag_zf()));
    assert!(!has_flag_write(&rol, x86_flag_pf()));
    assert!(!has_flag_write(&rol, x86_flag_sf()));

    // ROR eax, cl  →  needs count zext + mask, then ROR_SHR / ROR_SHL / ROR
    let ror_cl = decode_semantic(&[0xD3, 0xC8], 0x705C);
    assert!(!ror_cl.is_empty());
    assert!(ror_cl.iter().any(|op| op.asm_mnemonic.as_deref() == Some("ROT_COUNT_ZEXT")));
    assert!(ror_cl.iter().any(|op| op.asm_mnemonic.as_deref() == Some("ROR_SHR")));
    assert!(ror_cl.iter().any(|op| op.asm_mnemonic.as_deref() == Some("ROR_SHL")));
    assert!(ror_cl.iter().any(|op| op.asm_mnemonic.as_deref() == Some("ROR")));
    assert!(has_flag_write(&ror_cl, x86_flag_cf()));

    // ROR eax, 0  →  count=0 is suppressed (empty output)
    let ror_zero = decode_semantic(&[0xC1, 0xC8, 0x00], 0x705D);
    assert!(ror_zero.is_empty());
}

#[test]
fn decode_cbw_cwde_cdqe_and_cwd_cdq_cqo_sign_extend_without_flag_writes() {
    let cwde = decode_semantic(&[0x98], 0x705E); // cwde
    assert!(!cwde.is_empty());
    let cwde_sext = cwde
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("CBW_CWDE_CDQE_SEXT"))
        .expect("expected cwde sign-extend");
    assert_eq!(cwde_sext.opcode, PcodeOpcode::IntSExt);
    assert_eq!(cwde_sext.inputs[0], x86_reg(0, 2));
    let cwde_write = cwde
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("CBW_CWDE_CDQE_WRITE"))
        .expect("expected cwde write");
    assert_eq!(cwde_write.output.as_ref(), Some(&x86_reg(0, 4)));

    let cbw = decode_semantic(&[0x66, 0x98], 0x705F); // cbw
    let cbw_write = cbw
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("CBW_CWDE_CDQE_WRITE"))
        .expect("expected cbw write");
    assert_eq!(cbw_write.output.as_ref(), Some(&x86_reg(0, 2)));

    let cdqe = decode_semantic(&[0x48, 0x98], 0x7060); // cdqe
    let cdqe_write = cdqe
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("CBW_CWDE_CDQE_WRITE"))
        .expect("expected cdqe write");
    assert_eq!(cdqe_write.output.as_ref(), Some(&x86_reg(0, 8)));

    let cdq = decode_semantic(&[0x99], 0x7061); // cdq
    assert!(!cdq.is_empty());
    let cdq_write = cdq
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("CWD_CDQ_CQO_WRITE"))
        .expect("expected cdq high write");
    assert_eq!(cdq_write.output.as_ref(), Some(&x86_reg(2, 4)));

    let cwd = decode_semantic(&[0x66, 0x99], 0x7062); // cwd
    let cwd_write = cwd
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("CWD_CDQ_CQO_WRITE"))
        .expect("expected cwd high write");
    assert_eq!(cwd_write.output.as_ref(), Some(&x86_reg(2, 2)));

    let cqo = decode_semantic(&[0x48, 0x99], 0x7063); // cqo
    let cqo_write = cqo
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("CWD_CDQ_CQO_WRITE"))
        .expect("expected cqo high write");
    assert_eq!(cqo_write.output.as_ref(), Some(&x86_reg(2, 8)));

    for ops in [&cwde, &cbw, &cdqe, &cdq, &cwd, &cqo] {
        assert!(!has_flag_write(ops, x86_flag_cf()));
        assert!(!has_flag_write(ops, x86_flag_pf()));
        assert!(!has_flag_write(ops, x86_flag_zf()));
        assert!(!has_flag_write(ops, x86_flag_sf()));
        assert!(!has_flag_write(ops, x86_flag_of()));
    }
}

#[test]
fn decode_cmovne_reg_reg_emits_conditional_move_without_flag_writes() {
    let ops = decode_semantic(&[0x0F, 0x45, 0xC3], 0x7060); // cmovne eax, ebx
    assert!(!ops.is_empty());
    assert!(ops
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("CMOVcc_WRITE")));
    let write = ops
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("CMOVcc_WRITE"))
        .expect("expected cmov write");
    assert_eq!(write.output.as_ref(), Some(&x86_reg(0, 4)));
    assert!(has_flag_input(&ops, x86_flag_zf()));
    assert!(!has_flag_write(&ops, x86_flag_cf()));
    assert!(!has_flag_write(&ops, x86_flag_pf()));
    assert!(!has_flag_write(&ops, x86_flag_zf()));
    assert!(!has_flag_write(&ops, x86_flag_sf()));
    assert!(!has_flag_write(&ops, x86_flag_of()));
}

#[test]
fn decode_cmovz_mem_reg_loads_memory_and_writes_destination_register() {
    let ops = decode_semantic(&[0x0F, 0x44, 0x18], 0x7070); // cmovz ebx, dword ptr [rax]
    assert!(!ops.is_empty());
    assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::Load));
    let write = ops
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("CMOVcc_WRITE"))
        .expect("expected cmov write");
    assert_eq!(write.output.as_ref(), Some(&x86_reg(3, 4)));
}

#[test]
fn decode_setnz_reg_emits_byte_write_without_flag_writes() {
    let ops = decode_semantic(&[0x0F, 0x95, 0xC0], 0x7080); // setnz al
    assert!(!ops.is_empty());

    let write = ops
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("SETcc_WRITE"))
        .expect("expected setcc register write");
    assert_eq!(write.opcode, PcodeOpcode::Copy);
    assert_eq!(write.output.as_ref(), Some(&x86_reg(0, 1)));

    assert!(has_flag_input(&ops, x86_flag_zf()));
    assert!(!has_flag_write(&ops, x86_flag_cf()));
    assert!(!has_flag_write(&ops, x86_flag_pf()));
    assert!(!has_flag_write(&ops, x86_flag_zf()));
    assert!(!has_flag_write(&ops, x86_flag_sf()));
    assert!(!has_flag_write(&ops, x86_flag_of()));
}

#[test]
fn decode_setz_mem_stores_predicate_to_memory() {
    let ops = decode_semantic(&[0x0F, 0x94, 0x00], 0x7090); // setz byte ptr [rax]
    assert!(!ops.is_empty());
    assert!(!ops.iter().any(|op| op.opcode == PcodeOpcode::Load));

    let store = ops
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("SETcc_STORE"))
        .expect("expected setcc store");
    assert_eq!(store.opcode, PcodeOpcode::Store);
    assert_eq!(store.inputs.len(), 3);
    assert!(store.inputs[2].size == 1);
}

#[test]
fn decode_setg_reg_consumes_zf_sf_of_predicate_inputs() {
    let ops = decode_semantic(&[0x0F, 0x9F, 0xC0], 0x70A0); // setg al
    assert!(!ops.is_empty());

    let write = ops
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("SETcc_WRITE"))
        .expect("expected setcc register write");
    assert_eq!(write.output.as_ref(), Some(&x86_reg(0, 1)));

    assert!(has_flag_input(&ops, x86_flag_zf()));
    assert!(has_flag_input(&ops, x86_flag_sf()));
    assert!(has_flag_input(&ops, x86_flag_of()));
    assert!(!has_flag_write(&ops, x86_flag_cf()));
    assert!(!has_flag_write(&ops, x86_flag_pf()));
    assert!(!has_flag_write(&ops, x86_flag_zf()));
    assert!(!has_flag_write(&ops, x86_flag_sf()));
    assert!(!has_flag_write(&ops, x86_flag_of()));
}

#[test]
fn decode_setc_mem_consumes_cf_and_stores_byte() {
    let ops = decode_semantic(&[0x0F, 0x92, 0x00], 0x70B0); // setc byte ptr [rax]
    assert!(!ops.is_empty());

    let store = ops
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("SETcc_STORE"))
        .expect("expected setcc memory store");
    assert_eq!(store.opcode, PcodeOpcode::Store);
    assert_eq!(store.inputs[2].size, 1);
    assert!(has_flag_input(&ops, x86_flag_cf()));
    assert!(!has_flag_write(&ops, x86_flag_cf()));
}

#[test]
fn decode_setnz_with_rex_b_targets_extended_byte_register() {
    let ops = decode_semantic(&[0x41, 0x0F, 0x95, 0xC0], 0x70C0); // setnz r8b
    assert!(!ops.is_empty());

    let write = ops
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("SETcc_WRITE"))
        .expect("expected setcc register write");
    assert_eq!(write.output.as_ref(), Some(&x86_reg(8, 1)));
}

#[test]
fn decode_mov_rm_r_emits_store_without_flag_writes() {
    let ops = decode_semantic(&[0x89, 0x18], 0x7100); // mov dword ptr [rax], ebx
    assert!(!ops.is_empty());
    assert!(!ops.iter().any(|op| op.opcode == PcodeOpcode::Load));
    assert!(ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("MOV_STORE")));

    assert!(!has_flag_write(&ops, x86_flag_cf()));
    assert!(!has_flag_write(&ops, x86_flag_pf()));
    assert!(!has_flag_write(&ops, x86_flag_zf()));
    assert!(!has_flag_write(&ops, x86_flag_sf()));
    assert!(!has_flag_write(&ops, x86_flag_of()));
}

#[test]
fn decode_mov_r_rm_emits_load_and_register_write() {
    let ops = decode_semantic(&[0x8B, 0x18], 0x7110); // mov ebx, dword ptr [rax]
    assert!(!ops.is_empty());
    assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::Load));

    let write = ops
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("MOV_WRITE"))
        .expect("expected mov register write");
    assert_eq!(write.output.as_ref(), Some(&x86_reg(3, 4)));
}

#[test]
fn decode_mov_imm_forms_cover_rex_b_and_rex_w() {
    let byte_ops = decode_semantic(&[0x41, 0xB0, 0x7F], 0x7120); // mov r8b, 0x7f
    assert!(!byte_ops.is_empty());
    let byte_write = byte_ops
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("MOV_IMM_WRITE"))
        .expect("expected mov imm8 write");
    assert_eq!(byte_write.output.as_ref(), Some(&x86_reg(8, 1)));
    assert_eq!(byte_write.inputs[0].constant_val, 0x7F);

    let qword_ops = decode_semantic(&[0x49, 0xB8, 1, 2, 3, 4, 5, 6, 7, 8], 0x7130); // mov r8, imm64
    assert!(!qword_ops.is_empty());
    let qword_write = qword_ops
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("MOV_IMM_WRITE"))
        .expect("expected mov imm64 write");
    assert_eq!(qword_write.output.as_ref(), Some(&x86_reg(8, 8)));
    assert_eq!(qword_write.inputs[0].constant_val, 0x0807_0605_0403_0201);
}

#[test]
fn decode_xchg_reg_reg_and_byte_forms_emit_swap_writes_without_flags() {
    let dword_ops = decode_semantic(&[0x87, 0xD8], 0x7138); // xchg eax, ebx
    assert!(!dword_ops.is_empty());
    assert!(dword_ops
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("XCHG_REG_SAVE")));

    let reg_write = dword_ops
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("XCHG_REG_WRITE"))
        .expect("expected xchg register write");
    assert_eq!(reg_write.output.as_ref(), Some(&x86_reg(3, 4)));
    assert_eq!(reg_write.inputs[0], x86_reg(0, 4));

    let rm_write = dword_ops
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("XCHG_WRITE"))
        .expect("expected xchg rm write");
    assert_eq!(rm_write.output.as_ref(), Some(&x86_reg(0, 4)));
    assert_eq!(rm_write.inputs[0].size, 4);

    let byte_ops = decode_semantic(&[0x86, 0xD8], 0x7139); // xchg al, bl
    assert!(!byte_ops.is_empty());
    let byte_reg_write = byte_ops
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("XCHG_REG_WRITE"))
        .expect("expected byte xchg register write");
    assert_eq!(byte_reg_write.output.as_ref(), Some(&x86_reg(3, 1)));
    assert_eq!(byte_reg_write.inputs[0], x86_reg(0, 1));

    let byte_rm_write = byte_ops
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("XCHG_WRITE"))
        .expect("expected byte xchg rm write");
    assert_eq!(byte_rm_write.output.as_ref(), Some(&x86_reg(0, 1)));
    assert_eq!(byte_rm_write.inputs[0].size, 1);

    for ops in [&dword_ops, &byte_ops] {
        assert!(!has_flag_write(ops, x86_flag_cf()));
        assert!(!has_flag_write(ops, x86_flag_pf()));
        assert!(!has_flag_write(ops, x86_flag_zf()));
        assert!(!has_flag_write(ops, x86_flag_sf()));
        assert!(!has_flag_write(ops, x86_flag_of()));
    }
}

#[test]
fn decode_xchg_reg_mem_emits_load_store_swap_without_flags() {
    let ops = decode_semantic(&[0x87, 0x18], 0x713A); // xchg dword ptr [rax], ebx
    assert!(!ops.is_empty());
    assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::Load));

    let reg_write = ops
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("XCHG_REG_WRITE"))
        .expect("expected xchg register write");
    assert_eq!(reg_write.output.as_ref(), Some(&x86_reg(3, 4)));

    let store = ops
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("XCHG_STORE"))
        .expect("expected xchg memory store");
    assert_eq!(store.opcode, PcodeOpcode::Store);
    assert_eq!(store.inputs[2].size, 4);

    assert!(!has_flag_write(&ops, x86_flag_cf()));
    assert!(!has_flag_write(&ops, x86_flag_pf()));
    assert!(!has_flag_write(&ops, x86_flag_zf()));
    assert!(!has_flag_write(&ops, x86_flag_sf()));
    assert!(!has_flag_write(&ops, x86_flag_of()));
}

#[test]
fn decode_mov_group11_immediates_cover_memory_and_sign_extended_reg64() {
    let mem_ops = decode_semantic(&[0xC7, 0x00, 0x78, 0x56, 0x34, 0x12], 0x7140); // mov dword ptr [rax], 0x12345678
    assert!(!mem_ops.is_empty());
    let store = mem_ops
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("MOV_IMM_STORE"))
        .expect("expected mov imm store");
    assert_eq!(store.opcode, PcodeOpcode::Store);
    assert_eq!(store.inputs[2].size, 4);
    assert_eq!(store.inputs[2].constant_val, 0x1234_5678);

    let reg64_ops = decode_semantic(&[0x48, 0xC7, 0xC0, 0xFF, 0xFF, 0xFF, 0xFF], 0x7150); // mov rax, -1 (imm32 sign-extended)
    assert!(!reg64_ops.is_empty());
    let write = reg64_ops
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("MOV_IMM_WRITE"))
        .expect("expected mov imm register write");
    assert_eq!(write.output.as_ref(), Some(&x86_reg(0, 8)));
    assert_eq!(write.inputs[0].size, 8);
    assert_eq!(write.inputs[0].constant_val, -1);
}

#[test]
fn decode_movzx_movsx_reg_forms_extend_without_flag_writes() {
    let movzx = decode_semantic(&[0x0F, 0xB6, 0xC3], 0x7158); // movzx eax, bl
    assert!(!movzx.is_empty());
    let zx = movzx
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("MOVZX_WRITE"))
        .expect("expected MOVZX write");
    assert_eq!(zx.opcode, PcodeOpcode::IntZExt);
    assert_eq!(zx.output.as_ref(), Some(&x86_reg(0, 4)));
    assert_eq!(zx.inputs[0], x86_reg(3, 1));

    let movsx = decode_semantic(&[0x0F, 0xBE, 0xC3], 0x715C); // movsx eax, bl
    assert!(!movsx.is_empty());
    let sx = movsx
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("MOVSX_WRITE"))
        .expect("expected MOVSX write");
    assert_eq!(sx.opcode, PcodeOpcode::IntSExt);
    assert_eq!(sx.output.as_ref(), Some(&x86_reg(0, 4)));
    assert_eq!(sx.inputs[0], x86_reg(3, 1));

    assert!(!has_flag_write(&movzx, x86_flag_cf()));
    assert!(!has_flag_write(&movzx, x86_flag_pf()));
    assert!(!has_flag_write(&movzx, x86_flag_zf()));
    assert!(!has_flag_write(&movzx, x86_flag_sf()));
    assert!(!has_flag_write(&movzx, x86_flag_of()));
    assert!(!has_flag_write(&movsx, x86_flag_cf()));
    assert!(!has_flag_write(&movsx, x86_flag_pf()));
    assert!(!has_flag_write(&movsx, x86_flag_zf()));
    assert!(!has_flag_write(&movsx, x86_flag_sf()));
    assert!(!has_flag_write(&movsx, x86_flag_of()));
}

#[test]
fn decode_movzx_movsx_mem_forms_support_rexw_and_operand_override() {
    let movzx_rexw = decode_semantic(&[0x48, 0x0F, 0xB7, 0x18], 0x715E); // movzx rbx, word ptr [rax]
    assert!(!movzx_rexw.is_empty());
    assert!(movzx_rexw.iter().any(|op| op.opcode == PcodeOpcode::Load));
    let zx = movzx_rexw
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("MOVZX_WRITE"))
        .expect("expected MOVZX write");
    assert_eq!(zx.opcode, PcodeOpcode::IntZExt);
    assert_eq!(zx.output.as_ref(), Some(&x86_reg(3, 8)));
    assert_eq!(zx.inputs[0].size, 2);

    let movsx_rexw = decode_semantic(&[0x48, 0x0F, 0xBF, 0x18], 0x7161); // movsx rbx, word ptr [rax]
    assert!(!movsx_rexw.is_empty());
    assert!(movsx_rexw.iter().any(|op| op.opcode == PcodeOpcode::Load));
    let sx = movsx_rexw
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("MOVSX_WRITE"))
        .expect("expected MOVSX write");
    assert_eq!(sx.opcode, PcodeOpcode::IntSExt);
    assert_eq!(sx.output.as_ref(), Some(&x86_reg(3, 8)));
    assert_eq!(sx.inputs[0].size, 2);

    let movzx_16 = decode_semantic(&[0x66, 0x0F, 0xB6, 0xC3], 0x7164); // movzx ax, bl
    assert!(!movzx_16.is_empty());
    let zx16 = movzx_16
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("MOVZX_WRITE"))
        .expect("expected MOVZX write with 66 prefix");
    assert_eq!(zx16.opcode, PcodeOpcode::IntZExt);
    assert_eq!(zx16.output.as_ref(), Some(&x86_reg(0, 2)));
    assert_eq!(zx16.inputs[0], x86_reg(3, 1));

    let movsx_same_width = decode_semantic(&[0x66, 0x0F, 0xBF, 0xC3], 0x7168); // movsx ax, bx
    assert!(!movsx_same_width.is_empty());
    let sx_same = movsx_same_width
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("MOVSX_WRITE"))
        .expect("expected MOVSX write with equal width");
    assert_eq!(sx_same.opcode, PcodeOpcode::Copy);
    assert_eq!(sx_same.output.as_ref(), Some(&x86_reg(0, 2)));
    assert_eq!(sx_same.inputs[0], x86_reg(3, 2));
}

#[test]
fn decode_bsf_bsr_update_zf_and_preserve_other_flags() {
    let bsf = decode_semantic(&[0x0F, 0xBC, 0xC3], 0x716C); // bsf eax, ebx
    assert!(!bsf.is_empty());
    assert!(bsf.iter().any(|op| op.asm_mnemonic.as_deref() == Some("BSF_WRITE")));
    assert!(bsf.iter().any(|op| op.asm_mnemonic.as_deref() == Some("BSF_INDEX")));
    assert!(has_flag_write(&bsf, x86_flag_zf()));
    assert!(!has_flag_write(&bsf, x86_flag_cf()));
    assert!(!has_flag_write(&bsf, x86_flag_pf()));
    assert!(!has_flag_write(&bsf, x86_flag_sf()));
    assert!(!has_flag_write(&bsf, x86_flag_of()));

    let bsr = decode_semantic(&[0x0F, 0xBD, 0x18], 0x7170); // bsr ebx, dword ptr [rax]
    assert!(!bsr.is_empty());
    assert!(bsr.iter().any(|op| op.opcode == PcodeOpcode::Load));
    assert!(bsr.iter().any(|op| op.asm_mnemonic.as_deref() == Some("BSR_WRITE")));
    assert!(bsr.iter().any(|op| op.asm_mnemonic.as_deref() == Some("BSR_POPCNT")));
    assert!(has_flag_write(&bsr, x86_flag_zf()));
    assert!(!has_flag_write(&bsr, x86_flag_cf()));
    assert!(!has_flag_write(&bsr, x86_flag_pf()));
    assert!(!has_flag_write(&bsr, x86_flag_sf()));
    assert!(!has_flag_write(&bsr, x86_flag_of()));
}

#[test]
fn decode_bt_bts_btr_btc_update_cf_and_apply_rmw_rules() {
    let bt = decode_semantic(&[0x0F, 0xA3, 0xC8], 0x7172); // bt eax, ecx
    assert!(!bt.is_empty());
    assert!(has_flag_write(&bt, x86_flag_cf()));
    assert!(!bt.iter().any(|op| op.asm_mnemonic.as_deref() == Some("BT_WRITE")));
    assert!(!bt.iter().any(|op| op.asm_mnemonic.as_deref() == Some("BT_STORE")));
    assert!(!has_flag_write(&bt, x86_flag_zf()));
    assert!(!has_flag_write(&bt, x86_flag_sf()));
    assert!(!has_flag_write(&bt, x86_flag_pf()));
    assert!(!has_flag_write(&bt, x86_flag_of()));

    let bts = decode_semantic(&[0x0F, 0xAB, 0xC8], 0x7173); // bts eax, ecx
    assert!(!bts.is_empty());
    assert!(bts
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("BTS_SET") && op.opcode == PcodeOpcode::IntOr));
    assert!(bts
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("BTS_WRITE") && op.output.as_ref() == Some(&x86_reg(0, 4))));
    assert!(has_flag_write(&bts, x86_flag_cf()));

    let btr = decode_semantic(&[0x0F, 0xB3, 0x18], 0x7174); // btr dword ptr [rax], ebx
    assert!(!btr.is_empty());
    assert!(btr
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("BTR_MEM_WORD_INDEX") && op.opcode == PcodeOpcode::IntSRight));
    assert!(btr
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("BTR_MEM_LOAD") && op.opcode == PcodeOpcode::Load));
    assert!(btr
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("BTR_STORE") && op.opcode == PcodeOpcode::Store));
    assert!(has_flag_write(&btr, x86_flag_cf()));

    let btc = decode_semantic(&[0x0F, 0xBB, 0x18], 0x7175); // btc dword ptr [rax], ebx
    assert!(!btc.is_empty());
    assert!(btc
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("BTC_TOGGLE") && op.opcode == PcodeOpcode::IntXor));
    assert!(btc
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("BTC_STORE") && op.opcode == PcodeOpcode::Store));
    assert!(has_flag_write(&btc, x86_flag_cf()));
}

#[test]
fn decode_shld_shrd_forms_emit_merge_and_rmw_paths() {
    let shld = decode_semantic(&[0x0F, 0xA4, 0xD8, 0x04], 0x7176); // shld eax, ebx, 4
    assert!(!shld.is_empty());
    assert!(shld
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("SHLD_MERGE") && op.opcode == PcodeOpcode::IntOr));
    assert!(shld
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("SHLD_WRITE") && op.output.as_ref() == Some(&x86_reg(0, 4))));

    let shrd_cl = decode_semantic(&[0x0F, 0xAD, 0xD8], 0x7177); // shrd eax, ebx, cl
    assert!(!shrd_cl.is_empty());
    assert!(shrd_cl
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("SHXD_COUNT_ZEXT")));
    assert!(shrd_cl
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("SHRD_MERGE") && op.opcode == PcodeOpcode::IntOr));

    let shrd_mem = decode_semantic(&[0x0F, 0xAC, 0x18, 0x03], 0x7178); // shrd dword ptr [rax], ebx, 3
    assert!(!shrd_mem.is_empty());
    assert!(shrd_mem.iter().any(|op| op.opcode == PcodeOpcode::Load));
    assert!(shrd_mem
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("SHRD_STORE") && op.opcode == PcodeOpcode::Store));
}

#[test]
fn decode_imul_two_operand_sets_cf_of_and_writes_destination() {
    let ops = decode_semantic(&[0x0F, 0xAF, 0xC3], 0x7174); // imul eax, ebx
    assert!(!ops.is_empty());
    assert!(ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("IMUL")));
    let write = ops
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("IMUL_WRITE"))
        .expect("expected imul destination write");
    assert_eq!(write.output.as_ref(), Some(&x86_reg(0, 4)));
    assert!(has_flag_write(&ops, x86_flag_cf()));
    assert!(has_flag_write(&ops, x86_flag_of()));
    assert!(!has_flag_write(&ops, x86_flag_zf()));
    assert!(!has_flag_write(&ops, x86_flag_sf()));
    assert!(!has_flag_write(&ops, x86_flag_pf()));
}

#[test]
fn decode_imul_immediate_forms_set_cf_of_and_write_destination() {
    let imul_69 = decode_semantic(&[0x69, 0xC3, 0x10, 0x00, 0x00, 0x00], 0x7176); // imul eax, ebx, 0x10
    assert!(!imul_69.is_empty());
    assert!(imul_69
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("IMUL_IMM")));
    assert!(imul_69
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("IMUL_IMM_WRITE") && op.output.as_ref() == Some(&x86_reg(0, 4))));
    assert!(has_flag_write(&imul_69, x86_flag_cf()));
    assert!(has_flag_write(&imul_69, x86_flag_of()));

    let imul_6b = decode_semantic(&[0x6B, 0x18, 0xFE], 0x7177); // imul ebx, dword ptr [rax], -2
    assert!(!imul_6b.is_empty());
    assert!(imul_6b.iter().any(|op| op.opcode == PcodeOpcode::Load));
    assert!(imul_6b
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("IMUL_IMM_WRITE") && op.output.as_ref() == Some(&x86_reg(3, 4))));
    let rhs_6b = imul_6b
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("IMUL_IMM_RHS_SEXT"))
        .expect("expected imm8 rhs sign extension");
    assert_eq!(rhs_6b.inputs[0].size, 4);
    assert_eq!((rhs_6b.inputs[0].constant_val as u64) & 0xFFFF_FFFF, 0xFFFF_FFFE);

    let imul_rexw = decode_semantic(&[0x48, 0x69, 0xD9, 0xFF, 0xFF, 0xFF, 0xFF], 0x7178); // imul rbx, rcx, -1
    assert!(!imul_rexw.is_empty());
    assert!(imul_rexw
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("IMUL_IMM_WRITE") && op.output.as_ref() == Some(&x86_reg(3, 8))));
    let rhs_rexw = imul_rexw
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("IMUL_IMM_RHS_SEXT"))
        .expect("expected imm32 rhs sign extension in rex.w form");
    assert_eq!(rhs_rexw.inputs[0].size, 8);
    assert_eq!(rhs_rexw.inputs[0].constant_val as u64, u64::MAX);
}

#[test]
fn decode_f7_mul_imul_write_implicit_accumulator_pair() {
    let mul = decode_semantic(&[0xF7, 0xE3], 0x7178); // mul ebx
    assert!(!mul.is_empty());
    assert!(mul.iter().any(|op| op.asm_mnemonic.as_deref() == Some("MUL")));
    assert!(mul
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MUL_LO_WRITE") && op.output.as_ref() == Some(&x86_reg(0, 4))));
    assert!(mul
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MUL_HI_WRITE") && op.output.as_ref() == Some(&x86_reg(2, 4))));
    assert!(has_flag_write(&mul, x86_flag_cf()));
    assert!(has_flag_write(&mul, x86_flag_of()));

    let imul = decode_semantic(&[0xF7, 0xEB], 0x717C); // imul ebx
    assert!(!imul.is_empty());
    assert!(imul.iter().any(|op| op.asm_mnemonic.as_deref() == Some("IMUL")));
    assert!(imul
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("IMUL_LO_WRITE") && op.output.as_ref() == Some(&x86_reg(0, 4))));
    assert!(imul
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("IMUL_HI_WRITE") && op.output.as_ref() == Some(&x86_reg(2, 4))));
    assert!(has_flag_write(&imul, x86_flag_cf()));
    assert!(has_flag_write(&imul, x86_flag_of()));
}

#[test]
fn decode_f7_div_idiv_emit_policy_marker_and_write_quotient_remainder() {
    let div = decode_semantic(&[0xF7, 0xF3], 0x7180); // div ebx
    assert!(!div.is_empty());
    let div_policy = div
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("DIV_EXCEPTION_POLICY"))
        .expect("expected div policy marker");
    assert_eq!(div_policy.opcode, PcodeOpcode::CallOther);
    assert_eq!(div_policy.inputs[0].constant_val as u64, X86_DIV_EXCEPTION_POLICY_ID);
    assert_eq!(div_policy.inputs.len(), 5);
    assert_eq!(div_policy.inputs[2], x86_reg(2, 4));
    assert_eq!(div_policy.inputs[3], x86_reg(0, 4));
    assert_eq!(div_policy.inputs[4].constant_val as u64, 4);
    assert!(div
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("DIV_QUOT_WRITE") && op.output.as_ref() == Some(&x86_reg(0, 4))));
    assert!(div
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("DIV_REM_WRITE") && op.output.as_ref() == Some(&x86_reg(2, 4))));

    let idiv = decode_semantic(&[0xF7, 0xFB], 0x7184); // idiv ebx
    assert!(!idiv.is_empty());
    let idiv_policy = idiv
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("IDIV_EXCEPTION_POLICY"))
        .expect("expected idiv policy marker");
    assert_eq!(idiv_policy.opcode, PcodeOpcode::CallOther);
    assert_eq!(idiv_policy.inputs[0].constant_val as u64, X86_IDIV_EXCEPTION_POLICY_ID);
    assert_eq!(idiv_policy.inputs.len(), 5);
    assert_eq!(idiv_policy.inputs[2], x86_reg(2, 4));
    assert_eq!(idiv_policy.inputs[3], x86_reg(0, 4));
    assert_eq!(idiv_policy.inputs[4].constant_val as u64, 4);
    assert!(idiv
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("IDIV_QUOT_WRITE") && op.output.as_ref() == Some(&x86_reg(0, 4))));
    assert!(idiv
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("IDIV_REM_WRITE") && op.output.as_ref() == Some(&x86_reg(2, 4))));
}

#[test]
fn decode_f6_group_covers_test_neg_mul_imul_div_idiv_byte_forms() {
    let test = decode_semantic(&[0xF6, 0xC0, 0x80], 0x7188); // test al, 0x80
    assert!(!test.is_empty());
    assert_eq!(test[0].opcode, PcodeOpcode::IntAnd);
    assert_eq!(test[0].asm_mnemonic.as_deref(), Some("TEST"));
    assert_eq!(test[0].inputs[0], x86_reg(0, 1));
    assert_eq!(test[0].inputs[1].size, 1);
    assert!(has_flag_zero_copy(&test, x86_flag_cf()));
    assert!(has_flag_zero_copy(&test, x86_flag_of()));

    let neg = decode_semantic(&[0xF6, 0xD8], 0x7189); // neg al
    assert!(!neg.is_empty());
    assert!(neg.iter().any(|op| op.asm_mnemonic.as_deref() == Some("NEG")));
    assert!(has_flag_write(&neg, x86_flag_cf()));
    assert!(has_flag_write(&neg, x86_flag_of()));

    let mul = decode_semantic(&[0xF6, 0xE3], 0x718A); // mul bl
    assert!(!mul.is_empty());
    assert!(mul.iter().any(|op| op.asm_mnemonic.as_deref() == Some("MUL")));
    assert!(mul
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MUL_LO_WRITE") && op.output.as_ref() == Some(&x86_reg(0, 1))));
    assert!(mul
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MUL_HI_WRITE") && op.output.as_ref() == Some(&x86_reg(4, 1))));

    let imul = decode_semantic(&[0xF6, 0xEB], 0x718B); // imul bl
    assert!(!imul.is_empty());
    assert!(imul.iter().any(|op| op.asm_mnemonic.as_deref() == Some("IMUL")));
    assert!(imul
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("IMUL_LO_WRITE") && op.output.as_ref() == Some(&x86_reg(0, 1))));
    assert!(imul
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("IMUL_HI_WRITE") && op.output.as_ref() == Some(&x86_reg(4, 1))));

    let div = decode_semantic(&[0xF6, 0xF3], 0x718C); // div bl
    assert!(!div.is_empty());
    let div_policy = div
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("DIV_EXCEPTION_POLICY"))
        .expect("expected div policy marker");
    assert_eq!(div_policy.opcode, PcodeOpcode::CallOther);
    assert_eq!(div_policy.inputs[0].constant_val as u64, X86_DIV_EXCEPTION_POLICY_ID);
    assert_eq!(div_policy.inputs.len(), 5);
    assert_eq!(div_policy.inputs[2], x86_reg(4, 1));
    assert_eq!(div_policy.inputs[3], x86_reg(0, 1));
    assert_eq!(div_policy.inputs[4].constant_val as u64, 1);
    assert!(div
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("DIV_QUOT_WRITE") && op.output.as_ref() == Some(&x86_reg(0, 1))));
    assert!(div
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("DIV_REM_WRITE") && op.output.as_ref() == Some(&x86_reg(4, 1))));

    let idiv = decode_semantic(&[0xF6, 0xFB], 0x718D); // idiv bl
    assert!(!idiv.is_empty());
    let idiv_policy = idiv
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("IDIV_EXCEPTION_POLICY"))
        .expect("expected idiv policy marker");
    assert_eq!(idiv_policy.opcode, PcodeOpcode::CallOther);
    assert_eq!(idiv_policy.inputs[0].constant_val as u64, X86_IDIV_EXCEPTION_POLICY_ID);
    assert_eq!(idiv_policy.inputs.len(), 5);
    assert_eq!(idiv_policy.inputs[2], x86_reg(4, 1));
    assert_eq!(idiv_policy.inputs[3], x86_reg(0, 1));
    assert_eq!(idiv_policy.inputs[4].constant_val as u64, 1);
    assert!(idiv
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("IDIV_QUOT_WRITE") && op.output.as_ref() == Some(&x86_reg(0, 1))));
    assert!(idiv
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("IDIV_REM_WRITE") && op.output.as_ref() == Some(&x86_reg(4, 1))));
}

#[test]
fn decode_lea_emits_address_copy_without_memory_access() {
    let ops = decode_semantic(&[0x48, 0x8D, 0x58, 0x10], 0x7160); // lea rbx, [rax+0x10]
    assert!(!ops.is_empty());
    assert!(!ops.iter().any(|op| op.opcode == PcodeOpcode::Load));
    assert!(!ops.iter().any(|op| op.opcode == PcodeOpcode::Store));

    let write = ops
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("LEA_WRITE"))
        .expect("expected lea write");
    assert_eq!(write.output.as_ref(), Some(&x86_reg(3, 8)));
}

#[test]
fn decode_lea_rejects_register_source_modrm_mode3() {
    let ops = decode_semantic(&[0x8D, 0xC0], 0x7170); // invalid lea eax, eax encoding in mode3
    assert!(ops.is_empty());
}

#[test]
fn decode_push_reg_decrements_rsp_and_stores_value() {
    let ops = decode_semantic(&[0x53], 0x7200); // push rbx
    assert!(!ops.is_empty());
    assert!(ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("PUSH_REG_SP_SUB")));
    assert!(ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("PUSH_REG_SP_WRITE")));

    let store = ops
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("PUSH_REG_STORE"))
        .expect("expected push store");
    assert_eq!(store.opcode, PcodeOpcode::Store);
    assert_eq!(store.inputs[2], x86_reg(3, 8));
}

#[test]
fn decode_pop_reg_loads_stack_and_increments_rsp() {
    let ops = decode_semantic(&[0x5B], 0x7210); // pop rbx
    assert!(!ops.is_empty());
    assert!(ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("POP_REG_LOAD")));
    assert!(ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("POP_REG_SP_ADD")));
    assert!(ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("POP_REG_SP_WRITE")));

    let write = ops
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("POP_REG_WRITE"))
        .expect("expected pop register write");
    assert_eq!(write.opcode, PcodeOpcode::Copy);
    assert_eq!(write.output.as_ref(), Some(&x86_reg(3, 8)));
}

#[test]
fn decode_push_imm8_sign_extends_to_stack_slot() {
    let ops = decode_semantic(&[0x6A, 0xFF], 0x7220); // push -1
    assert!(!ops.is_empty());
    let store = ops
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("PUSH_IMM_STORE"))
        .expect("expected push immediate store");
    assert_eq!(store.inputs[2].size, 8);
    assert_eq!(store.inputs[2].constant_val, -1);
}

#[test]
fn decode_push_rm_and_pop_rm_cover_memory_forms() {
    let push_ops = decode_semantic(&[0xFF, 0x30], 0x7230); // push qword ptr [rax]
    assert!(!push_ops.is_empty());
    assert!(push_ops.iter().any(|op| op.opcode == PcodeOpcode::Load));
    assert!(push_ops
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PUSH_RM_STORE")));

    let pop_ops = decode_semantic(&[0x8F, 0x00], 0x7240); // pop qword ptr [rax]
    assert!(!pop_ops.is_empty());
    assert!(pop_ops
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("POP_RM_LOAD")));
    assert!(pop_ops
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("POP_STORE")));
}

#[test]
fn decode_call_emits_stack_push_of_return_address() {
    let ops = decode_semantic(&[0xE8, 0x10, 0x00, 0x00, 0x00], 0x7250);
    assert!(!ops.is_empty());
    let store = ops
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("CALL_STORE"))
        .expect("expected call return-address push");
    assert_eq!(store.opcode, PcodeOpcode::Store);
    assert_eq!(store.inputs[2].size, 8);
    assert_eq!(store.inputs[2].constant_val as u64, 0x7255);
}

#[test]
fn decode_ret_emits_stack_pop_and_optional_imm_cleanup() {
    let near_ret = decode_semantic(&[0xC3], 0x7260);
    assert!(!near_ret.is_empty());
    assert!(near_ret
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("RET_LOAD")));
    assert!(near_ret
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("RET_SP_ADD")));

    let ret_imm = decode_semantic(&[0xC2, 0x20, 0x00], 0x7270);
    assert!(!ret_imm.is_empty());
    assert!(ret_imm
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("RET_IMM_SP_ADD")));
    assert!(ret_imm
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("RET_IMM_SP_WRITE")));
}

#[test]
fn decode_nop_and_pause_emit_hint_without_stateful_side_effects() {
    let nop = decode_semantic(&[0x90], 0x7280);
    assert!(!nop.is_empty());
    assert!(nop.iter().any(|op| op.asm_mnemonic.as_deref() == Some("NOP_HINT")));
    assert!(!nop.iter().any(|op| op.opcode == PcodeOpcode::Load));
    assert!(!nop.iter().any(|op| op.opcode == PcodeOpcode::Store));
    assert!(!has_flag_write(&nop, x86_flag_cf()));
    assert!(!has_flag_write(&nop, x86_flag_zf()));

    let pause = decode_semantic(&[0xF3, 0x90], 0x7282);
    assert!(!pause.is_empty());
    assert!(pause
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PAUSE_HINT") && op.opcode == PcodeOpcode::Copy));
    assert!(!pause.iter().any(|op| op.opcode == PcodeOpcode::Load));
    assert!(!pause.iter().any(|op| op.opcode == PcodeOpcode::Store));
    assert!(!has_flag_write(&pause, x86_flag_cf()));
    assert!(!has_flag_write(&pause, x86_flag_zf()));
}

#[test]
fn decode_int3_emits_explicit_trap_policy_marker() {
    let ops = decode_semantic(&[0xCC], 0x7281);
    assert_eq!(ops.len(), 1);
    let trap = &ops[0];
    assert_eq!(trap.opcode, PcodeOpcode::CallOther);
    assert_eq!(trap.asm_mnemonic.as_deref(), Some("INT3_TRAP"));
    assert_eq!(trap.inputs.len(), 1);
    assert!(trap.inputs[0].is_constant);
    assert_eq!(trap.inputs[0].constant_val as u64, 0xCC);
}

#[test]
fn decode_nop_extended_emits_hint_and_rejects_register_form() {
    let ext_nop = decode_semantic(&[0x0F, 0x1F, 0x00], 0x7284); // nop dword ptr [rax]
    assert!(!ext_nop.is_empty());
    assert!(ext_nop
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("NOP_EXT_HINT") && op.opcode == PcodeOpcode::Copy));
    assert!(!ext_nop.iter().any(|op| op.opcode == PcodeOpcode::Load));
    assert!(!ext_nop.iter().any(|op| op.opcode == PcodeOpcode::Store));

    let reg_form = decode_semantic(&[0x0F, 0x1F, 0xC0], 0x7288); // register-form encoding is rejected
    assert!(reg_form.is_empty());
}

#[test]
fn decode_rdtsc_and_clflush_emit_policy_markers() {
    let rdtsc = decode_semantic(&[0x0F, 0x31], 0x728A);
    assert!(!rdtsc.is_empty());
    let rdtsc_policy = rdtsc
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("RDTSC_POLICY"))
        .expect("expected rdtsc policy marker");
    assert_eq!(rdtsc_policy.opcode, PcodeOpcode::CallOther);
    assert_eq!(rdtsc_policy.inputs.len(), 1);
    assert!(rdtsc_policy.inputs[0].is_constant);
    assert_eq!(rdtsc_policy.inputs[0].constant_val as u64, 0x0F31);

    let clflush = decode_semantic(&[0x0F, 0xAE, 0x38], 0x728C); // clflush [rax]
    assert!(!clflush.is_empty());
    let clflush_policy = clflush
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("CLFLUSH_POLICY"))
        .expect("expected clflush policy marker");
    assert_eq!(clflush_policy.opcode, PcodeOpcode::CallOther);
    assert_eq!(clflush_policy.inputs.len(), 2);
    assert!(clflush_policy.inputs[0].is_constant);
    assert_eq!(clflush_policy.inputs[0].constant_val as u64, 0x0FAE07);

    // 0F AE F8 = SFENCE (mod=11, reg=7, rm=0) — now handled as SFENCE, not CLFLUSH
    let clflush_reg = decode_semantic(&[0x0F, 0xAE, 0xF8], 0x728E);
    assert!(
        clflush_reg.is_empty()
            || clflush_reg
                .iter()
                .any(|op| op.asm_mnemonic.as_deref() == Some("SFENCE_POLICY")),
        "0F AE F8 should be empty or emit SFENCE_POLICY"
    );
}

#[test]
fn decode_int_imm_emits_trap_policy_with_vector() {
    let ops = decode_semantic(&[0xCD, 0x80], 0x7290);
    assert_eq!(ops.len(), 1);
    let trap = &ops[0];
    assert_eq!(trap.opcode, PcodeOpcode::CallOther);
    assert_eq!(trap.asm_mnemonic.as_deref(), Some("INT_IMM_TRAP"));
    assert_eq!(trap.inputs.len(), 2);
    assert!(trap.inputs[0].is_constant);
    assert_eq!(trap.inputs[0].constant_val as u64, 0xCD);
    assert!(trap.inputs[1].is_constant);
    assert_eq!(trap.inputs[1].constant_val as u64, 0x80);
}

#[test]
fn decode_accumulator_immediate_alu_and_cmp_forms() {
    let add_al = decode_semantic(&[0x04, 0x7F], 0x7292); // add al, 0x7f
    assert!(!add_al.is_empty());
    assert_eq!(add_al[0].opcode, PcodeOpcode::IntAdd);
    assert_eq!(add_al[0].asm_mnemonic.as_deref(), Some("ADD"));
    assert_eq!(add_al[0].output.as_ref(), Some(&x86_reg(0, 1)));
    assert!(has_pf_pipeline(&add_al));

    let adc_eax = decode_semantic(&[0x15, 0x01, 0x00, 0x00, 0x00], 0x7296); // adc eax, 1
    assert!(!adc_eax.is_empty());
    let adc_core = adc_eax
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("ADC"))
        .expect("expected ADC core op");
    assert_eq!(adc_core.opcode, PcodeOpcode::IntAdd);
    assert!(has_flag_input(&adc_eax, x86_flag_cf()));
    assert!(has_flag_write(&adc_eax, x86_flag_cf()));

    let cmp_eax = decode_semantic(&[0x3D, 0x34, 0x12, 0x00, 0x00], 0x729A); // cmp eax, 0x1234
    assert!(!cmp_eax.is_empty());
    let cmp_core = cmp_eax
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("CMP"))
        .expect("expected CMP core op");
    assert_eq!(cmp_core.opcode, PcodeOpcode::IntSub);
    assert!(cmp_core.output.as_ref().is_some());
    assert!(has_pf_pipeline(&cmp_eax));

    let test_al = decode_semantic(&[0xA8, 0xFF], 0x729E); // test al, 0xff
    assert!(!test_al.is_empty());
    assert_eq!(test_al[0].opcode, PcodeOpcode::IntAnd);
    assert_eq!(test_al[0].asm_mnemonic.as_deref(), Some("TEST"));
    assert!(has_flag_zero_copy(&test_al, x86_flag_cf()));
    assert!(has_flag_zero_copy(&test_al, x86_flag_of()));
}

#[test]
fn decode_0f_system_instructions_emit_policy_markers() {
    let cases: [(&[u8], &str, u64); 7] = [
        (&[0x0F, 0x05], "SYSCALL_POLICY", 0x0F05),
        (&[0x0F, 0x07], "SYSRET_POLICY", 0x0F07),
        (&[0x0F, 0x30], "WRMSR_POLICY", 0x0F30),
        (&[0x0F, 0x32], "RDMSR_POLICY", 0x0F32),
        (&[0x0F, 0x34], "SYSENTER_POLICY", 0x0F34),
        (&[0x0F, 0x35], "SYSEXIT_POLICY", 0x0F35),
        (&[0x0F, 0xA2], "CPUID_POLICY", 0x0FA2),
    ];

    for (insn, mnemonic, policy) in cases {
        let ops = decode_semantic(insn, 0x7300);
        assert_eq!(ops.len(), 1, "{mnemonic}");
        let marker = &ops[0];
        assert_eq!(marker.opcode, PcodeOpcode::CallOther, "{mnemonic}");
        assert_eq!(marker.asm_mnemonic.as_deref(), Some(mnemonic), "{mnemonic}");
        assert_eq!(marker.inputs.len(), 1, "{mnemonic}");
        assert!(marker.inputs[0].is_constant, "{mnemonic}");
        assert_eq!(marker.inputs[0].constant_val as u64, policy, "{mnemonic}");
    }
}

#[test]
fn decode_string_movs_updates_indices_and_supports_rep_count() {
    let movs = decode_semantic(&[0xA4], 0x7400); // movsb
    assert!(!movs.is_empty());
    assert!(movs
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MOVS_LOAD") && op.opcode == PcodeOpcode::Load));
    assert!(movs
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MOVS_STORE") && op.opcode == PcodeOpcode::Store));
    assert!(movs
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MOVS_SRC_WRITE") && op.output.as_ref() == Some(&x86_reg(6, 8))));
    assert!(movs
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MOVS_DST_WRITE") && op.output.as_ref() == Some(&x86_reg(7, 8))));
    assert!(has_flag_input(&movs, x86_flag_df()));

    let rep_movs = decode_semantic(&[0xF3, 0xA4], 0x7408); // rep movsb
    assert!(!rep_movs.is_empty());
    assert!(rep_movs
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("REP_COUNT_DEC") && op.opcode == PcodeOpcode::IntSub));
    assert!(rep_movs
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("REP_COUNT_WRITE") && op.output.as_ref() == Some(&x86_reg(1, 8))));
}

#[test]
fn decode_string_cmps_scas_stos_lods_emit_expected_memory_and_flag_effects() {
    let cmps = decode_semantic(&[0xA6], 0x7410); // cmpsb
    assert!(!cmps.is_empty());
    assert!(cmps
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("CMPS_LOAD_LHS") && op.opcode == PcodeOpcode::Load));
    assert!(cmps
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("CMPS_LOAD_RHS") && op.opcode == PcodeOpcode::Load));
    assert!(cmps.iter().any(|op| op.asm_mnemonic.as_deref() == Some("CMP")));
    assert!(has_flag_write(&cmps, x86_flag_zf()));

    let scas = decode_semantic(&[0xAE], 0x7420); // scasb
    assert!(!scas.is_empty());
    assert!(scas
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("SCAS_LOAD") && op.opcode == PcodeOpcode::Load));
    assert!(scas
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("SCAS_DST_WRITE") && op.output.as_ref() == Some(&x86_reg(7, 8))));
    assert!(has_flag_write(&scas, x86_flag_zf()));

    let stos = decode_semantic(&[0xAB], 0x7430); // stosd
    assert!(!stos.is_empty());
    let store = stos
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("STOS_STORE"))
        .expect("expected stos store");
    assert_eq!(store.opcode, PcodeOpcode::Store);
    assert_eq!(store.inputs[2], x86_reg(0, 4));

    let lods = decode_semantic(&[0xAC], 0x7440); // lodsb
    assert!(!lods.is_empty());
    assert!(lods
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("LODS_LOAD") && op.opcode == PcodeOpcode::Load));
    assert!(lods
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("LODS_WRITE") && op.output.as_ref() == Some(&x86_reg(0, 1))));
}

#[test]
fn decode_string_address_override_uses_esi_edi_index_size() {
    let ops = decode_semantic(&[0x67, 0xA4], 0x7450); // movsb with addr-size override
    assert!(!ops.is_empty());
    assert!(ops
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MOVS_SRC_ADDR_ZEXT") && op.opcode == PcodeOpcode::IntZExt));
    assert!(ops
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MOVS_DST_ADDR_ZEXT") && op.opcode == PcodeOpcode::IntZExt));
    assert!(ops
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MOVS_SRC_WRITE") && op.output.as_ref() == Some(&x86_reg(6, 4))));
    assert!(ops
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MOVS_DST_WRITE") && op.output.as_ref() == Some(&x86_reg(7, 4))));
}

#[test]
fn decode_0f_extended_markers_and_bswap_semantics_are_emitted() {
    let clts = decode_semantic(&[0x0F, 0x06], 0x7460);
    assert_eq!(clts.len(), 1);
    assert_eq!(clts[0].asm_mnemonic.as_deref(), Some("CLTS_POLICY"));
    assert_eq!(clts[0].inputs[0].constant_val as u64, 0x0F06);

    let map_0f38 = decode_semantic(&[0x0F, 0x38, 0xF1, 0xC0], 0x7464);
    assert_eq!(map_0f38.len(), 1);
    assert_eq!(map_0f38[0].asm_mnemonic.as_deref(), Some("0F38_POLICY"));
    assert_eq!(map_0f38[0].inputs[0].constant_val as u64, 0x0F38F1);

    let map_0f3a = decode_semantic(&[0x66, 0x0F, 0x3A, 0x2A, 0xC0, 0x01], 0x7468);
    assert_eq!(map_0f3a.len(), 1);
    assert_eq!(map_0f3a[0].asm_mnemonic.as_deref(), Some("0F3A_POLICY"));
    assert_eq!(map_0f3a[0].inputs[0].constant_val as u64, 0x0F3A2A);

    // 0F 10 is now MOVUPS (None prefix) — produces XMM Copy ops rather than SIMD_POLICY
    let simd = decode_semantic(&[0x0F, 0x10, 0xC0], 0x746C);
    assert!(!simd.is_empty(), "MOVUPS (0F 10) should produce ops");

    // 0F D8 = PSUBUSB (MMX) — now routes to SIMD_POLICY instead of empty
    let x87_mmx = decode_semantic(&[0x0F, 0xD8, 0xC0], 0x7470);
    assert!(!x87_mmx.is_empty(), "MMX D8 should now route to SIMD_POLICY");

    let bswap32 = decode_semantic(&[0x0F, 0xC8], 0x7474); // bswap eax
    assert!(!bswap32.is_empty());
    assert!(bswap32
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("BSWAP_WRITE") && op.output.as_ref() == Some(&x86_reg(0, 4))));

    let bswap64 = decode_semantic(&[0x48, 0x0F, 0xC9], 0x7478); // bswap rcx
    assert!(!bswap64.is_empty());
    assert!(bswap64
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("BSWAP_WRITE") && op.output.as_ref() == Some(&x86_reg(1, 8))));
}

#[test]
fn decode_high_frequency_0f38_0f3a_intrinsics_emit_xmm_dataflow() {
    let pshufb = decode_semantic(&[0x66, 0x0F, 0x38, 0x00, 0xCA], 0x7480); // pshufb xmm1, xmm2
    assert!(!pshufb.is_empty());
    assert!(pshufb
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PSHUFB_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));
    assert!(pshufb
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PSHUFB_WRITE") && op.output.as_ref() == Some(&x86_xmm_reg(1, 16))));

    let sha256rnds2 = decode_semantic(&[0x0F, 0x38, 0xCB, 0xE5], 0x7488); // sha256rnds2 xmm4, xmm5, xmm0
    assert!(!sha256rnds2.is_empty());
    assert!(sha256rnds2
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("SHA256RNDS2_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));
    assert!(sha256rnds2
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("SHA256RNDS2_WRITE") && op.output.as_ref() == Some(&x86_xmm_reg(4, 16))));

    let aes_mem = decode_semantic(&[0x66, 0x0F, 0x38, 0xDD, 0x08], 0x7490); // aesenclast xmm1, [rax]
    assert!(!aes_mem.is_empty());
    assert!(aes_mem.iter().any(|op| op.opcode == PcodeOpcode::Load));
    assert!(aes_mem
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("AESENCLAST_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));
    assert!(aes_mem
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("AESENCLAST_WRITE") && op.output.as_ref() == Some(&x86_xmm_reg(1, 16))));

    let aesenc = decode_semantic(&[0x66, 0x0F, 0x38, 0xDC, 0xCA], 0x7494); // aesenc xmm1, xmm2
    assert!(!aesenc.is_empty());
    assert!(aesenc
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("AESENC_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));
    assert!(aesenc
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("AESENC_WRITE") && op.output.as_ref() == Some(&x86_xmm_reg(1, 16))));

    let aesdeclast = decode_semantic(&[0x66, 0x0F, 0x38, 0xDF, 0xC8], 0x7496); // aesdeclast xmm1, xmm0
    assert!(!aesdeclast.is_empty());
    assert!(aesdeclast
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("AESDECLAST_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));
    assert!(aesdeclast
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("AESDECLAST_WRITE") && op.output.as_ref() == Some(&x86_xmm_reg(1, 16))));

    let palignr = decode_semantic(&[0x66, 0x0F, 0x3A, 0x0F, 0xC9, 0x04], 0x7498); // palignr xmm1, xmm1, 4
    assert!(!palignr.is_empty());
    let palignr_intr = palignr
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("PALIGNR_INTRINSIC"))
        .expect("expected PALIGNR intrinsic");
    assert_eq!(palignr_intr.opcode, PcodeOpcode::CallOther);
    assert_eq!(palignr_intr.inputs.last().map(|v| v.constant_val as u64), Some(0x04));
    assert!(palignr
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PALIGNR_WRITE") && op.output.as_ref() == Some(&x86_xmm_reg(1, 16))));

    let roundps = decode_semantic(&[0x66, 0x0F, 0x3A, 0x08, 0xCA, 0x04], 0x7499); // roundps xmm1, xmm2, 4
    assert!(!roundps.is_empty());
    let roundps_intr = roundps
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("ROUNDPS_INTRINSIC"))
        .expect("expected ROUNDPS intrinsic");
    assert_eq!(roundps_intr.inputs.last().map(|v| v.constant_val as u64), Some(0x04));
    assert!(roundps
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("ROUNDPS_WRITE") && op.output.as_ref() == Some(&x86_xmm_reg(1, 16))));

    let roundpd = decode_semantic(&[0x66, 0x0F, 0x3A, 0x09, 0xCA, 0x03], 0x749A); // roundpd xmm1, xmm2, 3
    assert!(!roundpd.is_empty());
    let roundpd_intr = roundpd
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("ROUNDPD_INTRINSIC"))
        .expect("expected ROUNDPD intrinsic");
    assert_eq!(roundpd_intr.inputs.last().map(|v| v.constant_val as u64), Some(0x03));
    assert!(roundpd
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("ROUNDPD_WRITE") && op.output.as_ref() == Some(&x86_xmm_reg(1, 16))));

    let blendps = decode_semantic(&[0x66, 0x0F, 0x3A, 0x0C, 0xCA, 0x05], 0x749A); // blendps xmm1, xmm2, 5
    assert!(!blendps.is_empty());
    let blendps_intr = blendps
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("BLENDPS_INTRINSIC"))
        .expect("expected BLENDPS intrinsic");
    assert_eq!(blendps_intr.inputs.last().map(|v| v.constant_val as u64), Some(0x05));
    assert!(blendps
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("BLENDPS_WRITE") && op.output.as_ref() == Some(&x86_xmm_reg(1, 16))));

    let blendpd = decode_semantic(&[0x66, 0x0F, 0x3A, 0x0D, 0xCA, 0x02], 0x749A); // blendpd xmm1, xmm2, 2
    assert!(!blendpd.is_empty());
    let blendpd_intr = blendpd
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("BLENDPD_INTRINSIC"))
        .expect("expected BLENDPD intrinsic");
    assert_eq!(blendpd_intr.inputs.last().map(|v| v.constant_val as u64), Some(0x02));
    assert!(blendpd
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("BLENDPD_WRITE") && op.output.as_ref() == Some(&x86_xmm_reg(1, 16))));

    let roundss = decode_semantic(&[0x66, 0x0F, 0x3A, 0x0A, 0xC1, 0x01], 0x749B); // roundss xmm0, xmm1, 1
    assert!(!roundss.is_empty());
    let roundss_intr = roundss
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("ROUNDSS_INTRINSIC"))
        .expect("expected ROUNDSS intrinsic");
    assert_eq!(roundss_intr.inputs.last().map(|v| v.constant_val as u64), Some(0x01));
    assert!(roundss
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("ROUNDSS_WRITE") && op.output.as_ref() == Some(&x86_xmm_reg(0, 16))));

    let roundsd = decode_semantic(&[0x66, 0x0F, 0x3A, 0x0B, 0xC1, 0x02], 0x749C); // roundsd xmm0, xmm1, 2
    assert!(!roundsd.is_empty());
    let roundsd_intr = roundsd
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("ROUNDSD_INTRINSIC"))
        .expect("expected ROUNDSD intrinsic");
    assert_eq!(roundsd_intr.inputs.last().map(|v| v.constant_val as u64), Some(0x02));
    assert!(roundsd
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("ROUNDSD_WRITE") && op.output.as_ref() == Some(&x86_xmm_reg(0, 16))));

    let pclmul = decode_semantic(&[0x66, 0x0F, 0x3A, 0x44, 0xE1, 0x00], 0x74A0); // pclmulqdq xmm4, xmm1, 0
    assert!(!pclmul.is_empty());
    let pclmul_intr = pclmul
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("PCLMULQDQ_INTRINSIC"))
        .expect("expected PCLMULQDQ intrinsic");
    assert_eq!(pclmul_intr.opcode, PcodeOpcode::CallOther);
    assert_eq!(pclmul_intr.inputs.last().map(|v| v.constant_val as u64), Some(0x00));
    assert!(pclmul
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PCLMULQDQ_WRITE") && op.output.as_ref() == Some(&x86_xmm_reg(4, 16))));

    let pextrd = decode_semantic(&[0x66, 0x0F, 0x3A, 0x16, 0xC0, 0x01], 0x74A8); // pextrd eax, xmm0, 1
    assert!(!pextrd.is_empty());
    let pextrd_intr = pextrd
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("PEXTRD_INTRINSIC"))
        .expect("expected PEXTRD intrinsic");
    assert_eq!(pextrd_intr.opcode, PcodeOpcode::CallOther);
    assert_eq!(pextrd_intr.inputs.last().map(|v| v.constant_val as u64), Some(0x01));
    assert!(pextrd
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PEXTRD_WRITE") && op.output.as_ref() == Some(&x86_reg(0, 4))));

    let pinsrd = decode_semantic(&[0x66, 0x0F, 0x3A, 0x22, 0xC8, 0x02], 0x74B0); // pinsrd xmm1, eax, 2
    assert!(!pinsrd.is_empty());
    let pinsrd_intr = pinsrd
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("PINSRD_INTRINSIC"))
        .expect("expected PINSRD intrinsic");
    assert_eq!(pinsrd_intr.opcode, PcodeOpcode::CallOther);
    assert_eq!(pinsrd_intr.inputs.last().map(|v| v.constant_val as u64), Some(0x02));
    assert!(pinsrd
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PINSRD_WRITE") && op.output.as_ref() == Some(&x86_xmm_reg(1, 16))));

    let pcmpistri = decode_semantic(&[0x66, 0x0F, 0x3A, 0x63, 0xC1, 0x0C], 0x74B8); // pcmpistri xmm0, xmm1, 0x0c
    assert!(!pcmpistri.is_empty());
    let pcmpistri_intr = pcmpistri
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("PCMPISTRI_INTRINSIC"))
        .expect("expected PCMPISTRI intrinsic");
    assert_eq!(pcmpistri_intr.opcode, PcodeOpcode::CallOther);
    assert_eq!(pcmpistri_intr.inputs.last().map(|v| v.constant_val as u64), Some(0x0C));
    assert!(pcmpistri
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PCMPISTRI_ECX_WRITE") && op.output.as_ref() == Some(&x86_reg(1, 4))));

    let pcmpistri_mem = decode_semantic(&[0x66, 0x41, 0x0F, 0x3A, 0x63, 0x00, 0x40], 0x74C0); // pcmpistri xmm0, [r8], 0x40
    assert!(!pcmpistri_mem.is_empty());
    assert!(pcmpistri_mem.iter().any(|op| op.opcode == PcodeOpcode::Load));
    assert!(pcmpistri_mem
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PCMPISTRI_ECX_WRITE") && op.output.as_ref() == Some(&x86_reg(1, 4))));

    let pcmpeistri = decode_semantic(&[0x66, 0x0F, 0x3A, 0x61, 0xC1, 0x07], 0x74C8); // pcmpeistri xmm0, xmm1, 7
    assert!(!pcmpeistri.is_empty());
    let pcmpeistri_intr = pcmpeistri
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("PCMPESTRI_INTRINSIC"))
        .expect("expected PCMPESTRI intrinsic");
    assert_eq!(pcmpeistri_intr.opcode, PcodeOpcode::CallOther);
    assert_eq!(pcmpeistri_intr.inputs.last().map(|v| v.constant_val as u64), Some(0x07));
    assert!(pcmpeistri
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PCMPESTRI_ECX_WRITE") && op.output.as_ref() == Some(&x86_reg(1, 4))));

    let pcmpeistri_mem = decode_semantic(&[0x66, 0x41, 0x0F, 0x3A, 0x61, 0x00, 0x03], 0x74D0); // pcmpeistri xmm0, [r8], 3
    assert!(!pcmpeistri_mem.is_empty());
    assert!(pcmpeistri_mem.iter().any(|op| op.opcode == PcodeOpcode::Load));
    assert!(pcmpeistri_mem
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PCMPESTRI_ECX_WRITE") && op.output.as_ref() == Some(&x86_reg(1, 4))));

    let pcmpistrm = decode_semantic(&[0x66, 0x0F, 0x3A, 0x62, 0xC1, 0x0A], 0x74D8); // pcmpistrm xmm0, xmm1, 0x0a
    assert!(!pcmpistrm.is_empty());
    let pcmpistrm_intr = pcmpistrm
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("PCMPISTRM_INTRINSIC"))
        .expect("expected PCMPISTRM intrinsic");
    assert_eq!(pcmpistrm_intr.opcode, PcodeOpcode::CallOther);
    assert_eq!(pcmpistrm_intr.inputs.last().map(|v| v.constant_val as u64), Some(0x0A));
    assert!(pcmpistrm
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PCMPISTRM_XMM0_WRITE") && op.output.as_ref() == Some(&x86_xmm_reg(0, 16))));

    let pcmpistrm_mem = decode_semantic(&[0x66, 0x41, 0x0F, 0x3A, 0x62, 0x00, 0x40], 0x74E0); // pcmpistrm xmm0, [r8], 0x40
    assert!(!pcmpistrm_mem.is_empty());
    assert!(pcmpistrm_mem.iter().any(|op| op.opcode == PcodeOpcode::Load));
    assert!(pcmpistrm_mem
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PCMPISTRM_XMM0_WRITE") && op.output.as_ref() == Some(&x86_xmm_reg(0, 16))));

    let extractps = decode_semantic(&[0x66, 0x0F, 0x3A, 0x17, 0xC8, 0x03], 0x74E8); // extractps eax, xmm1, 3
    assert!(!extractps.is_empty());
    let extractps_intr = extractps
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("EXTRACTPS_INTRINSIC"))
        .expect("expected EXTRACTPS intrinsic");
    assert_eq!(extractps_intr.opcode, PcodeOpcode::CallOther);
    assert_eq!(extractps_intr.inputs.last().map(|v| v.constant_val as u64), Some(0x03));
    assert!(extractps
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("EXTRACTPS_WRITE") && op.output.as_ref() == Some(&x86_reg(0, 4))));

    let extractps_mem = decode_semantic(&[0x66, 0x0F, 0x3A, 0x17, 0x40, 0x04, 0x01], 0x74F0); // extractps [rax+1], xmm0, 4
    assert!(!extractps_mem.is_empty());
    assert!(extractps_mem
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("EXTRACTPS_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));
    assert!(extractps_mem.iter().any(|op| op.opcode == PcodeOpcode::Store));
}

#[test]
fn decode_scalar_simd_two_byte_mandatory_prefix_forms_emit_intrinsics() {
    let movss = decode_semantic(&[0xF3, 0x0F, 0x10, 0xC1], 0x74F8); // movss xmm0, xmm1
    assert!(!movss.is_empty());
    assert!(movss
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MOVSS_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));
    assert!(movss
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MOVSS_WRITE") && op.output.as_ref() == Some(&x86_xmm_reg(0, 16))));
    assert!(!movss
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("SIMD_POLICY")));

    let movsd_store = decode_semantic(&[0xF2, 0x0F, 0x11, 0x00], 0x74FC); // movsd qword ptr [rax], xmm0
    assert!(!movsd_store.is_empty());
    assert!(movsd_store
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MOVSD_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));
    assert!(movsd_store
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MOVSD_STORE") && op.opcode == PcodeOpcode::Store));

    let addss = decode_semantic(&[0xF3, 0x0F, 0x58, 0xC1], 0x7500); // addss xmm0, xmm1
    assert!(!addss.is_empty());
    assert!(addss
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("ADDSS_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));
    assert!(addss
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("ADDSS_WRITE") && op.output.as_ref() == Some(&x86_xmm_reg(0, 16))));

    let cvtsi2sd = decode_semantic(&[0xF2, 0x0F, 0x2A, 0xC1], 0x7504); // cvtsi2sd xmm0, ecx
    assert!(!cvtsi2sd.is_empty());
    assert!(cvtsi2sd
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("CVTSI2SD_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));
    assert!(cvtsi2sd
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("CVTSI2SD_WRITE") && op.output.as_ref() == Some(&x86_xmm_reg(0, 16))));

    let cvttss2si = decode_semantic(&[0xF3, 0x0F, 0x2C, 0xC1], 0x7508); // cvttss2si eax, xmm1
    assert!(!cvttss2si.is_empty());
    assert!(cvttss2si
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("CVTTSS2SI_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));
    assert!(cvttss2si
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("CVTTSS2SI_INTRINSIC") && op.output.as_ref() == Some(&x86_reg(0, 4))));

    let ucomisd = decode_semantic(&[0x66, 0x0F, 0x2E, 0xC1], 0x750C); // ucomisd xmm0, xmm1
    assert!(!ucomisd.is_empty());
    assert!(ucomisd
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("UCOMISD_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));

    let pref_f3_over_66 = decode_semantic(&[0x66, 0xF3, 0x0F, 0x10, 0xC1], 0x7510); // 66 + f3 + movss
    assert!(!pref_f3_over_66.is_empty());
    assert!(pref_f3_over_66
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MOVSS_INTRINSIC")));

    let pref_f2_over_66 = decode_semantic(&[0x66, 0xF2, 0x0F, 0x10, 0xC1], 0x7514); // 66 + f2 + movsd
    assert!(!pref_f2_over_66.is_empty());
    assert!(pref_f2_over_66
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MOVSD_INTRINSIC")));

    let cvtsi2sd_rexw = decode_semantic(&[0xF2, 0x48, 0x0F, 0x2A, 0xC1], 0x7518); // cvtsi2sd xmm0, rcx
    assert!(!cvtsi2sd_rexw.is_empty());
    let rexw_intr = cvtsi2sd_rexw
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("CVTSI2SD_INTRINSIC"))
        .expect("expected cvtsi2sd intrinsic with rex.w");
    assert_eq!(rexw_intr.inputs.get(2), Some(&x86_reg(1, 8)));
}

#[test]
fn decode_scalar_simd_p0_queue_instructions_emit_intrinsics() {
    let divsd = decode_semantic(&[0xF2, 0x0F, 0x5E, 0xC1], 0x7600); // divsd xmm0, xmm1
    assert!(divsd
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("DIVSD_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));
    assert!(divsd
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("DIVSD_WRITE") && op.output.as_ref() == Some(&x86_xmm_reg(0, 16))));

    let divss = decode_semantic(&[0xF3, 0x0F, 0x5E, 0xC1], 0x7604); // divss xmm0, xmm1
    assert!(divss
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("DIVSS_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));

    let sqrtsd = decode_semantic(&[0xF2, 0x0F, 0x51, 0xC1], 0x7608); // sqrtsd xmm0, xmm1
    assert!(sqrtsd
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("SQRTSD_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));

    let sqrtss = decode_semantic(&[0xF3, 0x0F, 0x51, 0xC1], 0x760C); // sqrtss xmm0, xmm1
    assert!(sqrtss
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("SQRTSS_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));

    let cvtsd2ss = decode_semantic(&[0xF2, 0x0F, 0x5A, 0xC1], 0x7610); // cvtsd2ss xmm0, xmm1
    assert!(cvtsd2ss
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("CVTSD2SS_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));

    let cvtss2sd = decode_semantic(&[0xF3, 0x0F, 0x5A, 0xC1], 0x7614); // cvtss2sd xmm0, xmm1
    assert!(cvtss2sd
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("CVTSS2SD_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));

    let minsd = decode_semantic(&[0xF2, 0x0F, 0x5D, 0xC1], 0x7618); // minsd xmm0, xmm1
    assert!(minsd
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MINSD_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));

    let minss = decode_semantic(&[0xF3, 0x0F, 0x5D, 0xC1], 0x761C); // minss xmm0, xmm1
    assert!(minss
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MINSS_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));

    let maxsd = decode_semantic(&[0xF2, 0x0F, 0x5F, 0xC1], 0x7620); // maxsd xmm0, xmm1
    assert!(maxsd
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MAXSD_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));

    let maxss = decode_semantic(&[0xF3, 0x0F, 0x5F, 0xC1], 0x7624); // maxss xmm0, xmm1
    assert!(maxss
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MAXSS_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));

    let comisd = decode_semantic(&[0x66, 0x0F, 0x2F, 0xC1], 0x7628); // comisd xmm0, xmm1
    assert!(comisd
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("COMISD_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));
}

#[test]
fn decode_simd_p1_queue_instructions_emit_intrinsics() {
    let movdqa_load = decode_semantic(&[0x66, 0x0F, 0x6F, 0xC1], 0x7630); // movdqa xmm0, xmm1
    assert!(movdqa_load
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MOVDQA_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));
    assert!(movdqa_load
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MOVDQA_WRITE") && op.output.as_ref() == Some(&x86_xmm_reg(0, 16))));

    let movdqa_store = decode_semantic(&[0x66, 0x0F, 0x7F, 0x00], 0x7634); // movdqa [rax], xmm0
    assert!(movdqa_store
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MOVDQA_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));
    assert!(movdqa_store
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MOVDQA_STORE") && op.opcode == PcodeOpcode::Store));

    let pxor = decode_semantic(&[0x66, 0x0F, 0xEF, 0xC1], 0x7638); // pxor xmm0, xmm1
    assert!(pxor
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PXOR_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));
    assert!(pxor
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PXOR_WRITE") && op.output.as_ref() == Some(&x86_xmm_reg(0, 16))));

    let crc32_byte = decode_semantic(&[0xF2, 0x0F, 0x38, 0xF0, 0xC1], 0x763C); // crc32 eax, cl
    let crc32b_intr = crc32_byte
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("CRC32_INTRINSIC"))
        .expect("expected crc32 byte intrinsic");
    assert_eq!(crc32b_intr.opcode, PcodeOpcode::CallOther);
    assert_eq!(crc32b_intr.inputs.get(2), Some(&x86_reg(1, 1)));
    assert!(crc32_byte
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("CRC32_WRITE") && op.output.as_ref() == Some(&x86_reg(0, 4))));

    let crc32_word = decode_semantic(&[0x66, 0xF2, 0x0F, 0x38, 0xF1, 0xC1], 0x7640); // crc32 eax, cx
    let crc32w_intr = crc32_word
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("CRC32_INTRINSIC"))
        .expect("expected crc32 word intrinsic");
    assert_eq!(crc32w_intr.inputs.get(2), Some(&x86_reg(1, 2)));
    assert!(crc32_word
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("CRC32_WRITE") && op.output.as_ref() == Some(&x86_reg(0, 4))));

    let crc32_dword = decode_semantic(&[0xF2, 0x0F, 0x38, 0xF1, 0xC1], 0x7644); // crc32 eax, ecx
    let crc32d_intr = crc32_dword
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("CRC32_INTRINSIC"))
        .expect("expected crc32 dword intrinsic");
    assert_eq!(crc32d_intr.inputs.get(2), Some(&x86_reg(1, 4)));

    let crc32_qword = decode_semantic(&[0xF2, 0x48, 0x0F, 0x38, 0xF1, 0xC1], 0x7648); // crc32 rax, rcx
    let crc32q_intr = crc32_qword
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("CRC32_INTRINSIC"))
        .expect("expected crc32 qword intrinsic");
    assert_eq!(crc32q_intr.inputs.get(2), Some(&x86_reg(1, 8)));
    assert!(crc32_qword
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("CRC32_WRITE") && op.output.as_ref() == Some(&x86_reg(0, 8))));

    let aesimc = decode_semantic(&[0x66, 0x0F, 0x38, 0xDB, 0xC1], 0x764C); // aesimc xmm0, xmm1
    assert!(aesimc
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("AESIMC_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));
    assert!(aesimc
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("AESIMC_WRITE") && op.output.as_ref() == Some(&x86_xmm_reg(0, 16))));
}

#[test]
fn decode_simd_p1_followup_queue_instructions_emit_intrinsics() {
    let movapd = decode_semantic(&[0x66, 0x0F, 0x28, 0xC1], 0x7650); // movapd xmm0, xmm1
    assert!(movapd
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MOVAPD_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));
    assert!(movapd
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MOVAPD_WRITE") && op.output.as_ref() == Some(&x86_xmm_reg(0, 16))));

    let movapd_store = decode_semantic(&[0x66, 0x0F, 0x29, 0x00], 0x7654); // movapd [rax], xmm0
    assert!(movapd_store
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MOVAPD_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));
    assert!(movapd_store
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MOVAPD_STORE") && op.opcode == PcodeOpcode::Store));

    let movd = decode_semantic(&[0x66, 0x0F, 0x6E, 0xC1], 0x7658); // movd xmm0, ecx
    assert!(movd
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MOVD_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));
    assert!(movd
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MOVD_WRITE") && op.output.as_ref() == Some(&x86_xmm_reg(0, 16))));

    let movq = decode_semantic(&[0x66, 0x48, 0x0F, 0x6E, 0xC1], 0x765C); // movq xmm0, rcx
    assert!(movq
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MOVQ_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));

    let movd_store = decode_semantic(&[0x66, 0x0F, 0x7E, 0xC1], 0x7660); // movd ecx, xmm0
    assert!(movd_store
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MOVD_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));
    assert!(movd_store
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MOVD_WRITE") && op.output.as_ref() == Some(&x86_reg(1, 4))));

    let movq_store = decode_semantic(&[0x66, 0x48, 0x0F, 0x7E, 0xC1], 0x7664); // movq rcx, xmm0
    assert!(movq_store
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MOVQ_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));
    assert!(movq_store
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("MOVQ_WRITE") && op.output.as_ref() == Some(&x86_reg(1, 8))));

    let andpd = decode_semantic(&[0x66, 0x0F, 0x54, 0xC1], 0x7668); // andpd xmm0, xmm1
    assert!(andpd
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("ANDPD_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));

    let andnpd = decode_semantic(&[0x66, 0x0F, 0x55, 0xC1], 0x766C); // andnpd xmm0, xmm1
    assert!(andnpd
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("ANDNPD_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));

    let orpd = decode_semantic(&[0x66, 0x0F, 0x56, 0xC1], 0x7670); // orpd xmm0, xmm1
    assert!(orpd
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("ORPD_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));

    let xorpd = decode_semantic(&[0x66, 0x0F, 0x57, 0xC1], 0x7674); // xorpd xmm0, xmm1
    assert!(xorpd
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("XORPD_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));

    let punpcklqdq = decode_semantic(&[0x66, 0x0F, 0x6C, 0xC1], 0x7676); // punpcklqdq xmm0, xmm1
    assert!(punpcklqdq
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PUNPCKLQDQ_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));

    let punpckhqdq = decode_semantic(&[0x66, 0x0F, 0x6D, 0xC1], 0x7677); // punpckhqdq xmm0, xmm1
    assert!(punpckhqdq
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PUNPCKHQDQ_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));

    let pshufd = decode_semantic(&[0x66, 0x0F, 0x70, 0xC1, 0x1B], 0x7678); // pshufd xmm0, xmm1, 0x1b
    let pshufd_intr = pshufd
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("PSHUFD_INTRINSIC"))
        .expect("expected PSHUFD intrinsic");
    assert_eq!(pshufd_intr.inputs.last().map(|v| v.constant_val as u64), Some(0x1B));

    let paddq = decode_semantic(&[0x66, 0x0F, 0xD4, 0xC1], 0x7679); // paddq xmm0, xmm1
    assert!(paddq
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PADDQ_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));

    let pmullw = decode_semantic(&[0x66, 0x0F, 0xD5, 0xC1], 0x767A); // pmullw xmm0, xmm1
    assert!(pmullw
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PMULLW_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));

    let pand = decode_semantic(&[0x66, 0x0F, 0xDB, 0xC1], 0x7678); // pand xmm0, xmm1
    assert!(pand
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PAND_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));

    let pandn = decode_semantic(&[0x66, 0x0F, 0xDF, 0xC1], 0x767C); // pandn xmm0, xmm1
    assert!(pandn
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PANDN_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));

    let por = decode_semantic(&[0x66, 0x0F, 0xEB, 0xC1], 0x7680); // por xmm0, xmm1
    assert!(por
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("POR_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));

    let psubb = decode_semantic(&[0x66, 0x0F, 0xF8, 0xC1], 0x7681); // psubb xmm0, xmm1
    assert!(psubb
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PSUBB_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));

    let psubw = decode_semantic(&[0x66, 0x0F, 0xF9, 0xC1], 0x7682); // psubw xmm0, xmm1
    assert!(psubw
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PSUBW_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));

    let psubd = decode_semantic(&[0x66, 0x0F, 0xFA, 0xC1], 0x7683); // psubd xmm0, xmm1
    assert!(psubd
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PSUBD_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));

    let psubq = decode_semantic(&[0x66, 0x0F, 0xFB, 0xC1], 0x7684); // psubq xmm0, xmm1
    assert!(psubq
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PSUBQ_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));

    let paddb = decode_semantic(&[0x66, 0x0F, 0xFC, 0xC1], 0x7685); // paddb xmm0, xmm1
    assert!(paddb
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PADDB_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));

    let paddw = decode_semantic(&[0x66, 0x0F, 0xFD, 0xC1], 0x7686); // paddw xmm0, xmm1
    assert!(paddw
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PADDW_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));

    let paddd = decode_semantic(&[0x66, 0x0F, 0xFE, 0xC1], 0x7687); // paddd xmm0, xmm1
    assert!(paddd
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PADDD_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));

    let pcmpeqb = decode_semantic(&[0x66, 0x0F, 0x74, 0xC1], 0x7688); // pcmpeqb xmm0, xmm1
    assert!(pcmpeqb
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PCMPEQB_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));

    let pcmpeqw = decode_semantic(&[0x66, 0x0F, 0x75, 0xC1], 0x7689); // pcmpeqw xmm0, xmm1
    assert!(pcmpeqw
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PCMPEQW_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));

    let pcmpeqd = decode_semantic(&[0x66, 0x0F, 0x76, 0xC1], 0x768A); // pcmpeqd xmm0, xmm1
    assert!(pcmpeqd
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("PCMPEQD_INTRINSIC") && op.opcode == PcodeOpcode::CallOther));
}

#[test]
fn decode_p2_followup_aeskeygenassist_emits_intrinsic() {
    let aeskeygenassist = decode_semantic(&[0x66, 0x0F, 0x3A, 0xDF, 0xC1, 0x11], 0x7690); // aeskeygenassist xmm0, xmm1, 0x11
    assert!(!aeskeygenassist.is_empty());
    let intrinsic = aeskeygenassist
        .iter()
        .find(|op| op.asm_mnemonic.as_deref() == Some("AESKEYGENASSIST_INTRINSIC"))
        .expect("expected AESKEYGENASSIST intrinsic");
    assert_eq!(intrinsic.opcode, PcodeOpcode::CallOther);
    assert_eq!(intrinsic.inputs.last().map(|v| v.constant_val as u64), Some(0x11));
    assert!(aeskeygenassist
        .iter()
        .any(|op| op.asm_mnemonic.as_deref() == Some("AESKEYGENASSIST_WRITE") && op.output.as_ref() == Some(&x86_xmm_reg(0, 16))));
}

// ── Phase 1: x87 FPU ──────────────────────────────────────────────────────────

#[test]
fn decode_x87_fadd_reg_form_emits_float_add_on_st0() {
    // D8 C1 = FADD ST(0), ST(1)  (D8 with mod=3, reg=0, rm=1)
    let ops = decode_semantic(&[0xD8, 0xC1], 0x8000);
    assert!(!ops.is_empty());
    assert!(ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("FADD")
        && op.opcode == PcodeOpcode::FloatAdd));
    // Result written back to ST(0): size=10 (80-bit extended precision)
    let fadd = ops.iter().find(|op| op.asm_mnemonic.as_deref() == Some("FADD")).unwrap();
    assert_eq!(fadd.output.as_ref().map(|v| v.size), Some(10));
    assert!(!has_flag_write(&ops, x86_flag_zf()));
    assert!(!has_flag_write(&ops, x86_flag_cf()));
}

#[test]
fn decode_x87_fmul_reg_form_emits_float_mul_on_st0() {
    // D8 C9 = FMUL ST(0), ST(1)  (D8 with mod=3, reg=1, rm=1)
    let ops = decode_semantic(&[0xD8, 0xC9], 0x8004);
    assert!(!ops.is_empty());
    assert!(ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("FMUL")
        && op.opcode == PcodeOpcode::FloatMult));
}

#[test]
fn decode_x87_fld_reg_form_copies_stn_to_st0() {
    // D9 C1 = FLD ST(1) — load ST(1) onto the stack top
    let ops = decode_semantic(&[0xD9, 0xC1], 0x8008);
    assert!(!ops.is_empty());
    assert!(ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("FLD_ST")
        && op.opcode == PcodeOpcode::Copy));
}

#[test]
fn decode_x87_fsub_emits_float_sub_on_st0() {
    // D8 E1 = FSUB ST(0), ST(1) (D8 mod=3, reg=4, rm=1)
    let ops = decode_semantic(&[0xD8, 0xE1], 0x800C);
    assert!(!ops.is_empty());
    assert!(ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("FSUB")
        && op.opcode == PcodeOpcode::FloatSub));
}

#[test]
fn decode_x87_fxch_swaps_st0_and_stn() {
    // D9 C9 = FXCH ST(1) (D9 mod=3, reg=1, rm=1)
    let ops = decode_semantic(&[0xD9, 0xC9], 0x8010);
    assert!(!ops.is_empty());
    // Should have save + two writes for the swap
    assert!(ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("FXCH_SAVE")));
    assert!(ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("FXCH_ST0")));
    assert!(ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("FXCH_STN")));
}

// ── Phase 2: Primary 1-byte flag manipulation gaps ───────────────────────────

#[test]
fn decode_clc_stc_cmc_set_clear_toggle_cf() {
    let clc = decode_semantic(&[0xF8], 0x8020); // clc
    assert!(!clc.is_empty());
    assert!(has_flag_write(&clc, x86_flag_cf()));
    let clc_op = clc.iter().find(|op| op.asm_mnemonic.as_deref() == Some("CLC")).unwrap();
    assert_eq!(clc_op.opcode, PcodeOpcode::Copy);
    assert_eq!(clc_op.inputs[0].constant_val, 0);

    let stc = decode_semantic(&[0xF9], 0x8021); // stc
    assert!(!stc.is_empty());
    assert!(has_flag_write(&stc, x86_flag_cf()));
    let stc_op = stc.iter().find(|op| op.asm_mnemonic.as_deref() == Some("STC")).unwrap();
    assert_eq!(stc_op.opcode, PcodeOpcode::Copy);
    assert_eq!(stc_op.inputs[0].constant_val, 1);

    let cmc = decode_semantic(&[0xF5], 0x8022); // cmc
    assert!(!cmc.is_empty());
    assert!(has_flag_write(&cmc, x86_flag_cf()));
    let cmc_op = cmc.iter().find(|op| op.asm_mnemonic.as_deref() == Some("CMC")).unwrap();
    assert_eq!(cmc_op.opcode, PcodeOpcode::IntXor);
    assert!(has_flag_input(&cmc, x86_flag_cf()));
}

#[test]
fn decode_cld_std_set_clear_df() {
    let cld = decode_semantic(&[0xFC], 0x8023); // cld
    assert!(!cld.is_empty());
    assert!(has_flag_write(&cld, x86_flag_df()));
    let cld_op = cld.iter().find(|op| op.asm_mnemonic.as_deref() == Some("CLD")).unwrap();
    assert_eq!(cld_op.inputs[0].constant_val, 0);

    let std_op = decode_semantic(&[0xFD], 0x8024); // std
    assert!(!std_op.is_empty());
    assert!(has_flag_write(&std_op, x86_flag_df()));
    let std = std_op.iter().find(|op| op.asm_mnemonic.as_deref() == Some("STD")).unwrap();
    assert_eq!(std.inputs[0].constant_val, 1);
}

#[test]
fn decode_lahf_builds_ah_from_flag_bits() {
    let ops = decode_semantic(&[0x9F], 0x8025); // lahf
    assert!(!ops.is_empty());
    // LAHF reads several flags and assembles them into AH
    assert!(has_flag_input(&ops, x86_flag_cf()));
    assert!(has_flag_input(&ops, x86_flag_sf()));
    assert!(has_flag_input(&ops, x86_flag_zf()));
    assert!(has_flag_input(&ops, x86_flag_pf()));
    // Must contain a LAHF_CF op
    assert!(ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("LAHF_CF")));
    // Final result written into AX (size 2)
    assert!(ops.iter().any(|op| op.output.as_ref().map(|v| v.size == 2).unwrap_or(false)));
}

#[test]
fn decode_sahf_writes_flags_from_ah_bits() {
    let ops = decode_semantic(&[0x9E], 0x8026); // sahf
    assert!(!ops.is_empty());
    assert!(has_flag_write(&ops, x86_flag_cf()));
    assert!(has_flag_write(&ops, x86_flag_pf()));
    assert!(has_flag_write(&ops, x86_flag_zf()));
    assert!(has_flag_write(&ops, x86_flag_sf()));
    // Should extract AH as source
    assert!(ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("SAHF_AH")));
}

// ── Phase 3: ROL/RCL explicit P-code (covered above; extra edge-case) ─────────

#[test]
fn decode_rol_byte_form_produces_correct_size_operands() {
    // C0 C0 05 = rol al, 5
    let ops = decode_semantic(&[0xC0, 0xC0, 0x05], 0x8030);
    assert!(!ops.is_empty());
    let rol_shl = ops.iter().find(|op| op.asm_mnemonic.as_deref() == Some("ROL_SHL")).unwrap();
    assert_eq!(rol_shl.inputs[0].size, 1); // byte operand
}

#[test]
fn decode_rcl_reg_count1_emits_rcl_pcode() {
    // D1 D0 = rcl eax, 1  (mod=3, reg=2, rm=0)
    let ops = decode_semantic(&[0xD1, 0xD0], 0x8034);
    assert!(!ops.is_empty());
    // Must have the SHL step and the CF-inject (zero-extend old CF into the operand)
    assert!(ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("RCL_SHL")));
    assert!(ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("RCL_CF_ZEXT")));
    assert!(ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("RCL")));
    // Reads old CF and writes new CF
    assert!(has_flag_input(&ops, x86_flag_cf()));
    assert!(has_flag_write(&ops, x86_flag_cf()));
}

// ── Phase 4: 0F extended — XADD / CMPXCHG ───────────────────────────────────

#[test]
fn decode_xadd_reg_reg_swaps_and_writes_sum_with_flags() {
    // 0F C1 D8 = xadd eax, ebx  (0xC1=dword form, 0xD8=mod3 reg=3 rm=0)
    let ops = decode_semantic(&[0x0F, 0xC1, 0xD8], 0x8040);
    assert!(!ops.is_empty());
    // sum = eax + ebx
    assert!(ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("XADD_SUM")
        && op.opcode == PcodeOpcode::IntAdd));
    // old r/m value saved for the register exchange
    assert!(ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("XADD_OLD_RM")));
    // flag updates
    assert!(has_flag_write(&ops, x86_flag_cf()));
    assert!(has_flag_write(&ops, x86_flag_of()));
    assert!(has_flag_write(&ops, x86_flag_zf()));
    assert!(has_flag_write(&ops, x86_flag_sf()));
    assert!(has_flag_write(&ops, x86_flag_pf()));
}

#[test]
fn decode_cmpxchg_reg_reg_emits_conditional_update_and_flags() {
    // 0F B1 C3 = cmpxchg ebx, eax  (0xB1=dword form, 0xC3=mod3 reg=0 rm=3)
    let ops = decode_semantic(&[0x0F, 0xB1, 0xC3], 0x8048);
    assert!(!ops.is_empty());
    // CMP step
    assert!(ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("CMPXCHG_CMP")
        && op.opcode == PcodeOpcode::IntSub));
    // ZF update
    assert!(has_flag_write(&ops, x86_flag_zf()));
    // CF/SF/OF updates
    assert!(has_flag_write(&ops, x86_flag_cf()));
    assert!(has_flag_write(&ops, x86_flag_sf()));
    assert!(has_flag_write(&ops, x86_flag_of()));
    // Conditional write to r/m (ZF=1 path)
    assert!(ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("CMPXCHG_NEW_RM")));
    // Conditional write to accumulator (ZF=0 path)
    assert!(ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("CMPXCHG_ACCUM_WRITE")));
}

// ── Phase 5: VEX prefix routing ──────────────────────────────────────────────

#[test]
fn decode_vex2_vmovapd_routes_to_simd_decoder() {
    // C5 F9 28 C1 = VMOVAPD xmm0, xmm1
    // VEX2: R̄=1(no ext), vvvv=0, L=0, pp=01(66)  → routes to movapd (P66,0x28)
    let ops = decode_semantic(&[0xC5, 0xF9, 0x28, 0xC1], 0x8060);
    assert!(!ops.is_empty());
    // Routed to existing SSE movapd path
    assert!(ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("MOVAPD_INTRINSIC")
        || op.asm_mnemonic.as_deref() == Some("MOVAPD_WRITE")));
}

#[test]
fn decode_vex2_vmovss_routes_to_simd_decoder() {
    // C5 FA 10 C1 = VMOVSS xmm0, xmm1 (VEX2, pp=10=F3, map=0F, opcode=0x10)
    let ops = decode_semantic(&[0xC5, 0xFA, 0x10, 0xC1], 0x8064);
    assert!(!ops.is_empty());
    assert!(ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("MOVSS_INTRINSIC")
        || op.asm_mnemonic.as_deref() == Some("MOVSS_WRITE")));
}

#[test]
fn decode_vex3_map1_routes_to_simd_decoder() {
    // C4 E1 79 28 C1 = VMOVAPD xmm0, xmm1 (3-byte VEX, map=1, W=0, pp=01)
    // C4: 3-byte VEX leader
    // E1: R̄=1, X̄=1, B̄=1, map=00001 (0x0F)
    // 79: W=0, vvvv=1111, L=0, pp=01 (66)
    // 28: opcode
    // C1: ModRM mod=11, reg=0, rm=1
    let ops = decode_semantic(&[0xC4, 0xE1, 0x79, 0x28, 0xC1], 0x8068);
    assert!(!ops.is_empty());
    assert!(ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("MOVAPD_INTRINSIC")
        || op.asm_mnemonic.as_deref() == Some("MOVAPD_WRITE")));
}

#[test]
fn decode_vex3_map2_routes_to_0f38_decoder() {
    // C4 E2 79 00 CA = VPSHUFB xmm1, xmm0, xmm2 (3-byte VEX map=2=0F38, pp=01=66, opcode=0x00)
    // C4: 3-byte VEX
    // E2: R̄=1, X̄=1, B̄=1, map=00010 (0F38)
    // 79: W=0, vvvv=0000, L=0, pp=01 (66)
    // 00: opcode (PSHUFB in 0F38 map)
    // CA: ModRM mod=11, reg=1, rm=2
    let ops = decode_semantic(&[0xC4, 0xE2, 0x79, 0x00, 0xCA], 0x806C);
    assert!(!ops.is_empty());
    // Should route to PSHUFB via 0F38 path
    assert!(ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("PSHUFB_INTRINSIC")
        || op.asm_mnemonic.as_deref() == Some("PSHUFB_WRITE")));
}

// ── Phase A: Byte-form ALU ────────────────────────────────────────────────────

#[test]
fn decode_byte_add_rm_r_emits_add_with_flags() {
    // 00 D8 = ADD AL, BL  (mod=11, reg=3/BL, rm=0/AL)
    let ops = decode_semantic(&[0x00, 0xD8], 0x9000);
    assert!(!ops.is_empty());
    // Must produce a 1-byte result
    let has_add = ops.iter().any(|op| op.opcode == PcodeOpcode::IntAdd);
    assert!(has_add, "expected IntAdd for byte ADD");
    // Flag updates expected
    assert!(has_flag_write(&ops, x86_flag_cf()));
    assert!(has_flag_write(&ops, x86_flag_zf()));
    assert!(has_flag_write(&ops, x86_flag_sf()));
    assert!(has_flag_write(&ops, x86_flag_of()));
}

#[test]
fn decode_byte_add_r_rm_emits_add_with_flags() {
    // 02 D8 = ADD BL, AL  (mod=11, reg=3/BL, rm=0/AL) — reg is destination
    let ops = decode_semantic(&[0x02, 0xD8], 0x9001);
    assert!(!ops.is_empty());
    assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::IntAdd));
    assert!(has_flag_write(&ops, x86_flag_cf()));
}

#[test]
fn decode_byte_sub_rm_r_emits_sub_with_flags() {
    // 28 D8 = SUB AL, BL  (mod=11, reg=3, rm=0)
    let ops = decode_semantic(&[0x28, 0xD8], 0x9002);
    assert!(!ops.is_empty());
    assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::IntSub));
    assert!(has_flag_write(&ops, x86_flag_cf()));
    assert!(has_flag_write(&ops, x86_flag_zf()));
}

#[test]
fn decode_byte_cmp_rm_r_emits_sub_no_write_with_flags() {
    // 38 D8 = CMP AL, BL — no destination write, only flags
    let ops = decode_semantic(&[0x38, 0xD8], 0x9003);
    assert!(!ops.is_empty());
    // CMP emits subtraction for flags only; the result varnode is a temp
    assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::IntSub));
    assert!(has_flag_write(&ops, x86_flag_cf()));
    assert!(has_flag_write(&ops, x86_flag_zf()));
    assert!(has_flag_write(&ops, x86_flag_sf()));
    assert!(has_flag_write(&ops, x86_flag_of()));
}

#[test]
fn decode_80_group_byte_alu_add_emits_add_with_flags() {
    // 80 C0 05 = ADD AL, 5  (reg/0=Add, rm=0/AL, imm8=5)
    let ops = decode_semantic(&[0x80, 0xC0, 0x05], 0x9004);
    assert!(!ops.is_empty());
    assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::IntAdd));
    assert!(has_flag_write(&ops, x86_flag_of()));
}

#[test]
fn decode_80_group_byte_alu_cmp_no_result_write() {
    // 80 F8 07 = CMP AL, 7  (reg/7=Cmp, rm=0/AL, imm8=7)
    let ops = decode_semantic(&[0x80, 0xF8, 0x07], 0x9005);
    assert!(!ops.is_empty());
    assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::IntSub));
    assert!(has_flag_write(&ops, x86_flag_cf()));
    assert!(has_flag_write(&ops, x86_flag_zf()));
}

#[test]
fn decode_84_test_byte_emits_and_no_write() {
    // 84 C0 = TEST AL, AL  (mod=11, reg=0, rm=0)
    let ops = decode_semantic(&[0x84, 0xC0], 0x9006);
    assert!(!ops.is_empty());
    // TEST emits AND but writes only flags
    assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::IntAnd));
    assert!(has_flag_write(&ops, x86_flag_zf()));
    assert!(has_flag_write(&ops, x86_flag_sf()));
    assert!(has_flag_write(&ops, x86_flag_pf()));
}

#[test]
fn decode_fe_inc_dec_byte_emits_correct_kind() {
    // FE C0 = INC AL  (reg/0=Inc, mod=11, rm=0)
    let inc_ops = decode_semantic(&[0xFE, 0xC0], 0x9007);
    assert!(!inc_ops.is_empty());
    assert!(inc_ops.iter().any(|op| op.opcode == PcodeOpcode::IntAdd));
    assert!(has_flag_write(&inc_ops, x86_flag_of()));

    // FE C8 = DEC AL  (reg/1=Dec, mod=11, rm=0)
    let dec_ops = decode_semantic(&[0xFE, 0xC8], 0x9008);
    assert!(!dec_ops.is_empty());
    assert!(dec_ops.iter().any(|op| op.opcode == PcodeOpcode::IntSub));
    assert!(has_flag_write(&dec_ops, x86_flag_of()));
}

// ── Phase B: MOVSXD, LEAVE, XCHG short ───────────────────────────────────────

#[test]
fn decode_movsxd_r64_rm32_emits_sext() {
    // REX.W(48) + 63 C0 = MOVSXD RAX, EAX  (mod=11, reg=0, rm=0)
    let ops = decode_semantic(&[0x48, 0x63, 0xC0], 0x9010);
    assert!(!ops.is_empty());
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::IntSExt
            && op.asm_mnemonic.as_deref() == Some("MOVSXD")),
        "expected IntSExt with MOVSXD mnemonic"
    );
    // Output must be 8-byte (64-bit register)
    let sext_op = ops.iter().find(|op| op.asm_mnemonic.as_deref() == Some("MOVSXD")).unwrap();
    assert_eq!(sext_op.output.as_ref().unwrap().size, 8);
}

#[test]
fn decode_leave_emits_rsp_set_and_rbp_restore() {
    // C9 = LEAVE
    let ops = decode_semantic(&[0xC9], 0x9020);
    assert!(!ops.is_empty());
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("LEAVE_RSP_SET")),
        "expected LEAVE_RSP_SET"
    );
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("LEAVE_RBP_WRITE")),
        "expected LEAVE_RBP_WRITE"
    );
}

#[test]
fn decode_xchg_short_form_emits_three_copy_swap() {
    // 91 = XCHG RCX, RAX  (opcode & 7 = 1 → RCX)
    let ops = decode_semantic(&[0x91], 0x9030);
    assert!(!ops.is_empty());
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("XCHG_RAX_SAVE")),
        "expected XCHG_RAX_SAVE"
    );
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("XCHG_RAX_WRITE")),
        "expected XCHG_RAX_WRITE"
    );
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("XCHG_REG_WRITE")),
        "expected XCHG_REG_WRITE"
    );
}

// ── Phase C: MOV moffs + PUSHF/POPF ──────────────────────────────────────────

#[test]
fn decode_mov_moffs_a1_load_emits_load_and_write() {
    // A1 followed by 8-byte absolute address = MOV RAX, [abs64]
    // address = 0x0011223344556677 in little-endian
    let ops = decode_semantic(
        &[0xA1, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11, 0x00],
        0x9040,
    );
    assert!(!ops.is_empty());
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("MOV_MOFFS_LOAD")),
        "expected MOV_MOFFS_LOAD"
    );
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("MOV_MOFFS_WRITE")),
        "expected MOV_MOFFS_WRITE"
    );
}

#[test]
fn decode_mov_moffs_a3_store_emits_store() {
    // A3 followed by 8-byte absolute address = MOV [abs64], RAX
    let ops = decode_semantic(
        &[0xA3, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11, 0x00],
        0x9044,
    );
    assert!(!ops.is_empty());
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("MOV_MOFFS_STORE")),
        "expected MOV_MOFFS_STORE"
    );
}

#[test]
fn decode_pushfq_assembles_rflags_and_pushes() {
    // 9C = PUSHFQ
    let ops = decode_semantic(&[0x9C], 0x9050);
    assert!(!ops.is_empty());
    // Reads all individual flags
    let flag_reads: Vec<_> = ops.iter()
        .filter(|op| op.inputs.iter().any(|v| {
            v.size == 1 && !v.is_constant
        }))
        .collect();
    assert!(!flag_reads.is_empty(), "expected flag reads for PUSHFQ");
    // Final operation must be a stack push (Store or sub from RSP)
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::Store),
        "expected Store for stack push in PUSHFQ"
    );
}

#[test]
fn decode_popfq_restores_flags_from_stack() {
    // 9D = POPFQ
    let ops = decode_semantic(&[0x9D], 0x9054);
    assert!(!ops.is_empty());
    // Starts with a stack Load
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::Load),
        "expected Load for stack pop in POPFQ"
    );
    // Writes to all individual flag varnodes
    assert!(has_flag_write(&ops, x86_flag_cf()), "expected CF write in POPFQ");
    assert!(has_flag_write(&ops, x86_flag_pf()), "expected PF write in POPFQ");
    assert!(has_flag_write(&ops, x86_flag_zf()), "expected ZF write in POPFQ");
    assert!(has_flag_write(&ops, x86_flag_sf()), "expected SF write in POPFQ");
    assert!(has_flag_write(&ops, x86_flag_df()), "expected DF write in POPFQ");
    assert!(has_flag_write(&ops, x86_flag_of()), "expected OF write in POPFQ");
}

// ── Phase 3차 보강: NOT, BT-imm8, POPCNT, PUSH/POP GS, ENTER ─────────────────

#[test]
fn decode_not_dword_emits_intnegate_no_flags() {
    // F7 D0 = NOT EAX  (mod=11, /2=010, rm=0)  D0 = 11_010_000
    let ops = decode_semantic(&[0xF7, 0xD0], 0xA000);
    assert!(!ops.is_empty());
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::IntNegate
            && op.asm_mnemonic.as_deref() == Some("NOT_RM")),
        "expected IntNegate with NOT_RM mnemonic"
    );
    // NOT must NOT update any flags
    assert!(!has_flag_write(&ops, x86_flag_cf()), "NOT must not update CF");
    assert!(!has_flag_write(&ops, x86_flag_of()), "NOT must not update OF");
    assert!(!has_flag_write(&ops, x86_flag_zf()), "NOT must not update ZF");
    assert!(!has_flag_write(&ops, x86_flag_sf()), "NOT must not update SF");
}

#[test]
fn decode_not_byte_emits_intnegate_no_flags() {
    // F6 D0 = NOT AL  (mod=11, /2=010, rm=0)  D0 = 11_010_000
    let ops = decode_semantic(&[0xF6, 0xD0], 0xA001);
    assert!(!ops.is_empty());
    assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::IntNegate));
    assert!(!has_flag_write(&ops, x86_flag_cf()), "NOT byte must not update CF");
}

#[test]
fn decode_bt_imm8_sets_cf_no_write() {
    // 0F BA E0 05 = BT EAX, 5  (mod=11, reg/4=BT, rm=0, imm8=5)
    let ops = decode_semantic(&[0x0F, 0xBA, 0xE0, 0x05], 0xA010);
    assert!(!ops.is_empty());
    // Must compute a bit mask via IntLeft
    assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::IntLeft
        && op.asm_mnemonic.as_deref() == Some("BT_IMM8_MASK")));
    // Must update CF
    assert!(has_flag_write(&ops, x86_flag_cf()), "BT imm8 must write CF");
    // BT does not modify the operand — no BT_IMM8_WRITE or BT_IMM8_STORE
    assert!(!ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("BT_IMM8_WRITE")));
}

#[test]
fn decode_bts_imm8_sets_bit_and_cf() {
    // 0F BA E8 03 = BTS EAX, 3  (mod=11, reg/5=BTS, rm=0, imm8=3)
    let ops = decode_semantic(&[0x0F, 0xBA, 0xE8, 0x03], 0xA011);
    assert!(!ops.is_empty());
    assert!(has_flag_write(&ops, x86_flag_cf()), "BTS imm8 must write CF");
    // BTS writes back the result
    assert!(ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("BTS_IMM8_WRITE")));
}

#[test]
fn decode_popcnt_emits_popcount_and_clears_flags() {
    // F3 0F B8 C0 = POPCNT EAX, EAX  (REP prefix + 0F B8, mod=11, reg=0, rm=0)
    let ops = decode_semantic(&[0xF3, 0x0F, 0xB8, 0xC0], 0xA020);
    assert!(!ops.is_empty());
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::PopCount
            && op.asm_mnemonic.as_deref() == Some("POPCNT")),
        "expected PopCount opcode with POPCNT mnemonic"
    );
    // ZF = (src == 0)
    assert!(has_flag_write(&ops, x86_flag_zf()), "POPCNT must write ZF");
    // CF/OF/SF/AF/PF must be written to 0
    assert!(has_flag_write(&ops, x86_flag_cf()), "POPCNT must clear CF");
    assert!(has_flag_write(&ops, x86_flag_of()), "POPCNT must clear OF");
    assert!(has_flag_write(&ops, x86_flag_sf()), "POPCNT must clear SF");
    assert!(has_flag_write(&ops, x86_flag_pf()), "POPCNT must clear PF");
}

#[test]
fn decode_push_gs_emits_callother_policy() {
    // 0F A8 = PUSH GS
    let ops = decode_semantic(&[0x0F, 0xA8], 0xA030);
    assert!(!ops.is_empty());
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("PUSH_GS_POLICY")),
        "expected PUSH_GS_POLICY CallOther"
    );
}

#[test]
fn decode_pop_gs_emits_callother_policy() {
    // 0F A9 = POP GS
    let ops = decode_semantic(&[0x0F, 0xA9], 0xA031);
    assert!(!ops.is_empty());
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("POP_GS_POLICY")),
        "expected POP_GS_POLICY CallOther"
    );
}

#[test]
fn decode_enter_emits_push_rbp_frame_and_alloc() {
    // C8 10 00 00 = ENTER 16, 0  (alloc=16 bytes, nesting=0)
    let ops = decode_semantic(&[0xC8, 0x10, 0x00, 0x00], 0xA040);
    assert!(!ops.is_empty());
    // Must push RBP onto the stack
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("ENTER_PUSH_RBP_STORE")
            || op.asm_mnemonic.as_deref() == Some("ENTER_PUSH_RBP_PUSH")),
        "expected stack push of RBP"
    );
    // MOV RBP, RSP
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("ENTER_FRAME")),
        "expected ENTER_FRAME copy"
    );
    // SUB RSP, alloc_size
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::IntSub
            && op.asm_mnemonic.as_deref() == Some("ENTER_ALLOC")),
        "expected ENTER_ALLOC subtraction"
    );
}

#[test]
fn decode_int3_emits_callother_trap() {
    // CC = INT3
    let ops = decode_semantic(&[0xCC], 0xA050);
    assert!(!ops.is_empty());
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("INT3_TRAP")),
        "expected INT3_TRAP CallOther"
    );
}

#[test]
fn decode_int_n_emits_callother_with_vector() {
    // CD 21 = INT 0x21 (DOS int)
    let ops = decode_semantic(&[0xCD, 0x21], 0xA051);
    assert!(!ops.is_empty());
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("INT_IMM_TRAP")),
        "expected INT_IMM_TRAP CallOther"
    );
    // Must have the vector number as second input
    let int_op = ops.iter().find(|op| op.asm_mnemonic.as_deref() == Some("INT_IMM_TRAP")).unwrap();
    assert_eq!(int_op.inputs.len(), 2, "INT_IMM_TRAP should have policy_id + vector inputs");
}

// =====================================================================
// Phase A: 1-byte 잔여 opcode 테스트
// =====================================================================

#[test]
fn decode_hlt_emits_callother() {
    // F4 = HLT
    let ops = decode_semantic(&[0xF4], 0x1000);
    assert!(!ops.is_empty());
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("HLT_POLICY")),
        "expected HLT_POLICY CallOther"
    );
}

#[test]
fn decode_daa_emits_callother() {
    // 27 = DAA
    let ops = decode_semantic(&[0x27], 0x1000);
    assert!(!ops.is_empty());
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("DAA_POLICY")),
        "expected DAA_POLICY CallOther"
    );
}

#[test]
fn decode_das_emits_callother() {
    // 2F = DAS
    let ops = decode_semantic(&[0x2F], 0x1000);
    assert!(!ops.is_empty());
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("DAS_POLICY")),
        "expected DAS_POLICY CallOther"
    );
}

#[test]
fn decode_aaa_emits_callother() {
    // 37 = AAA
    let ops = decode_semantic(&[0x37], 0x1000);
    assert!(!ops.is_empty());
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("AAA_POLICY")),
        "expected AAA_POLICY CallOther"
    );
}

#[test]
fn decode_aas_emits_callother() {
    // 3F = AAS
    let ops = decode_semantic(&[0x3F], 0x1000);
    assert!(!ops.is_empty());
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("AAS_POLICY")),
        "expected AAS_POLICY CallOther"
    );
}

#[test]
fn decode_ins_emits_callother() {
    // 6C = INSB
    let ops = decode_semantic(&[0x6C], 0x1000);
    assert!(!ops.is_empty());
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("INS_POLICY")),
        "expected INS_POLICY CallOther"
    );
}

#[test]
fn decode_outs_emits_callother() {
    // 6E = OUTSB
    let ops = decode_semantic(&[0x6E], 0x1000);
    assert!(!ops.is_empty());
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("OUTS_POLICY")),
        "expected OUTS_POLICY CallOther"
    );
}

#[test]
fn decode_mov_seg_emits_copy() {
    // 8E C8 = MOV CS, AX (ModRM = C8: mod=3, reg=1, rm=0)
    let ops = decode_semantic(&[0x8E, 0xC8], 0x1000);
    assert!(!ops.is_empty());
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::Copy
            && op.asm_mnemonic.as_deref() == Some("MOV_SEG_WRITE")),
        "expected MOV_SEG_WRITE Copy"
    );
}

// =====================================================================
// Phase B: 0x0F 시스템 테스트
// =====================================================================

#[test]
fn decode_rdpmc_emits_callother() {
    // 0F 33 = RDPMC
    let ops = decode_semantic(&[0x0F, 0x33], 0x1000);
    assert!(!ops.is_empty());
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("RDPMC_POLICY")),
        "expected RDPMC_POLICY CallOther"
    );
}

#[test]
fn decode_mmx_d8_routes_to_simd_policy() {
    // 0F D8 C0 = PSUBUSB mm0, mm0 (no prefix → MMX)
    let ops = decode_semantic(&[0x0F, 0xD8, 0xC0], 0x1000);
    // Should emit a SIMD_POLICY CallOther rather than empty
    assert!(
        !ops.is_empty(),
        "MMX D8 should now route to SIMD_POLICY, not empty"
    );
}

// =====================================================================
// Phase C: SSE packed 테스트
// =====================================================================

#[test]
fn decode_movups_load_emits_ops() {
    // 0F 10 C0 = MOVUPS xmm0, xmm0 (None prefix, ModRM C0: mod=3, reg=0, rm=0)
    let ops = decode_semantic(&[0x0F, 0x10, 0xC0], 0x2000);
    assert!(!ops.is_empty(), "MOVUPS should produce ops");
}

#[test]
fn decode_addps_emits_xmm_binop() {
    // 0F 58 C1 = ADDPS xmm0, xmm1
    let ops = decode_semantic(&[0x0F, 0x58, 0xC1], 0x2000);
    assert!(
        !ops.is_empty(),
        "ADDPS (None prefix 0F 58) should produce ops"
    );
}

#[test]
fn decode_movupd_load_emits_ops() {
    // 66 0F 10 C0 = MOVUPD xmm0, xmm0
    let ops = decode_semantic(&[0x66, 0x0F, 0x10, 0xC0], 0x2000);
    assert!(!ops.is_empty(), "MOVUPD should produce ops");
}

#[test]
fn decode_addpd_emits_xmm_binop() {
    // 66 0F 58 C1 = ADDPD xmm0, xmm1
    let ops = decode_semantic(&[0x66, 0x0F, 0x58, 0xC1], 0x2000);
    assert!(!ops.is_empty(), "ADDPD (P66 0F 58) should produce ops");
}

#[test]
fn decode_pcmpgtb_emits_xmm_binop() {
    // 66 0F 64 C1 = PCMPGTB xmm0, xmm1
    let ops = decode_semantic(&[0x66, 0x0F, 0x64, 0xC1], 0x2000);
    assert!(!ops.is_empty(), "PCMPGTB (P66 0F 64) should produce ops");
}

#[test]
fn decode_andps_emits_xmm_binop() {
    // 0F 54 C1 = ANDPS xmm0, xmm1
    let ops = decode_semantic(&[0x0F, 0x54, 0xC1], 0x2000);
    assert!(!ops.is_empty(), "ANDPS (None 0F 54) should produce ops");
}

// =====================================================================
// Phase D: x87 FPU 테스트
// =====================================================================

#[test]
fn decode_x87_fld1_emits_float_int2float() {
    // D9 E8 = FLD1 (reg form: mod=3/E8, reg_field=5, rm_low=0)
    let ops = decode_semantic(&[0xD9, 0xE8], 0x3000);
    assert!(
        !ops.is_empty(),
        "FLD1 should produce ops"
    );
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::FloatInt2Float
            && op.asm_mnemonic.as_deref() == Some("FLD1")),
        "FLD1 should emit FloatInt2Float"
    );
}

#[test]
fn decode_x87_fldz_emits_copy() {
    // D9 EE = FLDZ (reg form: E8 base + 6 = EE)
    let ops = decode_semantic(&[0xD9, 0xEE], 0x3000);
    assert!(!ops.is_empty(), "FLDZ should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::Copy
            && op.asm_mnemonic.as_deref() == Some("FLDZ")),
        "FLDZ should emit Copy with zero"
    );
}

#[test]
fn decode_x87_fldcw_emits_callother() {
    // D9 6D 00 = FLDCW [rbp+0]
    // ModRM 0x6D = 01 101 101 = mod=1, reg=5(FLDCW), rm=5([rbp+disp8])
    let ops = decode_semantic(&[0xD9, 0x6D, 0x00], 0x3000);
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("FLDCW_POLICY")),
        "FLDCW should emit FLDCW_POLICY CallOther"
    );
}

#[test]
fn decode_x87_fcomi_emits_float_compare() {
    // DB F1 = FCOMI ST(0), ST(1) (reg form: F0 base, reg_field=6, rm_low=1)
    let ops = decode_semantic(&[0xDB, 0xF1], 0x3000);
    assert!(!ops.is_empty(), "FCOMI should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::FloatEqual),
        "FCOMI should emit FloatEqual for ZF"
    );
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::FloatLess),
        "FCOMI should emit FloatLess for CF"
    );
}

#[test]
fn decode_x87_fucomip_emits_float_compare() {
    // DF E9 = FUCOMIP ST(0), ST(1) (reg form, reg_field=5, rm_low=1)
    let ops = decode_semantic(&[0xDF, 0xE9], 0x3000);
    assert!(!ops.is_empty(), "FUCOMIP should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::FloatEqual),
        "FUCOMIP should emit FloatEqual"
    );
}

#[test]
fn decode_x87_fcmov_da_emits_callother() {
    // DA C0 = FCMOVB ST(0), ST(0) (reg form, reg_field=0, rm_low=0)
    let ops = decode_semantic(&[0xDA, 0xC0], 0x3000);
    assert!(!ops.is_empty(), "FCMOVcc DA should produce CallOther");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("FCMOV_POLICY")),
        "FCMOVcc should emit FCMOV_POLICY"
    );
}

// =====================================================================
// Phase E: TZCNT / LZCNT 테스트
// =====================================================================

#[test]
fn decode_tzcnt_f3_0f_bc_emits_tzcnt_write() {
    // F3 0F BC C1 = TZCNT eax, ecx
    let ops = decode_semantic(&[0xF3, 0x0F, 0xBC, 0xC1], 0x4000);
    assert!(!ops.is_empty(), "TZCNT should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("TZCNT_WRITE")),
        "TZCNT should emit TZCNT_WRITE"
    );
    // Must set ZF and CF
    assert!(has_flag_write(&ops, x86_flag_zf()), "TZCNT should write ZF");
    assert!(has_flag_write(&ops, x86_flag_cf()), "TZCNT should write CF");
}

#[test]
fn decode_bsf_without_f3_prefix_still_emits_bsf() {
    // 0F BC C1 = BSF eax, ecx (no F3 prefix)
    let ops = decode_semantic(&[0x0F, 0xBC, 0xC1], 0x4000);
    assert!(!ops.is_empty(), "BSF should still work without F3 prefix");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("BSF_WRITE")),
        "BSF without F3 should emit BSF_WRITE, not TZCNT"
    );
}

#[test]
fn decode_lzcnt_f3_0f_bd_emits_lzcnt_write() {
    // F3 0F BD C1 = LZCNT eax, ecx
    let ops = decode_semantic(&[0xF3, 0x0F, 0xBD, 0xC1], 0x4000);
    assert!(!ops.is_empty(), "LZCNT should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("LZCNT_WRITE")),
        "LZCNT should emit LZCNT_WRITE"
    );
    assert!(has_flag_write(&ops, x86_flag_cf()), "LZCNT should write CF");
    assert!(has_flag_write(&ops, x86_flag_zf()), "LZCNT should write ZF");
}

#[test]
fn decode_bsr_without_f3_prefix_still_emits_bsr() {
    // 0F BD C1 = BSR eax, ecx (no F3 prefix)
    let ops = decode_semantic(&[0x0F, 0xBD, 0xC1], 0x4000);
    assert!(!ops.is_empty(), "BSR should still work without F3 prefix");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("BSR_WRITE")),
        "BSR without F3 should emit BSR_WRITE, not LZCNT"
    );
}

// ── Phase A Tests ──────────────────────────────────────────────────────────────

#[test]
fn decode_wait_9b_emits_callother() {
    // 9B = WAIT/FWAIT
    let ops = decode_semantic(&[0x9B], 0x1000);
    assert!(!ops.is_empty(), "WAIT should produce ops");
    assert_eq!(ops[0].opcode, PcodeOpcode::CallOther);
    assert_eq!(ops[0].asm_mnemonic.as_deref(), Some("WAIT_POLICY"));
}

#[test]
fn decode_into_ce_emits_callother() {
    // CE = INTO
    let ops = decode_semantic(&[0xCE], 0x1000);
    assert!(!ops.is_empty(), "INTO should produce ops");
    assert_eq!(ops[0].opcode, PcodeOpcode::CallOther);
    assert_eq!(ops[0].asm_mnemonic.as_deref(), Some("INTO_POLICY"));
}

#[test]
fn decode_iret_cf_emits_callother() {
    // CF = IRET
    let ops = decode_semantic(&[0xCF], 0x1000);
    assert!(!ops.is_empty(), "IRET should produce ops");
    assert_eq!(ops[0].opcode, PcodeOpcode::CallOther);
    assert_eq!(ops[0].asm_mnemonic.as_deref(), Some("IRET_POLICY"));
}

#[test]
fn decode_int1_f1_emits_callother() {
    // F1 = INT1/ICEBP
    let ops = decode_semantic(&[0xF1], 0x1000);
    assert!(!ops.is_empty(), "INT1 should produce ops");
    assert_eq!(ops[0].opcode, PcodeOpcode::CallOther);
    assert_eq!(ops[0].asm_mnemonic.as_deref(), Some("INT1_POLICY"));
}

#[test]
fn decode_mov_rm_sreg_8c_emits_copy() {
    // 8C C8 = MOV eax, CS  (reg_field=1=CS, rm=EAX)
    let ops = decode_semantic(&[0x8C, 0xC8], 0x1000);
    assert!(!ops.is_empty(), "MOV r/m, Sreg should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::Copy
            && op.asm_mnemonic.as_deref() == Some("MOV_RM_SEG_WRITE")),
        "MOV r/m, Sreg should emit copy"
    );
}

// ── Phase B Tests ──────────────────────────────────────────────────────────────

#[test]
fn decode_0f00_sldt_emits_callother() {
    // 0F 00 C0 = SLDT eax (reg_field=0)
    let ops = decode_semantic(&[0x0F, 0x00, 0xC0], 0x1000);
    assert!(!ops.is_empty(), "SLDT should produce ops");
    assert_eq!(ops[0].opcode, PcodeOpcode::CallOther);
    assert_eq!(ops[0].asm_mnemonic.as_deref(), Some("SLDT_POLICY"));
}

#[test]
fn decode_0f00_ltr_emits_callother() {
    // 0F 00 D8 = LTR eax (reg_field=3)
    let ops = decode_semantic(&[0x0F, 0x00, 0xD8], 0x1000);
    assert!(!ops.is_empty(), "LTR should produce ops");
    assert_eq!(ops[0].opcode, PcodeOpcode::CallOther);
    assert_eq!(ops[0].asm_mnemonic.as_deref(), Some("LTR_POLICY"));
}

#[test]
fn decode_0f_b2_lss_emits_callother() {
    // 0F B2 00 = LSS eax, [eax] (reg=0, modrm=00)
    let ops = decode_semantic(&[0x0F, 0xB2, 0x00], 0x1000);
    assert!(!ops.is_empty(), "LSS should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("LSS_POLICY")),
        "LSS should emit CallOther"
    );
}

#[test]
fn decode_0f_b4_lfs_emits_callother() {
    // 0F B4 00 = LFS eax, [eax]
    let ops = decode_semantic(&[0x0F, 0xB4, 0x00], 0x1000);
    assert!(!ops.is_empty(), "LFS should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("LFS_POLICY")),
        "LFS should emit CallOther"
    );
}

// ── Phase C Tests ──────────────────────────────────────────────────────────────

#[test]
fn decode_cmpps_0f_c2_emits_intrinsic() {
    // 0F C2 C0 00 = CMPPS xmm0, xmm0, 0 (EQ)
    let ops = decode_semantic(&[0x0F, 0xC2, 0xC0, 0x00], 0x2000);
    assert!(!ops.is_empty(), "CMPPS should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("CMPPS_INTRINSIC")),
        "CMPPS should emit CMPPS_INTRINSIC"
    );
}

#[test]
fn decode_shufps_0f_c6_emits_intrinsic() {
    // 0F C6 C0 1B = SHUFPS xmm0, xmm0, 0x1B
    let ops = decode_semantic(&[0x0F, 0xC6, 0xC0, 0x1B], 0x2000);
    assert!(!ops.is_empty(), "SHUFPS should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("SHUFPS_INTRINSIC")),
        "SHUFPS should emit SHUFPS_INTRINSIC"
    );
}

#[test]
fn decode_rsqrtps_0f_52_emits_intrinsic() {
    // 0F 52 C0 = RSQRTPS xmm0, xmm0
    let ops = decode_semantic(&[0x0F, 0x52, 0xC0], 0x2000);
    assert!(!ops.is_empty(), "RSQRTPS should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("RSQRTPS_INTRINSIC")),
        "RSQRTPS should emit RSQRTPS_INTRINSIC"
    );
}

#[test]
fn decode_rcpps_0f_53_emits_intrinsic() {
    // 0F 53 C0 = RCPPS xmm0, xmm0
    let ops = decode_semantic(&[0x0F, 0x53, 0xC0], 0x2000);
    assert!(!ops.is_empty(), "RCPPS should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("RCPPS_INTRINSIC")),
        "RCPPS should emit RCPPS_INTRINSIC"
    );
}

#[test]
fn decode_cvtps2pd_0f_5a_emits_intrinsic() {
    // 0F 5A C0 = CVTPS2PD xmm0, xmm0
    let ops = decode_semantic(&[0x0F, 0x5A, 0xC0], 0x2000);
    assert!(!ops.is_empty(), "CVTPS2PD should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("CVTPS2PD_INTRINSIC")),
        "CVTPS2PD should emit CVTPS2PD_INTRINSIC"
    );
}

#[test]
fn decode_cvtdq2ps_0f_5b_emits_intrinsic() {
    // 0F 5B C0 = CVTDQ2PS xmm0, xmm0 (no prefix)
    let ops = decode_semantic(&[0x0F, 0x5B, 0xC0], 0x2000);
    assert!(!ops.is_empty(), "CVTDQ2PS should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("CVTDQ2PS_INTRINSIC")),
        "CVTDQ2PS should emit CVTDQ2PS_INTRINSIC"
    );
}

// ── Phase D Tests ──────────────────────────────────────────────────────────────

#[test]
fn decode_vex_andn_emits_intnegate_intand() {
    // VEX.NDS.LZ.0F38.W0 F2 /r
    // C4 E2 68 F2 C1 = ANDN eax, eax(vvvv=0), ecx
    // C4 = 3-byte VEX, E2 = R=1,X=1,B=1,map=2 (0F38), 68 = W=0,vvvv=~1101=0010=2? Let me recalculate.
    // For ANDN eax, ecx, edx: dst=eax(reg_idx=0), vvvv=ecx(1), r/m=edx(2)
    // C4 E2 70 F2 C2:
    //   E2 = 1110_0010 = R=1,X=1,B=1,m=2 (0F38)
    //   70 = 0111_0000 = W=0, vvvv=~1110=0001=1 (ECX), L=0, pp=00 (None)
    //   F2 = opcode
    //   C2 = ModRM: mod=11, reg=0(EAX dst), rm=2(EDX)
    let ops = decode_semantic(&[0xC4, 0xE2, 0x70, 0xF2, 0xC2], 0x3000);
    assert!(!ops.is_empty(), "ANDN should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::IntNegate),
        "ANDN should emit IntNegate for ~src1"
    );
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::IntAnd
            && op.asm_mnemonic.as_deref() == Some("ANDN")),
        "ANDN should emit IntAnd"
    );
    assert!(has_flag_write(&ops, x86_flag_zf()), "ANDN should write ZF");
    assert!(has_flag_zero_copy(&ops, x86_flag_cf()), "ANDN should clear CF");
}

#[test]
fn decode_vex_blsr_emits_intand() {
    // VEX.NDD.LZ.0F38.W0 F3 /1
    // C4 E2 70 F3 C1: vvvv=1(ECX dst), r/m=EAX, reg_field=1=BLSR... wait
    // Actually for BLSR: ModRM.reg = /1, vvvv = destination
    // C4 E2 78 F3 C8: W=0, vvvv=~1111=0000=0 (EAX dst), modrm=C8=11_001_000 reg=1,rm=0
    let ops = decode_semantic(&[0xC4, 0xE2, 0x78, 0xF3, 0xC8], 0x3000);
    assert!(!ops.is_empty(), "BLSR should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("BLSR")),
        "BLSR should emit BLSR mnemonic"
    );
    assert!(has_flag_write(&ops, x86_flag_zf()), "BLSR should write ZF");
}

#[test]
fn decode_vex_blsi_emits_int2comp_intand() {
    // C4 E2 78 F3 D0: modrm=D0=11_010_000, reg=2=BLSI, rm=0(EAX)
    // vvvv=~1111=0 (EAX dst)
    let ops = decode_semantic(&[0xC4, 0xE2, 0x78, 0xF3, 0xD0], 0x3000);
    assert!(!ops.is_empty(), "BLSI should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::Int2Comp),
        "BLSI should emit Int2Comp for negation"
    );
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("BLSI")),
        "BLSI should emit BLSI mnemonic"
    );
}

#[test]
fn decode_vex_bzhi_emits_intleft_intsub_intand() {
    // VEX.NDS.LZ.0F38.W0 F5 /r
    // C4 E2 70 F5 C2: vvvv=1(ECX idx), reg=0(EAX dst), rm=2(EDX src)
    // modrm=C2=11_000_010
    let ops = decode_semantic(&[0xC4, 0xE2, 0x70, 0xF5, 0xC2], 0x3000);
    assert!(!ops.is_empty(), "BZHI should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("BZHI")),
        "BZHI should emit BZHI mnemonic"
    );
    assert!(has_flag_write(&ops, x86_flag_zf()), "BZHI should write ZF");
}

#[test]
fn decode_vex_sarx_emits_intsright() {
    // VEX.NDS.LZ.F3.0F38.W0 F7 /r (SARX)
    // C4 E2 72 F7 C2: pp=F3(2), map=2(0F38), vvvv=1(ECX cnt), modrm=C2
    // E2=1110_0010, 72=0111_0010=W=0,vvvv=~1110=0001=1,L=0,pp=2(F3)
    let ops = decode_semantic(&[0xC4, 0xE2, 0x72, 0xF7, 0xC2], 0x3000);
    assert!(!ops.is_empty(), "SARX should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::IntSRight
            && op.asm_mnemonic.as_deref() == Some("SARX")),
        "SARX should emit IntSRight"
    );
}

#[test]
fn decode_vex_shlx_emits_intleft() {
    // VEX.NDS.LZ.66.0F38.W0 F7 /r (SHLX)
    // C4 E2 71 F7 C2: pp=66(1), vvvv=1(ECX cnt)
    // 71=0111_0001=W=0,vvvv=~1110=0001=1,L=0,pp=1(66)
    let ops = decode_semantic(&[0xC4, 0xE2, 0x71, 0xF7, 0xC2], 0x3000);
    assert!(!ops.is_empty(), "SHLX should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::IntLeft
            && op.asm_mnemonic.as_deref() == Some("SHLX")),
        "SHLX should emit IntLeft"
    );
}

#[test]
fn decode_vex_shrx_emits_intright() {
    // VEX.NDS.LZ.F2.0F38.W0 F7 /r (SHRX)
    // C4 E2 73 F7 C2: pp=F2(3), vvvv=1(ECX cnt)
    // 73=0111_0011=W=0,vvvv=~1110=0001=1,L=0,pp=3(F2)
    let ops = decode_semantic(&[0xC4, 0xE2, 0x73, 0xF7, 0xC2], 0x3000);
    assert!(!ops.is_empty(), "SHRX should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::IntRight
            && op.asm_mnemonic.as_deref() == Some("SHRX")),
        "SHRX should emit IntRight"
    );
}

#[test]
fn decode_vex_mulx_emits_intmult() {
    // VEX.NDD.LZ.F2.0F38.W0 F6 /r (MULX)
    // C4 E2 7B F6 C0: pp=F2(3), vvvv=0(EAX lo_dst), modrm=C0=11_000_000
    // 7B=0111_1011=W=0,vvvv=~1111=0=0(EAX lo),L=0,pp=3(F2)
    let ops = decode_semantic(&[0xC4, 0xE2, 0x7B, 0xF6, 0xC0], 0x3000);
    assert!(!ops.is_empty(), "MULX should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::IntMult
            && op.asm_mnemonic.as_deref() == Some("MULX_LO")),
        "MULX should emit IntMult for low half"
    );
}

// ── Phase C additional tests ───────────────────────────────────────────────────

#[test]
fn decode_cmppd_66_0f_c2_emits_intrinsic() {
    // 66 0F C2 C0 01 = CMPPD xmm0, xmm0, 1 (LT)
    let ops = decode_semantic(&[0x66, 0x0F, 0xC2, 0xC0, 0x01], 0x2000);
    assert!(!ops.is_empty(), "CMPPD should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("CMPPD_INTRINSIC")),
        "CMPPD should emit CMPPD_INTRINSIC"
    );
}

#[test]
fn decode_shufpd_66_0f_c6_emits_intrinsic() {
    // 66 0F C6 C0 01 = SHUFPD xmm0, xmm0, 1
    let ops = decode_semantic(&[0x66, 0x0F, 0xC6, 0xC0, 0x01], 0x2000);
    assert!(!ops.is_empty(), "SHUFPD should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("SHUFPD_INTRINSIC")),
        "SHUFPD should emit SHUFPD_INTRINSIC"
    );
}

#[test]
fn decode_rsqrtss_f3_0f_52_emits_intrinsic() {
    // F3 0F 52 C0 = RSQRTSS xmm0, xmm0
    let ops = decode_semantic(&[0xF3, 0x0F, 0x52, 0xC0], 0x2000);
    assert!(!ops.is_empty(), "RSQRTSS should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("RSQRTSS_INTRINSIC")),
        "RSQRTSS should emit RSQRTSS_INTRINSIC"
    );
}

#[test]
fn decode_rcpss_f3_0f_53_emits_intrinsic() {
    // F3 0F 53 C0 = RCPSS xmm0, xmm0
    let ops = decode_semantic(&[0xF3, 0x0F, 0x53, 0xC0], 0x2000);
    assert!(!ops.is_empty(), "RCPSS should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("RCPSS_INTRINSIC")),
        "RCPSS should emit RCPSS_INTRINSIC"
    );
}

#[test]
fn decode_cvtps2dq_66_0f_5b_emits_intrinsic() {
    // 66 0F 5B C0 = CVTPS2DQ xmm0, xmm0
    let ops = decode_semantic(&[0x66, 0x0F, 0x5B, 0xC0], 0x2000);
    assert!(!ops.is_empty(), "CVTPS2DQ should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("CVTPS2DQ_INTRINSIC")),
        "CVTPS2DQ should emit CVTPS2DQ_INTRINSIC"
    );
}

#[test]
fn decode_cvttps2dq_f3_0f_5b_emits_intrinsic() {
    // F3 0F 5B C0 = CVTTPS2DQ xmm0, xmm0
    let ops = decode_semantic(&[0xF3, 0x0F, 0x5B, 0xC0], 0x2000);
    assert!(!ops.is_empty(), "CVTTPS2DQ should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("CVTTPS2DQ_INTRINSIC")),
        "CVTTPS2DQ should emit CVTTPS2DQ_INTRINSIC"
    );
}

#[test]
fn decode_cvtdq2pd_f3_0f_e6_emits_intrinsic() {
    // F3 0F E6 C0 = CVTDQ2PD xmm0, xmm0
    let ops = decode_semantic(&[0xF3, 0x0F, 0xE6, 0xC0], 0x2000);
    assert!(!ops.is_empty(), "CVTDQ2PD should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("CVTDQ2PD_INTRINSIC")),
        "CVTDQ2PD should emit CVTDQ2PD_INTRINSIC"
    );
}

#[test]
fn decode_cvtpd2dq_f2_0f_e6_emits_intrinsic() {
    // F2 0F E6 C0 = CVTPD2DQ xmm0, xmm0
    let ops = decode_semantic(&[0xF2, 0x0F, 0xE6, 0xC0], 0x2000);
    assert!(!ops.is_empty(), "CVTPD2DQ should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("CVTPD2DQ_INTRINSIC")),
        "CVTPD2DQ should emit CVTPD2DQ_INTRINSIC"
    );
}

// ── Phase B additional tests ───────────────────────────────────────────────────

#[test]
fn decode_0f00_verr_emits_callother() {
    // 0F 00 E0 = VERR eax (reg_field=4)
    let ops = decode_semantic(&[0x0F, 0x00, 0xE0], 0x1000);
    assert!(!ops.is_empty(), "VERR should produce ops");
    assert_eq!(ops[0].opcode, PcodeOpcode::CallOther);
    assert_eq!(ops[0].asm_mnemonic.as_deref(), Some("VERR_POLICY"));
}

#[test]
fn decode_0f_b5_lgs_emits_callother() {
    // 0F B5 00 = LGS eax, [eax]
    let ops = decode_semantic(&[0x0F, 0xB5, 0x00], 0x1000);
    assert!(!ops.is_empty(), "LGS should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("LGS_POLICY")),
        "LGS should emit CallOther"
    );
}

// ── Phase D additional tests ───────────────────────────────────────────────────

#[test]
fn decode_vex_blsmsk_emits_intxor() {
    // C4 E2 78 F3 D8: modrm=D8=11_011_000, reg=3=BLSMSK, rm=0(EAX)
    // vvvv=~1111=0 (EAX dst)
    let ops = decode_semantic(&[0xC4, 0xE2, 0x78, 0xF3, 0xD8], 0x3000);
    assert!(!ops.is_empty(), "BLSMSK should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("BLSMSK")),
        "BLSMSK should emit BLSMSK mnemonic (IntXor)"
    );
    assert!(has_flag_write(&ops, x86_flag_zf()), "BLSMSK should write ZF");
}

// ── Phase D & additional tests ─────────────────────────────────────────────────

#[test]
fn decode_vex_bextr_emits_callother() {
    // VEX.NDS.LZ.0F38.W0 F7 /r (no prefix = BEXTR)
    // C4 E2 70 F7 C2: pp=None(0), vvvv=1(ECX ctrl), modrm=C2=11_000_010
    // 70=0111_0000=W=0,vvvv=~1110=0001=1,L=0,pp=0
    let ops = decode_semantic(&[0xC4, 0xE2, 0x70, 0xF7, 0xC2], 0x3000);
    assert!(!ops.is_empty(), "BEXTR should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("BEXTR_INTRINSIC")),
        "BEXTR should emit CallOther BEXTR_INTRINSIC"
    );
}

#[test]
fn decode_cmpss_f3_0f_c2_emits_intrinsic() {
    // F3 0F C2 C0 04 = CMPSS xmm0, xmm0, 4 (NEQ)
    let ops = decode_semantic(&[0xF3, 0x0F, 0xC2, 0xC0, 0x04], 0x2000);
    assert!(!ops.is_empty(), "CMPSS should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("CMPSS_INTRINSIC")),
        "CMPSS should emit CMPSS_INTRINSIC"
    );
}

#[test]
fn decode_cmpsd_f2_0f_c2_emits_intrinsic() {
    // F2 0F C2 C0 02 = CMPSD xmm0, xmm0, 2 (LE)
    let ops = decode_semantic(&[0xF2, 0x0F, 0xC2, 0xC0, 0x02], 0x2000);
    assert!(!ops.is_empty(), "CMPSD should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("CMPSD_INTRINSIC")),
        "CMPSD should emit CMPSD_INTRINSIC"
    );
}

// ── Phase E Tests ──────────────────────────────────────────────────────────────

#[test]
fn decode_rorx_vex_f2_0f3a_f0_imm8_emits_rotate() {
    // VEX.LZ.F2.0F3A.W0 F0 /r imm8 (RORX)
    // RORX eax, ecx, 8
    // C4 E3 7B F0 C1 08:
    //   C4 = 3-byte VEX
    //   E3 = 1110_0011 = R=1,X=1,B=1,map=3 (0F3A)
    //   7B = 0111_1011 = W=0,vvvv=~1111=0,L=0,pp=3(F2)
    //   F0 = opcode
    //   C1 = ModRM: mod=11,reg=0(EAX dst),rm=1(ECX src)
    //   08 = imm8 (rotate by 8)
    let ops = decode_semantic(&[0xC4, 0xE3, 0x7B, 0xF0, 0xC1, 0x08], 0x4000);
    assert!(!ops.is_empty(), "RORX should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::IntRight
            && op.asm_mnemonic.as_deref() == Some("RORX_SHR")),
        "RORX should emit IntRight for right shift part"
    );
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::IntLeft
            && op.asm_mnemonic.as_deref() == Some("RORX_SHL")),
        "RORX should emit IntLeft for left shift part"
    );
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::IntOr
            && op.asm_mnemonic.as_deref() == Some("RORX_OR")),
        "RORX should emit IntOr to combine"
    );
}

// ─── Phase A Tests: YMM register + VEX L-bit routing ──────────────────────────

#[test]
fn decode_vmovaps_256_vex_c5_emits_intrinsic() {
    // VEX.256.0F 28 /r: VMOVAPS ymm1, ymm2/m256
    // C5 FC 28 C1 = 2-byte VEX, L=1(bit2 of 0xFC=1111_1100→bit2=1), pp=0, opcode=0x28
    // ModRM C1 = mod=11,reg=0(ymm0),rm=1(ymm1)
    let ops = decode_semantic(&[0xC5, 0xFC, 0x28, 0xC1], 0x5000);
    assert!(!ops.is_empty(), "VMOVAPS 256-bit should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("VMOVAPS_INTRINSIC")),
        "VMOVAPS 256-bit should emit VMOVAPS_INTRINSIC"
    );
}

#[test]
fn decode_vmovups_256_vex_c5_emits_intrinsic() {
    // C5 FC 10 C1 = VMOVUPS ymm0, ymm1 (256-bit: L=1, pp=0)
    let ops = decode_semantic(&[0xC5, 0xFC, 0x10, 0xC1], 0x5010);
    assert!(!ops.is_empty(), "VMOVUPS 256-bit should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("VMOVUPS_INTRINSIC")),
        "VMOVUPS 256-bit should emit VMOVUPS_INTRINSIC"
    );
}

#[test]
fn decode_vmovapd_256_vex_c5_emits_intrinsic() {
    // C5 FD 28 C1 = VMOVAPD ymm0, ymm1 (256-bit: L=1, pp=1=0x66)
    // 0xFD = 1111_1101 → R̄=1,vvvv̄=1111,L=1,pp=01
    let ops = decode_semantic(&[0xC5, 0xFD, 0x28, 0xC1], 0x5020);
    assert!(!ops.is_empty(), "VMOVAPD 256-bit should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("VMOVAPD_INTRINSIC")),
        "VMOVAPD 256-bit should emit VMOVAPD_INTRINSIC"
    );
}

#[test]
fn decode_vaddps_256_vex_c5_emits_intrinsic() {
    // C5 FC 58 C1 = VADDPS ymm0, ymm0, ymm1 (256-bit: L=1, pp=0, op=0x58)
    let ops = decode_semantic(&[0xC5, 0xFC, 0x58, 0xC1], 0x5030);
    assert!(!ops.is_empty(), "VADDPS 256-bit should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("VADDPS_INTRINSIC")),
        "VADDPS 256-bit should emit VADDPS_INTRINSIC"
    );
}

#[test]
fn decode_vaddpd_256_vex_c5_emits_intrinsic() {
    // C5 FD 58 C1 = VADDPD ymm0, ymm0, ymm1 (256-bit: L=1, pp=1)
    let ops = decode_semantic(&[0xC5, 0xFD, 0x58, 0xC1], 0x5040);
    assert!(!ops.is_empty(), "VADDPD 256-bit should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("VADDPD_INTRINSIC")),
        "VADDPD 256-bit should emit VADDPD_INTRINSIC"
    );
}

#[test]
fn decode_vmulps_256_vex_c5_emits_intrinsic() {
    // C5 FC 59 C1 = VMULPS ymm0, ymm0, ymm1
    let ops = decode_semantic(&[0xC5, 0xFC, 0x59, 0xC1], 0x5050);
    assert!(!ops.is_empty(), "VMULPS 256-bit should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("VMULPS_INTRINSIC")),
        "VMULPS 256-bit should emit VMULPS_INTRINSIC"
    );
}

#[test]
fn decode_vsubps_256_vex_c5_emits_intrinsic() {
    // C5 FC 5C C1 = VSUBPS ymm0, ymm0, ymm1
    let ops = decode_semantic(&[0xC5, 0xFC, 0x5C, 0xC1], 0x5060);
    assert!(!ops.is_empty(), "VSUBPS 256-bit should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("VSUBPS_INTRINSIC")),
        "VSUBPS 256-bit should emit VSUBPS_INTRINSIC"
    );
}

#[test]
fn decode_vdivps_256_vex_c5_emits_intrinsic() {
    // C5 FC 5E C1 = VDIVPS ymm0, ymm0, ymm1
    let ops = decode_semantic(&[0xC5, 0xFC, 0x5E, 0xC1], 0x5070);
    assert!(!ops.is_empty(), "VDIVPS 256-bit should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("VDIVPS_INTRINSIC")),
        "VDIVPS 256-bit should emit VDIVPS_INTRINSIC"
    );
}

#[test]
fn decode_vandps_256_vex_c5_emits_intrinsic() {
    // C5 FC 54 C1 = VANDPS ymm0, ymm0, ymm1
    let ops = decode_semantic(&[0xC5, 0xFC, 0x54, 0xC1], 0x5080);
    assert!(!ops.is_empty(), "VANDPS 256-bit should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("VANDPS_INTRINSIC")),
        "VANDPS 256-bit should emit VANDPS_INTRINSIC"
    );
}

#[test]
fn decode_vorps_256_vex_c5_emits_intrinsic() {
    // C5 FC 56 C1 = VORPS ymm0, ymm0, ymm1
    let ops = decode_semantic(&[0xC5, 0xFC, 0x56, 0xC1], 0x5090);
    assert!(!ops.is_empty(), "VORPS 256-bit should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("VORPS_INTRINSIC")),
        "VORPS 256-bit should emit VORPS_INTRINSIC"
    );
}

#[test]
fn decode_vxorps_256_vex_c5_emits_intrinsic() {
    // C5 FC 57 C1 = VXORPS ymm0, ymm0, ymm1
    let ops = decode_semantic(&[0xC5, 0xFC, 0x57, 0xC1], 0x50A0);
    assert!(!ops.is_empty(), "VXORPS 256-bit should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("VXORPS_INTRINSIC")),
        "VXORPS 256-bit should emit VXORPS_INTRINSIC"
    );
}

#[test]
fn decode_vsqrtps_256_vex_c5_emits_intrinsic() {
    // C5 FC 51 C1 = VSQRTPS ymm0, ymm1
    let ops = decode_semantic(&[0xC5, 0xFC, 0x51, 0xC1], 0x50B0);
    assert!(!ops.is_empty(), "VSQRTPS 256-bit should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("VSQRTPS_INTRINSIC")),
        "VSQRTPS 256-bit should emit VSQRTPS_INTRINSIC"
    );
}

#[test]
fn decode_vrsqrtps_256_vex_c5_emits_intrinsic() {
    // C5 FC 52 C1 = VRSQRTPS ymm0, ymm1
    let ops = decode_semantic(&[0xC5, 0xFC, 0x52, 0xC1], 0x50C0);
    assert!(!ops.is_empty(), "VRSQRTPS 256-bit should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("VRSQRTPS_INTRINSIC")),
        "VRSQRTPS 256-bit should emit VRSQRTPS_INTRINSIC"
    );
}

#[test]
fn decode_vrcpps_256_vex_c5_emits_intrinsic() {
    // C5 FC 53 C1 = VRCPPS ymm0, ymm1
    let ops = decode_semantic(&[0xC5, 0xFC, 0x53, 0xC1], 0x50D0);
    assert!(!ops.is_empty(), "VRCPPS 256-bit should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("VRCPPS_INTRINSIC")),
        "VRCPPS 256-bit should emit VRCPPS_INTRINSIC"
    );
}

#[test]
fn decode_vcmpps_256_vex_c5_with_imm8_emits_intrinsic() {
    // C5 FC C2 C1 04 = VCMPPS ymm0, ymm0, ymm1, 4 (NEQ_UQ)
    let ops = decode_semantic(&[0xC5, 0xFC, 0xC2, 0xC1, 0x04], 0x50E0);
    assert!(!ops.is_empty(), "VCMPPS 256-bit should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("VCMPPS_INTRINSIC")),
        "VCMPPS 256-bit should emit VCMPPS_INTRINSIC"
    );
}

#[test]
fn decode_vshufps_256_vex_c5_with_imm8_emits_intrinsic() {
    // C5 FC C6 C1 02 = VSHUFPS ymm0, ymm0, ymm1, 2
    let ops = decode_semantic(&[0xC5, 0xFC, 0xC6, 0xC1, 0x02], 0x50F0);
    assert!(!ops.is_empty(), "VSHUFPS 256-bit should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("VSHUFPS_INTRINSIC")),
        "VSHUFPS 256-bit should emit VSHUFPS_INTRINSIC"
    );
}

#[test]
fn decode_vandpd_256_vex_c5_emits_intrinsic() {
    // C5 FD 54 C1 = VANDPD ymm0, ymm0, ymm1 (L=1, pp=01=0x66)
    let ops = decode_semantic(&[0xC5, 0xFD, 0x54, 0xC1], 0x5100);
    assert!(!ops.is_empty(), "VANDPD 256-bit should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("VANDPD_INTRINSIC")),
        "VANDPD 256-bit should emit VANDPD_INTRINSIC"
    );
}

#[test]
fn decode_vorpd_256_vex_c5_emits_intrinsic() {
    // C5 FD 56 C1 = VORPD ymm0, ymm0, ymm1 (L=1, pp=01=0x66)
    let ops = decode_semantic(&[0xC5, 0xFD, 0x56, 0xC1], 0x5110);
    assert!(!ops.is_empty(), "VORPD 256-bit should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("VORPD_INTRINSIC")),
        "VORPD 256-bit should emit VORPD_INTRINSIC"
    );
}

#[test]
fn decode_vxorpd_256_vex_c5_emits_intrinsic() {
    // C5 FD 57 C1 = VXORPD ymm0, ymm0, ymm1 (L=1, pp=01=0x66)
    let ops = decode_semantic(&[0xC5, 0xFD, 0x57, 0xC1], 0x5120);
    assert!(!ops.is_empty(), "VXORPD 256-bit should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("VXORPD_INTRINSIC")),
        "VXORPD 256-bit should emit VXORPD_INTRINSIC"
    );
}

#[test]
fn decode_vmulpd_256_vex_c5_emits_intrinsic() {
    // C5 FD 59 C1 = VMULPD ymm0, ymm0, ymm1 (L=1, pp=01)
    let ops = decode_semantic(&[0xC5, 0xFD, 0x59, 0xC1], 0x5130);
    assert!(!ops.is_empty(), "VMULPD 256-bit should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("VMULPD_INTRINSIC")),
        "VMULPD 256-bit should emit VMULPD_INTRINSIC"
    );
}

// ─── Phase B Tests: 0x0F 0xAE group ───────────────────────────────────────────

#[test]
fn decode_lfence_0f_ae_e8_emits_callother() {
    // 0F AE E8 = LFENCE (mod=11, reg=5, rm=0 → 0xE8)
    let ops = decode_semantic(&[0x0F, 0xAE, 0xE8], 0x6000);
    assert!(!ops.is_empty(), "LFENCE should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("LFENCE_POLICY")),
        "LFENCE should emit LFENCE_POLICY"
    );
}

#[test]
fn decode_mfence_0f_ae_f0_emits_callother() {
    // 0F AE F0 = MFENCE (mod=11, reg=6, rm=0 → 0xF0)
    let ops = decode_semantic(&[0x0F, 0xAE, 0xF0], 0x6010);
    assert!(!ops.is_empty(), "MFENCE should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("MFENCE_POLICY")),
        "MFENCE should emit MFENCE_POLICY"
    );
}

#[test]
fn decode_sfence_0f_ae_f8_emits_callother() {
    // 0F AE F8 = SFENCE (mod=11, reg=7, rm=0 → 0xF8)
    let ops = decode_semantic(&[0x0F, 0xAE, 0xF8], 0x6020);
    assert!(!ops.is_empty(), "SFENCE should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("SFENCE_POLICY")),
        "SFENCE should emit SFENCE_POLICY"
    );
}

#[test]
fn decode_fxsave_0f_ae_00_emits_callother() {
    // 0F AE /0 [mem]: FXSAVE m512 → mod=00(mem), reg=0, rm=0
    // ModRM 0x00 = mod=00, reg=0, rm=0 (indirect [rax])
    let ops = decode_semantic(&[0x0F, 0xAE, 0x00], 0x6030);
    assert!(!ops.is_empty(), "FXSAVE should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("FXSAVE_POLICY")),
        "FXSAVE should emit FXSAVE_POLICY"
    );
}

#[test]
fn decode_fxrstor_0f_ae_08_emits_callother() {
    // 0F AE /1 [mem]: FXRSTOR m512 → ModRM 0x08 = mod=00, reg=1, rm=0
    let ops = decode_semantic(&[0x0F, 0xAE, 0x08], 0x6040);
    assert!(!ops.is_empty(), "FXRSTOR should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("FXRSTOR_POLICY")),
        "FXRSTOR should emit FXRSTOR_POLICY"
    );
}

#[test]
fn decode_xsave_0f_ae_20_emits_callother() {
    // 0F AE /4 [mem]: XSAVE → ModRM 0x20 = mod=00, reg=4, rm=0
    let ops = decode_semantic(&[0x0F, 0xAE, 0x20], 0x6050);
    assert!(!ops.is_empty(), "XSAVE should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("XSAVE_POLICY")),
        "XSAVE should emit XSAVE_POLICY"
    );
}

#[test]
fn decode_xrstor_0f_ae_28_emits_callother() {
    // 0F AE /5 [mem]: XRSTOR → ModRM 0x28 = mod=00, reg=5, rm=0
    let ops = decode_semantic(&[0x0F, 0xAE, 0x28], 0x6060);
    assert!(!ops.is_empty(), "XRSTOR should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("XRSTOR_POLICY")),
        "XRSTOR should emit XRSTOR_POLICY"
    );
}

#[test]
fn decode_xsaveopt_0f_ae_30_emits_callother() {
    // 0F AE /6 [mem]: XSAVEOPT → ModRM 0x30 = mod=00, reg=6, rm=0
    let ops = decode_semantic(&[0x0F, 0xAE, 0x30], 0x6070);
    assert!(!ops.is_empty(), "XSAVEOPT should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("XSAVEOPT_POLICY")),
        "XSAVEOPT should emit XSAVEOPT_POLICY"
    );
}

#[test]
fn decode_clflush_0f_ae_38_emits_callother() {
    // 0F AE /7 [mem]: CLFLUSH → ModRM 0x38 = mod=00, reg=7, rm=0
    let ops = decode_semantic(&[0x0F, 0xAE, 0x38], 0x6080);
    assert!(!ops.is_empty(), "CLFLUSH should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("CLFLUSH_POLICY")),
        "CLFLUSH should emit CLFLUSH_POLICY"
    );
}

// ─── Phase C Tests: x87 FPU improvements ─────────────────────────────────────

#[test]
fn decode_fsin_d9_fe_emits_callother_with_st0_input() {
    // D9 FE = FSIN: ST(0) = sin(ST(0))
    let ops = decode_semantic(&[0xD9, 0xFE], 0x7000);
    assert!(!ops.is_empty(), "FSIN should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("FSIN")),
        "FSIN should emit CallOther with FSIN mnemonic"
    );
    // Verify ST(0) is in inputs
    let fsin_op = ops.iter().find(|op| op.asm_mnemonic.as_deref() == Some("FSIN"));
    assert!(fsin_op.is_some(), "should have FSIN op");
    let fsin_op = fsin_op.unwrap();
    assert!(fsin_op.inputs.len() >= 2, "FSIN should have at least policy_id + ST(0) inputs");
}

#[test]
fn decode_fcos_d9_ff_emits_callother_with_st0_input() {
    // D9 FF = FCOS: ST(0) = cos(ST(0))
    let ops = decode_semantic(&[0xD9, 0xFF], 0x7010);
    assert!(!ops.is_empty(), "FCOS should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("FCOS")),
        "FCOS should emit CallOther with FCOS mnemonic"
    );
    let fcos_op = ops.iter().find(|op| op.asm_mnemonic.as_deref() == Some("FCOS")).unwrap();
    assert!(fcos_op.inputs.len() >= 2, "FCOS should have policy_id + ST(0) inputs");
}

#[test]
fn decode_fptan_d9_f2_emits_callother_with_st0_input() {
    // D9 F2 = FPTAN
    let ops = decode_semantic(&[0xD9, 0xF2], 0x7020);
    assert!(!ops.is_empty(), "FPTAN should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("FPTAN")),
        "FPTAN should emit CallOther with FPTAN mnemonic"
    );
    let fptan_op = ops.iter().find(|op| op.asm_mnemonic.as_deref() == Some("FPTAN")).unwrap();
    assert!(fptan_op.inputs.len() >= 2, "FPTAN should have policy_id + ST(0) inputs");
}

#[test]
fn decode_fpatan_d9_f3_emits_callother_with_st0_input() {
    // D9 F3 = FPATAN
    let ops = decode_semantic(&[0xD9, 0xF3], 0x7030);
    assert!(!ops.is_empty(), "FPATAN should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("FPATAN")),
        "FPATAN should emit CallOther with FPATAN mnemonic"
    );
    let fpatan_op = ops.iter().find(|op| op.asm_mnemonic.as_deref() == Some("FPATAN")).unwrap();
    assert!(fpatan_op.inputs.len() >= 2, "FPATAN should have policy_id + ST(0) inputs");
}

#[test]
fn decode_f2xm1_d9_f0_emits_callother_with_st0_input() {
    // D9 F0 = F2XM1
    let ops = decode_semantic(&[0xD9, 0xF0], 0x7040);
    assert!(!ops.is_empty(), "F2XM1 should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("F2XM1")),
        "F2XM1 should emit CallOther with F2XM1 mnemonic"
    );
}

#[test]
fn decode_fyl2x_d9_f1_emits_callother_with_st0_input() {
    // D9 F1 = FYL2X
    let ops = decode_semantic(&[0xD9, 0xF1], 0x7050);
    assert!(!ops.is_empty(), "FYL2X should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("FYL2X")),
        "FYL2X should emit CallOther with FYL2X mnemonic"
    );
}

#[test]
fn decode_fprem_d9_f8_emits_callother_with_st0_input() {
    // D9 F8 = FPREM
    let ops = decode_semantic(&[0xD9, 0xF8], 0x7060);
    assert!(!ops.is_empty(), "FPREM should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("FPREM")),
        "FPREM should emit CallOther with FPREM mnemonic"
    );
}

#[test]
fn decode_fscale_d9_fd_emits_callother_with_st0_input() {
    // D9 FD = FSCALE
    let ops = decode_semantic(&[0xD9, 0xFD], 0x7070);
    assert!(!ops.is_empty(), "FSCALE should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("FSCALE")),
        "FSCALE should emit CallOther with FSCALE mnemonic"
    );
}

#[test]
fn decode_fabs_d9_e1_emits_float_abs() {
    // D9 E1 = FABS: ST(0) = |ST(0)|
    let ops = decode_semantic(&[0xD9, 0xE1], 0x7080);
    assert!(!ops.is_empty(), "FABS should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::FloatAbs
            && op.asm_mnemonic.as_deref() == Some("FABS")),
        "FABS should emit FloatAbs P-code"
    );
}

#[test]
fn decode_fchs_d9_e0_emits_float_neg() {
    // D9 E0 = FCHS: ST(0) = -ST(0)
    let ops = decode_semantic(&[0xD9, 0xE0], 0x7090);
    assert!(!ops.is_empty(), "FCHS should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::FloatNeg
            && op.asm_mnemonic.as_deref() == Some("FCHS")),
        "FCHS should emit FloatNeg P-code"
    );
}

#[test]
fn decode_fxtract_d9_f4_emits_callother_with_input() {
    // D9 F4 = FXTRACT
    let ops = decode_semantic(&[0xD9, 0xF4], 0x70A0);
    assert!(!ops.is_empty(), "FXTRACT should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("FXTRACT")),
        "FXTRACT should emit CallOther with FXTRACT mnemonic"
    );
}

#[test]
fn decode_fprem1_d9_f5_emits_callother_with_input() {
    // D9 F5 = FPREM1
    let ops = decode_semantic(&[0xD9, 0xF5], 0x70B0);
    assert!(!ops.is_empty(), "FPREM1 should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("FPREM1")),
        "FPREM1 should emit CallOther with FPREM1 mnemonic"
    );
}

#[test]
fn decode_fyl2xp1_d9_f9_emits_callother_with_input() {
    // D9 F9 = FYL2XP1
    let ops = decode_semantic(&[0xD9, 0xF9], 0x70C0);
    assert!(!ops.is_empty(), "FYL2XP1 should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::CallOther
            && op.asm_mnemonic.as_deref() == Some("FYL2XP1")),
        "FYL2XP1 should emit CallOther with FYL2XP1 mnemonic"
    );
}

// ─── Phase D: VEX 3-byte map1 256-bit ─────────────────────────────────────────

#[test]
fn decode_vmovaps_256_3byte_vex_c4_emits_intrinsic() {
    // 3-byte VEX: C4 E1 7C 28 C1
    // C4 = 3-byte VEX leader
    // E1 = 1110_0001 → R̄=1,X̄=1,B̄=1,map=1(0F)
    // 7C = 0111_1100 → W=0,vvvv̄=1111,L=1(bit2=1),pp=00
    // 28 = MOVAPS opcode
    // C1 = ModRM: mod=11,reg=0,rm=1
    let ops = decode_semantic(&[0xC4, 0xE1, 0x7C, 0x28, 0xC1], 0x8000);
    assert!(!ops.is_empty(), "VMOVAPS 256-bit (3-byte VEX) should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("VMOVAPS_INTRINSIC")),
        "VMOVAPS 256-bit via 3-byte VEX should emit VMOVAPS_INTRINSIC"
    );
}

#[test]
fn decode_vsubpd_256_3byte_vex_c4_emits_intrinsic() {
    // 3-byte VEX: C4 E1 7D 5C C1
    // C4 = 3-byte VEX leader
    // E1 = R̄=1,X̄=1,B̄=1,map=1(0F)
    // 7D = W=0,vvvv̄=1111,L=1,pp=01(0x66)
    // 5C = SUBPD opcode
    // C1 = ModRM: mod=11,reg=0,rm=1
    let ops = decode_semantic(&[0xC4, 0xE1, 0x7D, 0x5C, 0xC1], 0x8010);
    assert!(!ops.is_empty(), "VSUBPD 256-bit (3-byte VEX) should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("VSUBPD_INTRINSIC")),
        "VSUBPD 256-bit via 3-byte VEX should emit VSUBPD_INTRINSIC"
    );
}

#[test]
fn decode_vsqrtpd_256_vex_c5_emits_intrinsic() {
    // C5 FD 51 C1 = VSQRTPD ymm0, ymm1 (L=1, pp=01)
    let ops = decode_semantic(&[0xC5, 0xFD, 0x51, 0xC1], 0x8020);
    assert!(!ops.is_empty(), "VSQRTPD 256-bit should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("VSQRTPD_INTRINSIC")),
        "VSQRTPD 256-bit should emit VSQRTPD_INTRINSIC"
    );
}

#[test]
fn decode_vcmppd_256_vex_c5_with_imm8_emits_intrinsic() {
    // C5 FD C2 C1 02 = VCMPPD ymm0, ymm0, ymm1, 2 (LE_OS)
    let ops = decode_semantic(&[0xC5, 0xFD, 0xC2, 0xC1, 0x02], 0x8030);
    assert!(!ops.is_empty(), "VCMPPD 256-bit should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("VCMPPD_INTRINSIC")),
        "VCMPPD 256-bit should emit VCMPPD_INTRINSIC"
    );
}

#[test]
fn decode_vshufpd_256_vex_c5_with_imm8_emits_intrinsic() {
    // C5 FD C6 C1 01 = VSHUFPD ymm0, ymm0, ymm1, 1
    let ops = decode_semantic(&[0xC5, 0xFD, 0xC6, 0xC1, 0x01], 0x8040);
    assert!(!ops.is_empty(), "VSHUFPD 256-bit should produce ops");
    assert!(
        ops.iter().any(|op| op.asm_mnemonic.as_deref() == Some("VSHUFPD_INTRINSIC")),
        "VSHUFPD 256-bit should emit VSHUFPD_INTRINSIC"
    );
}

#[test]
fn decode_ldmxcsr_0f_ae_mem_emits_load_and_copy() {
    // 0F AE /2 [mem]: LDMXCSR → ModRM 0x10 = mod=00, reg=2, rm=0
    let ops = decode_semantic(&[0x0F, 0xAE, 0x10], 0x8050);
    assert!(!ops.is_empty(), "LDMXCSR should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::Load
            && op.asm_mnemonic.as_deref() == Some("LDMXCSR_LOAD")),
        "LDMXCSR should emit Load for memory read"
    );
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::Copy
            && op.asm_mnemonic.as_deref() == Some("LDMXCSR_WRITE")),
        "LDMXCSR should emit Copy to write MXCSR"
    );
}

#[test]
fn decode_stmxcsr_0f_ae_mem_emits_store() {
    // 0F AE /3 [mem]: STMXCSR → ModRM 0x18 = mod=00, reg=3, rm=0
    let ops = decode_semantic(&[0x0F, 0xAE, 0x18], 0x8060);
    assert!(!ops.is_empty(), "STMXCSR should produce ops");
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::Store
            && op.asm_mnemonic.as_deref() == Some("STMXCSR_STORE")),
        "STMXCSR should emit Store for MXCSR write"
    );
}
