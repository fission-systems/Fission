use super::FactStore;
use super::nir_types::NirWorkerRequest;
use super::nir_worker::{execute_nir_worker_request, nir_worker_timeout_ms};
use fission_loader::loader::LoadedBinary;
use fission_pcode::{
    NirBuildStats, NirHintStats, NirRenderOptions, NirTypeContext, PcodeFunction, PcodeOpcode,
    PcodeOptimizer, PcodeOptimizerConfig, pcode_has_indirect_control_flow, render_nir_with_context,
    take_last_nir_build_stats, take_last_nir_hint_stats,
};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Instant;
use tracing::trace_span;

fn panic_payload_to_string(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    if let Some(message) = payload.downcast_ref::<&'static str>() {
        return (*message).to_string();
    }
    "panic payload unavailable".to_string()
}

fn surface_render_panic(address: u64, payload: &(dyn std::any::Any + Send)) -> String {
    let detail = panic_payload_to_string(payload);
    nir_diag_event(address, "render_preview_panic", format!("detail={detail}"));
    format!("nir_structuring_failure[unsupported_cfg_region_shape]: render panicked: {detail}")
}

pub(crate) fn pcode_total_ops(pcode: &PcodeFunction) -> usize {
    pcode.blocks.iter().map(|block| block.ops.len()).sum()
}

pub(crate) fn max_multiequal_fanin(pcode: &PcodeFunction) -> usize {
    pcode
        .blocks
        .iter()
        .flat_map(|block| block.ops.iter())
        .filter(|op| op.opcode == PcodeOpcode::MultiEqual)
        .map(|op| op.inputs.len())
        .max()
        .unwrap_or(0)
}

pub(crate) fn nir_diag_stage(address: u64, stage: &str, start: Instant) {
    if std::env::var_os("FISSION_PREVIEW_DIAG").is_some() {
        eprintln!(
            "[PREVIEW-DIAG] fn=0x{address:x} stage={stage} elapsed_ms={:.1}",
            start.elapsed().as_secs_f64() * 1000.0
        );
    }
}

pub(crate) fn nir_diag_event(address: u64, stage: &str, detail: impl AsRef<str>) {
    if std::env::var_os("FISSION_PREVIEW_DIAG").is_some() {
        eprintln!(
            "[PREVIEW-DIAG] fn=0x{address:x} stage={stage} {}",
            detail.as_ref()
        );
    }
}

pub(crate) fn build_nir_type_context_from_facts(
    binary: &LoadedBinary,
    fact_store: &FactStore,
    address: u64,
) -> NirTypeContext {
    crate::analysis::decomp::nir_context::build_nir_type_context(binary, fact_store, address)
}

pub(crate) fn make_nir_request(
    pcode_json: &str,
    address: u64,
    name: &str,
    options: NirRenderOptions,
    type_context: NirTypeContext,
) -> NirWorkerRequest {
    NirWorkerRequest {
        pcode_json: pcode_json.to_string(),
        address,
        name: name.to_string(),
        options,
        type_context,
    }
}

pub(crate) fn nir_options_with_recovery(
    binary: &LoadedBinary,
    region_linearize_structuring: bool,
    force_linear_structuring: bool,
) -> NirRenderOptions {
    let options = NirRenderOptions::from_loaded_binary(binary);
    apply_nir_recovery_flags(
        options,
        region_linearize_structuring,
        force_linear_structuring,
    )
}

fn apply_nir_recovery_flags(
    mut options: NirRenderOptions,
    region_linearize_structuring: bool,
    force_linear_structuring: bool,
) -> NirRenderOptions {
    options.region_linearize_structuring = region_linearize_structuring;
    options.force_linear_structuring = force_linear_structuring;
    options.conservative_irreducible_fallback = options.conservative_irreducible_fallback
        || std::env::var_os("FISSION_NIR_CONSERVATIVE_IRREDUCIBLE_FALLBACK").is_some();
    options
}

