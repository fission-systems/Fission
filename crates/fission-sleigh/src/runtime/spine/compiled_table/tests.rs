use super::*;
use crate::compiler::{
    compile_frontend_for_entry_spec, compile_x86_64_frontend, discovery, spec_root_for_arch,
    CompiledOperandSpec, CompiledSpaceRef, CompiledTemplateSource,
};
use std::path::PathBuf;

macro_rules! require_packaged_ghidra_sla {
    () => {};
}

fn assert_spec_derived_lift(
    compiled: &CompiledFrontend,
    bytes: &[u8],
    address: u64,
) -> Vec<PcodeOp> {
    let (ops, length, details) = decode_and_lift_with_details(compiled, bytes, address)
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
    assert_eq!(checked_runtime_length_u64(7, "test").unwrap(), 7);
    if usize::BITS > u32::BITS {
        assert!(checked_runtime_length_u32(u32::MAX as usize + 1, "test").is_err());
    }
    assert_eq!(checked_context_commit_handle_index(3).unwrap(), 3);
}

#[test]
fn context_commit_const_space_check_requires_primary_space_metadata() {
    let handle = RuntimeHandle {
        operand_index: 0,
        spec: CompiledOperandSpec::ContextFieldExtraction {
            bit_offset: 0,
            bit_width: 1,
            sign_extend: false,
        },
        fixed: RuntimeFixedHandle::default(),
        debug_value: None,
        subtable_state: None,
    };

    let err = context_commit_handle_is_const_space(&handle)
        .expect_err("context commit handle must carry decoded primary space metadata");

    assert!(err
        .to_string()
        .contains("context commit handle missing primary space metadata"));

    let mut const_handle = handle;
    const_handle.fixed.space = Some(CompiledSpaceRef {
        name: "const".to_string(),
        index: 0,
        word_size: 1,
        addr_size: 8,
    });
    assert!(context_commit_handle_is_const_space(&const_handle).unwrap());
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
    let decoded = decode_instruction(&compiled, &[0xC3], 0x1000).expect("generated ret");
    assert_eq!(decoded.length, 1);
    assert!(matches!(decoded.flow_kind, DecodedFlowKind::Return));
    let ops = assert_spec_derived_lift(&compiled, &[0xC3], 0x1000);
    assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::Return));
}

