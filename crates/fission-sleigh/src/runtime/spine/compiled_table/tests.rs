use super::*;
use crate::compiler::{
    compile_frontend_for_entry_spec, compile_x86_64_frontend, CompiledTemplateSource,
};
use std::path::PathBuf;

fn assert_spec_derived_lift_or_typed_unsupported(
    compiled: &CompiledFrontend,
    bytes: &[u8],
    address: u64,
) {
    match decode_and_lift_with_details(compiled, None, bytes, address) {
        Ok((ops, length, details)) => {
            assert_eq!(length as usize, bytes.len());
            assert!(
                !details.compat_emitter_used,
                "raw p-code path must not use compatibility emitter"
            );
            assert!(
                details.template_source == Some(CompiledTemplateSource::SpecDerived)
                    || details.template_source == Some(CompiledTemplateSource::NativeFission),
                "expected SpecDerived or NativeFission, got {:?}",
                details.template_source
            );
            assert!(!ops.is_empty(), "spec-derived template emitted no p-code");
        }
        Err(err) => {
            let rendered = err.to_string();
            assert!(
                rendered.contains("UnsupportedPcodeTemplate"),
                "unsupported raw p-code must be typed: {rendered}"
            );
            assert!(
                !rendered.contains("compatibility_lowered_template_not_canonical"),
                "x86-64 generated rows should now resolve to .sla templates: {rendered}"
            );
        }
    }
}

#[test]
fn generated_runtime_decodes_ret_with_spec_derived_lift() {
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let decoded = decode_instruction(&compiled, None, &[0xC3], 0x1000).expect("generated ret");
    assert_eq!(decoded.length, 1);
    assert!(matches!(decoded.flow_kind, DecodedFlowKind::Return));
    assert_spec_derived_lift_or_typed_unsupported(&compiled, &[0xC3], 0x1000);
}

#[test]
fn generated_runtime_decodes_mov_imm64_without_compatibility_lift() {
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let bytes = [0x48, 0xB8, 0x34, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    let decoded = decode_instruction(&compiled, None, &bytes, 0x1000).expect("generated mov");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "mov");
    assert_spec_derived_lift_or_typed_unsupported(&compiled, &bytes, 0x1000);
}

#[test]
fn generated_runtime_decodes_jcc_rel8_without_compatibility_lift() {
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let decoded =
        decode_instruction(&compiled, None, &[0x75, 0x05], 0x1000).expect("generated jne");
    assert_eq!(decoded.length, 2);
    assert_eq!(decoded.mnemonic, "jnz");
    assert!(matches!(
        decoded.flow_kind,
        DecodedFlowKind::ConditionalJump
    ));
    assert_spec_derived_lift_or_typed_unsupported(&compiled, &[0x75, 0x05], 0x1000);
}

#[test]
fn generated_runtime_renders_jle_condition_mnemonic_display_only() {
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let decoded =
        decode_instruction(&compiled, None, &[0x7e, 0x05], 0x1000).expect("generated jle");
    assert_eq!(decoded.length, 2);
    assert_eq!(decoded.mnemonic, "jle");
    assert!(matches!(
        decoded.flow_kind,
        DecodedFlowKind::ConditionalJump
    ));
    assert_spec_derived_lift_or_typed_unsupported(&compiled, &[0x7e, 0x05], 0x1000);
}

#[test]
fn generated_runtime_decodes_startup_store_mov_mem32_imm32_without_compatibility_lift() {
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
    assert!(!details.compat_emitter_used);
    assert_eq!(ops.len(), 2);
    assert_eq!(ops[0].opcode, PcodeOpcode::Copy);
    assert_eq!(ops[1].opcode, PcodeOpcode::Store);
    assert_eq!(ops[1].inputs[1].space_id, 4);
    assert_eq!(ops[1].inputs[1].offset, 0);
    assert_eq!(ops[1].inputs[1].size, 8);
}

#[test]
fn generated_runtime_decodes_startup_sub_rsp_imm8_without_compatibility_lift() {
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let bytes = [0x48, 0x83, 0xEC, 0x28];
    let decoded =
        decode_instruction(&compiled, None, &bytes, 0x1000).expect("generated sub rsp, imm8");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "sub");
    assert_spec_derived_lift_or_typed_unsupported(&compiled, &bytes, 0x1000);
}

#[test]
fn generated_runtime_decodes_startup_rip_relative_load_without_compatibility_lift() {
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let bytes = [0x48, 0x8B, 0x05, 0x15, 0x30, 0x00, 0x00];
    let address = 0x1400_013e4;
    let decoded =
        decode_instruction(&compiled, None, &bytes, address).expect("generated rip-relative mov");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "mov");
    assert_spec_derived_lift_or_typed_unsupported(&compiled, &bytes, address);
    let (ops, length, details) = decode_and_lift_with_details(&compiled, None, &bytes, address)
        .expect("lift rip-relative mov");
    assert_eq!(length as usize, bytes.len());
    assert_eq!(
        details.template_source,
        Some(CompiledTemplateSource::SpecDerived)
    );
    assert!(!details.compat_emitter_used);
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
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let bytes = [0xE8, 0x1A, 0xFC, 0xFF, 0xFF];
    let decoded =
        decode_instruction(&compiled, None, &bytes, 0x1400_013ef).expect("generated call rel32");
    assert_eq!(decoded.length, bytes.len());
    assert!(matches!(decoded.flow_kind, DecodedFlowKind::Call));
    assert_spec_derived_lift_or_typed_unsupported(&compiled, &bytes, 0x1400_013ef);
}

