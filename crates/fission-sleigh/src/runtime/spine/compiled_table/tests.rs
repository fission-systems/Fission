use super::*;
use crate::compiler::{
    compile_frontend_for_entry_spec, compile_x86_64_frontend, discovery, spec_root_for_arch,
    CompiledTemplateSource,
};
use std::path::PathBuf;

macro_rules! require_packaged_ghidra_sla {
    () => {
        if !discovery::ghidra_packaged_sla_available() {
            eprintln!(
                "skip: packaged Ghidra .sla not found (vendor/ghidra layout or FISSION_GHIDRA_DIR)"
            );
            return;
        }
    };
}

fn assert_spec_derived_lift(
    compiled: &CompiledFrontend,
    bytes: &[u8],
    address: u64,
) -> Vec<PcodeOp> {
    let (ops, length, details) = decode_and_lift_with_details(compiled, None, bytes, address)
        .expect("expected SpecDerived .sla template lift");
    assert_eq!(length as usize, bytes.len());
    assert_eq!(
        details.template_source,
        Some(CompiledTemplateSource::SpecDerived)
    );
    assert!(!ops.is_empty(), "spec-derived template emitted no p-code");
    ops
}

#[test]
fn runtime_window_and_length_helpers_fail_closed_on_invalid_widths() {
    assert_eq!(checked_memory_window_offset(0x1000, 0x1003).unwrap(), 3);
    assert!(checked_memory_window_offset(0x1003, 0x1000).is_err());
    if u64::try_from(usize::MAX) != Ok(u64::MAX) {
        assert!(checked_memory_window_offset(0, u64::MAX).is_err());
    }

    assert_eq!(checked_runtime_length_u32(7, "test").unwrap(), 7);
    if usize::BITS > u32::BITS {
        assert!(checked_runtime_length_u32(u32::MAX as usize + 1, "test").is_err());
    }
    assert_eq!(checked_context_commit_handle_index(3).unwrap(), 3);
}

#[test]
fn sla_template_feature_audit_smoke() {
    require_packaged_ghidra_sla!();
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let audit = audit_sla_template_features(&compiled);
    let ctor_count: usize = compiled
        .subtables
        .values()
        .map(|s| s.constructors.len())
        .sum();
    assert!(
        ctor_count > 0,
        "expected at least one executable constructor"
    );
    let _ = audit.opcode_cross_build
        + audit.opcode_delay_slot_indirect
        + audit.const_flow_ref
        + audit.const_flow_dest;
}

#[test]
fn generated_runtime_decodes_ret_with_spec_derived_lift() {
    require_packaged_ghidra_sla!();
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let decoded = decode_instruction(&compiled, None, &[0xC3], 0x1000).expect("generated ret");
    assert_eq!(decoded.length, 1);
    assert!(matches!(decoded.flow_kind, DecodedFlowKind::Return));
    let ops = assert_spec_derived_lift(&compiled, &[0xC3], 0x1000);
    assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::Return));
}

#[test]
fn generated_runtime_decodes_mov_imm64_without_compatibility_lift() {
    require_packaged_ghidra_sla!();
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let bytes = [0x48, 0xB8, 0x34, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    let decoded = decode_instruction(&compiled, None, &bytes, 0x1000).expect("generated mov");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "mov");
    let ops = assert_spec_derived_lift(&compiled, &bytes, 0x1000);
    assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::Copy));
}

#[test]
fn generated_runtime_decodes_jcc_rel8_without_compatibility_lift() {
    require_packaged_ghidra_sla!();
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let decoded =
        decode_instruction(&compiled, None, &[0x75, 0x05], 0x1000).expect("generated jne");
    assert_eq!(decoded.length, 2);
    assert_eq!(decoded.mnemonic, "jnz");
    assert!(matches!(
        decoded.flow_kind,
        DecodedFlowKind::ConditionalJump
    ));
    let ops = assert_spec_derived_lift(&compiled, &[0x75, 0x05], 0x1000);
    assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::CBranch));
}

#[test]
fn generated_runtime_renders_jle_condition_mnemonic_display_only() {
    require_packaged_ghidra_sla!();
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let decoded =
        decode_instruction(&compiled, None, &[0x7e, 0x05], 0x1000).expect("generated jle");
    assert_eq!(decoded.length, 2);
    assert_eq!(decoded.mnemonic, "jle");
    assert!(matches!(
        decoded.flow_kind,
        DecodedFlowKind::ConditionalJump
    ));
    let ops = assert_spec_derived_lift(&compiled, &[0x7e, 0x05], 0x1000);
    assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::CBranch));
}

#[test]
fn generated_runtime_decodes_startup_store_mov_mem32_imm32_without_compatibility_lift() {
    require_packaged_ghidra_sla!();
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let bytes = [0xC7, 0x00, 0x01, 0x00, 0x00, 0x00];
    let decoded =
        decode_instruction(&compiled, None, &bytes, 0x1000).expect("generated mov [rax], imm32");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "mov");
    let (ops, length, details) = decode_and_lift_with_details(&compiled, None, &bytes, 0x1000)
        .expect("lift mov [rax], imm32");
    assert_eq!(length as usize, bytes.len());
    assert_eq!(
        details.template_source,
        Some(CompiledTemplateSource::SpecDerived)
    );
    assert_eq!(ops.len(), 2);
    assert_eq!(ops[0].opcode, PcodeOpcode::Copy);
    assert_eq!(ops[1].opcode, PcodeOpcode::Store);
    assert_eq!(ops[1].inputs[1].space_id, 4);
    assert_eq!(ops[1].inputs[1].offset, 0);
    assert_eq!(ops[1].inputs[1].size, 8);
}

#[test]
fn generated_runtime_decodes_startup_sub_rsp_imm8_without_compatibility_lift() {
    require_packaged_ghidra_sla!();
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let bytes = [0x48, 0x83, 0xEC, 0x28];
    let decoded =
        decode_instruction(&compiled, None, &bytes, 0x1000).expect("generated sub rsp, imm8");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "sub");
    let ops = assert_spec_derived_lift(&compiled, &bytes, 0x1000);
    assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::IntSub));
}

#[test]
fn generated_runtime_decodes_startup_rip_relative_load_without_compatibility_lift() {
    require_packaged_ghidra_sla!();
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let bytes = [0x48, 0x8B, 0x05, 0x15, 0x30, 0x00, 0x00];
    let address = 0x1400_013e4;
    let decoded =
        decode_instruction(&compiled, None, &bytes, address).expect("generated rip-relative mov");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "mov");
    let (ops, length, details) = decode_and_lift_with_details(&compiled, None, &bytes, address)
        .expect("lift rip-relative mov");
    assert_eq!(length as usize, bytes.len());
    assert_eq!(
        details.template_source,
        Some(CompiledTemplateSource::SpecDerived)
    );
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].opcode, PcodeOpcode::Copy);
    assert_eq!(ops[0].inputs[0].space_id, 3);
    assert_eq!(ops[0].inputs[0].offset, 0x1400_04400);
    assert_eq!(ops[0].inputs[0].size, 8);
    assert_eq!(ops[0].output.as_ref().expect("copy output").space_id, 4);
    assert_eq!(ops[0].output.as_ref().expect("copy output").offset, 0);
    assert_eq!(ops[0].output.as_ref().expect("copy output").size, 8);
}

