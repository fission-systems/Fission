use crate::rust_sleigh::{
    RustSleighDecompileConfig, RustSleighDecompileResult, RustSleighPipelineEvidence,
};
use crate::{
    NirBuildStats, PcodeOp, WrapperClass, render_contracted_wrapper_summary,
    summarize_direct_tail_wrapper_from_ops,
};
use fission_loader::loader::LoadedBinary;
use fission_sleigh::runtime::{PackedContextOverride, RuntimeSleighFrontend};
use std::time::Instant;

fn prepare_wrapper_probe_decode(
    binary: &LoadedBinary,
    lifter: &RuntimeSleighFrontend,
    entry_address: u64,
    function_size: usize,
    config: &RustSleighDecompileConfig,
) -> Option<(u64, Option<PackedContextOverride>, Vec<PcodeOp>)> {
    if !config.enable_wrapper_contraction_probe || config.wrapper_probe_instruction_limit == 0 {
        return None;
    }
    let address_state = lifter.normalize_low_bit_code_address(entry_address);
    let decode_entry_address = address_state.address;
    let initial_context_override =
        fission_static::analysis::function_discovery::decode_context_for_address(
            binary,
            lifter,
            entry_address,
            address_state.context_override,
        );

    let probe_max_bytes = if function_size > 0 {
        function_size.min(config.wrapper_probe_max_bytes.max(1))
    } else if config.use_next_function_distance_if_unknown {
        binary
            .function_after(decode_entry_address)
            .and_then(|next| {
                let dist = next.address.saturating_sub(decode_entry_address) as usize;
                (dist > 0).then_some(dist.min(config.wrapper_probe_max_bytes.max(1)))
            })
            .unwrap_or(config.wrapper_probe_max_bytes.max(1))
    } else {
        config.wrapper_probe_max_bytes.max(1)
    }
    .max(1);

    let probe_bytes = binary.view_bytes(decode_entry_address, probe_max_bytes)?;
    let (probe_ops, _, _) = lifter
        .decode_and_lift_with_context_override(
            &probe_bytes,
            decode_entry_address,
            initial_context_override,
        )
        .ok()?;

    Some((decode_entry_address, initial_context_override, probe_ops))
}

