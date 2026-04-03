use super::super::decompile_render::{
    DecompEntry, RenderedCode, decompile_code_with_profile, make_assembly_fallback,
    strip_inferred_structs, strip_warnings,
};
use super::super::decompile_targets::collect_target_functions;
use super::super::*;
use super::output::{attach_native_timing_if_present, decompile_and_output};

fn sleigh_language_for_arch_spec(arch_spec: &str) -> Option<&'static str> {
    if arch_spec.starts_with("AARCH64:LE:64") && arch_spec.contains("AppleSilicon") {
        return Some("AARCH64_AppleSilicon");
    }
    if arch_spec.starts_with("AARCH64:LE:64") {
        return Some("AARCH64");
    }
    if arch_spec.starts_with("AARCH64:BE:64") {
        return Some("AARCH64BE");
    }
    if arch_spec.starts_with("x86:LE:64") {
        return Some("x86-64");
    }
    if arch_spec.starts_with("x86:LE:32") || arch_spec.starts_with("x86:LE:16") {
        return Some("x86");
    }
    None
}

fn is_terminal_control_flow(opcode: fission_pcode::PcodeOpcode) -> bool {
    matches!(
        opcode,
        fission_pcode::PcodeOpcode::Branch
            | fission_pcode::PcodeOpcode::CBranch
            | fission_pcode::PcodeOpcode::BranchInd
            | fission_pcode::PcodeOpcode::Return
    )
}

fn decode_rust_sleigh_pcode(
    lifter: &fission_sleigh::lifter::SleighLifter,
    binary: &LoadedBinary,
    func: &FunctionInfo,
) -> Result<fission_pcode::PcodeFunction, FissionError> {
    let max_bytes = if func.size > 0 {
        usize::try_from(func.size)
            .unwrap_or(0x400)
            .min(0x1000)
            .max(1)
    } else {
        0x100
    };

    let bytes = binary.view_bytes(func.address, max_bytes).ok_or_else(|| {
        FissionError::decompiler(format!(
            "rust_sleigh: unable to read bytes at 0x{:x}",
            func.address
        ))
    })?;

    let mut ops = Vec::new();
    let mut offset = 0usize;
    let mut current = func.address;
    let mut global_seq = 0u32;
    let mut instruction_count = 0usize;

    while offset < bytes.len() && instruction_count < 256 {
        let remaining = &bytes[offset..];
        let (mut ins_ops, decoded_len) = lifter
            .decode_and_lift_with_len(remaining, current)
            .map_err(|err| {
                FissionError::decompiler(format!(
                    "rust_sleigh: decode failed at 0x{:x}: {:#}",
                    current, err
                ))
            })?;

        if decoded_len == 0 {
            return Err(FissionError::decompiler(format!(
                "rust_sleigh: decoder returned zero length at 0x{:x}",
                current
            )));
        }

        let step = usize::try_from(decoded_len).map_err(|_| {
            FissionError::decompiler(format!(
                "rust_sleigh: decoded length does not fit usize at 0x{:x}",
                current
            ))
        })?;

        if step > remaining.len() {
            return Err(FissionError::decompiler(format!(
                "rust_sleigh: decoded length {} exceeds available bytes {} at 0x{:x}",
                step,
                remaining.len(),
                current
            )));
        }

        for op in &mut ins_ops {
            op.seq_num = global_seq;
            global_seq = global_seq.saturating_add(1);
        }

        let terminates = ins_ops
            .last()
            .map(|op| is_terminal_control_flow(op.opcode))
            .unwrap_or(false);

        ops.extend(ins_ops);
        offset = offset.saturating_add(step);
        current = current.saturating_add(decoded_len);
        instruction_count = instruction_count.saturating_add(1);

        if terminates {
            break;
        }
    }

    if ops.is_empty() {
        return Err(FissionError::decompiler(format!(
            "rust_sleigh: failed to decode function {} at 0x{:x}",
            func.name, func.address
        )));
    }

    Ok(fission_pcode::PcodeFunction {
        blocks: vec![fission_pcode::PcodeBasicBlock {
            index: 0,
            start_address: func.address,
            successors: Vec::new(),
            ops,
        }],
    })
}

fn format_varnode_for_pcode(vn: &fission_pcode::Varnode) -> String {
    if vn.is_constant {
        format!("const(0x{:x}:{} )", vn.constant_val as u64, vn.size)
    } else {
        format!("v(space={},off=0x{:x},size={})", vn.space_id, vn.offset, vn.size)
    }
}