#[test]
fn generated_runtime_decodes_startup_call_rel32_without_compatibility_lift() {
    require_packaged_ghidra_sla!();
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let bytes = [0xE8, 0x1A, 0xFC, 0xFF, 0xFF];
    let decoded =
        decode_instruction(&compiled, None, &bytes, 0x1400_013ef).expect("generated call rel32");
    assert_eq!(decoded.length, bytes.len());
    assert!(matches!(decoded.flow_kind, DecodedFlowKind::Call));
    let ops = assert_spec_derived_lift(&compiled, &bytes, 0x1400_013ef);
    assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::Call));
}

#[test]
fn vendor_x86_pe_c7_moffs_imm32_uses_sla_extents() {
    require_packaged_ghidra_sla!();
    let x86_spec = spec_root_for_arch("x86").join("x86.slaspec");
    let compiled = compile_frontend_for_entry_spec(&x86_spec).expect("compile x86 frontend");
    let bytes = [0xc7, 0x05, 0x34, 0x50, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00];

    let decoded =
        decode_instruction(&compiled, None, &bytes, 0x4014e3).expect("decode mov moffs32, imm32");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "mov");

    let (ops, length, details) = decode_and_lift_with_details(&compiled, None, &bytes, 0x4014e3)
        .expect("lift mov moffs32, imm32");
    assert_eq!(length as usize, bytes.len());
    assert_eq!(
        details.template_source,
        Some(CompiledTemplateSource::SpecDerived)
    );
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].opcode, PcodeOpcode::Copy);
    assert_eq!(ops[0].inputs[0].constant_val, 0);
    assert_eq!(ops[0].inputs[0].size, 4);
    let output = ops[0].output.as_ref().expect("copy output");
    assert_eq!(output.space_id, 3);
    assert_eq!(output.offset, 0x405034);
    assert_eq!(output.size, 4);
}

#[test]
fn vendor_x86_pe_call_rel32_uses_construct_inst_next_extent() {
    require_packaged_ghidra_sla!();
    let x86_spec = spec_root_for_arch("x86").join("x86.slaspec");
    let compiled = compile_frontend_for_entry_spec(&x86_spec).expect("compile x86 frontend");
    let bytes = [0xe8, 0x0e, 0x0d, 0x00, 0x00];

    let decoded = decode_instruction(&compiled, None, &bytes, 0x4014ed).expect("decode call rel32");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "call");

    let (ops, length, details) =
        decode_and_lift_with_details(&compiled, None, &bytes, 0x4014ed).expect("lift call rel32");
    assert_eq!(length as usize, bytes.len());
    assert_eq!(
        details.template_source,
        Some(CompiledTemplateSource::SpecDerived)
    );
    let call = ops
        .iter()
        .find(|op| op.opcode == PcodeOpcode::Call)
        .expect("call p-code op");
    assert_eq!(call.inputs[0].space_id, 3);
    assert_eq!(call.inputs[0].offset, 0x402200);
}

#[test]
fn generated_runtime_records_decision_trace_for_startup_store() {
    require_packaged_ghidra_sla!();
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let ctx = CompiledInstructionContext::parse(&[0xC7, 0x00, 0x01, 0x00, 0x00, 0x00], 0x1000)
        .expect("decode context");
    let selection = select_constructor(&compiled, "instruction", &ctx)
        .expect("constructor selection")
        .expect("constructor match");
    let strategy = RuntimeDecodeStrategy::for_table(&compiled, None, "instruction", &ctx);
    let state = bind_instruction(&compiled, strategy, &ctx, selection).expect("bind instruction");
    assert_eq!(state.match_trace.root_bucket, "instruction");
    assert!(!state.match_trace.probes.is_empty());
    assert!(!state.construct_nodes.is_empty());
    assert!(
        !state.handles.is_empty() || state.exported_handle.is_some(),
        "walker should materialize operand or exported handle state"
    );
}

#[test]
fn generated_runtime_decodes_reg32_lea_without_decode_no_match_or_compatibility_lift() {
    require_packaged_ghidra_sla!();
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let bytes = [0x8d, 0x04, 0x11];
    let decoded = decode_instruction(&compiled, None, &bytes, 0x1400_1450).expect("generated lea");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "lea");
    let (ops, length, details) =
        decode_and_lift_with_details(&compiled, None, &bytes, 0x1400_1450).expect("lift lea");
    assert_eq!(length as usize, bytes.len());
    assert_eq!(
        details.template_source,
        Some(CompiledTemplateSource::SpecDerived)
    );
    assert_eq!(
        ops.iter().map(|op| op.opcode).collect::<Vec<_>>(),
        vec![
            PcodeOpcode::IntMult,
            PcodeOpcode::IntAdd,
            PcodeOpcode::SubPiece,
            PcodeOpcode::IntZExt,
        ]
    );
    assert_eq!(ops[0].inputs[0].space_id, 4);
    assert_eq!(ops[0].inputs[0].offset, 16);
    assert_eq!(ops[0].inputs[1].constant_val, 1);
    assert_eq!(ops[1].inputs[0].space_id, 4);
    assert_eq!(ops[1].inputs[0].offset, 8);
    assert_eq!(ops[2].output.as_ref().expect("subpiece output").space_id, 4);
    assert_eq!(ops[2].output.as_ref().expect("subpiece output").offset, 0);
    assert_eq!(ops[2].output.as_ref().expect("subpiece output").size, 4);
    assert_eq!(ops[3].output.as_ref().expect("zext output").space_id, 4);
    assert_eq!(ops[3].output.as_ref().expect("zext output").offset, 0);
    assert_eq!(ops[3].output.as_ref().expect("zext output").size, 8);
}

#[test]
fn generated_runtime_decodes_lea_negative_displacement_const_without_decode_error() {
    require_packaged_ghidra_sla!();
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let bytes = [0x8d, 0x41, 0xff];
    let (ops, length, details) = decode_and_lift_with_details(&compiled, None, &bytes, 0x1400_148e)
        .expect("lift lea negative displacement");
    assert_eq!(length as usize, bytes.len());
    assert_eq!(
        details.template_source,
        Some(CompiledTemplateSource::SpecDerived)
    );
    assert_eq!(
        ops.iter().map(|op| op.opcode).collect::<Vec<_>>(),
        vec![
            PcodeOpcode::IntAdd,
            PcodeOpcode::SubPiece,
            PcodeOpcode::IntZExt,
        ]
    );
    assert_eq!(ops[0].inputs[1].constant_val, -1);
    assert_eq!(ops[0].inputs[1].offset, u64::MAX);
}