#[test]
fn generated_runtime_records_decision_trace_for_startup_store() {
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let ctx = CompiledInstructionContext::parse(&[0xC7, 0x00, 0x01, 0x00, 0x00, 0x00], 0x1000)
        .expect("decode context");
    let selection =
        select_constructor(&compiled, "instruction", &ctx).expect("constructor selection");
    let state = bind_instruction(&compiled, None, &ctx, selection).expect("bind instruction");
    assert_eq!(state.match_trace.root_bucket, "global");
    assert!(!state.match_trace.probes.is_empty());
    assert!(!state.construct_nodes.is_empty());
    assert!(state.handles.iter().any(|handle| matches!(
        handle.spec,
        CompiledOperandSpec::TokenFieldExtraction { .. }
    )));
}

#[test]
fn generated_runtime_decodes_reg32_lea_without_decode_no_match_or_compatibility_lift() {
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let bytes = [0x8d, 0x04, 0x11];
    let decoded = decode_instruction(&compiled, None, &bytes, 0x1400_1450).expect("generated lea");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "lea");
    assert_spec_derived_lift_or_typed_unsupported(&compiled, &bytes, 0x1400_1450);
    let (ops, length, details) =
        decode_and_lift_with_details(&compiled, None, &bytes, 0x1400_1450).expect("lift lea");
    assert_eq!(length as usize, bytes.len());
    assert_eq!(
        details.template_source,
        Some(CompiledTemplateSource::SpecDerived)
    );
    assert!(!details.compat_emitter_used);
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
fn generated_runtime_decodes_rip_relative_mov32_without_decode_no_match() {
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
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let bytes = [0x48, 0x63, 0x41, 0x3c];
    let decoded =
        decode_instruction(&compiled, None, &bytes, 0x1400_2600).expect("generated movsxd");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "movsxd");
    assert_spec_derived_lift_or_typed_unsupported(&compiled, &bytes, 0x1400_2600);
}

#[test]
fn generated_runtime_zero_extends_reg32_decode_without_compatibility_lift() {
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let bytes = [0x31, 0xc0];
    let decoded =
        decode_instruction(&compiled, None, &bytes, 0x1400_19e0).expect("generated xor eax, eax");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "xor");
    assert_spec_derived_lift_or_typed_unsupported(&compiled, &bytes, 0x1400_19e0);
}

#[test]
fn generated_runtime_decodes_fninit_without_decode_no_match() {
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
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let bytes = [0xdb, 0xe3];
    assert_spec_derived_lift_or_typed_unsupported(&compiled, &bytes, 0x1400_25c0);
}

#[test]
fn generated_runtime_rejects_or_lifts_cmp_templates_without_compatibility() {
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let bytes = [0x83, 0xf9, 0x01];
    assert_spec_derived_lift_or_typed_unsupported(&compiled, &bytes, 0x1400_1485);
}

#[test]
fn generated_runtime_rejects_or_lifts_push_templates_without_compatibility() {
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let bytes = [0x41, 0x57];
    assert_spec_derived_lift_or_typed_unsupported(&compiled, &bytes, 0x1400_1470);
}

#[test]
fn generated_runtime_rejects_or_lifts_lea_templates_without_compatibility() {
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let bytes = [0x8d, 0x04, 0x11];
    assert_spec_derived_lift_or_typed_unsupported(&compiled, &bytes, 0x1400_1450);
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
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repo root")
        .to_path_buf();
    let aarch64_spec =
        repo_root.join("crates/fission-sleigh/specs/languages/AARCH64/AARCH64.slaspec");
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
fn compiled_table_policy_symbols_stay_architecture_neutral() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let files = [
        manifest_dir.join("src/runtime/spine/compiled_table/mod.rs"),
        manifest_dir.join("src/runtime/spine/compiled_table/selection.rs"),
        manifest_dir.join("src/runtime/spine/compiled_table/walker.rs"),
        manifest_dir.join("src/runtime/spine/compiled_table/token.rs"),
        manifest_dir.join("src/runtime/spine/decision.rs"),
    ];
    for file in files {
        let source = std::fs::read_to_string(&file)
            .unwrap_or_else(|error| panic!("read {}: {error}", file.display()));
        assert!(
            !source.contains("is_x86_compat_language"),
            "{} still uses architecture-named compatibility predicate",
            file.display()
        );
    }
}