/// Cross-checked against real Ghidra 12.0.4 (`InstructionPrototype.getInstructionMask()`,
/// via a headless script printing `bytes`/`mask` for this exact instruction) for
/// `48 b8 34 12 00 00 00 00 00 00` (`mov rax, 0x1234`): Ghidra prints
/// `mask: f8 f8 00 00 00 00 00 00 00 00` -- exact match.
///
/// The REX.W prefix byte (0) and the `B8+r` opcode byte (1) both come out
/// `0xf8` (top 5 bits fixed identity, low 3 bits free -- REX's `WRXB`
/// extension bits and the opcode's register-selection field, respectively);
/// the 8-byte immediate is fully unmasked. Byte 0 only shows up because
/// `replaced_wrapper_patterns` carries it forward: the REX byte's own
/// constructor matches just that byte then hands off entirely to the
/// constructor for the rest of the instruction (`constructor_replaces_current`
/// in `walker.rs`), and without `replaced_wrapper_patterns` recording that
/// wrapper's pattern before it gets discarded, this byte's mask silently
/// comes out as `0x00`.
#[test]
fn fid_mask_matches_ghidra_exactly_for_mov_imm64_including_rex_prefix() {
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let bytes = [0x48, 0xB8, 0x34, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    let state = decode_instruction_raw_state(&compiled, &bytes, 0x1000).expect("decode raw state");
    let mask = instruction_pattern_mask(&state, state.length);
    assert_eq!(
        mask,
        vec![0xf8, 0xf8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
    );
}

/// Cross-checked against real Ghidra 12.0.4 for `75 05` (`jnz +5`): Ghidra
/// prints `mask: ff 00` (opcode byte fully fixed, rel8 displacement fully
/// free). No prefix byte involved, so this is an exact match end to end --
/// unlike the `mov rax, imm64` case above, which has a known prefix-byte gap.
#[test]
fn fid_mask_matches_ghidra_exactly_for_prefix_free_jnz_rel8() {
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let bytes = [0x75, 0x05];
    let state = decode_instruction_raw_state(&compiled, &bytes, 0x1000).expect("decode raw state");
    let mask = instruction_pattern_mask(&state, state.length);
    assert_eq!(mask, vec![0xff, 0x00]);
}

/// Cross-checked against real Ghidra for `66 b8 34 12` (`mov ax, 0x1234`,
/// operand-size-override prefix rather than REX): Ghidra prints
/// `mask: ff f8 00 00` -- exact match. A second, independent prefix-wrapper
/// case (0x66 is a single fixed-value byte, unlike REX's `WRXB` extension
/// bits, so its mask is `0xff` not `0xf8`) confirming `replaced_wrapper_patterns`
/// isn't a REX-specific fix.
#[test]
fn fid_mask_matches_ghidra_exactly_for_operand_size_prefix() {
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let bytes = [0x66, 0xB8, 0x34, 0x12];
    let state = decode_instruction_raw_state(&compiled, &bytes, 0x1000).expect("decode raw state");
    let mask = instruction_pattern_mask(&state, state.length);
    assert_eq!(mask, vec![0xff, 0xf8, 0x00, 0x00]);
}

/// Cross-checked against real Ghidra 12.0.4 (headless, `FidService.hashFunction`)
/// for `55 48 89 e5 b8 2a 00 00 00 5d c3` (`push rbp; mov rbp,rsp; mov eax,0x2a;
/// pop rbp; ret`): Ghidra prints `FID full hash: 37783a7364fbdfe5`,
/// `FID code unit size: 5`.
///
/// Register offsets (RAX=0x0, RSP=0x20, RBP=0x28 -- the standard Ghidra
/// x86-64 SLEIGH register-space layout, independently confirmed earlier this
/// session via the register-locals feature work, e.g. RDX resolving to
/// 0x10) are hardcoded here rather than resolved through
/// `fission_pcode::midend::cspec::RegisterModel`, since that would require
/// a `fission-pcode` dependency this crate deliberately doesn't have (see
/// `fid_full_hash`'s doc comment).
#[test]
fn fid_full_hash_matches_ghidra_exactly_for_register_only_function() {
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let instruction_bytes: [&[u8]; 5] = [
        &[0x55],                         // push rbp
        &[0x48, 0x89, 0xE5],             // mov rbp, rsp
        &[0xB8, 0x2A, 0x00, 0x00, 0x00], // mov eax, 0x2a
        &[0x5D],                         // pop rbp
        &[0xC3],                         // ret
    ];
    let mut address = 0x1000u64;
    let mut extent = Vec::new();
    for bytes in instruction_bytes {
        let decoded = decode_instruction(&compiled, bytes, address).expect("decode instruction");
        address += decoded.length as u64;
        extent.push(decoded);
    }

    let resolve_register_offset = |name: &str| -> Option<i64> {
        match name.to_ascii_uppercase().as_str() {
            "RAX" | "EAX" => Some(0x0),
            "RSP" | "ESP" => Some(0x20),
            "RBP" | "EBP" => Some(0x28),
            _ => None,
        }
    };

    let hashes = fid_hash::compute_fid_hashes(&compiled, &extent, &resolve_register_offset)
        .expect("function has enough code units to hash");
    assert_eq!(hashes.full_count, 5);
    assert_eq!(hashes.full_hash, 0x37783a7364fbdfe5);
}

/// Cross-checked against real Ghidra 12.0.4 (headless, `FidService.hashFunction`)
/// for `55 48 89 e5 8b 45 08 5d c3` (`push rbp; mov rbp,rsp; mov eax,[rbp+8];
/// pop rbp; ret` -- same shape as the register-only case above but with the
/// immediate replaced by a `[rbp+8]` memory operand, exercising
/// `trace_simple_memory_address`/`mix_memory_operand_full` and the
/// `display_template.pieces`-derived operand ordering -- "mov eax,[rbp+8]"
/// has EAX at Ghidra display position 0 and the memory reference at
/// position 1, but `state.handles` orders them the other way around, with a
/// third, hidden, non-displayed handle (a zero-extend wrapper) alongside):
/// Ghidra prints `FID full hash: 82d2e963fd88461b`, `FID code unit size: 5`.
#[test]
fn fid_full_hash_matches_ghidra_exactly_for_function_with_memory_operand() {
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let instruction_bytes: [&[u8]; 5] = [
        &[0x55],             // push rbp
        &[0x48, 0x89, 0xE5], // mov rbp, rsp
        &[0x8B, 0x45, 0x08], // mov eax, [rbp+8]
        &[0x5D],             // pop rbp
        &[0xC3],             // ret
    ];
    let mut address = 0x1000u64;
    let mut extent = Vec::new();
    for bytes in instruction_bytes {
        let decoded = decode_instruction(&compiled, bytes, address).expect("decode instruction");
        address += decoded.length as u64;
        extent.push(decoded);
    }

    let resolve_register_offset = |name: &str| -> Option<i64> {
        match name.to_ascii_uppercase().as_str() {
            "RAX" | "EAX" => Some(0x0),
            "RSP" | "ESP" => Some(0x20),
            "RBP" | "EBP" => Some(0x28),
            _ => None,
        }
    };

    let hashes = fid_hash::compute_fid_hashes(&compiled, &extent, &resolve_register_offset)
        .expect("function has enough code units to hash");
    assert_eq!(hashes.full_count, 5);
    assert_eq!(hashes.full_hash, 0x82d2e963fd88461b);
}

/// Cross-checked against real Ghidra 12.0.4 (headless, `FidService.hashFunction`,
/// plus `Instruction.getOpObjects(ii)` printed directly) for three SIB-addressed
/// (`base+index*scale[+disp]`) variants of `xor edx,edx; mov eax,[SIB]; add
/// eax,edx; ret`, GCC-assembled so the byte encoding matches a real compiler's
/// SIB byte choices:
///
/// - `31 d2 8b 44 88 10 01 d0 c3` (`[rax+rcx*4+0x10]`, disp present):
///   `getOpObjects` prints `Register(RAX) Register(RCX) Scalar(0x4) Scalar(0x10)`
///   (scale *and* displacement both present as separate `Scalar` objects) --
///   `FID full hash: 45285b0d87470466`, `FID code unit size: 4`.
/// - `31 d2 8b 04 08 01 d0 c3` (`[rax+rcx*1]`, disp == 0, scale == 1):
///   `getOpObjects` prints `Register(RAX) Register(RCX) Scalar(0x1)` -- the
///   scale `Scalar` is present *even at scale == 1*, but the displacement
///   `Scalar` is omitted entirely when disp == 0 -- `FID full hash:
///   71e530ce7190c262`, `FID code unit size: 4`.
/// - `31 d2 8b 84 c8 00 01 00 00 01 d0 c3` (`[rax+rcx*8+0x100]`, disp present,
///   32-bit displacement encoding): `FID full hash: f66301fb4931933a`,
///   `FID code unit size: 4`.
///
/// Fission's own p-code for these three (inspected via `fission_cli
/// raw-pcode`) confirmed the two backward-trace shapes
/// `trace_simple_memory_address` needs to recognize: `IntAdd(base,disp) ->
/// IntMult(index,scale) -> IntAdd(combine)` when disp != 0, and directly
/// `IntMult(index,scale) -> IntAdd(base,combine)` when disp == 0 (no
/// intermediate base+disp op at all).
#[test]
fn fid_full_hash_matches_ghidra_exactly_for_sib_addressing() {
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let resolve_register_offset = |name: &str| -> Option<i64> {
        match name.to_ascii_uppercase().as_str() {
            "RAX" | "EAX" => Some(0x0),
            "RCX" | "ECX" => Some(0x8),
            "RDX" | "EDX" => Some(0x10),
            _ => None,
        }
    };
    let decode_extent = |instruction_bytes: &[&[u8]]| -> Vec<crate::runtime::DecodedInstruction> {
        let mut address = 0x1000u64;
        let mut extent = Vec::new();
        for bytes in instruction_bytes {
            let decoded =
                decode_instruction(&compiled, bytes, address).expect("decode instruction");
            address += decoded.length as u64;
            extent.push(decoded);
        }
        extent
    };

    // xor edx,edx; mov eax,[rax+rcx*4+0x10]; add eax,edx; ret
    let extent = decode_extent(&[
        &[0x31, 0xD2],
        &[0x8B, 0x44, 0x88, 0x10],
        &[0x01, 0xD0],
        &[0xC3],
    ]);
    let hashes = fid_hash::compute_fid_hashes(&compiled, &extent, &resolve_register_offset)
        .expect("SIB with scale+disp hashes");
    assert_eq!(hashes.full_count, 4);
    assert_eq!(hashes.full_hash, 0x45285b0d87470466);

    // xor edx,edx; mov eax,[rax+rcx*1]; add eax,edx; ret
    let extent = decode_extent(&[&[0x31, 0xD2], &[0x8B, 0x04, 0x08], &[0x01, 0xD0], &[0xC3]]);
    let hashes = fid_hash::compute_fid_hashes(&compiled, &extent, &resolve_register_offset)
        .expect("SIB with scale==1, disp==0 hashes");
    assert_eq!(hashes.full_count, 4);
    assert_eq!(hashes.full_hash, 0x71e530ce7190c262);

    // xor edx,edx; mov eax,[rax+rcx*8+0x100]; add eax,edx; ret
    let extent = decode_extent(&[
        &[0x31, 0xD2],
        &[0x8B, 0x84, 0xC8, 0x00, 0x01, 0x00, 0x00],
        &[0x01, 0xD0],
        &[0xC3],
    ]);
    let hashes = fid_hash::compute_fid_hashes(&compiled, &extent, &resolve_register_offset)
        .expect("SIB with scale+32-bit disp hashes");
    assert_eq!(hashes.full_count, 4);
    assert_eq!(hashes.full_hash, 0xf66301fb4931933a);
}

#[test]
fn generated_runtime_decodes_mov_imm64_without_compatibility_lift() {
    require_packaged_ghidra_sla!();
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let bytes = [0x48, 0xB8, 0x34, 0x12, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
    let decoded = decode_instruction(&compiled, &bytes, 0x1000).expect("generated mov");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "mov");
    let ops = assert_spec_derived_lift(&compiled, &bytes, 0x1000);
    assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::Copy));
}