#[test]
fn generated_runtime_decodes_sib_stack_disp8_from_sla_terminal_extent() {
    require_packaged_ghidra_sla!();
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let bytes = [0x48, 0x89, 0x5c, 0x24, 0x08];
    let (ops, length, details) = decode_and_lift_with_details(&compiled, None, &bytes, 0x1800_85d0)
        .expect("lift mov [rsp + disp8], rbx");
    assert_eq!(length as usize, bytes.len());
    assert_eq!(
        details.template_source,
        Some(CompiledTemplateSource::SpecDerived)
    );

    let int_add = ops
        .iter()
        .find(|op| op.opcode == PcodeOpcode::IntAdd)
        .expect("address INT_ADD");
    assert_eq!(
        int_add.inputs[0].constant_val, 8,
        "disp8 must be read after the ModRM+SIB terminal extent, not from the SIB byte"
    );
    assert_eq!(
        ops.iter().map(|op| op.opcode).collect::<Vec<_>>(),
        vec![PcodeOpcode::IntAdd, PcodeOpcode::Copy, PcodeOpcode::Store,],
        "dynamic memory COPY must materialize through the Ghidra temp before STORE"
    );
}

#[test]
fn generated_runtime_decodes_rip_relative_mov32_without_decode_no_match() {
    require_packaged_ghidra_sla!();
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let bytes = [0x8b, 0x05, 0x6a, 0x56, 0x00, 0x00];
    let decoded = decode_instruction(&compiled, None, &bytes, 0x1400_19c0)
        .expect("generated mov rip-relative");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "mov");
    assert!(matches!(
        decoded.references.first().map(|reference| reference.kind),
        Some(DecodedReferenceKind::RipRelativeAddress)
    ));
}

#[test]
fn generated_runtime_decodes_movsxd_without_decode_no_match_or_compatibility_lift() {
    require_packaged_ghidra_sla!();
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let bytes = [0x48, 0x63, 0x41, 0x3c];
    let decoded =
        decode_instruction(&compiled, None, &bytes, 0x1400_2600).expect("generated movsxd");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "movsxd");
    let ops = assert_spec_derived_lift(&compiled, &bytes, 0x1400_2600);
    assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::IntSExt));
}

#[test]
fn generated_runtime_zero_extends_reg32_decode_without_compatibility_lift() {
    require_packaged_ghidra_sla!();
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let bytes = [0x31, 0xc0];
    let decoded =
        decode_instruction(&compiled, None, &bytes, 0x1400_19e0).expect("generated xor eax, eax");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "xor");
    let ops = assert_spec_derived_lift(&compiled, &bytes, 0x1400_19e0);
    assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::IntXor));
}

#[test]
fn generated_runtime_decodes_fninit_without_decode_no_match() {
    require_packaged_ghidra_sla!();
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let bytes = [0xdb, 0xe3];
    let decoded =
        decode_instruction(&compiled, None, &bytes, 0x1400_25c0).expect("generated fninit decode");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "fninit");
    assert!(matches!(decoded.flow_kind, DecodedFlowKind::None));
}

#[test]
fn generated_runtime_lifts_fninit_without_compatibility_emitter() {
    require_packaged_ghidra_sla!();
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let bytes = [0xdb, 0xe3];
    let ops = assert_spec_derived_lift(&compiled, &bytes, 0x1400_25c0);
    assert!(ops.iter().all(|op| op.opcode == PcodeOpcode::Copy));
    assert_eq!(ops.len(), 10);
}

#[test]
fn generated_runtime_lifts_cmp_template_without_compatibility() {
    require_packaged_ghidra_sla!();
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let bytes = [0x83, 0xf9, 0x01];
    let ops = assert_spec_derived_lift(&compiled, &bytes, 0x1400_1485);
    assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::IntSub));
    assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::IntEqual));
}

#[test]
fn generated_runtime_lifts_push_template_without_compatibility() {
    require_packaged_ghidra_sla!();
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let bytes = [0x41, 0x57];
    let ops = assert_spec_derived_lift(&compiled, &bytes, 0x1400_1470);
    assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::IntSub));
    assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::Store));
}

#[test]
fn generated_runtime_lifts_lea_template_without_compatibility() {
    require_packaged_ghidra_sla!();
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let bytes = [0x8d, 0x04, 0x11];
    let ops = assert_spec_derived_lift(&compiled, &bytes, 0x1400_1450);
    assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::IntAdd));
    assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::IntZExt));
}

#[test]
fn generated_runtime_lifts_x86_scalar_float_templates_without_unsupported_cutover() {
    require_packaged_ghidra_sla!();
    let compiled = compile_x86_64_frontend().expect("compile frontend");

    let mulsd = [0xf2, 0x0f, 0x59, 0x05, 0xc0, 0x37, 0x00, 0x00];
    let mul_ops = assert_spec_derived_lift(&compiled, &mulsd, 0x1400_1860);
    assert!(mul_ops.iter().any(|op| op.opcode == PcodeOpcode::FloatMult));

    let cvtsi2sd = [0xf2, 0x0f, 0x2a, 0xca];
    let convert_ops = assert_spec_derived_lift(&compiled, &cvtsi2sd, 0x1400_18c8);
    assert!(convert_ops
        .iter()
        .any(|op| op.opcode == PcodeOpcode::FloatInt2Float));

    let divsd = [0xf2, 0x0f, 0x5e, 0xc1];
    let div_ops = assert_spec_derived_lift(&compiled, &divsd, 0x1400_18cc);
    assert!(div_ops.iter().any(|op| op.opcode == PcodeOpcode::FloatDiv));
}

#[test]
fn packed_context_word_write_matches_ghidra_bit_numbering() {
    let mut context = 0u64;
    set_packed_context_word(&mut context, 0, 1u32 << 31, 1u32 << 31).expect("set context word");
    assert_eq!(packed_context_bits(context, 0, 1).expect("bit 0"), 1);
    assert_eq!(packed_context_bits(context, 31, 1).expect("bit 31"), 0);
}

#[test]
fn packed_context_bit_write_matches_ghidra_bit_numbering() {
    let mut context = 0u64;
    set_packed_context_bits(&mut context, 0, 1, 1).expect("set bit 0");
    assert_eq!(packed_context_bits(context, 0, 1).expect("bit 0"), 1);
    assert_eq!(packed_context_bits(context, 31, 1).expect("bit 31"), 0);

    set_packed_context_bits(&mut context, 31, 2, 0b11).expect("set cross-word bits");
    assert_eq!(
        packed_context_bits(context, 31, 2).expect("cross-word bits"),
        0b11
    );
}

#[test]
fn packed_context_bit_reads_cross_word_boundaries_like_ghidra() {
    let mut context = 0u64;
    set_packed_context_word(&mut context, 0, 0x0000_0001, 0x0000_0001).expect("set low word");
    set_packed_context_word(&mut context, 1, 0x8000_0000, 0x8000_0000).expect("set high word");
    assert_eq!(
        packed_context_bits(context, 31, 2).expect("cross-word bits"),
        0b11
    );
}