fn render_pcode_text(func: &FunctionInfo, pcode: &fission_pcode::PcodeFunction) -> String {
    let mut out = String::new();
    out.push_str(&format!("// rust_sleigh direct pcode output: {}\n", func.name));
    for block in &pcode.blocks {
        out.push_str(&format!("block_{} @ 0x{:x}\n", block.index, block.start_address));
        for op in &block.ops {
            let out_vn = op
                .output
                .as_ref()
                .map(format_varnode_for_pcode)
                .unwrap_or_else(|| "-".to_string());
            let in_vn = op
                .inputs
                .iter()
                .map(format_varnode_for_pcode)
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!(
                "  [{:04}] 0x{:x} {:?}  {} <- {}\n",
                op.seq_num, op.address, op.opcode, out_vn, in_vn
            ));
        }
    }
    out
}

fn render_with_rust_sleigh(
    binary: &LoadedBinary,
    func: &FunctionInfo,
) -> Result<RenderedCode, FissionError> {
    let language = sleigh_language_for_arch_spec(&binary.arch_spec).ok_or_else(|| {
        FissionError::decompiler(format!(
            "rust_sleigh: unsupported arch_spec '{}'",
            binary.arch_spec
        ))
    })?;

    let lifter = fission_sleigh::lifter::SleighLifter::new_for_language(language)
        .map_err(|e| FissionError::decompiler(format!("rust_sleigh: {e:#}")))?;

    let pcode = decode_rust_sleigh_pcode(&lifter, binary, func)?;

    let options = fission_pcode::NirRenderOptions::from_loaded_binary(binary);
    let code = match fission_pcode::render_nir_with_context(&pcode, &func.name, func.address, &options, None) {
        Ok(rendered) => rendered,
        Err(e) => {
            let err_text = e.to_string();
            if err_text
                .to_ascii_lowercase()
                .contains("unsupported architecture in mlil-preview")
            {
                return Ok(RenderedCode {
                    code: render_pcode_text(func, &pcode),
                    postprocess_sec: 0.0,
                    engine_used: "rust_sleigh",
                    fell_back: true,
                    fallback_reason: Some("nir_unsupported_arch:pcode_dump".to_string()),
                    preview_build_stats: None,
                    preview_hint_stats: None,
                });
            }
            return Err(FissionError::decompiler(format!("rust_sleigh render failed: {}", err_text)));
        }
    };

    let build_stats = fission_pcode::take_last_nir_build_stats();
    let hint_stats = fission_pcode::take_last_nir_hint_stats();

    Ok(RenderedCode {
        code,
        postprocess_sec: 0.0,
        engine_used: "rust_sleigh",
        fell_back: false,
        fallback_reason: None,
        preview_build_stats: build_stats,
        preview_hint_stats: hint_stats,
    })
}