#[test]
fn generated_runtime_decodes_jcc_rel8_without_compatibility_lift() {
    require_packaged_ghidra_sla!();
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let decoded = decode_instruction(&compiled, &[0x75, 0x05], 0x1000).expect("generated jne");
    assert_eq!(decoded.length, 2);
    assert_eq!(decoded.mnemonic, "jnz");
    assert!(matches!(
        decoded.flow_kind,
        DecodedFlowKind::ConditionalJump
    ));
    assert_eq!(decoded.direct_target, Some(0x1007));
    assert_eq!(
        decoded.references.first().map(|reference| reference.kind),
        Some(DecodedReferenceKind::BranchTarget)
    );
    assert_eq!(
        decoded.references.first().map(|reference| reference.target),
        Some(0x1007)
    );
    let ops = assert_spec_derived_lift(&compiled, &[0x75, 0x05], 0x1000);
    assert!(ops.iter().any(|op| op.opcode == PcodeOpcode::CBranch));
}

#[test]
fn generated_runtime_renders_jle_condition_mnemonic_display_only() {
    require_packaged_ghidra_sla!();
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let decoded = decode_instruction(&compiled, &[0x7e, 0x05], 0x1000).expect("generated jle");
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
        decode_instruction(&compiled, &bytes, 0x1000).expect("generated mov [rax], imm32");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "mov");
    let (ops, length, details) =
        decode_and_lift_with_details(&compiled, &bytes, 0x1000).expect("lift mov [rax], imm32");
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
    let decoded = decode_instruction(&compiled, &bytes, 0x1000).expect("generated sub rsp, imm8");
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
        decode_instruction(&compiled, &bytes, address).expect("generated rip-relative mov");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "mov");
    let (ops, length, details) =
        decode_and_lift_with_details(&compiled, &bytes, address).expect("lift rip-relative mov");
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
        decode_instruction(&compiled, &bytes, 0x1400_013ef).expect("generated call rel32");
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
        decode_instruction(&compiled, &bytes, 0x4014e3).expect("decode mov moffs32, imm32");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "mov");

    let (ops, length, details) =
        decode_and_lift_with_details(&compiled, &bytes, 0x4014e3).expect("lift mov moffs32, imm32");
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

    let decoded = decode_instruction(&compiled, &bytes, 0x4014ed).expect("decode call rel32");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "call");

    let (ops, length, details) =
        decode_and_lift_with_details(&compiled, &bytes, 0x4014ed).expect("lift call rel32");
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
    let strategy = RuntimeDecodeStrategy::for_table();
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
    let decoded = decode_instruction(&compiled, &bytes, 0x1400_1450).expect("generated lea");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "lea");
    let (ops, length, details) =
        decode_and_lift_with_details(&compiled, &bytes, 0x1400_1450).expect("lift lea");
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
    let (ops, length, details) = decode_and_lift_with_details(&compiled, &bytes, 0x1400_148e)
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
    let (ops, length, details) = decode_and_lift_with_details(&compiled, &bytes, 0x1800_85d0)
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
    let decoded =
        decode_instruction(&compiled, &bytes, 0x1400_19c0).expect("generated mov rip-relative");
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
    let decoded = decode_instruction(&compiled, &bytes, 0x1400_2600).expect("generated movsxd");
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
        decode_instruction(&compiled, &bytes, 0x1400_19e0).expect("generated xor eax, eax");
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
        decode_instruction(&compiled, &bytes, 0x1400_25c0).expect("generated fninit decode");
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
    let decoded = decode_instruction(&compiled, &bytes, 0x100000).expect("decode aarch64");
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

    let decoded = decode_instruction(&compiled, &bytes, 0x10000c).expect("decode movk");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "movk");

    let (ops, length, details) = decode_and_lift_with_details(&compiled, &bytes, 0x10000c)
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

    let decoded = decode_instruction(&compiled, &bytes, 0x100058).expect("decode cneg");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "cneg");

    let (ops, length, details) =
        decode_and_lift_with_details(&compiled, &bytes, 0x100058).expect("lift cneg");
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

    let decoded = decode_instruction(&compiled, &bytes, 0x100054).expect("decode subs");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "subs");

    let (ops, length, details) =
        decode_and_lift_with_details(&compiled, &bytes, 0x100054).expect("lift subs");
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

    let decoded = decode_instruction(&compiled, &bytes, 0x10002c).expect("decode udiv");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "udiv");

    let (ops, length, details) =
        decode_and_lift_with_details(&compiled, &bytes, 0x10002c).expect("lift udiv");
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

    let decoded = decode_instruction(&compiled, &bytes, 0x1000b8).expect("decode mov");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "mov");

    let (ops, length, details) =
        decode_and_lift_with_details(&compiled, &bytes, 0x1000b8).expect("lift mov");
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

    let decoded = decode_instruction(&compiled, &bytes, 0x100590).expect("decode lui");
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

    let decoded = decode_instruction(&compiled, &bytes, 0x102e8).expect("decode stmdb");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "stmdb");

    let (ops, length, details) =
        decode_and_lift_with_details(&compiled, &bytes, 0x102e8).expect("lift ARM mode stmdb");
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

    let decoded = decode_instruction(&compiled, &bytes, 0x100044).expect("decode subcc");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "subcc");

    let (ops, length, details) = decode_and_lift_with_details(&compiled, &bytes, 0x100044)
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

        let (_ops, length, details) = decode_and_lift_with_details(&compiled, &bytes, 0x100016)
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

    let decoded = decode_instruction(&compiled, &bytes, 0x100048).expect("decode sbc");
    assert_eq!(decoded.length, bytes.len());
    assert_eq!(decoded.mnemonic, "sbc");

    let (ops, length, details) = decode_and_lift_with_details(&compiled, &bytes, 0x100048)
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
    let const_space_runtime_materialization = [
        "name: \"const\".to_string()",
        "word_size: 0",
        "addr_size: 0",
    ]
    .join("\n");
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
    let token_field_checked_lossy_end = "checked_add(encoded_size as usize)";
    let constructor_minimum_checked_lossy_end =
        "checked_add(self.selection.constructor.minimum_length as usize)";
    let token_read_unchecked_end = "offset + size as usize";
    let token_read_unchecked_absolute = "base_cursor + off";
    let operand_offset_lossy_signed_add = "base as i64 + i64::from(reloffset)";
    let subtable_offset_lossy_signed_add = "base as i64 + i64::from(rel)";
    let relative_offset_lossy_usize_cast = "Ok(offset as usize)";
    let constructor_cursor_unchecked_add = "cursor: ctx.cursor + opcode_len";
    let export_inst_next_saturating = "self.ctx.address.saturating_add(self.minimum_length as u64)";
    let export_inst_next_lossy_length = "checked_add(self.minimum_length as u64)";
    let pattern_inst_next_saturating_constructor =
        "saturating_add(self.selection.constructor.minimum_length as usize)";
    let pattern_inst_next_saturating_address =
        "self.ctx.address.saturating_add(next_offset as u64)";
    let pattern_inst_next_lossy_offset = "checked_add(next_offset as u64)";
    let subtable_cursor_saturating_delta = "self.cursor.saturating_sub(self.ctx.cursor)";
    let subtable_decode_address_wrapping = "sub_ctx.address.wrapping_add(sub_ctx.cursor as u64)";
    let subtable_decode_address_lossy_cursor = "checked_add(ctx.cursor as u64)";
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
    let non_offset_handle_plus_wrapping_fallback = "wrapping_add(*plus)";
    let context_commit_inst_next_saturating =
        "instruction_address.saturating_add(decoded.length as u64)";
    let decoded_length_lossy_cast = "decoded.length as u64";
    let template_inst_length_lossy_cast = "checked_add(inst_length as u64)";
    let template_memory_offset_lossy_cast = "})? as usize";
    let relative_label_encode_lossy_cast = "label_num as i64";
    let relative_label_decode_lossy_cast = "sentinel as i64";
    let build_operand_index_lossy_cast = "Some(*value as usize)";
    let selector_lossy_usize_cast = ["selector", "as", "usize"].join(" ");
    let selector_lossy_u32_cast = ["selector", "as", "u32"].join(" ");
    let selector_index_lossy_u32_cast = ["selector_index", "as", "u32"].join(" ");
    let delay_slot_length_lossy_cast_local = "slot_state.length as u32";
    let template_positive_int_lossy_cast = "Ok(*value as u64)";
    let signed_value_lossy_cast = ["value", "as", "u64"].join(" ");
    let template_state_length_lossy_cast = "state.length as u64";
    let template_space_size_lossy_cast = "space.addr_size as u64";
    let handle_size_lossy_cast = "handle.fixed.size as u64";
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
    let context_commit_missing_space_non_const_fallback =
        [".map(|s| s.name == ", "\"const\")", ".unwrap_or(false)"].join("");
    let static_handle_addr_unit_max_fallback = [".word_size", ".max(1) as u64"].join("\n");
    let context_commit_addr_unit_wrap = "offset.wrapping_mul(addr_unit)";
    let static_handle_addr_unit_wrap = "offset_offset.wrapping_mul(addr_unit)";
    let swallowed_context_word_error = "let _ = set_packed_context_word";
    let packed_context_byte_remainder_lossy =
        "let remaining = bytesize as i32 - 4 + byte_offset as i32";
    let packed_context_bit_remainder_lossy =
        "let remaining = bitsize as i32 - 32 + bit_offset as i32";
    let decoded_bytes_truncation_fallback = "bytes.get(..length).unwrap_or(bytes)";
    let immediate_byte_unchecked_shift = "u64::from(*byte) << (index * 8)";
    let tokenfield_saturating_range = "byte_end.saturating_sub(byte_start)";
    let tokenfield_saturating_bit_range = "bit_end.saturating_sub(bit_start)";
    let encoded_size_max_fallback = "((*byte_end - *byte_start) + 1).max(1)";
    let tokenfield_unchecked_accumulation = "res = (res << 8) | u64::from(byte)";
    let tokenfield_unchecked_right_shift = "res >> (shift as u32)";
    let tokenfield_unchecked_left_shift = "res << ((-shift) as u32)";
    let tokenfield_offset_lossy_cast = "        } as usize;";
    let immediate_size_lossy_cast = "let end = offset\n        .checked_add(size as usize)";
    let signed_immediate_unnamed_cast = "((value << shift) as i64)";
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
    let context_expr_lossy_value_word_cast = "Ok(value as u32)";
    let context_expr_unchecked_left_shift = "raw << (change.shift as u32)";
    let context_expr_unchecked_right_shift = "raw >> ((-change.shift) as u32)";
    let context_expr_lossy_left_shift = ".checked_shl(shift as u32)";
    let context_expr_lossy_right_shift = ".checked_neg()\n            .ok_or_else(|| anyhow!(\"context expression shift underflow\"))?\n            as u32";
    let pattern_token_lossy_i64_cast = ")? as i64)";
    let pattern_token_at_lossy_i64_cast = ")? as i64),";
    let pattern_context_lossy_sign_extend = "Ok(((raw << shift) as i64) >> shift)";
    let operand_context_lossy_sign_extend = "((val << shift) as i64 >> shift) as u64";
    let pattern_context_lossy_raw_cast = "Ok(raw as i64)";
    let pattern_address_lossy_cast = "self.ctx.address as i64";
    let pattern_inst_next_lossy_result_cast = "as i64)";
    let pattern_operand_const_lossy_cast = "return Ok(fixed.offset_offset as i64);";
    let pattern_right_shift_lossy_lhs_cast = "let shifted = (lhs as u64)";
    let pattern_right_shift_lossy_result_cast = "Ok(shifted as i64)";
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
    let decision_probe_error_swallow = "evaluator.probe_values(probe).ok()?";
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
            !source.contains(&const_space_runtime_materialization),
            "{} still materializes missing SLA const-space metadata instead of failing closed",
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
                && !source.contains(token_field_unchecked_end)
                && !source.contains(token_field_checked_lossy_end)
                && !source.contains(constructor_minimum_checked_lossy_end),
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
                && !source.contains(&context_commit_missing_space_non_const_fallback)
                && !source.contains(&static_handle_addr_unit_max_fallback)
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
            !source.contains(packed_context_byte_remainder_lossy)
                && !source.contains(packed_context_bit_remainder_lossy),
            "{} still computes packed context cross-word remainders through lossy casts",
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
                && !source.contains(tokenfield_saturating_bit_range)
                && !source.contains(encoded_size_max_fallback),
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
            !source.contains(tokenfield_offset_lossy_cast)
                && !source.contains(immediate_size_lossy_cast)
                && !source.contains(signed_immediate_unnamed_cast),
            "{} still uses lossy token/immediate casts",
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
                && !source.contains(subtable_decode_address_lossy_cursor)
                && !source.contains(&delay_slot_length_lossy_cast)
                && !source.contains(&bind_target_offset_lossy_cast)
                && !source.contains(&context_commit_handle_lossy_cast)
                && !source.contains(template_delay_slot_inst_next_saturating)
                && !source.contains(template_delay_slot_pc_saturating)
                && !source.contains(template_delay_slot_fall_saturating)
                && !source.contains(export_inst_next_lossy_length)
                && !source.contains(template_const_inst_next_saturating)
                && !source.contains(template_const_inst_next_zero_fallback)
                && !source.contains(template_const_inst_next2_saturating)
                && !source.contains(pattern_inst_next_lossy_offset)
                && !source.contains(non_offset_handle_plus_wrapping_fallback)
                && !source.contains(context_commit_inst_next_saturating)
                && !source.contains(decoded_length_lossy_cast),
            "{} still hides malformed SLA parser cursor, InstNext arithmetic, or non-offset_plus handle plus",
            file.display()
        );
        assert!(
            !source.contains(template_inst_length_lossy_cast)
                && !source.contains(template_memory_offset_lossy_cast)
                && !source.contains(relative_label_encode_lossy_cast)
                && !source.contains(relative_label_decode_lossy_cast)
                && !source.contains(build_operand_index_lossy_cast)
                && !source.contains(&selector_lossy_usize_cast)
                && !source.contains(&selector_lossy_u32_cast)
                && !source.contains(&selector_index_lossy_u32_cast)
                && !source.contains(delay_slot_length_lossy_cast_local)
                && !source.contains(template_positive_int_lossy_cast)
                && !source.contains(&signed_value_lossy_cast)
                && !source.contains(template_state_length_lossy_cast)
                && !source.contains(template_space_size_lossy_cast)
                && !source.contains(handle_size_lossy_cast),
            "{} still uses lossy template evaluator or selector integer casts",
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
                && !source.contains(context_expr_lossy_value_word_cast)
                && !source.contains(context_expr_unchecked_left_shift)
                && !source.contains(context_expr_unchecked_right_shift)
                && !source.contains(context_expr_lossy_left_shift)
                && !source.contains(context_expr_lossy_right_shift),
            "{} still truncates or unchecked-shifts context-change pattern expressions",
            file.display()
        );
        assert!(
            !source.contains(pattern_token_lossy_i64_cast)
                && !source.contains(pattern_token_at_lossy_i64_cast)
                && !source.contains(pattern_context_lossy_sign_extend)
                && !source.contains(operand_context_lossy_sign_extend)
                && !source.contains(pattern_context_lossy_raw_cast)
                && !source.contains(pattern_address_lossy_cast)
                && !source.contains(pattern_inst_next_lossy_result_cast)
                && !source.contains(pattern_operand_const_lossy_cast)
                && !source.contains(pattern_right_shift_lossy_lhs_cast)
                && !source.contains(pattern_right_shift_lossy_result_cast),
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
            !source.contains(decision_probe_error_swallow),
            "{} still hides decision-probe evaluator errors as constructor no-match",
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

/// RIP-relative with trailing immediates must use full instruction length for
/// `inst_next` (Intel: relative to end of whole insn). Real musl alloc_meta encodings.
#[test]
fn rip_relative_trailing_imm_inst_next_targets() {
    require_packaged_ghidra_sla!();
    let compiled = compile_x86_64_frontend().expect("compile");

    let ram_off = |ops: &[PcodeOp]| -> Option<u64> {
        ops.iter().find_map(|op| {
            op.output
                .as_ref()
                .filter(|o| o.space_id == 3)
                .map(|o| o.offset)
                .or_else(|| {
                    op.inputs
                        .iter()
                        .find(|vn| vn.space_id == 3)
                        .map(|vn| vn.offset)
                })
        })
    };

    // mov qword [rip+0x511e], rax @ 0x1002E1B → 0x1007F40
    let bytes = [0x48u8, 0x89, 0x05, 0x1e, 0x51, 0x00, 0x00];
    let (ops, len, _) =
        decode_and_lift_with_details(&compiled, &bytes, 0x1002E1B).expect("lift mov");
    assert_eq!(len as usize, 7);
    assert_eq!(ram_off(&ops), Some(0x1007F40));

    // add qword [rip+0x511e], 1 @ 0x1002E22 → 0x1007F48 (was off-by-1 before two-pass)
    let bytes = [0x48u8, 0x83, 0x05, 0x1e, 0x51, 0x00, 0x00, 0x01];
    let (ops, len, _) =
        decode_and_lift_with_details(&compiled, &bytes, 0x1002E22).expect("lift add");
    assert_eq!(len as usize, 8);
    assert_eq!(ram_off(&ops), Some(0x1007F48));

    // mov dword [rip+0x527b], 1 @ 0x1002C9B → 0x1007F20 (was off-by-4)
    let bytes = [0xc7u8, 0x05, 0x7b, 0x52, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00];
    let (ops, len, _) =
        decode_and_lift_with_details(&compiled, &bytes, 0x1002C9B).expect("lift mov imm");
    assert_eq!(len as usize, 10);
    assert_eq!(ram_off(&ops), Some(0x1007F20));
}

/// Cross-checked against real Ghidra 12.0.4 for `31 d2 8b 05 00 01 00 00 01
/// d0 c3` (`xor edx,edx; mov eax,[rip+0x100]; add eax,edx; ret`, GCC-assembled
/// so the RIP-relative encoding matches a real compiler, at address `0x401702`
/// so the resolved absolute target matches the real binary's `0x40180a`
/// exactly): Ghidra's `getOpObjects` for the `mov`'s memory operand prints a
/// single `GenericAddress(0040180a)` object -- Fission's own runtime
/// independently classifies this same operand as `BoundOperand::Immediate`
/// (RIP-relative displacement resolves to a literal target address at decode
/// time, confirmed via `decode_instruction_raw_state` -- `debug_value:
/// Some(Immediate { value: 0x40180a, .. })`), which `mix_operand_full`
/// already handles. That turns out to be *correct*, not a coincidence that
/// happens to cancel out: `MessageDigestFidHasher.java` treats `Address` and
/// `Scalar` objects identically for the *full* hash (`fullUpdate +=
/// 0xfeeddead` either way -- they only diverge for the *specific* hash,
/// which isn't implemented here). No code change was needed for this case;
/// this test exists to prove that rather than assume it.
/// `FID full hash: 3768fc2909545fcc`, `FID code unit size: 4`.
#[test]
fn fid_full_hash_matches_ghidra_exactly_for_rip_relative_memory_load() {
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let resolve_register_offset = |name: &str| -> Option<i64> {
        match name.to_ascii_uppercase().as_str() {
            "RAX" | "EAX" => Some(0x0),
            "RDX" | "EDX" => Some(0x10),
            _ => None,
        }
    };
    let instruction_bytes: [&[u8]; 4] = [
        &[0x31, 0xD2],                         // xor edx,edx
        &[0x8B, 0x05, 0x00, 0x01, 0x00, 0x00], // mov eax,[rip+0x100]
        &[0x01, 0xD0],                         // add eax,edx
        &[0xC3],                               // ret
    ];
    let mut address = 0x401702u64;
    let mut extent = Vec::new();
    for bytes in instruction_bytes {
        let decoded = decode_instruction(&compiled, bytes, address).expect("decode instruction");
        address += decoded.length as u64;
        extent.push(decoded);
    }
    let hashes = fid_hash::compute_fid_hashes(&compiled, &extent, &resolve_register_offset)
        .expect("RIP-relative memory load hashes");
    assert_eq!(hashes.full_count, 4);
    assert_eq!(hashes.full_hash, 0x3768fc2909545fcc);
}

/// Cross-checked against real Ghidra 12.0.4 for `31 d2 48 8d 05 00 02 00 00
/// 48 01 d0 c3` (`xor edx,edx; lea rax,[rip+0x200]; add rax,rdx; ret` at
/// `0x40170d`, matching the real binary's resolved LEA target `0x401916`):
/// `LEA` doesn't dereference memory, so Ghidra's `getOpObjects` for its
/// RIP-relative operand prints `Scalar(0x401916)` instead of an `Address` --
/// a different Java object type than the memory-load case above, but per
/// the same full-hash-treats-them-identically fact, this needed no special
/// handling either; Fission also classifies it as `BoundOperand::Immediate`.
/// `FID full hash: ae465fd70004f692`, `FID code unit size: 4`.
#[test]
fn fid_full_hash_matches_ghidra_exactly_for_rip_relative_lea() {
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let resolve_register_offset = |name: &str| -> Option<i64> {
        match name.to_ascii_uppercase().as_str() {
            "RAX" | "EAX" => Some(0x0),
            "RDX" | "EDX" => Some(0x10),
            _ => None,
        }
    };
    let instruction_bytes: [&[u8]; 4] = [
        &[0x31, 0xD2],                               // xor edx,edx
        &[0x48, 0x8D, 0x05, 0x00, 0x02, 0x00, 0x00], // lea rax,[rip+0x200]
        &[0x48, 0x01, 0xD0],                         // add rax,rdx
        &[0xC3],                                     // ret
    ];
    let mut address = 0x40170du64;
    let mut extent = Vec::new();
    for bytes in instruction_bytes {
        let decoded = decode_instruction(&compiled, bytes, address).expect("decode instruction");
        address += decoded.length as u64;
        extent.push(decoded);
    }
    let hashes = fid_hash::compute_fid_hashes(&compiled, &extent, &resolve_register_offset)
        .expect("RIP-relative LEA hashes");
    assert_eq!(hashes.full_count, 4);
    assert_eq!(hashes.full_hash, 0xae465fd70004f692);
}

/// Cross-checked against real Ghidra 12.0.4 (`FidService.hashFunction`, plus
/// `Instruction.getOperandType(ii)`/`OperandType.isScalar`/`isAddress`
/// printed directly) for six `xor edx,edx; <op>; add eax/rax,edx/rdx; ret`
/// GCC-compiled variants, isolating each operand-classification case the
/// specific hash needs to distinguish:
///
/// - `imm_func` (`mov eax,0x2a`): plain immediate, `isScalar=true
///   isAddress=false` -- real value used, counted.
/// - `rip_load_func` (`mov eax,[rip+0x100]`): RIP-relative memory
///   dereference, `isScalar=false isAddress=true` (an `Address` object, not
///   `Scalar`) -- placeholder, not counted.
/// - `rip_lea_func` (`lea rax,[rip+0x200]`): RIP-relative *computed value*,
///   `isScalar=true isAddress=false` -- despite also being RIP-relative,
///   the opposite classification from the memory-load case, since `LEA`
///   computes a value rather than dereferencing one -- real value used,
///   counted. (Fission distinguishes the two via `RuntimeFixedHandle::space`:
///   `"ram"` for the dereference, `"const"` for `LEA`.)
/// - `sib_func` (`mov eax,[rax+rcx*4+0x10]`): compound/dynamic operand,
///   `isScalar=false isAddress=false` -- both the scale and displacement
///   sub-scalars are small enough (`-256 < v < 256`) to use their real
///   values, both counted.
/// - `call_func` (`call imm_func`): direct call target, `isScalar=false
///   isAddress=true` -- placeholder, not counted; also drops out of
///   `fullCount` (3, not 4) since `CALL` code units are excluded from the
///   reported count while still being hashed.
/// - `abs_addr_func` (`mov eax,ds:0x404040`, `-no-pie` so it's a real
///   absolute address rather than RIP-relative): same classification as
///   `rip_load_func` -- `isAddress=true`, placeholder, not counted.
#[test]
fn fid_hashes_match_ghidra_exactly_for_specific_hash_operand_classification() {
    let compiled = compile_x86_64_frontend().expect("compile frontend");
    let resolve_register_offset = |name: &str| -> Option<i64> {
        match name.to_ascii_uppercase().as_str() {
            "RAX" | "EAX" => Some(0x0),
            "RCX" | "ECX" => Some(0x8),
            "RDX" | "EDX" => Some(0x10),
            _ => None,
        }
    };
    let decode_extent =
        |address: u64, instruction_bytes: &[&[u8]]| -> Vec<crate::runtime::DecodedInstruction> {
            let mut address = address;
            let mut extent = Vec::new();
            for bytes in instruction_bytes {
                let decoded =
                    decode_instruction(&compiled, bytes, address).expect("decode instruction");
                address += decoded.length as u64;
                extent.push(decoded);
            }
            extent
        };

    // imm_func @ 0x40171e: xor edx,edx; mov eax,0x2a; add eax,edx; ret
    let extent = decode_extent(
        0x40171e,
        &[
            &[0x31, 0xD2],
            &[0xB8, 0x2A, 0x00, 0x00, 0x00],
            &[0x01, 0xD0],
            &[0xC3],
        ],
    );
    let hashes = fid_hash::compute_fid_hashes(&compiled, &extent, &resolve_register_offset)
        .expect("imm_func hashes");
    assert_eq!(hashes.full_count, 4);
    assert_eq!(hashes.full_hash, 0xc4654e18387e22d8);
    assert_eq!(hashes.specific_count, 1);
    assert_eq!(hashes.specific_hash, 0x860bebdb442635e3);

    // rip_load_func @ 0x401728: xor edx,edx; mov eax,[rip+0x100]; add eax,edx; ret
    let extent = decode_extent(
        0x401728,
        &[
            &[0x31, 0xD2],
            &[0x8B, 0x05, 0x00, 0x01, 0x00, 0x00],
            &[0x01, 0xD0],
            &[0xC3],
        ],
    );
    let hashes = fid_hash::compute_fid_hashes(&compiled, &extent, &resolve_register_offset)
        .expect("rip_load_func hashes");
    assert_eq!(hashes.full_count, 4);
    assert_eq!(hashes.full_hash, 0x3768fc2909545fcc);
    assert_eq!(hashes.specific_count, 0);
    assert_eq!(hashes.specific_hash, 0xa3e9a2fb37c9be98);

    // rip_lea_func @ 0x401733: xor edx,edx; lea rax,[rip+0x200]; add rax,rdx; ret
    let extent = decode_extent(
        0x401733,
        &[
            &[0x31, 0xD2],
            &[0x48, 0x8D, 0x05, 0x00, 0x02, 0x00, 0x00],
            &[0x48, 0x01, 0xD0],
            &[0xC3],
        ],
    );
    let hashes = fid_hash::compute_fid_hashes(&compiled, &extent, &resolve_register_offset)
        .expect("rip_lea_func hashes");
    assert_eq!(hashes.full_count, 4);
    assert_eq!(hashes.full_hash, 0xae465fd70004f692);
    assert_eq!(hashes.specific_count, 1);
    assert_eq!(hashes.specific_hash, 0x49aeb0721995d677);

    // sib_func @ 0x401740: xor edx,edx; mov eax,[rax+rcx*4+0x10]; add eax,edx; ret
    let extent = decode_extent(
        0x401740,
        &[
            &[0x31, 0xD2],
            &[0x8B, 0x44, 0x88, 0x10],
            &[0x01, 0xD0],
            &[0xC3],
        ],
    );
    let hashes = fid_hash::compute_fid_hashes(&compiled, &extent, &resolve_register_offset)
        .expect("sib_func hashes");
    assert_eq!(hashes.full_count, 4);
    assert_eq!(hashes.full_hash, 0x45285b0d87470466);
    assert_eq!(hashes.specific_count, 2);
    assert_eq!(hashes.specific_hash, 0x4a89d3b5375081ca);

    // call_func @ 0x401749: xor edx,edx; call imm_func; add eax,edx; ret
    let extent = decode_extent(
        0x401749,
        &[
            &[0x31, 0xD2],
            &[0xE8, 0xCE, 0xFF, 0xFF, 0xFF],
            &[0x01, 0xD0],
            &[0xC3],
        ],
    );
    let hashes = fid_hash::compute_fid_hashes(&compiled, &extent, &resolve_register_offset)
        .expect("call_func hashes");
    assert_eq!(hashes.full_count, 3);
    assert_eq!(hashes.full_hash, 0xd6299a1775049934);
    assert_eq!(hashes.specific_count, 0);
    assert_eq!(hashes.specific_hash, 0x743b8c40dc55c620);

    // abs_addr_func @ 0x401753: xor edx,edx; mov eax,ds:0x404040; add eax,edx; ret
    let extent = decode_extent(
        0x401753,
        &[
            &[0x31, 0xD2],
            &[0x8B, 0x04, 0x25, 0x40, 0x40, 0x40, 0x00],
            &[0x01, 0xD0],
            &[0xC3],
        ],
    );
    let hashes = fid_hash::compute_fid_hashes(&compiled, &extent, &resolve_register_offset)
        .expect("abs_addr_func hashes");
    assert_eq!(hashes.full_count, 4);
    assert_eq!(hashes.full_hash, 0x3f84cd43d7843202);
    assert_eq!(hashes.specific_count, 0);
    assert_eq!(hashes.specific_hash, 0x7f404fe629d3715e);
}