#[test]
fn generated_runtime_decodes_aarch64_smoke_without_constructor_loop() {
    require_packaged_ghidra_sla!();
    let aarch64_spec = spec_root_for_arch("AARCH64").join("AARCH64.slaspec");
    let compiled = compile_frontend_for_entry_spec(&aarch64_spec).expect("compile aarch64");
    let bytes = [0x0c, 0x10, 0x8e, 0xd2];
    let decoded = decode_instruction(&compiled, None, &bytes, 0x100000).expect("decode aarch64");
    assert_eq!(decoded.length, bytes.len());
    assert!(
        !decoded.mnemonic.is_empty(),
        "expected resolved aarch64 mnemonic"
    );
    assert_ne!(
        decoded.mnemonic, "udf",
        "expected terminal verification to avoid udf fallback"
    );
}

#[test]
fn generated_runtime_decodes_aarch64_movk_shifted_immediate_from_exported_handle() {
    require_packaged_ghidra_sla!();
    let aarch64_spec = spec_root_for_arch("AARCH64").join("AARCH64.slaspec");
    let compiled = compile_frontend_for_entry_spec(&aarch64_spec).expect("compile aarch64");
    let bytes = [0x0c, 0x0c, 0xaa, 0xf2];

    let decoded = decode_instruction(&compiled, None, &bytes, 0x10000c).expect("decode movk");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "movk");

    let (ops, length, details) = decode_and_lift_with_details(&compiled, None, &bytes, 0x10000c)
        .expect("lift movk shifted immediate");
    assert_eq!(length as usize, bytes.len());
    assert_eq!(
        details.template_source,
        Some(CompiledTemplateSource::SpecDerived)
    );
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::IntOr
            && op
                .inputs
                .get(1)
                .is_some_and(|input| input.is_constant && input.constant_val == 0x5060_0000)),
        "expected movk INT_OR to use exported shifted immediate; ops={ops:?}"
    );
}

#[test]
fn generated_runtime_lifts_aarch64_cneg_from_sla_int_2comp_template() {
    require_packaged_ghidra_sla!();
    let aarch64_spec = spec_root_for_arch("AARCH64").join("AARCH64.slaspec");
    let compiled = compile_frontend_for_entry_spec(&aarch64_spec).expect("compile aarch64");
    let bytes = [0x00, 0x85, 0x88, 0x5a]; // cneg w0, w8, ls

    let decoded = decode_instruction(&compiled, None, &bytes, 0x100058).expect("decode cneg");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "cneg");

    let (ops, length, details) =
        decode_and_lift_with_details(&compiled, None, &bytes, 0x100058).expect("lift cneg");
    assert_eq!(length as usize, bytes.len());
    assert_eq!(
        details.template_source,
        Some(CompiledTemplateSource::SpecDerived)
    );
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::Int2Comp),
        "expected cneg template to emit INT_2COMP; ops={ops:?}"
    );
}

#[test]
fn generated_runtime_lifts_aarch64_subs_shifted_from_sla_compare_template() {
    require_packaged_ghidra_sla!();
    let aarch64_spec = spec_root_for_arch("AARCH64").join("AARCH64.slaspec");
    let compiled = compile_frontend_for_entry_spec(&aarch64_spec).expect("compile aarch64");
    let bytes = [0x08, 0x00, 0x01, 0x6b]; // subs w8, w0, w1

    let decoded = decode_instruction(&compiled, None, &bytes, 0x100054).expect("decode subs");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "subs");

    let (ops, length, details) =
        decode_and_lift_with_details(&compiled, None, &bytes, 0x100054).expect("lift subs");
    assert_eq!(length as usize, bytes.len());
    assert_eq!(
        details.template_source,
        Some(CompiledTemplateSource::SpecDerived)
    );
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::IntLessEqual),
        "expected subs flag template to emit INT_LESSEQUAL; ops={ops:?}"
    );
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::IntSub),
        "expected subs to emit INT_SUB; ops={ops:?}"
    );
}

#[test]
fn generated_runtime_lifts_aarch64_udiv_from_sla_int_div_template() {
    require_packaged_ghidra_sla!();
    let aarch64_spec = spec_root_for_arch("AARCH64").join("AARCH64.slaspec");
    let compiled = compile_frontend_for_entry_spec(&aarch64_spec).expect("compile aarch64");
    let bytes = [0x28, 0x09, 0xc8, 0x1a]; // udiv w8, w9, w8

    let decoded = decode_instruction(&compiled, None, &bytes, 0x10002c).expect("decode udiv");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "udiv");

    let (ops, length, details) =
        decode_and_lift_with_details(&compiled, None, &bytes, 0x10002c).expect("lift udiv");
    assert_eq!(length as usize, bytes.len());
    assert_eq!(
        details.template_source,
        Some(CompiledTemplateSource::SpecDerived)
    );
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::IntDiv),
        "expected udiv template to emit INT_DIV; ops={ops:?}"
    );
}

#[test]
fn generated_runtime_lifts_aarch64_vector_lane_mov_from_operand_value_expression() {
    require_packaged_ghidra_sla!();
    let aarch64_spec = spec_root_for_arch("AARCH64").join("AARCH64.slaspec");
    let compiled = compile_frontend_for_entry_spec(&aarch64_spec).expect("compile aarch64");
    let bytes = [0x22, 0x64, 0x1c, 0x6e]; // mov v2.s[3], v1.s[3]

    let decoded = decode_instruction(&compiled, None, &bytes, 0x1000b8).expect("decode mov");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "mov");

    let (ops, length, details) =
        decode_and_lift_with_details(&compiled, None, &bytes, 0x1000b8).expect("lift mov");
    assert_eq!(length as usize, bytes.len());
    assert_eq!(
        details.template_source,
        Some(CompiledTemplateSource::SpecDerived)
    );
    assert!(!ops.is_empty(), "vector lane mov emitted no p-code");
}

#[test]
fn generated_runtime_lifts_riscv_lui_shift_count_at_sla_const_width() {
    require_packaged_ghidra_sla!();
    let riscv_spec = spec_root_for_arch("RISCV").join("riscv.lp64d.slaspec");
    let compiled = compile_frontend_for_entry_spec(&riscv_spec).expect("compile riscv");
    let bytes = [0xb7, 0x87, 0x35, 0x01]; // lui a5,0x1358

    let decoded = decode_instruction(&compiled, None, &bytes, 0x100590).expect("decode lui");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "lui");

    let ops = assert_spec_derived_lift(&compiled, &bytes, 0x100590);
    let int_left = ops
        .iter()
        .find(|op| op.opcode == PcodeOpcode::IntLeft)
        .unwrap_or_else(|| panic!("expected lui to emit INT_LEFT; ops={ops:?}"));
    assert!(
        int_left.inputs.get(1).is_some_and(|input| {
            input.is_constant && input.constant_val == 12 && input.size == 4
        }),
        "expected shift count const 12 to keep SLA varnode size 4; op={int_left:?}"
    );
}

