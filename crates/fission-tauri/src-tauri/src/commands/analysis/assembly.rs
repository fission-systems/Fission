//! Disassembly and decompilation commands.

use crate::dto::*;
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;
use fission_loader::loader::LoadedBinary;
use fission_pcode::{render_nir, NirRenderOptions, PcodeFunction, Varnode};
use fission_sleigh::lifter::SleighLifter;
use fission_static::analysis::decomp::fallback_reason_with_kind;
use fission_static::analysis::decomp::postprocess::pass::PassContext;
use fission_static::analysis::decomp::postprocess::registry::create_default_registry;
use fission_static::analysis::decomp::postprocess::PostProcessor;
use fission_static::analysis::decomp::RustPostProcessOptions as StaticRustPostProcessOptions;
use std::time::Duration;
use tauri::State;

// ============================================================================
// Commands
// ============================================================================

struct DecompileOutcome {
    code: String,
    engine_used: DecompilerEngineMode,
    fell_back: bool,
    fallback_reason: Option<String>,
}

// GUI guardrails for Rust-only lifting quality and responsiveness.
const GUI_MAX_DECODE_BYTES: usize = 0x2000; // 8 KiB
const GUI_MAX_INSTRUCTION_BUDGET: usize = 4096;

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

fn format_varnode_for_pcode(vn: &Varnode) -> String {
    if vn.is_constant {
        format!("const(0x{:x}:{})", vn.constant_val as u64, vn.size)
    } else {
        format!("v(space={},off=0x{:x},size={})", vn.space_id, vn.offset, vn.size)
    }
}

fn render_pcode_text(name: &str, address: u64, pcode: &PcodeFunction, error: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "// Rust-only NIR fallback (pcode dump): {error}\n// Function: {name} @ 0x{address:x}\n"
    ));
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

fn map_rust_postprocess_options(
    options: crate::dto::RustPostProcessOptions,
) -> StaticRustPostProcessOptions {
    StaticRustPostProcessOptions {
        clean_rust: options.clean_rust,
        clean_go: options.clean_go,
        swift_demangle: options.swift_demangle,
        field_offsets: options.field_offsets,
        insert_casts: options.insert_casts,
        arithmetic_idioms: options.arithmetic_idioms,
        temp_var_inlining: options.temp_var_inlining,
        stack_var_normalization: options.stack_var_normalization,
        piece_access_normalization: options.piece_access_normalization,
        deref_to_array: options.deref_to_array,
        bitop_to_logicop: options.bitop_to_logicop,
        remove_dead_branches: options.remove_dead_branches,
        simplify_if: options.simplify_if,
        while_to_for: options.while_to_for,
        dead_assign_removal: options.dead_assign_removal,
        rename_induction_vars: options.rename_induction_vars,
        rename_semantic_vars: options.rename_semantic_vars,
        loop_idioms: options.loop_idioms,
        switch_reconstruction: options.switch_reconstruction,
        mul_to_shift: options.mul_to_shift,
        dwarf_names: options.dwarf_names,
        string_pointers: options.string_pointers,
    }
}

fn postprocess_rust_only(
    code: &str,
    binary: &LoadedBinary,
    options: StaticRustPostProcessOptions,
) -> String {
    let mut registry = match create_default_registry() {
        Ok(registry) => registry,
        Err(_) => {
            return PostProcessor::new()
                .with_options(options)
                .with_string_map(Some(binary.inner().string_map.clone()))
                .process(code);
        }
    };

    if !options.clean_rust {
        registry.disable("clean_rust");
    }
    if !options.clean_go {
        registry.disable("clean_go");
    }
    if !options.swift_demangle {
        registry.disable("swift_demangle");
    }
    if !options.field_offsets {
        registry.disable("field_offsets");
    }
    if !options.insert_casts {
        registry.disable("insert_casts");
    }
    if !options.arithmetic_idioms {
        registry.disable("arithmetic_idioms");
    }
    if !options.temp_var_inlining {
        registry.disable("inline_single_use_temps");
    }
    if !options.stack_var_normalization {
        registry.disable("normalize_stack_artifacts");
    }
    if !options.piece_access_normalization {
        registry.disable("aggregate_copy_cleanup");
        registry.disable("normalize_piece_accesses");
    }
    if !options.deref_to_array {
        registry.disable("deref_to_array");
    }
    if !options.bitop_to_logicop {
        registry.disable("bitop_to_logicop");
    }
    if !options.remove_dead_branches {
        registry.disable("remove_dead_branches");
    }
    if !options.simplify_if {
        registry.disable("simplify_if");
    }
    if !options.while_to_for {
        registry.disable("while_true_to_cond");
        registry.disable("while_true_to_for");
        registry.disable("while_cond_to_for");
        registry.disable("do_while_to_for");
        registry.disable("while_true_to_for_ever");
    }
    if !options.dead_assign_removal {
        registry.disable("remove_dead_assigns");
    }
    if !options.rename_induction_vars {
        registry.disable("rename_induction_vars");
    }
    if !options.rename_semantic_vars {
        registry.disable("rename_semantic_vars");
    }
    if !options.loop_idioms {
        registry.disable("loop_idioms");
    }
    if !options.switch_reconstruction {
        registry.disable("switch_from_bst");
        registry.disable("switch_from_if_else");
        registry.disable("switch_case_clustering");
    }
    if !options.mul_to_shift {
        registry.disable("mul_to_shift");
    }
    if !options.dwarf_names {
        registry.disable("dwarf_names");
    }
    if !options.string_pointers {
        registry.disable("replace_string_pointers");
    }

    // Rust-only profile: disable legacy/native-oriented cleanup passes.
    registry.disable("promote_rect_params");
    registry.disable("clean_slate");

    let context = PassContext::new().with_string_map(Some(binary.inner().string_map.clone()));
    match registry.execute_all(code, &context) {
        Ok(output) => output.into_owned(),
        Err(_) => code.to_string(),
    }
}

