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

    let clflush_reg = decode_semantic(&[0x0F, 0xAE, 0xF8], 0x728E); // /7 but register form
    assert!(clflush_reg.is_empty());
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

    let simd = decode_semantic(&[0x0F, 0x10, 0xC0], 0x746C);
    assert_eq!(simd.len(), 1);
    assert_eq!(simd[0].asm_mnemonic.as_deref(), Some("SIMD_POLICY"));

    let x87 = decode_semantic(&[0x0F, 0xD8, 0xC0], 0x7470);
    assert_eq!(x87.len(), 1);
    assert_eq!(x87[0].asm_mnemonic.as_deref(), Some("X87_POLICY"));

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