#[test]
fn generated_runtime_decodes_arm7_le_arm_mode_stmdb_from_sla_template() {
    require_packaged_ghidra_sla!();
    let arm_spec = spec_root_for_arch("ARM").join("ARM7_le.slaspec");
    let compiled = compile_frontend_for_entry_spec(&arm_spec).expect("compile ARM7_le");
    let bytes = [0x08, 0x40, 0x2d, 0xe9];

    let decoded = decode_instruction(&compiled, None, &bytes, 0x102e8).expect("decode stmdb");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "stmdb");

    let (ops, length, details) = decode_and_lift_with_details(&compiled, None, &bytes, 0x102e8)
        .expect("lift ARM mode stmdb");
    assert_eq!(length as usize, bytes.len());
    assert_eq!(
        details.template_source,
        Some(CompiledTemplateSource::SpecDerived)
    );
    assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::Store));
}

#[test]
fn generated_runtime_preserves_arm_conditional_execution_wrapper_pcode() {
    require_packaged_ghidra_sla!();
    let arm_spec = spec_root_for_arch("ARM").join("ARM4t_be.slaspec");
    let compiled = compile_frontend_for_entry_spec(&arm_spec).expect("compile ARM4t_be");
    let bytes = [0x30, 0x40, 0x20, 0x01]; // subcc r2,r0,r1

    let decoded = decode_instruction(&compiled, None, &bytes, 0x100044).expect("decode subcc");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "subcc");

    let (ops, length, details) = decode_and_lift_with_details(&compiled, None, &bytes, 0x100044)
        .expect("lift ARM conditional sub");
    assert_eq!(length as usize, bytes.len());
    assert_eq!(
        details.template_source,
        Some(CompiledTemplateSource::SpecDerived)
    );
    let cbranch = ops
        .iter()
        .find(|op| op.opcode == PcodeOpcode::CBranch)
        .unwrap_or_else(|| panic!("expected conditional wrapper CBRANCH; ops={ops:?}"));
    assert!(
        cbranch
            .inputs
            .first()
            .is_some_and(|target| !target.is_constant && target.offset == 0x100048),
        "expected subcc guard to skip to inst_next; op={cbranch:?}"
    );
}

#[test]
fn generated_runtime_reports_thumb_it_context_commits_in_lift_details() {
    require_packaged_ghidra_sla!();
    for (entry_id, bytes) in [
        ("ARM8m_le", [0x88, 0xbf]), // it hi
        ("ARM8m_be", [0xbf, 0x88]), // it hi
    ] {
        let arm_spec = spec_root_for_arch("ARM").join(format!("{entry_id}.slaspec"));
        let compiled = compile_frontend_for_entry_spec(&arm_spec)
            .unwrap_or_else(|err| panic!("compile {entry_id}: {err:#}"));

        let (_ops, length, details) =
            decode_and_lift_with_details(&compiled, None, &bytes, 0x100016)
                .unwrap_or_else(|err| panic!("lift {entry_id} Thumb IT: {err:#}"));

        assert_eq!(length as usize, bytes.len(), "{entry_id} IT length");
        assert!(
            details
                .pending_context_commits
                .iter()
                .any(|(target, _, mask, value)| {
                    *target == 0x100018 && *mask != 0 && (*value & *mask) != 0
                }),
            "{entry_id} IT lift details must expose pending context commits: {details:?}"
        );
    }
}

#[test]
fn generated_runtime_executes_arm_bool_xor_template_opcode() {
    require_packaged_ghidra_sla!();
    let arm_spec = spec_root_for_arch("ARM").join("ARM4_le.slaspec");
    let compiled = compile_frontend_for_entry_spec(&arm_spec).expect("compile ARM4_le");
    let bytes = [0x00, 0x30, 0xcc, 0xe2]; // sbc r3,r12,#0

    let decoded = decode_instruction(&compiled, None, &bytes, 0x100048).expect("decode sbc");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "sbc");

    let (ops, length, details) = decode_and_lift_with_details(&compiled, None, &bytes, 0x100048)
        .expect("lift ARM sbc with BOOL_XOR template opcode");
    assert_eq!(length as usize, bytes.len());
    assert_eq!(
        details.template_source,
        Some(CompiledTemplateSource::SpecDerived)
    );
    assert!(
        ops.iter().any(|op| op.opcode == PcodeOpcode::BoolXor),
        "expected ARM sbc template to emit BOOL_XOR; ops={ops:?}"
    );
}