fn run_rust_sleigh_decompilation(
    cli: &OneShotArgs,
    binary: &LoadedBinary,
    binary_data: &[u8],
    functions: &[&FunctionInfo],
    effective_no_header: bool,
    effective_no_warnings: bool,
    effective_json: bool,
    init_start: std::time::Instant,
) -> io::Result<()> {
    let mut all_output = String::new();
    let mut json_results = Vec::new();
    let mut total_decomp_secs = 0.0;
    let mut total_postprocess_secs = 0.0;

    for func in functions {
        let start = std::time::Instant::now();
        match render_with_rust_sleigh(binary, func) {
            Ok(rendered) => {
                let decomp_sec = start.elapsed().as_secs_f64();
                total_decomp_secs += decomp_sec;
                total_postprocess_secs += rendered.postprocess_sec;

                let mut filtered = rendered.code.clone();
                if effective_no_warnings {
                    filtered = strip_warnings(&filtered);
                }
                if cli.ghidra_compat {
                    filtered = strip_inferred_structs(&filtered);
                }

                if effective_json {
                    let mut entry = serde_json::json!({
                        "address": format!("0x{:x}", func.address),
                        "name": func.name,
                        "code": filtered,
                        "engine_used": rendered.engine_used,
                        "fell_back": rendered.fell_back,
                        "fallback_reason": rendered.fallback_reason,
                    });
                    if let Some(stats) = rendered.preview_build_stats {
                        entry["preview_build_stats"] = serde_json::json!(stats);
                    }
                    if let Some(stats) = rendered.preview_hint_stats {
                        entry["preview_hint_stats"] = serde_json::json!(stats);
                    }
                    if cli.benchmark {
                        entry["decomp_sec"] =
                            serde_json::json!((decomp_sec * 1_000_000.0).round() / 1_000_000.0);
                        entry["postprocess_sec"] = serde_json::json!(
                            (rendered.postprocess_sec * 1_000_000.0).round() / 1_000_000.0
                        );
                    }
                    json_results.push(entry);
                } else {
                    if !effective_no_header {
                        all_output.push_str("// ============================================\n");
                        all_output.push_str(&format!(
                            "// Function: {} @ 0x{:x}\n",
                            func.name, func.address
                        ));
                        all_output.push_str("// ============================================\n\n");
                    }
                    all_output.push_str(&filtered);
                    all_output.push_str("\n\n");
                }
            }
            Err(e) => {
                let decomp_sec = start.elapsed().as_secs_f64();
                total_decomp_secs += decomp_sec;
                let error_text = e.to_string();
                if let Some(fallback) = make_assembly_fallback(binary, binary_data, func, &error_text)
                {
                    if effective_json {
                        let mut entry = serde_json::json!({
                            "address": format!("0x{:x}", func.address),
                            "name": func.name,
                            "code": fallback,
                            "engine_used": "rust_sleigh",
                            "fell_back": true,
                            "fallback": "assembly",
                            "fallback_reason": fallback_reason_with_kind("assembly_fallback", &error_text),
                        });
                        if cli.benchmark {
                            entry["decomp_sec"] = serde_json::json!(
                                (decomp_sec * 1_000_000.0).round() / 1_000_000.0
                            );
                        }
                        json_results.push(entry);
                    } else {
                        if !effective_no_header {
                            all_output.push_str("// ============================================\n");
                            all_output.push_str(&format!(
                                "// Function: {} @ 0x{:x}\n",
                                func.name, func.address
                            ));
                            all_output.push_str("// ============================================\n\n");
                        }
                        all_output.push_str(&fallback);
                        all_output.push_str("\n\n");
                    }
                } else if effective_json {
                    let mut entry = serde_json::json!({
                        "address": format!("0x{:x}", func.address),
                        "name": func.name,
                        "engine_used": "rust_sleigh",
                        "fell_back": true,
                        "fallback_reason": fallback_reason_with_kind("rust_sleigh", &error_text),
                        "error": error_text,
                    });
                    if cli.benchmark {
                        entry["decomp_sec"] =
                            serde_json::json!((decomp_sec * 1_000_000.0).round() / 1_000_000.0);
                    }
                    json_results.push(entry);
                } else {
                    all_output.push_str(&format!(
                        "// Error decompiling {} (0x{:x}): {}\n\n",
                        func.name, func.address, error_text
                    ));
                }
            }
        }
    }

    let final_output = if cli.benchmark {
        let envelope = serde_json::json!({
            "_meta": {
                "tool": "fission",
                "version": env!("CARGO_PKG_VERSION"),
                "profile": cli.profile.as_deref().unwrap_or("balanced"),
                "engine": "rust-sleigh",
                "function_count": functions.len(),
                "init_sec": 0.0,
                "total_decomp_sec": (total_decomp_secs * 1_000_000.0).round() / 1_000_000.0,
                "total_postprocess_sec": (total_postprocess_secs * 1_000_000.0).round() / 1_000_000.0,
                "wall_clock_sec": (init_start.elapsed().as_secs_f64() * 1_000_000.0).round() / 1_000_000.0,
            },
            "functions": json_results,
        });
        serde_json::to_string_pretty(&envelope)
            .map_err(|e| io::Error::other(format!("JSON serialization failed: {}", e)))?
    } else if effective_json {
        serde_json::to_string_pretty(&json_results)
            .map_err(|e| io::Error::other(format!("JSON serialization failed: {}", e)))?
    } else {
        all_output
    };

    if let Some(ref output_path) = cli.output {
        let mut file = fs::File::create(output_path).map_err(|e| {
            io::Error::other(format!(
                "Failed to create output file '{}': {}",
                output_path.display(),
                e
            ))
        })?;
        file.write_all(final_output.as_bytes())?;
        if cli.verbose {
            eprintln!("[✓] Output written to: {}", output_path.display());
        }
    } else {
        let mut stdout = io::stdout().lock();
        stdout.write_all(final_output.as_bytes())?;
    }
    Ok(())
}

