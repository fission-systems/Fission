//! Application settings — load and persist user preferences.

use crate::dto::{AnalysisOptions, CppPostProcessOptions, DecompilerOptions};
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;
use fission_core::SETTINGS_FILENAME;
use tauri::Manager as _;
use tracing::warn;

// ============================================================================
// Private helpers
// ============================================================================

/// Path to the settings file inside the OS app-data directory.
fn settings_path(app_handle: &tauri::AppHandle) -> CmdResult<std::path::PathBuf> {
    let data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| CmdError::other(format!("Cannot resolve app-data dir: {e}")))?;
    std::fs::create_dir_all(&data_dir)
        .map_err(|e| CmdError::other(format!("Cannot create app-data dir: {e}")))?;
    Ok(data_dir.join("settings.json"))
}

// ============================================================================
// Commands
// ============================================================================

/// Load persisted application settings (or defaults if none saved yet).
#[tauri::command]
pub async fn get_settings(app_handle: tauri::AppHandle) -> CmdResult<crate::dto::AppSettings> {
    let path = settings_path(&app_handle)?;
    if !path.exists() {
        return Ok(crate::dto::AppSettings::default());
    }
    let json = std::fs::read_to_string(&path)
        .map_err(|e| CmdError::other(format!("Read settings failed: {e}")))?;
    // If schema is corrupt or outdated, fall back to defaults silently
    Ok(serde_json::from_str(&json).unwrap_or_else(|_| {
        warn!(
            file = SETTINGS_FILENAME,
            "settings invalid or schema changed, using defaults"
        );
        crate::dto::AppSettings::default()
    }))
}

/// Persist application settings.
#[tauri::command]
pub async fn save_settings(
    settings: crate::dto::AppSettings,
    app_handle: tauri::AppHandle,
) -> CmdResult<()> {
    let path = settings_path(&app_handle)?;
    let json = serde_json::to_string_pretty(&settings)
        .map_err(|e| CmdError::other(format!("Serialise settings failed: {e}")))?;
    std::fs::write(&path, json)
        .map_err(|e| CmdError::other(format!("Write settings failed: {e}")))?;
    Ok(())
}

/// Get current decompiler options (returns defaults if not yet configured).
#[tauri::command]
pub async fn get_decompiler_options(app_handle: tauri::AppHandle) -> CmdResult<DecompilerOptions> {
    let settings = get_settings(app_handle).await?;
    Ok(settings.decompiler_options.unwrap_or_default())
}

/// Apply decompiler options to the active decompiler engine and save to settings.
///
/// This command:
/// 1. Sends Analysis options to Ghidra via `set_feature()` FFI calls
/// 2. Sends C++ PostProcess options via `set_feature("pp_*")` FFI calls
/// 3. Sets Rust PostProcess options on CachingDecompiler
/// 4. Clears the decompiler cache (forces re-decompilation)
/// 5. Persists options to settings.json
#[tauri::command]
pub async fn apply_decompiler_options(
    options: DecompilerOptions,
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
) -> CmdResult<()> {
    // Apply to the native decompiler if loaded
    #[cfg(feature = "native_decomp")]
    {
        let mut decomp_lock = state.decompiler.lock().await;
        if let Some(decomp) = decomp_lock.as_mut() {
            // 1. Analysis options → FFI set_feature
            apply_analysis_options(decomp.inner_mut(), &options.analysis);

            // 2. C++ PostProcess options → FFI set_feature("pp_*")
            apply_cpp_postprocess_options(decomp.inner_mut(), &options.cpp_postprocess);

            // 3. Rust PostProcess options → CachingDecompiler
            let rust_opts = fission_static::analysis::decomp::RustPostProcessOptions {
                clean_rust: options.rust_postprocess.clean_rust,
                clean_go: options.rust_postprocess.clean_go,
                swift_demangle: options.rust_postprocess.swift_demangle,
                field_offsets: options.rust_postprocess.field_offsets,
                insert_casts: options.rust_postprocess.insert_casts,
                arithmetic_idioms: options.rust_postprocess.arithmetic_idioms,
                temp_var_inlining: options.rust_postprocess.temp_var_inlining,
                stack_var_normalization: options.rust_postprocess.stack_var_normalization,
                piece_access_normalization: options.rust_postprocess.piece_access_normalization,
                deref_to_array: options.rust_postprocess.deref_to_array,
                bitop_to_logicop: options.rust_postprocess.bitop_to_logicop,
                remove_dead_branches: options.rust_postprocess.remove_dead_branches,
                simplify_if: options.rust_postprocess.simplify_if,
                while_to_for: options.rust_postprocess.while_to_for,
                dead_assign_removal: options.rust_postprocess.dead_assign_removal,
                rename_induction_vars: options.rust_postprocess.rename_induction_vars,
                rename_semantic_vars: options.rust_postprocess.rename_semantic_vars,
                loop_idioms: options.rust_postprocess.loop_idioms,
                switch_reconstruction: options.rust_postprocess.switch_reconstruction,
                mul_to_shift: options.rust_postprocess.mul_to_shift,
                dwarf_names: options.rust_postprocess.dwarf_names,
                string_pointers: options.rust_postprocess.string_pointers,
            };
            decomp.set_rust_postprocess_options(rust_opts);

            // 4. Clear cache to force re-decompilation with new settings
            decomp.clear_cache();
        }
    }

    // 5. Persist to settings.json
    let mut settings = get_settings(app_handle.clone()).await?;
    settings.decompiler_options = Some(options);
    save_settings(settings, app_handle).await?;

    Ok(())
}

// ============================================================================
// Helpers — map DTO fields to FFI calls
// ============================================================================

#[cfg(feature = "native_decomp")]
fn apply_analysis_options(decomp: &mut fission_ffi::DecompilerNative, opts: &AnalysisOptions) {
    decomp.set_feature("infer_pointers", opts.infer_pointers);
    decomp.set_feature("analyze_loops", opts.analyze_loops);
    decomp.set_feature("readonly_propagate", opts.readonly_propagate);
    decomp.set_feature("record_jumploads", opts.record_jumploads);
    decomp.set_feature("allow_inline", opts.allow_inline);
    decomp.set_feature(
        "disable_toomanyinstructions_error",
        opts.disable_toomanyinstructions_error,
    );
}

#[cfg(feature = "native_decomp")]
fn apply_cpp_postprocess_options(
    decomp: &mut fission_ffi::DecompilerNative,
    opts: &CppPostProcessOptions,
) {
    decomp.set_feature("pp_apply_struct_definitions", opts.apply_struct_definitions);
    decomp.set_feature("pp_iat_symbols", opts.iat_symbols);
    decomp.set_feature("pp_strip_shadow_params", opts.strip_shadow_params);
    decomp.set_feature("pp_smart_constants", opts.smart_constants);
    decomp.set_feature("pp_inline_strings", opts.inline_strings);
    decomp.set_feature("pp_constants", opts.constants);
    decomp.set_feature("pp_guids", opts.guids);
    decomp.set_feature("pp_unicode_strings", opts.unicode_strings);
    decomp.set_feature("pp_interlocked_patterns", opts.interlocked_patterns);
    decomp.set_feature("pp_xunknown_types", opts.xunknown_types);
    decomp.set_feature("pp_seh_cleanup", opts.seh_cleanup);
    decomp.set_feature("pp_global_symbols", opts.global_symbols);
    decomp.set_feature("pp_internal_names", opts.internal_names);
    decomp.set_feature("pp_struct_offsets", opts.struct_offsets);
    decomp.set_feature("pp_fid_names", opts.fid_names);
}