fn decode_rust_sleigh_pcode(
    binary: &LoadedBinary,
    entry_address: u64,
    max_function_size: u32,
    max_instructions: u32,
) -> Result<PcodeFunction, CmdError> {
    let max_bytes_limit = usize::try_from(max_function_size)
        .unwrap_or(0x1000)
        .max(1)
        .min(GUI_MAX_DECODE_BYTES);
    let function_size = binary
        .function_at(entry_address)
        .map(|f| usize::try_from(f.size).unwrap_or(0))
        .unwrap_or(0);
    let max_bytes = if function_size > 0 {
        function_size.min(max_bytes_limit)
    } else {
        max_bytes_limit.min(0x1000)
    }
    .max(1);

    let bytes = binary.view_bytes(entry_address, max_bytes).ok_or_else(|| {
        CmdError::other(format!(
            "rust_sleigh: unable to read bytes at 0x{entry_address:x}"
        ))
    })?;

    let language = sleigh_language_for_arch_spec(&binary.arch_spec).ok_or_else(|| {
        CmdError::other(format!(
            "rust_sleigh: unsupported arch_spec '{}'",
            binary.arch_spec
        ))
    })?;

    let lifter = SleighLifter::new_for_language(language)
        .map_err(|e| CmdError::other(format!("rust_sleigh: {e:#}")))?;
    let instruction_limit = usize::try_from(max_instructions)
        .unwrap_or(512)
        .max(1)
        .min(GUI_MAX_INSTRUCTION_BUDGET);
    let lifted = lifter
        .lift_raw_pcode_function_with_contract(bytes, entry_address, instruction_limit)
        .map_err(|e| {
            CmdError::other(format!(
                "rust_sleigh: function lift failed at 0x{entry_address:x}: {e:#}"
            ))
        })?;

    Ok(lifted.function)
}

fn decompile_rust_only(
    binary: &LoadedBinary,
    address: u64,
    name: &str,
    decompiler_options: crate::dto::DecompilerOptions,
) -> Result<DecompileOutcome, CmdError> {
    let entry_address = binary
        .function_at(address)
        .map(|f| f.address)
        .unwrap_or(address);
    let pcode = decode_rust_sleigh_pcode(
        binary,
        entry_address,
        decompiler_options.performance.max_function_size,
        decompiler_options.performance.max_instructions,
    )?;

    let mut nir_options = NirRenderOptions::from_loaded_binary(binary);
    // Rust-Sleigh is used beyond PE/x64 binaries in the GUI path.
    nir_options.pe_x64_only = false;
    nir_options.conservative_irreducible_fallback = true;

    match render_nir(&pcode, name, entry_address, &nir_options) {
        Ok(raw_code) => {
            let rust_pp = map_rust_postprocess_options(decompiler_options.rust_postprocess);
            let processed = postprocess_rust_only(&raw_code, binary, rust_pp);
            Ok(DecompileOutcome {
                code: processed,
                engine_used: DecompilerEngineMode::Nir,
                fell_back: false,
                fallback_reason: None,
            })
        }
        Err(err) => {
            let err_text = err.to_string();
            Ok(DecompileOutcome {
                code: render_pcode_text(name, entry_address, &pcode, &err_text),
                engine_used: DecompilerEngineMode::Nir,
                fell_back: true,
                fallback_reason: Some(fallback_reason_with_kind("nir_render_error", &err_text)),
            })
        }
    }
}