fn run_sequential_decompilation<'a>(
    cli: &OneShotArgs,
    decomp: &mut DecompilerNative,
    binary: &LoadedBinary,
    binary_data: &[u8],
    functions: &[&'a FunctionInfo],
    selected_profile: &str,
    engine_mode: EngineMode,
    effective_no_header: bool,
    effective_no_warnings: bool,
    effective_json: bool,
) -> (String, Vec<serde_json::Value>, f64, f64) {
    let mut all_output = String::new();
    let mut json_results = Vec::new();
    let mut total_decomp_secs = 0.0;
    let mut total_postprocess_secs = 0.0;
    for func in functions {
        if cli.verbose {
            eprintln!("[*] Decompiling {} (0x{:x})...", func.name, func.address);
        }

        let _silencer = OutputSilencer::new_if(!cli.verbose);
        let func_start = std::time::Instant::now();
        match decompile_code_with_profile(
            selected_profile,
            engine_mode,
            decomp,
            binary,
            func.address,
            &func.name,
            cli.timeout_ms,
            cli.verbose,
        ) {
            Ok(rendered) => {
                let postprocess_sec = rendered.postprocess_sec;
                let decomp_sec = func_start.elapsed().as_secs_f64();
                total_decomp_secs += decomp_sec;
                total_postprocess_secs += postprocess_sec;
                let mut filtered = rendered.code.clone();
                if effective_no_warnings {
                    filtered = strip_warnings(&filtered);
                }
                if cli.ghidra_compat {
                    filtered = strip_inferred_structs(&filtered);
                }

                if effective_json {
                    let mut entry = serde_json::json!({
                        "address": format!("0x{:x}", func.address),
                        "name": func.name,
                        "code": filtered,
                        "engine_used": rendered.engine_used,
                        "fell_back": rendered.fell_back,
                        "fallback_reason": rendered.fallback_reason,
                    });
                    if let Some(stats) = rendered.preview_build_stats {
                        entry["preview_build_stats"] = serde_json::json!(stats);
                    }
                    if let Some(stats) = rendered.preview_hint_stats {
                        entry["preview_hint_stats"] = serde_json::json!(stats);
                    }
                    if cli.benchmark {
                        entry["decomp_sec"] =
                            serde_json::json!((decomp_sec * 1_000_000.0).round() / 1_000_000.0);
                        entry["postprocess_sec"] = serde_json::json!(
                            (postprocess_sec * 1_000_000.0).round() / 1_000_000.0
                        );
                        attach_native_timing_if_present(&mut entry, decomp);
                    }
                    json_results.push(entry);
                } else {
                    if !effective_no_header {
                        all_output.push_str("// ============================================\n");
                        all_output.push_str(&format!(
                            "// Function: {} @ 0x{:x}\n",
                            func.name, func.address
                        ));
                        all_output.push_str("// ============================================\n\n");
                    }
                    all_output.push_str(&filtered);
                    all_output.push_str("\n\n");
                }
            }
            Err(e) => {
                let decomp_sec = func_start.elapsed().as_secs_f64();
                total_decomp_secs += decomp_sec;
                let error_text = e.to_string();
                if let Some(fallback) =
                    make_assembly_fallback(binary, binary_data, func, &error_text)
                {
                    if effective_json {
                        let fallback_class = classify_native_failure_kind(&error_text);
                        let mut entry = serde_json::json!({
                            "address": format!("0x{:x}", func.address),
                            "name": func.name,
                            "code": fallback,
                            "engine_used": NirEngineMode::Legacy.as_str(),
                            "fell_back": true,
                            "fallback": "assembly",
                            "fallback_reason": fallback_reason_with_kind("assembly_fallback", &error_text),
                            "fallback_class": fallback_class
                        });
                        if cli.benchmark {
                            entry["decomp_sec"] =
                                serde_json::json!((decomp_sec * 1_000_000.0).round() / 1_000_000.0);
                            attach_native_timing_if_present(&mut entry, decomp);
                        }
                        json_results.push(entry);
                    } else {
                        if !effective_no_header {
                            all_output
                                .push_str("// ============================================\n");
                            all_output.push_str(&format!(
                                "// Function: {} @ 0x{:x}\n",
                                func.name, func.address
                            ));
                            all_output
                                .push_str("// ============================================\n\n");
                        }
                        all_output.push_str(&fallback);
                        all_output.push_str("\n\n");
                    }
                    continue;
                }
                if effective_json {
                    let routing = fission_static::analysis::decomp::native_failure_routing_decision(
                        &error_text,
                    );
                    let mut entry = serde_json::json!({
                        "address": format!("0x{:x}", func.address),
                        "name": func.name,
                        "engine_used": match routing.engine_used {
                            NirEngineMode::Legacy => NirEngineMode::Legacy.as_str(),
                            NirEngineMode::Nir => NirEngineMode::Nir.as_str(),
                            NirEngineMode::Auto => NirEngineMode::Auto.as_str(),
                        },
                        "fell_back": routing.fell_back,
                        "fallback_reason": routing.fallback_reason,
                        "error": error_text
                    });
                    if cli.benchmark {
                        entry["decomp_sec"] =
                            serde_json::json!((decomp_sec * 1_000_000.0).round() / 1_000_000.0);
                        attach_native_timing_if_present(&mut entry, decomp);
                    }
                    json_results.push(entry);
                } else {
                    all_output.push_str(&format!(
                        "// Error decompiling {} (0x{:x}): {}\n\n",
                        func.name, func.address, error_text
                    ));
                }
            }
        }
    }

    (
        all_output,
        json_results,
        total_decomp_secs,
        total_postprocess_secs,
    )
}