pub(crate) fn render_nir_from_pcode_with_type_context_and_options(
    pcode: &PcodeFunction,
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    enforce_auto_gate: bool,
    _timeout_ms: Option<u64>,
    type_context: NirTypeContext,
    base_options: NirRenderOptions,
    region_linearize_structuring: bool,
    force_linear_structuring: bool,
) -> Result<Option<(String, Option<NirBuildStats>, Option<NirHintStats>)>, String> {
    if enforce_auto_gate
        && !(binary.is_64bit
            && binary.format.to_ascii_uppercase().starts_with("PE")
            && pcode.blocks.len() <= 12
            && pcode_total_ops(pcode) <= 600
            && !pcode_has_indirect_control_flow(pcode)
            && max_multiequal_fanin(pcode) <= 4)
    {
        return Ok(None);
    }

    let options = apply_nir_recovery_flags(
        base_options,
        region_linearize_structuring,
        force_linear_structuring,
    );
    let render_start = Instant::now();
    match catch_unwind(AssertUnwindSafe(|| {
        render_nir_with_context(pcode, name, address, &options, Some(&type_context))
    })) {
        Ok(Ok(code)) => {
            let build_stats = take_last_nir_build_stats();
            let hint_stats = take_last_nir_hint_stats();
            nir_diag_stage(address, "render_preview_done", render_start);
            Ok(Some((code, build_stats, hint_stats)))
        }
        Ok(Err(err)) => {
            let surfaced_error = err
                .structuring_failure_kind()
                .map(|kind| {
                    format!(
                        "nir_structuring_failure[{}]: {err}",
                        kind.preview_block_signature()
                    )
                })
                .unwrap_or_else(|| format!("Fission NIR unavailable: {err}"));
            nir_diag_stage(address, "render_preview_error", render_start);
            Err(surfaced_error)
        }
        Err(payload) => {
            nir_diag_stage(address, "render_preview_error", render_start);
            Err(surface_render_panic(address, payload.as_ref()))
        }
    }
}

pub(crate) fn render_nir_request(
    request: &NirWorkerRequest,
) -> Result<(String, Option<NirBuildStats>, Option<NirHintStats>), String> {
    let parse_start = Instant::now();
    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        let _ = std::fs::write(
            format!("/tmp/fission_preview_{:x}.json", request.address),
            &request.pcode_json,
        );
    }
    let mut pcode = PcodeFunction::from_json(&request.pcode_json)
        .map_err(|e| format!("mlil-preview pcode parse failed: {e}"))?;
    nir_diag_stage(request.address, "parse_pcode_done", parse_start);
    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        let mut debug_dump = String::new();
        debug_dump.push_str(&format!(
            "[mlil-preview] function=0x{:x} blocks={} ops={}\n",
            request.address,
            pcode.blocks.len(),
            pcode.blocks.iter().map(|b| b.ops.len()).sum::<usize>()
        ));
        eprintln!(
            "[mlil-preview] function=0x{:x} blocks={} ops={}",
            request.address,
            pcode.blocks.len(),
            pcode.blocks.iter().map(|b| b.ops.len()).sum::<usize>()
        );
        for block in &pcode.blocks {
            let term = block
                .ops
                .last()
                .map(|op| format!("{:?}@0x{:x}", op.opcode, op.address))
                .unwrap_or_else(|| "<none>".to_string());
            debug_dump.push_str(&format!(
                "[mlil-preview] block 0x{:x} ops={} term={}\n",
                block.start_address,
                block.ops.len(),
                term
            ));
            eprintln!(
                "[mlil-preview] block 0x{:x} ops={} term={}",
                block.start_address,
                block.ops.len(),
                term
            );
        }
        let _ = std::fs::write(
            format!("/tmp/fission_preview_{:x}.log", request.address),
            debug_dump,
        );
    }
    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(format!("/tmp/fission_preview_{:x}.log", request.address))
            .and_then(|mut f| {
                std::io::Write::write_all(&mut f, b"[mlil-preview] stage=before_optimize\n")
            });
    }
    let mut optimizer = PcodeOptimizer::new(PcodeOptimizerConfig::default());
    let optimize_start = Instant::now();
    let optimize_result = catch_unwind(AssertUnwindSafe(|| optimizer.optimize(&mut pcode)));
    nir_diag_stage(request.address, "optimize_pcode_done", optimize_start);
    if optimize_result.is_err() && std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(format!("/tmp/fission_preview_{:x}.log", request.address))
            .and_then(|mut f| {
                std::io::Write::write_all(
                    &mut f,
                    b"[mlil-preview] stage=optimize_panicked_using_raw_pcode\n",
                )
            });
    }
    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(format!("/tmp/fission_preview_{:x}.log", request.address))
            .and_then(|mut f| {
                std::io::Write::write_all(&mut f, b"[mlil-preview] stage=after_optimize\n")
            });
    }
    if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(format!("/tmp/fission_preview_{:x}.log", request.address))
            .and_then(|mut f| {
                std::io::Write::write_all(&mut f, b"[mlil-preview] stage=before_render\n")
            });
    }
    let render_start = Instant::now();
    match catch_unwind(AssertUnwindSafe(|| {
        render_nir_with_context(
            &pcode,
            &request.name,
            request.address,
            &request.options,
            Some(&request.type_context),
        )
    })) {
        Ok(Ok(code)) => {
            let build_stats = take_last_nir_build_stats();
            let hint_stats = take_last_nir_hint_stats();
            nir_diag_stage(request.address, "render_preview_done", render_start);
            if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
                let _ = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(format!("/tmp/fission_preview_{:x}.log", request.address))
                    .and_then(|mut f| {
                        std::io::Write::write_all(&mut f, b"[mlil-preview] stage=render_ok\n")
                    });
            }
            Ok((code, build_stats, hint_stats))
        }
        Ok(Err(err)) => {
            let surfaced_error = err
                .structuring_failure_kind()
                .map(|kind| {
                    format!(
                        "nir_structuring_failure[{}]: {err}",
                        kind.preview_block_signature()
                    )
                })
                .unwrap_or_else(|| format!("Fission NIR unavailable: {err}"));
            nir_diag_stage(request.address, "render_preview_error", render_start);
            if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
                let _ = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(format!("/tmp/fission_preview_{:x}.log", request.address))
                    .and_then(|mut f| {
                        std::io::Write::write_all(
                            &mut f,
                            format!("[mlil-preview] stage=render_error err={surfaced_error}\n")
                                .as_bytes(),
                        )
                    });
            }
            Err(surfaced_error)
        }
        Err(payload) => {
            nir_diag_stage(request.address, "render_preview_error", render_start);
            let surfaced_error = surface_render_panic(request.address, payload.as_ref());
            if std::env::var_os("FISSION_PREVIEW_DEBUG").is_some() {
                let _ = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(format!("/tmp/fission_preview_{:x}.log", request.address))
                    .and_then(|mut f| {
                        std::io::Write::write_all(
                            &mut f,
                            format!("[mlil-preview] stage=render_panic err={surfaced_error}\n")
                                .as_bytes(),
                        )
                    });
            }
            Err(surfaced_error)
        }
    }
}