/// Decompile a function at the given address.
#[tauri::command]
pub async fn decompile_function(
    address: u64,
    app_handle: tauri::AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<DecompileResult> {
    // Step 1: Grab func_name and binary from the inner lock, then DROP it immediately
    let (func_name, binary) = {
        let inner = state.inner.lock().await;
        let func_name = inner
            .fact_store
            .as_ref()
            .and_then(|store| store.resolved_name(address).map(ToString::to_string))
            .or_else(|| inner.renamed_functions.get(&address).cloned())
            .or_else(|| {
                inner
                    .loaded_binary
                    .as_ref()
                    .and_then(|b| b.function_at(address))
                    .map(|f| f.name.clone())
            })
            .unwrap_or_else(|| format!("sub_{:x}", address));
        let binary = inner.loaded_binary.clone();
        (func_name, binary)
    };
    let binary = binary.ok_or_else(|| CmdError::other("No binary loaded"))?;
    let decompiler_options = crate::commands::workspace::settings::get_settings(app_handle)
        .await
        .ok()
        .and_then(|settings| settings.decompiler_options)
        .unwrap_or_default();

    let timeout_ms = decompiler_options.performance.timeout_ms.max(1);
    let binary_for_job = binary.clone();
    let func_name_for_job = func_name.clone();
    let options_for_job = decompiler_options.clone();

    let job = tokio::task::spawn_blocking(move || {
        decompile_rust_only(binary_for_job.as_ref(), address, &func_name_for_job, options_for_job)
    });

    let decomp_result = match tokio::time::timeout(Duration::from_millis(timeout_ms), job).await {
        Ok(joined) => match joined {
            Ok(result) => result,
            Err(e) => Err(CmdError::other(format!("Decompile task failed: {e}"))),
        },
        Err(_) => {
            return Ok(DecompileResult {
                code: format!(
                    "// Decompilation timed out\n// Function: {}\n// Address: 0x{:x}\n",
                    func_name, address
                ),
                function_name: func_name,
                address: format!("0x{:x}", address),
                engine_used: DecompilerEngineMode::Nir,
                fell_back: true,
                fallback_reason: Some(fallback_reason_with_kind(
                    "preview_timeout",
                    format!("decompilation exceeded {timeout_ms}ms"),
                )),
            });
        }
    };

    match decomp_result {
        Ok(result) => Ok(DecompileResult {
            code: result.code,
            function_name: func_name,
            address: format!("0x{:x}", address),
            engine_used: result.engine_used,
            fell_back: result.fell_back,
            fallback_reason: result.fallback_reason,
        }),
        Err(e) => Ok(DecompileResult {
            code: format!(
                "// Decompilation failed: {}\n// Function: {}\n// Address: 0x{:x}\n",
                e, func_name, address
            ),
            function_name: func_name,
            address: format!("0x{:x}", address),
            engine_used: DecompilerEngineMode::Nir,
            fell_back: true,
            fallback_reason: Some(fallback_reason_with_kind("rust_decomp_failure", e.to_string())),
        }),
    }
}

/// Get disassembled instructions at an address.
#[tauri::command]
pub async fn get_assembly(
    address: u64,
    count: usize,
    state: State<'_, AppState>,
) -> CmdResult<Vec<AsmInstructionDto>> {
    let inner = state.inner.lock().await;
    let binary = inner
        .loaded_binary
        .as_ref()
        .ok_or_else(|| CmdError::other("No binary loaded"))?;

    let byte_count = count * 15;
    let bytes = binary
        .get_bytes(address, byte_count)
        .ok_or_else(|| CmdError::other(format!("Cannot read bytes at 0x{:x}", address)))?;

    use iced_x86::{Decoder, DecoderOptions, Formatter, IntelFormatter};

    let bitness = if binary.is_64bit { 64 } else { 32 };
    let mut decoder = Decoder::with_ip(bitness, &bytes, address, DecoderOptions::NONE);
    let mut formatter = IntelFormatter::new();
    let mut output = String::new();
    let mut instructions = Vec::with_capacity(count);

    let mut i = 0;
    while decoder.can_decode() && i < count {
        let insn = decoder.decode();
        output.clear();
        formatter.format(&insn, &mut output);

        let start = (insn.ip() - address) as usize;
        let end = start + insn.len();
        let hex_bytes: String = bytes[start..end]
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join(" ");

        let parts: Vec<&str> = output.splitn(2, ' ').collect();
        let mnemonic = parts.first().unwrap_or(&"").to_string();
        let operands = parts.get(1).unwrap_or(&"").to_string();

        let comment = inner.comments.get(&insn.ip()).cloned();

        instructions.push(AsmInstructionDto {
            address: format!("0x{:x}", insn.ip()),
            bytes: hex_bytes,
            mnemonic,
            operands,
            comment,
        });

        i += 1;
    }

    Ok(instructions)
}

/// Clear the in-memory decompiler cache (forces re-decompilation on next request).
///
/// The actual decompile/asm cache is managed on the frontend; this command
/// serves as a hook for any future server-side cache that may be added.
#[tauri::command]
pub async fn clear_decompiler_cache(_state: State<'_, AppState>) -> CmdResult<()> {
    // Currently the decompile result cache lives in front-end React state.
    // This command is intentionally a no-op on the backend so the front-end
    // can call it and then clear its own cache in response.
    Ok(())
}