pub(crate) fn probe_wrapper_contraction(
    binary: &LoadedBinary,
    entry_address: u64,
    name: &str,
    function_size: usize,
    config: &RustSleighDecompileConfig,
    started_at: Instant,
) -> Result<Option<RustSleighDecompileResult>, String> {
    if !config.enable_wrapper_contraction_probe || config.wrapper_probe_instruction_limit == 0 {
        return Ok(None);
    }
    let load_spec = match binary.load_spec() {
        Some(load_spec) => load_spec,
        None => return Ok(None),
    };
    let lifter = match RuntimeSleighFrontend::new_for_load_spec(load_spec) {
        Ok(lifter) => lifter,
        Err(_) => return Ok(None),
    };
    let Some((decode_entry_address, _, probe_ops)) =
        prepare_wrapper_probe_decode(binary, &lifter, entry_address, function_size, config)
    else {
        return Ok(None);
    };

    let Some(summary) = summarize_direct_tail_wrapper_from_ops(
        &probe_ops,
        decode_entry_address,
        |target| {
            binary
                .function_at(target)
                .map(|func| func.name.clone())
                .unwrap_or_else(|| format!("sub_{target:x}"))
        },
        |target| {
            binary
                .function_at(target)
                .is_some_and(|func| func.is_import)
        },
    ) else {
        return Ok(None);
    };

    let render_start = Instant::now();
    let code = render_contracted_wrapper_summary(name, &summary);
    let mut build_stats = NirBuildStats {
        build_duration_ms: started_at.elapsed().as_millis() as usize,
        render_duration_ms: render_start.elapsed().as_millis() as usize,
        rendered_code_len: code.len(),
        procedure_summary_contracted_count: 1,
        ..NirBuildStats::default()
    };
    if let Some(proof) = summary.wrapper_contraction.as_ref() {
        match proof.wrapper_class {
            WrapperClass::TailForwarder | WrapperClass::PureAdapter => {
                build_stats.procedure_summary_tail_wrapper_count = 1;
            }
            WrapperClass::ImportThunk => {
                build_stats.procedure_summary_import_thunk_count = 1;
            }
            WrapperClass::None => {}
        }
    }

    // Wrapper contraction bypasses the render path; the output is the
    // contracted summary and there is no render snapshot to steal.
    Ok(Some(RustSleighDecompileResult {
        code,
        render_output: None,
        fell_back: false,
        fallback_reason: None,
        build_stats: Some(build_stats),
        hint_stats: None,
        evidence: RustSleighPipelineEvidence::default(),
        learned_facts: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::{prepare_wrapper_probe_decode, probe_wrapper_contraction};
    use crate::PcodeOpcode;
    use crate::rust_sleigh::RustSleighDecompileConfig;
    use fission_core::architecture::{
        BinaryLoadSpec, CompilerSpecId, GhidraLanguageId, LanguageCompilerSpecPair,
    };
    use fission_loader::loader::{DataBuffer, LoadedBinaryBuilder, SectionInfo};
    use fission_sleigh::runtime::RuntimeSleighFrontend;
    use std::time::Instant;

    #[test]
    fn wrapper_probe_respects_instruction_limit_zero() {
        let load_spec = BinaryLoadSpec {
            format: "ELF".to_string(),
            image_base: 0x1000,
            pair: LanguageCompilerSpecPair {
                language_id: GhidraLanguageId("x86:LE:64:default".to_string()),
                compiler_spec_id: CompilerSpecId("default".to_string()),
            },
            preferred: true,
            source: "test".to_string(),
        };
        // `jmp 0x2000` from 0x1000 -> `e9 fb 0f 00 00`
        let code = vec![0xe9, 0xfb, 0x0f, 0x00, 0x00, 0xc3];
        let binary = LoadedBinaryBuilder::new("jmp_wrap.elf".to_string(), DataBuffer::Heap(code))
            .format("ELF")
            .is_64bit(true)
            .load_spec(load_spec)
            .add_section(SectionInfo {
                name: ".text".to_string(),
                virtual_address: 0x1000,
                virtual_size: 6,
                file_offset: 0,
                file_size: 6,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            })
            .build()
            .expect("build x86-64 wrapper fixture");
        let lifter = RuntimeSleighFrontend::new_for_load_spec(binary.load_spec().expect("spec"))
            .expect("lifter");

        let mut disabled = RustSleighDecompileConfig::cli_defaults();
        disabled.wrapper_probe_instruction_limit = 0;
        assert!(
            prepare_wrapper_probe_decode(&binary, &lifter, 0x1000, 5, &disabled).is_none(),
            "limit 0 should suppress wrapper probe decode"
        );
        let res = probe_wrapper_contraction(&binary, 0x1000, "wrap", 5, &disabled, Instant::now())
            .expect("probe");
        assert!(res.is_none(), "limit 0 should suppress wrapper contraction");

        let enabled = RustSleighDecompileConfig::cli_defaults();
        let (addr, ctx, ops) = prepare_wrapper_probe_decode(&binary, &lifter, 0x1000, 5, &enabled)
            .expect("enabled wrapper probe should decode single-branch tail wrapper");
        assert_eq!(addr, 0x1000);
        assert!(ctx.is_none());
        assert!(
            ops.iter().any(|op| {
                op.opcode == PcodeOpcode::Branch
                    && op.inputs.first().is_some_and(|v| v.offset == 0x2000)
            }),
            "expected Branch target 0x2000 in ops={ops:?}"
        );
    }

    #[test]
    fn wrapper_probe_normalizes_odd_thumb_entry_address() {
        let load_spec = BinaryLoadSpec {
            format: "ELF".to_string(),
            image_base: 0x1000,
            pair: LanguageCompilerSpecPair {
                language_id: GhidraLanguageId("ARM:LE:32:Cortex".to_string()),
                compiler_spec_id: CompilerSpecId("default".to_string()),
            },
            preferred: true,
            source: "test".to_string(),
        };
        // Thumb-2 `b.w 0x1020` encoded at 0x1000 (PC = 0x1004, imm = +0x1c):
        // S=0, imm10=0 => 0xf000, J1=1, J2=1, imm11=0x00e => 0xb80e -> 00 f0 0e b8
        let code = vec![0x00, 0xf0, 0x0e, 0xb8, 0x00, 0xbf];
        let binary = LoadedBinaryBuilder::new("thumb_wrap.elf".to_string(), DataBuffer::Heap(code))
            .format("ELF")
            .is_64bit(false)
            .entry_point(0x1001)
            .load_spec(load_spec)
            .add_section(SectionInfo {
                name: ".text".to_string(),
                virtual_address: 0x1000,
                virtual_size: 6,
                file_offset: 0,
                file_size: 6,
                is_executable: true,
                is_readable: true,
                is_writable: false,
            })
            .build()
            .expect("build thumb wrapper fixture");
        let lifter = RuntimeSleighFrontend::new_for_load_spec(binary.load_spec().expect("spec"))
            .expect("lifter");

        let config = RustSleighDecompileConfig::cli_defaults();
        // Passing unnormalized odd Thumb address `0x1001` must strip the low bit
        // and decode at 0x1000 in Thumb mode rather than slicing at offset +1.
        let (addr, ctx, ops) = prepare_wrapper_probe_decode(&binary, &lifter, 0x1001, 4, &config)
            .expect("normalized odd Thumb address should decode Thumb b.w wrapper");
        assert_eq!(addr, 0x1000);
        assert!(
            ctx.is_some(),
            "odd Thumb address must produce Thumb context override"
        );
        assert!(
            ops.iter().any(|op| {
                op.opcode == PcodeOpcode::Branch
                    && op.inputs.first().is_some_and(|v| v.offset == 0x1020)
            }),
            "expected Thumb b.w target 0x1020 in ops={ops:?}"
        );
    }
}