#[cfg(feature = "native_decomp")]
fn run_parallel_decompilation<'a>(
    cli: &OneShotArgs,
    main_decomp: &mut DecompilerNative,
    binary: &LoadedBinary,
    binary_data: &[u8],
    functions: &[&'a FunctionInfo],
    _prepare_timings: &PrepareTimings,
    selected_profile: &str,
    engine_mode: EngineMode,
    _init_elapsed_sec: f64,
    _init_start: std::time::Instant,
    effective_no_header: bool,
    effective_no_warnings: bool,
    effective_json: bool,
) -> (String, Vec<serde_json::Value>, f64, f64) {
    let (compiler_id, _) = resolve_compiler_id(binary, cli.compiler_id.as_deref());
    let config = fission_core::config::Config::default();
    let gdt_path_owned = fission_core::PATHS
        .get_gdt_path(binary.is_64bit)
        .and_then(|p| p.to_str().map(String::from));
    let num_workers = 8;

    let mut buckets: Vec<Vec<&'a FunctionInfo>> = (0..num_workers).map(|_| Vec::new()).collect();
    for (i, func) in functions.iter().enumerate() {
        buckets[i % num_workers].push(*func);
    }

    let first_bucket_entries = if !buckets[0].is_empty() {
        let mut entries = Vec::with_capacity(buckets[0].len());
        for func in &buckets[0] {
            let start = std::time::Instant::now();
            let code_result = decompile_code_with_profile(
                selected_profile,
                engine_mode,
                main_decomp,
                binary,
                func.address,
                &func.name,
                cli.timeout_ms,
                false,
            );
            let decomp_sec = start.elapsed().as_secs_f64();
            let (code_result, postprocess_sec) = match code_result {
                Ok(rendered) => {
                    let postprocess_sec = rendered.postprocess_sec;
                    (Ok(rendered), postprocess_sec)
                }
                Err(e) => {
                    let error_text = e.to_string();
                    if let Some(fallback) =
                        make_assembly_fallback(binary, binary_data, func, &error_text)
                    {
                        (
                            Ok(RenderedCode {
                                code: fallback,
                                postprocess_sec: 0.0,
                                engine_used: NirEngineMode::Legacy.as_str(),
                                fell_back: true,
                                fallback_reason: Some(fallback_reason_with_kind(
                                    "assembly_fallback",
                                    &error_text,
                                )),
                                preview_build_stats: None,
                                preview_hint_stats: None,
                            }),
                            0.0,
                        )
                    } else {
                        (Err(e), 0.0)
                    }
                }
            };
            let timing = main_decomp.get_last_timing_json().ok();
            entries.push(DecompEntry {
                address: func.address,
                name: func.name.clone(),
                code: code_result,
                decomp_sec,
                postprocess_sec,
                last_timing_json: timing,
            });
        }
        entries
    } else {
        Vec::new()
    };

    let signatures_json = serialize_win_api_signatures_json();

    let rest_buckets: Vec<_> = buckets.into_iter().skip(1).collect();
    let rest_results: Vec<Vec<DecompEntry>> = rest_buckets
        .par_iter()
        .map(|bucket| {
            let mut decomp = init_decompiler(false);
            apply_profile(&mut decomp, selected_profile);
            let mut opts = PrepareOptions {
                verbose: false,
                compiler_id: compiler_id.as_deref(),
                gdt_path: gdt_path_owned.as_deref(),
                timeout_ms: Some(cli.timeout_ms.unwrap_or(config.decompiler.timeout_ms)),
                timings: None,
                signatures_json: signatures_json.as_deref(),
            };
            if prepare_native_decompiler_for_binary(&mut decomp, binary, binary_data, &mut opts)
                .is_err()
            {
                return bucket
                    .iter()
                    .map(|f| DecompEntry {
                        address: f.address,
                        name: f.name.clone(),
                        code: Err(fission_core::FissionError::decompiler("Prepare failed")),
                        decomp_sec: 0.0,
                        postprocess_sec: 0.0,
                        last_timing_json: None,
                    })
                    .collect();
            }

            let mut entries = Vec::with_capacity(bucket.len());
            for func in bucket.iter().copied() {
                let start = std::time::Instant::now();
                let code_result = decompile_code_with_profile(
                    selected_profile,
                    engine_mode,
                    &mut decomp,
                    binary,
                    func.address,
                    &func.name,
                    cli.timeout_ms,
                    false,
                );
                let decomp_sec = start.elapsed().as_secs_f64();
                let (code_result, postprocess_sec) = match code_result {
                    Ok(rendered) => {
                        let postprocess_sec = rendered.postprocess_sec;
                        (Ok(rendered), postprocess_sec)
                    }
                    Err(e) => {
                        let error_text = e.to_string();
                        if let Some(fallback) =
                            make_assembly_fallback(binary, binary_data, func, &error_text)
                        {
                            (
                                Ok(RenderedCode {
                                    code: fallback,
                                    postprocess_sec: 0.0,
                                    engine_used: NirEngineMode::Legacy.as_str(),
                                    fell_back: true,
                                    fallback_reason: Some(fallback_reason_with_kind(
                                        "assembly_fallback",
                                        &error_text,
                                    )),
                                    preview_build_stats: None,
                                    preview_hint_stats: None,
                                }),
                                0.0,
                            )
                        } else {
                            (Err(e), 0.0)
                        }
                    }
                };
                let timing = decomp.get_last_timing_json().ok();
                entries.push(DecompEntry {
                    address: func.address,
                    name: func.name.clone(),
                    code: code_result,
                    decomp_sec,
                    postprocess_sec,
                    last_timing_json: timing,
                });
            }
            entries
        })
        .collect();

    let all_entries: Vec<DecompEntry> = {
        let mut entries: Vec<DecompEntry> = first_bucket_entries
            .into_iter()
            .chain(rest_results.into_iter().flatten())
            .collect();
        entries.sort_by_key(|e| e.address);
        entries
    };

    let mut all_output = String::new();
    let mut json_results = Vec::new();
    let mut total_decomp_secs = 0.0;
    let mut total_postprocess_secs = 0.0;

    for entry in all_entries {
        total_decomp_secs += entry.decomp_sec;
        total_postprocess_secs += entry.postprocess_sec;

        match &entry.code {
            Ok(rendered) => {
                let mut filtered = rendered.code.clone();
                if effective_no_warnings {
                    filtered = strip_warnings(&filtered);
                }
                if cli.ghidra_compat {
                    filtered = strip_inferred_structs(&filtered);
                }

                if effective_json {
                    let mut json_entry = serde_json::json!({
                        "address": format!("0x{:x}", entry.address),
                        "name": entry.name,
                        "code": filtered,
                        "engine_used": rendered.engine_used,
                        "fell_back": rendered.fell_back,
                        "fallback_reason": rendered.fallback_reason,
                    });
                    if let Some(stats) = rendered.preview_build_stats {
                        json_entry["preview_build_stats"] = serde_json::json!(stats);
                    }
                    if let Some(stats) = rendered.preview_hint_stats {
                        json_entry["preview_hint_stats"] = serde_json::json!(stats);
                    }
                    if cli.benchmark {
                        json_entry["decomp_sec"] = serde_json::json!(
                            (entry.decomp_sec * 1_000_000.0).round() / 1_000_000.0
                        );
                        json_entry["postprocess_sec"] = serde_json::json!(
                            (entry.postprocess_sec * 1_000_000.0).round() / 1_000_000.0
                        );
                        if let Some(ref timing) = entry.last_timing_json {
                            if !timing.is_empty() && timing != "{}" {
                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(timing) {
                                    json_entry["native_timing"] = v;
                                }
                            }
                        }
                    }
                    json_results.push(json_entry);
                } else {
                    if !effective_no_header {
                        all_output.push_str("// ============================================\n");
                        all_output.push_str(&format!(
                            "// Function: {} @ 0x{:x}\n",
                            entry.name, entry.address
                        ));
                        all_output.push_str("// ============================================\n\n");
                    }
                    all_output.push_str(&filtered);
                    all_output.push_str("\n\n");
                }
            }
            Err(e) => {
                if effective_json {
                    let mut json_entry = serde_json::json!({
                        "address": format!("0x{:x}", entry.address),
                        "name": entry.name,
                        "engine_used": NirEngineMode::Legacy.as_str(),
                        "fell_back": true,
                        "fallback_reason": fallback_reason_with_kind(classify_native_failure_kind(&e.to_string()), e.to_string()),
                        "error": e.to_string()
                    });
                    if cli.benchmark {
                        json_entry["decomp_sec"] = serde_json::json!(
                            (entry.decomp_sec * 1_000_000.0).round() / 1_000_000.0
                        );
                        if let Some(ref timing) = entry.last_timing_json {
                            if !timing.is_empty() && timing != "{}" {
                                if let Ok(v) = serde_json::from_str::<serde_json::Value>(timing) {
                                    json_entry["native_timing"] = v;
                                }
                            }
                        }
                    }
                    json_results.push(json_entry);
                } else {
                    all_output.push_str(&format!(
                        "// Error decompiling {} (0x{:x}): {}\n\n",
                        entry.name, entry.address, e
                    ));
                }
            }
        }
    }

    (
        all_output,
        json_results,
        total_decomp_secs,
        total_postprocess_secs,
    )
}

