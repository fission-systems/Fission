use crate::rust_sleigh::{
    RustSleighDecompileConfig, RustSleighDecompileResult, RustSleighPipelineEvidence,
};
use fission_loader::loader::LoadedBinary;
use fission_pcode::{NirBuildStats, WrapperClass, render_contracted_wrapper_summary, summarize_direct_tail_wrapper_from_ops};
use fission_sleigh::runtime::RuntimeSleighFrontend;
use std::time::Instant;

pub(crate) fn probe_wrapper_contraction(
    binary: &LoadedBinary,
    entry_address: u64,
    name: &str,
    function_size: usize,
    config: &RustSleighDecompileConfig,
    started_at: Instant,
) -> Result<Option<RustSleighDecompileResult>, String> {
    if !config.enable_wrapper_contraction_probe {
        return Ok(None);
    }
    let probe_max_bytes = if function_size > 0 {
        function_size.min(config.wrapper_probe_max_bytes.max(1))
    } else if config.use_next_function_distance_if_unknown {
        binary
            .function_after(entry_address)
            .and_then(|next| {
                let dist = next.address.saturating_sub(entry_address) as usize;
                (dist > 0).then_some(dist.min(config.wrapper_probe_max_bytes.max(1)))
            })
            .unwrap_or(config.wrapper_probe_max_bytes.max(1))
    } else {
        config.wrapper_probe_max_bytes.max(1)
    }
    .max(1);

    let probe_bytes = match binary.view_bytes(entry_address, probe_max_bytes) {
        Some(bytes) => bytes,
        None => return Ok(None),
    };
    let load_spec = match binary.load_spec() {
        Some(load_spec) => load_spec,
        None => return Ok(None),
    };
    let lifter = match RuntimeSleighFrontend::new_for_load_spec(load_spec) {
        Ok(lifter) => lifter,
        Err(_) => return Ok(None),
    };
    let probe_ops = match lifter.decode_and_lift_with_len(&probe_bytes, entry_address) {
        Ok((ops, _)) => ops,
        Err(_) => return Ok(None),
    };

    let Some(summary) = summarize_direct_tail_wrapper_from_ops(
        &probe_ops,
        entry_address,
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

    Ok(Some(RustSleighDecompileResult {
        code,
        fell_back: false,
        fallback_reason: None,
        build_stats: Some(build_stats),
        hint_stats: None,
        evidence: RustSleighPipelineEvidence::default(),
    }))
}
