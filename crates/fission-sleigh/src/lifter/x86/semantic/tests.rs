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