pub(crate) fn run_decompilation(
    cli: &OneShotArgs,
    binary: &LoadedBinary,
    binary_data: &[u8],
) -> io::Result<()> {
    let init_start = std::time::Instant::now();
    let (selected_profile, unknown_profile) = resolve_profile(cli.profile.as_deref());
    let (engine_mode, unknown_engine, deprecated_engine_alias, deprecated_profile_alias) =
        resolve_engine_mode(cli.engine.as_deref(), cli.profile.as_deref());
    if let Some(other) = unknown_profile {
        eprintln!(
            "[!] Unknown --profile '{}', using balanced (quality|speed|balanced|nir)",
            other
        );
        warn!(
            profile = other,
            "unknown decompilation profile, using balanced"
        );
    }
    if let Some(other) = unknown_engine {
        eprintln!(
            "[!] Unknown --engine '{}', using auto (nir|auto|rust-sleigh)",
            other
        );
        warn!(engine = other, "unknown decompilation engine, using auto");
    }
    if matches!(engine_mode, EngineMode::Legacy) && cli.verbose {
        eprintln!(
            "[*] '--engine legacy' is a hidden compatibility mode; preview-first remains the product default"
        );
    }
    if deprecated_engine_alias && cli.verbose {
        eprintln!("[*] '--engine mlil-preview' is deprecated; use '--engine nir'");
    }
    if deprecated_profile_alias && cli.verbose {
        eprintln!(
            "[*] '--profile mlil-preview' is deprecated; use '--engine nir --profile quality'"
        );
    }
    if cli.verbose {
        eprintln!("[*] Decompilation profile = {}", selected_profile);
        eprintln!("[*] Decompilation engine = {:?}", engine_mode);
    }

    let functions = collect_target_functions(binary, cli.address, cli.decomp_all, cli.decomp_limit);

    if matches!(engine_mode, EngineMode::RustSleigh) {
        let effective_no_header = cli.no_header || cli.ghidra_compat;
        let effective_no_warnings = cli.no_warnings || cli.ghidra_compat;
        let effective_json = cli.json || cli.benchmark;

        if functions.is_empty() && cli.address.is_some() {
            if let Some(addr) = cli.address {
                let synthetic = FunctionInfo {
                    name: format!("sub_{:x}", addr),
                    address: addr,
                    size: 0,
                    is_export: false,
                    is_import: false,
                };
                let one = [&synthetic];
                return run_rust_sleigh_decompilation(
                    cli,
                    binary,
                    binary_data,
                    &one,
                    effective_no_header,
                    effective_no_warnings,
                    effective_json,
                    init_start,
                );
            }
        }

        return run_rust_sleigh_decompilation(
            cli,
            binary,
            binary_data,
            &functions,
            effective_no_header,
            effective_no_warnings,
            effective_json,
            init_start,
        );
    }

    let mut decomp = init_decompiler(cli.verbose);
    apply_profile(&mut decomp, selected_profile);

    let mut prepare_timings = PrepareTimings::default();
    {
        let (compiler_id, unknown_compiler) =
            resolve_compiler_id(binary, cli.compiler_id.as_deref());
        if let Some(user_compiler) = unknown_compiler {
            eprintln!(
                "[!] Unknown --compiler-id '{}', falling back to auto detection",
                user_compiler
            );
            warn!(
                compiler_id = user_compiler,
                "unknown compiler-id, falling back to auto detection"
            );
        }
        if cli.verbose {
            eprintln!(
                "[*] Decompiler compiler_id = {}",
                compiler_id.as_deref().unwrap_or("default")
            );
        }
        let config = fission_core::config::Config::default();
        let gdt_path_owned = fission_core::PATHS
            .get_gdt_path(binary.is_64bit)
            .and_then(|p| p.to_str().map(String::from));
        let signatures_json = serialize_win_api_signatures_json();
        let mut options = PrepareOptions {
            verbose: cli.verbose,
            compiler_id: compiler_id.as_deref(),
            gdt_path: gdt_path_owned.as_deref(),
            timeout_ms: Some(cli.timeout_ms.unwrap_or(config.decompiler.timeout_ms)),
            timings: if cli.benchmark {
                Some(&mut prepare_timings)
            } else {
                None
            },
            signatures_json: signatures_json.as_deref(),
        };
        if let Err(e) =
            prepare_native_decompiler_for_binary(&mut decomp, binary, binary_data, &mut options)
        {
            eprintln!("Error: Failed to prepare decompiler: {}", e);
            std::process::exit(1);
        }
    }

    let init_elapsed = init_start.elapsed();
    if cli.verbose {
        eprintln!(
            "[✓] Decompiler ready (init: {:.3}s)",
            init_elapsed.as_secs_f64()
        );
    }

    if functions.is_empty() && cli.address.is_some() {
        if let Some(addr) = cli.address {
            eprintln!("Warning: No function found at address 0x{:x}", addr);
            decompile_and_output(
                cli,
                &mut decomp,
                binary,
                binary_data,
                selected_profile,
                engine_mode,
                addr,
                &format!("sub_{:x}", addr),
            )?;
        }
        return Ok(());
    }

    let effective_no_header = cli.no_header || cli.ghidra_compat;
    let effective_no_warnings = cli.no_warnings || cli.ghidra_compat;
    let effective_json = cli.json || cli.benchmark;

    let use_parallel = (cli.decomp_all || cli.decomp_limit.is_some()) && functions.len() > 1;

    let (all_output, json_results, total_decomp_secs, total_postprocess_secs) = if use_parallel {
        run_parallel_decompilation(
            cli,
            &mut decomp,
            binary,
            binary_data,
            &functions,
            &prepare_timings,
            selected_profile,
            engine_mode,
            init_elapsed.as_secs_f64(),
            init_start,
            effective_no_header,
            effective_no_warnings,
            effective_json,
        )
    } else {
        run_sequential_decompilation(
            cli,
            &mut decomp,
            binary,
            binary_data,
            &functions,
            selected_profile,
            engine_mode,
            effective_no_header,
            effective_no_warnings,
            effective_json,
        )
    };

    let final_output = if cli.benchmark {
        let envelope = serde_json::json!({
            "_meta": {
                "tool": "fission",
                "version": env!("CARGO_PKG_VERSION"),
                "profile": cli.profile.as_deref().unwrap_or("balanced"),
                "engine": cli.engine.as_deref().unwrap_or("auto"),
                "function_count": functions.len(),
                "init_sec": (init_elapsed.as_secs_f64() * 1_000_000.0).round() / 1_000_000.0,
                "prepare_timings": &prepare_timings,
                "total_decomp_sec": (total_decomp_secs * 1_000_000.0).round() / 1_000_000.0,
                "total_postprocess_sec": (total_postprocess_secs * 1_000_000.0).round() / 1_000_000.0,
                "wall_clock_sec": (init_start.elapsed().as_secs_f64() * 1_000_000.0).round() / 1_000_000.0,
            },
            "functions": json_results
        });
        serde_json::to_string_pretty(&envelope)
            .map_err(|e| io::Error::other(format!("JSON serialization failed: {}", e)))?
    } else if effective_json {
        serde_json::to_string_pretty(&json_results)
            .map_err(|e| io::Error::other(format!("JSON serialization failed: {}", e)))?
    } else {
        all_output
    };

    if let Some(ref output_path) = cli.output {
        let mut file = fs::File::create(output_path).map_err(|e| {
            io::Error::other(format!(
                "Failed to create output file '{}': {}",
                output_path.display(),
                e
            ))
        })?;
        file.write_all(final_output.as_bytes())?;
        if cli.verbose {
            eprintln!("[✓] Output written to: {}", output_path.display());
        }
    } else {
        let mut stdout = io::stdout().lock();
        stdout.write_all(final_output.as_bytes())?;
    }
    Ok(())
}