pub(crate) fn render_nir_from_json_with_type_context(
    pcode_json: &str,
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    enforce_auto_gate: bool,
    timeout_ms: Option<u64>,
    type_context: NirTypeContext,
    region_linearize_structuring: bool,
    force_linear_structuring: bool,
) -> Result<Option<(String, Option<NirBuildStats>, Option<NirHintStats>)>, String> {
    let _render = trace_span!("nir_render_json", address = address, fn_name = name).entered();
    let parse_start = Instant::now();
    let pcode = PcodeFunction::from_json(pcode_json)
        .map_err(|e| format!("mlil-preview pcode parse failed: {e}"))?;
    nir_diag_stage(address, "parse_pcode_done", parse_start);
    if enforce_auto_gate
        && !(binary.is_64bit
            && binary.format.to_ascii_uppercase().starts_with("PE")
            && pcode.blocks.len() <= 12
            && pcode_total_ops(&pcode) <= 600
            && !pcode_has_indirect_control_flow(&pcode)
            && max_multiequal_fanin(&pcode) <= 4)
    {
        return Ok(None);
    }

    let options = nir_options_with_recovery(
        binary,
        region_linearize_structuring,
        force_linear_structuring,
    );
    let request = make_nir_request(pcode_json, address, name, options, type_context);

    let should_use_worker = binary.is_64bit
        && binary.format.to_ascii_uppercase().starts_with("PE")
        && !enforce_auto_gate
        && !(binary.is_64bit
            && binary.format.to_ascii_uppercase().starts_with("PE")
            && pcode.blocks.len() <= 12
            && pcode_total_ops(&pcode) <= 600
            && !pcode_has_indirect_control_flow(&pcode)
            && max_multiequal_fanin(&pcode) <= 4);

    if should_use_worker {
        let worker_timeout_ms = nir_worker_timeout_ms(timeout_ms);
        match execute_nir_worker_request(&request, worker_timeout_ms) {
            Ok((code, build_stats, hint_stats)) => {
                nir_diag_event(
                    address,
                    "worker_render_done",
                    format!("budget_ms={worker_timeout_ms}"),
                );
                return Ok(Some((code, build_stats, hint_stats)));
            }
            Err(err) if err == "nir worker unavailable" => {
                nir_diag_event(address, "worker_unavailable", "falling back to in-process");
            }
            Err(err) => return Err(err),
        }
    }

    match render_nir_request(&request) {
        Ok(result) => Ok(Some(result)),
        Err(err) => Err(err),
    }
}