#[test]
fn compiled_table_policy_symbols_stay_architecture_neutral() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let disabled_branch = ["if", "false"].join(" ");
    let guard_no_export_assignment = ["no_export_subtable_fallback", "= true"].join(" ");
    let arch_named_policy = ["compiled", "arch"].join(".");
    let x86_string_policy = ["eq_ignore_ascii_case", "(\"x86\")"].join("");
    let transitional_cursor_policy = ["shared", "token", "cursor"].join("_");
    let isa_opcode_policy = ["is", "instruction", "prefix", "byte"].join("_");
    let opcode_context_policy = ["opcode", "len", "from", "context"].join("_");
    let unknown_space_materialization = ["name: ", "\"unknown\""].concat();
    let unknown_space_match_arm = ["_ => ", "\"unknown\""].concat();
    let hardcoded_unique_space = ["2 => ", "\"unique\""].concat();
    let hardcoded_ram_space = ["3 => ", "\"ram\""].concat();
    let hardcoded_register_space = ["4 => ", "\"register\""].concat();
    let silent_u32_overflow_zero = ["u32::try_from(value).ok())", ".unwrap_or(0)"].join("\n");
    let debug_pattern_value_fallback = ["debug", "_value) = handle.debug_value.clone()"].concat();
    let callother_size_fallback = ["template_varnode_size(input, state).", "unwrap_or(8)"].concat();
    let delay_slot_zero_fallback = ["delay_slot_length", "unwrap_or(0)"].join(".");
    let label_zero_fallback = [".output\n                    .as_ref()", ".unwrap_or(0)"].join("");
    let offset_plus_zero_fallback = ["OffsetPlus)", "plus.unwrap_or(0)"].join("\n");
    let operand_offset_cursor_fallback = [
        "operand_absolute_offset(&template.spec)",
        "unwrap_or(self.cursor)",
    ]
    .join("\n");
    let operand_length_cursor_fallback = "saturating_sub(operand_absolute_offset)";
    let operand_base_saturating_end_fallback = "offset.saturating_add(length)";
    let operand_end_unchecked_add_fallback = "Some((*offset)? + (*length)?)";
    let constructor_minimum_unchecked_end = "self.ctx.cursor + self.minimum_length";
    let token_constructor_minimum_unchecked_end =
        "self.ctx.cursor + self.selection.constructor.minimum_length as usize";
    let constructor_relative_saturating_length = "length.saturating_sub(absolute_offset)";
    let subtable_relative_saturating_length = "sub_state.length.saturating_sub(self.ctx.cursor)";
    let operand_subtable_minimum_fallback = ".unwrap_or(template.minimum_length as usize)";
    let token_field_saturating_end = "token_base.saturating_add(encoded_size as usize)";
    let token_field_unchecked_end = "token_base + encoded_size as usize";
    let token_read_unchecked_end = "offset + size as usize";
    let token_read_unchecked_absolute = "base_cursor + off";
    let operand_offset_lossy_signed_add = "base as i64 + i64::from(reloffset)";
    let subtable_offset_lossy_signed_add = "base as i64 + i64::from(rel)";
    let relative_offset_lossy_usize_cast = "Ok(offset as usize)";
    let constructor_cursor_unchecked_add = "cursor: ctx.cursor + opcode_len";
    let export_inst_next_saturating = "self.ctx.address.saturating_add(self.minimum_length as u64)";
    let pattern_inst_next_saturating_constructor =
        "saturating_add(self.selection.constructor.minimum_length as usize)";
    let pattern_inst_next_saturating_address =
        "self.ctx.address.saturating_add(next_offset as u64)";
    let subtable_cursor_saturating_delta = "self.cursor.saturating_sub(self.ctx.cursor)";
    let subtable_decode_address_wrapping = "sub_ctx.address.wrapping_add(sub_ctx.cursor as u64)";
    let delay_slot_length_lossy_cast =
        ["Ok(decoded) => return Ok(decoded.length", "as u32)"].join(" ");
    let bind_target_offset_lossy_cast =
        ["target_address.checked_sub(memory_base)", "})? as usize"].join("\n");
    let context_commit_handle_lossy_cast =
        ["decoded.handles.get(hand_index", "as usize)"].join(" ");
    let template_delay_slot_inst_next_saturating =
        "self.address.saturating_add(inst_length as u64)";
    let template_delay_slot_pc_saturating = "self.address.saturating_add(fall_offset)";
    let template_delay_slot_fall_saturating =
        "fall_offset = fall_offset.saturating_add(u64::from(slot_len))";
    let template_const_inst_next_saturating = "self.address.saturating_add(state.length as u64)";
    let template_const_inst_next_zero_fallback = "self.flow.instruction_length.unwrap_or(0)";
    let template_const_inst_next2_saturating = "inst_next.saturating_add(delay_len)";
    let context_commit_inst_next_saturating =
        "instruction_address.saturating_add(decoded.length as u64)";
    let handle_ptr_offset_zero_fallback = [
        ".ptr_offset\n                        .as_ref()",
        ".unwrap_or(0)",
    ]
    .join("");
    let missing_sla_identity_slot_fallback = [
        ".sla_identity\n",
        ".map(|identity| identity.constructor_slot)",
        ".unwrap_or(constructor_index)",
    ]
    .join("");
    let handle_index_lossy_cast = "*handle_index as usize";
    let build_secnum_lossy_cast = "self.pcode_build_secnum = operand_index as i32";
    let exported_build_key_lossy_cast = "let handle_key = -((operand_index as i64) + 1)";
    let exported_display_fallbacks = [
        "ExportedDisplayFallback",
        "exported_handle_display_fallback",
        "exported_fixed_handle_needs_memory_display_fixup",
        "handle_tpl_operand_handle_indices",
        "display_operand_from_exported_fixed_handle",
    ];
    let constant_varnode_lossy_casts = [
        "Varnode::constant(handle.offset_offset as i64, size)",
        "Varnode::constant(value as i64, size)",
    ];
    let subtable_offset_base_fallback = "offsetbase.unwrap_or(-1)";
    let context_commit_temp_offset_target = [
        "let offset = if handle.fixed.offset_space.is_some()",
        "handle.fixed.temp_offset",
    ]
    .join(" ");
    let context_commit_addr_unit_fallback = ".unwrap_or(1)";
    let context_commit_addr_unit_wrap = "offset.wrapping_mul(addr_unit)";
    let static_handle_addr_unit_wrap = "offset_offset.wrapping_mul(addr_unit)";
    let swallowed_context_word_error = "let _ = set_packed_context_word";
    let decoded_bytes_truncation_fallback = "bytes.get(..length).unwrap_or(bytes)";
    let immediate_byte_unchecked_shift = "u64::from(*byte) << (index * 8)";
    let tokenfield_saturating_range = "byte_end.saturating_sub(byte_start)";
    let tokenfield_saturating_bit_range = "bit_end.saturating_sub(bit_start)";
    let tokenfield_unchecked_accumulation = "res = (res << 8) | u64::from(byte)";
    let tokenfield_unchecked_right_shift = "res >> (shift as u32)";
    let tokenfield_unchecked_left_shift = "res << ((-shift) as u32)";
    let empty_or_pattern_length_fallback = [
        ".map(disjoint_pattern_instruction_byte_len)",
        ".max()\n            .unwrap_or(0)",
    ]
    .join("\n");
    let missing_terminal_pattern_length_fallback = [
        ".matched_leaf_pattern\n            .as_ref()",
        ".map(disjoint_pattern_instruction_byte_len)",
        ".unwrap_or(0)",
    ]
    .join("\n");
    let pattern_length_overflow_fallback = ".unwrap_or(usize::MAX)";
    let context_commit_missing_handle_skip = [
        "decoded.handles.get(commit.hand_index as usize)",
        "else {\n                continue;\n            }",
    ]
    .join(" ");
    let context_expr_lossy_word_cast = "eval_pattern_expression(expr)? as u32";
    let context_expr_unchecked_left_shift = "raw << (change.shift as u32)";
    let context_expr_unchecked_right_shift = "raw >> ((-change.shift) as u32)";
    let pattern_token_lossy_i64_cast = ")? as i64)";
    let pattern_token_at_lossy_i64_cast = ")? as i64),";
    let pattern_context_lossy_sign_extend = "Ok(((raw << shift) as i64) >> shift)";
    let pattern_context_lossy_raw_cast = "Ok(raw as i64)";
    let pattern_expr_unchecked_add =
        "eval_pattern_expression(lhs)? + self.eval_pattern_expression(rhs)?";
    let pattern_expr_unchecked_sub =
        "eval_pattern_expression(lhs)? - self.eval_pattern_expression(rhs)?";
    let pattern_expr_unchecked_mul =
        "eval_pattern_expression(lhs)? * self.eval_pattern_expression(rhs)?";
    let pattern_expr_unchecked_left_shift = "<< (self.eval_pattern_expression(rhs)? as u32)";
    let pattern_expr_unchecked_right_shift = ">> (self.eval_pattern_expression(rhs)? as u32)";
    let pattern_expr_unchecked_negate = "-self.eval_pattern_expression(inner)?";
    let pattern_value_zero_fallback = [
        "block.value_words.get(index).copied()",
        "unwrap_or_default()",
    ]
    .join(".");
    let decision_probe_zero_padding = [
        "self.ctx.bytes.get(start + i).copied().unwrap_or(0)",
        "get(self.ctx.cursor + byte_offset as usize + i as usize)\n                            .copied()\n                            .unwrap_or(0)",
    ];
    let bit_constraint_zero_padding =
        "if let Some(byte) = ctx.bytes.get(ctx.cursor + *offset as usize + i)";
    let matcher_opcode_len_zero_fallback = ".max()\n            .unwrap_or(0)";
    let token_span_try_from_ok_fallback = "i32::try_from(byte_start).ok()?";
    let token_span_missing_subtable_none_fallback = "compiled.subtables.get(table_name)?";
    let decision_edge_lossy_u8_cast = "value: val as u8";
    let decision_probe_lossy_context_cast =
        "packed_context_bits(self.ctx.context_register, start_bit, bit_size)? as u8";
    let decision_probe_lossy_instruction_cast =
        "((word >> shift) & ((1u64 << bit_size) - 1)) as u8";
    let bit_constraint_unchecked_byte_shift = "inst_val |= u64::from(byte) << (i * 8)";
    let bit_constraint_unchecked_context_shift = "(ctx.context_register >> offset) & mask";
    let selection_unchecked_ranges = [
        ".get(self.ctx.cursor + usize::from(offset))",
        "self.ctx.cursor + byte_offset as usize + i as usize",
        "self.ctx.bytes.get(start + i)",
        "ctx.cursor..ctx.cursor + bytes.len()",
        "ctx.cursor..ctx.cursor + prefix.len()",
        ".get(ctx.cursor + prefix.len())",
    ];
    let files = [
        manifest_dir.join("src/runtime/spine/compiled_table/mod.rs"),
        manifest_dir.join("src/runtime/spine/compiled_table/strategy.rs"),
        manifest_dir.join("src/runtime/spine/compiled_table/selection.rs"),
        manifest_dir.join("src/runtime/spine/compiled_table/walker.rs"),
        manifest_dir.join("src/runtime/spine/compiled_table/handles.rs"),
        manifest_dir.join("src/runtime/spine/compiled_table/template_eval.rs"),
        manifest_dir.join("src/runtime/spine/compiled_table/token.rs"),
        manifest_dir.join("src/runtime/spine/decision.rs"),
        manifest_dir.join("src/runtime/spine/walker.rs"),
        manifest_dir.join("src/compiler/sla/templates.rs"),
    ];
    for file in files {
        let source = std::fs::read_to_string(&file)
            .unwrap_or_else(|error| panic!("read {}: {error}", file.display()));
        assert!(
            !source.contains("is_x86_compat_language"),
            "{} still uses architecture-named compatibility predicate",
            file.display()
        );
        assert!(
            !source.contains(&arch_named_policy),
            "{} still gates compiled-table runtime policy by frontend architecture string",
            file.display()
        );
        assert!(
            !source.contains(&x86_string_policy),
            "{} still gates compiled-table runtime policy by x86 string comparison",
            file.display()
        );
        assert!(
            !source.contains(&disabled_branch),
            "{} still carries disabled compatibility classifier code",
            file.display()
        );
        assert!(
            !source.contains(&guard_no_export_assignment),
            "{} still counts guard-only no-export subtables as fallback debt",
            file.display()
        );
        assert!(
            !source.contains(&transitional_cursor_policy),
            "{} still carries transitional shared-token cursor policy",
            file.display()
        );
        assert!(
            !source.contains(&isa_opcode_policy) && !source.contains(&opcode_context_policy),
            "{} still carries ISA-specific opcode cursor policy",
            file.display()
        );
        assert!(
            !source.contains(&unknown_space_materialization),
            "{} still materializes missing SLA spaces instead of failing closed",
            file.display()
        );
        assert!(
            !source.contains(&unknown_space_match_arm),
            "{} still maps missing SLA spaces to an unknown placeholder",
            file.display()
        );
        assert!(
            !source.contains(&hardcoded_unique_space)
                && !source.contains(&hardcoded_ram_space)
                && !source.contains(&hardcoded_register_space),
            "{} still hardcodes SLA space ids instead of using sla_spaces",
            file.display()
        );
        assert!(
            !source.contains(&silent_u32_overflow_zero),
            "{} still turns oversized SLA template sizes into zero",
            file.display()
        );
        assert!(
            !source.contains(&debug_pattern_value_fallback),
            "{} still evaluates pattern expressions from display/debug operands",
            file.display()
        );
        assert!(
            !source.contains(&callother_size_fallback),
            "{} still guesses CALLOTHER input size after template size resolution failure",
            file.display()
        );
        assert!(
            !source.contains(&delay_slot_zero_fallback),
            "{} still treats missing delay-slot length as zero",
            file.display()
        );
        assert!(
            !source.contains(&label_zero_fallback),
            "{} still treats malformed LABEL templates as label 0",
            file.display()
        );
        assert!(
            !source.contains(&offset_plus_zero_fallback),
            "{} still treats missing offset_plus ATTR_PLUS as zero",
            file.display()
        );
        assert!(
            !source.contains(&operand_offset_cursor_fallback),
            "{} still treats unresolved operand offsets as the current cursor",
            file.display()
        );
        assert!(
            !source.contains(operand_length_cursor_fallback),
            "{} still derives non-subtable operand lengths from the runtime cursor instead of SLA minimum_length",
            file.display()
        );
        assert!(
            !source.contains(operand_base_saturating_end_fallback)
                && !source.contains(operand_end_unchecked_add_fallback),
            "{} still hides malformed SLA operand end arithmetic",
            file.display()
        );
        assert!(
            !source.contains(constructor_minimum_unchecked_end)
                && !source.contains(token_constructor_minimum_unchecked_end)
                && !source.contains(constructor_relative_saturating_length)
                && !source.contains(subtable_relative_saturating_length),
            "{} still hides malformed SLA construct length arithmetic",
            file.display()
        );
        assert!(
            !source.contains(operand_subtable_minimum_fallback),
            "{} still falls back from missing subtable operand length to SLA minimum_length",
            file.display()
        );
        assert!(
            !source.contains(token_field_saturating_end)
                && !source.contains(token_field_unchecked_end),
            "{} still hides malformed SLA token-field cursor arithmetic",
            file.display()
        );
        assert!(
            !source.contains(&handle_ptr_offset_zero_fallback),
            "{} still treats missing HandleTpl ptr_offset as zero",
            file.display()
        );
        assert!(
            !source.contains(&missing_sla_identity_slot_fallback),
            "{} still treats missing SLA constructor identity as constructor_index",
            file.display()
        );
        assert!(
            !source.contains(handle_index_lossy_cast)
                && !source.contains(build_secnum_lossy_cast)
                && !source.contains(exported_build_key_lossy_cast),
            "{} still uses lossy BUILD/handle index casts",
            file.display()
        );
        for fallback in exported_display_fallbacks {
            assert!(
                !source.contains(fallback),
                "{} still derives exported display values through fallback helper: {fallback}",
                file.display()
            );
        }
        for forbidden in constant_varnode_lossy_casts {
            assert!(
                !source.contains(forbidden),
                "{} still materializes constant varnodes through unnamed bit-pattern casts",
                file.display()
            );
        }
        assert!(
            !source.contains(subtable_offset_base_fallback),
            "{} still treats missing subtable offset base as constructor start",
            file.display()
        );
        assert!(
            !source.contains(&context_commit_temp_offset_target),
            "{} still resolves context commit addresses from temp_offset instead of offset_offset",
            file.display()
        );
        assert!(
            !source.contains(context_commit_addr_unit_fallback)
                && !source.contains(context_commit_addr_unit_wrap)
                && !source.contains(static_handle_addr_unit_wrap),
            "{} still hides invalid address-unit scaling",
            file.display()
        );
        assert!(
            !source.contains(swallowed_context_word_error),
            "{} still ignores packed context word errors",
            file.display()
        );
        assert!(
            !source.contains(decoded_bytes_truncation_fallback),
            "{} still truncates decoded instruction bytes when decoded length exceeds input window",
            file.display()
        );
        assert!(
            !source.contains(immediate_byte_unchecked_shift),
            "{} still unchecked-shifts immediate bytes into u64",
            file.display()
        );
        assert!(
            !source.contains(tokenfield_saturating_range)
                && !source.contains(tokenfield_saturating_bit_range),
            "{} still accepts inverted SLA tokenfield ranges via saturating arithmetic",
            file.display()
        );
        assert!(
            !source.contains(tokenfield_unchecked_accumulation),
            "{} still accumulates SLA tokenfield bytes without u64 width checks",
            file.display()
        );
        assert!(
            !source.contains(tokenfield_unchecked_right_shift)
                && !source.contains(tokenfield_unchecked_left_shift),
            "{} still unchecked-shifts SLA tokenfield values",
            file.display()
        );
        assert!(
            !source.contains(token_read_unchecked_end)
                && !source.contains(token_read_unchecked_absolute),
            "{} still hides malformed SLA token byte read arithmetic",
            file.display()
        );
        assert!(
            !source.contains(operand_offset_lossy_signed_add)
                && !source.contains(subtable_offset_lossy_signed_add)
                && !source.contains(relative_offset_lossy_usize_cast),
            "{} still computes SLA relative offsets through lossy signed casts",
            file.display()
        );
        assert!(
            !source.contains(constructor_cursor_unchecked_add)
                && !source.contains(export_inst_next_saturating)
                && !source.contains(pattern_inst_next_saturating_constructor)
                && !source.contains(pattern_inst_next_saturating_address)
                && !source.contains(subtable_cursor_saturating_delta)
                && !source.contains(subtable_decode_address_wrapping)
                && !source.contains(&delay_slot_length_lossy_cast)
                && !source.contains(&bind_target_offset_lossy_cast)
                && !source.contains(&context_commit_handle_lossy_cast)
                && !source.contains(template_delay_slot_inst_next_saturating)
                && !source.contains(template_delay_slot_pc_saturating)
                && !source.contains(template_delay_slot_fall_saturating)
                && !source.contains(template_const_inst_next_saturating)
                && !source.contains(template_const_inst_next_zero_fallback)
                && !source.contains(template_const_inst_next2_saturating)
                && !source.contains(context_commit_inst_next_saturating),
            "{} still hides malformed SLA parser cursor or InstNext arithmetic",
            file.display()
        );
        assert!(
            !source.contains(&empty_or_pattern_length_fallback),
            "{} still treats empty SLA OR patterns as zero instruction bytes",
            file.display()
        );
        assert!(
            !source.contains(&missing_terminal_pattern_length_fallback),
            "{} still treats missing terminal SLA patterns as zero instruction bytes",
            file.display()
        );
        assert!(
            !source.contains(pattern_length_overflow_fallback),
            "{} still saturates malformed SLA pattern byte lengths",
            file.display()
        );
        assert!(
            !source.contains(&context_commit_missing_handle_skip),
            "{} still skips malformed context commit handle targets",
            file.display()
        );
        assert!(
            !source.contains(context_expr_lossy_word_cast)
                && !source.contains(context_expr_unchecked_left_shift)
                && !source.contains(context_expr_unchecked_right_shift),
            "{} still truncates or unchecked-shifts context-change pattern expressions",
            file.display()
        );
        assert!(
            !source.contains(pattern_token_lossy_i64_cast)
                && !source.contains(pattern_token_at_lossy_i64_cast)
                && !source.contains(pattern_context_lossy_sign_extend)
                && !source.contains(pattern_context_lossy_raw_cast),
            "{} still uses unnamed token/context pattern value bit casts",
            file.display()
        );
        assert!(
            !source.contains(pattern_expr_unchecked_add)
                && !source.contains(pattern_expr_unchecked_sub)
                && !source.contains(pattern_expr_unchecked_mul)
                && !source.contains(pattern_expr_unchecked_left_shift)
                && !source.contains(pattern_expr_unchecked_right_shift)
                && !source.contains(pattern_expr_unchecked_negate),
            "{} still evaluates pattern-expression arithmetic without checked helpers",
            file.display()
        );
        assert!(
            !source.contains(&pattern_value_zero_fallback),
            "{} still treats missing terminal pattern values as zero",
            file.display()
        );
        for forbidden in decision_probe_zero_padding {
            assert!(
                !source.contains(forbidden),
                "{} still zero-pads missing decision-probe instruction bytes",
                file.display()
            );
        }
        assert!(
            !source.contains(bit_constraint_zero_padding),
            "{} still zero-pads missing instruction bit-constraint bytes",
            file.display()
        );
        assert!(
            !source.contains(matcher_opcode_len_zero_fallback),
            "{} still treats matchers without instruction bytes as zero-length opcodes",
            file.display()
        );
        assert!(
            !source.contains(token_span_try_from_ok_fallback)
                && !source.contains(token_span_missing_subtable_none_fallback),
            "{} still hides malformed SLA token-span analysis as a non-sequential operand",
            file.display()
        );
        assert!(
            !source.contains(decision_edge_lossy_u8_cast)
                && !source.contains(decision_probe_lossy_context_cast)
                && !source.contains(decision_probe_lossy_instruction_cast),
            "{} still truncates SLA decision probe or edge values into u8",
            file.display()
        );
        assert!(
            !source.contains(bit_constraint_unchecked_byte_shift)
                && !source.contains(bit_constraint_unchecked_context_shift),
            "{} still unchecked-shifts constructor bit constraints",
            file.display()
        );
        for forbidden in selection_unchecked_ranges {
            assert!(
                !source.contains(forbidden),
                "{} still uses unchecked compiled-table selection byte ranges",
                file.display()
            );
        }
    }
}

#[test]
fn canonical_template_executor_has_no_compatibility_success_entrypoints() {
    let template_eval = include_str!("template_eval.rs");
    for forbidden in [
        "NativeFission",
        "CompatibilityLowered",
        "emit_compat",
        "semantic_ops_for_kind",
        "classify_construct_tpl_kind",
    ] {
        assert!(
            !template_eval.contains(forbidden),
            "canonical template executor must not expose compatibility p-code success path: {forbidden}"
        );
    }
}

#[test]
fn canonical_template_executor_does_not_materialize_from_bound_operand_helpers() {
    let template_eval = include_str!("template_eval.rs");
    for forbidden in [
        "fixed_handle_for_bound_operand",
        "BoundOperand",
        "CompiledVarnodeTpl::EffectiveAddress",
        "CompiledVarnodeTpl::FixedRegister",
        "CompiledVarnodeTpl::Flag",
    ] {
        assert!(
            !template_eval.contains(forbidden),
            "template execution must resolve .sla VarnodeTpl/HandleTpl, not manual operand helpers: {forbidden}"
        );
    }
}
