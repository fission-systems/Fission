#[cfg(feature = "native_decomp")]
use crate::analysis::decomp::DecompilerNative;
#[cfg(feature = "native_decomp")]
use fission_core::{Result, PATHS};
#[cfg(feature = "native_decomp")]
use fission_loader::loader::{FunctionInfo, LoadedBinary};

#[cfg(feature = "native_decomp")]
use std::collections::BTreeMap;
#[cfg(feature = "native_decomp")]
use std::time::Instant;

#[cfg(feature = "native_decomp")]
/// Per-step timing for prepare (milliseconds). Used with `PrepareOptions::timings` and `--benchmark`.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct PrepareTimings {
    pub load_binary_ms: f64,
    pub symbols_ms: f64,
    pub symbol_provider_ms: f64,
    pub sections_ms: f64,
    pub known_functions_ms: f64,
    pub fid_ms: f64,
    pub gdt_ms: f64,
}

#[cfg(feature = "native_decomp")]
/// Options for preparing a native decompiler instance for a specific binary.
pub struct PrepareOptions<'a> {
    pub verbose: bool,
    /// Optional compiler ID hint to pass through to Ghidra.
    pub compiler_id: Option<&'a str>,
    /// Optional path to a GDT (Ghidra Data Type) file for type information. Resolved from config/paths (e.g. `PATHS.get_gdt_path(binary.is_64bit)`).
    pub gdt_path: Option<&'a str>,
    /// Decompilation timeout in milliseconds (0 = no limit). Reserved for when the native layer exposes timeout; not yet applied.
    pub timeout_ms: Option<u64>,
    /// When set, per-step timings are written here (e.g. for `--benchmark` JSON).
    pub timings: Option<&'a mut PrepareTimings>,
}

#[cfg(feature = "native_decomp")]
fn prefer_function_name(candidate: &str, current: &str) -> bool {
    let candidate_is_sub = candidate.starts_with("sub_");
    let current_is_sub = current.starts_with("sub_");
    if candidate_is_sub != current_is_sub {
        return !candidate_is_sub;
    }
    candidate.len() > current.len()
}

#[cfg(feature = "native_decomp")]
fn register_memory_sections(
    decomp: &mut DecompilerNative,
    binary: &LoadedBinary,
    verbose: bool,
) {
    if verbose {
        eprintln!(
            "[*] Registering {} memory sections...",
            binary.sections.len()
        );
    }

    for section in &binary.sections {
        if let Err(e) = decomp.add_memory_block(
            &section.name,
            section.virtual_address,
            section.virtual_size,
            section.file_offset,
            section.file_size,
            section.is_executable,
            section.is_writable,
        ) {
            if verbose {
                eprintln!("[!] Failed to register section {}: {}", section.name, e);
            }
        }
    }
}

#[cfg(feature = "native_decomp")]
fn register_known_functions(
    decomp: &mut DecompilerNative,
    binary: &LoadedBinary,
    verbose: bool,
) {
    if verbose {
        eprintln!(
            "[*] Registering {} known functions...",
            binary.functions.len()
        );
    }

    let mut by_addr: BTreeMap<u64, &FunctionInfo> = BTreeMap::new();
    for func in &binary.functions {
        if func.address == 0 || func.name.is_empty() {
            continue;
        }
        match by_addr.get(&func.address) {
            None => {
                by_addr.insert(func.address, func);
            }
            Some(current) => {
                if prefer_function_name(&func.name, &current.name) {
                    by_addr.insert(func.address, func);
                }
            }
        }
    }

    for func in by_addr.values() {
        if func.address != 0
            && !func.name.is_empty()
            && let Err(e) = decomp.add_function(func.address, Some(&func.name))
        {
            if verbose {
                eprintln!(
                    "[!] Failed to register function at 0x{:x}: {}",
                    func.address, e
                );
            }
        }
    }
}

#[cfg(feature = "native_decomp")]
fn load_fid_databases(
    decomp: &mut DecompilerNative,
    binary: &LoadedBinary,
    verbose: bool,
) -> Result<()> {
    let mut fid_loaded_count = 0;
    let fid_paths = PATHS.get_all_fid_paths(binary.is_64bit);
    for fid_full in &fid_paths {
        if verbose {
            eprintln!("[*] Loading FID database: {}", fid_full.display());
        }
        if let Err(e) = decomp.load_fid_database(&fid_full.to_string_lossy()) {
            if verbose {
                eprintln!("[!] Warning: Failed to load FID database: {}", e);
            }
        } else {
            fid_loaded_count += 1;
            if verbose {
                eprintln!("[✓] FID database loaded");
            }
        }
    }

    if verbose && fid_loaded_count > 0 {
        eprintln!(
            "[✓] Loaded {} FID database(s) for function matching",
            fid_loaded_count
        );
    }

    Ok(())
}

#[cfg(feature = "native_decomp")]
/// Prepare a native decompiler for use with a specific binary.
///
/// This performs all per-binary initialization:
/// - load_binary
/// - IAT/global symbol registration
/// - symbol provider setup
/// - memory sections registration
/// - known function registration
/// - FID database loading
pub fn prepare_native_decompiler_for_binary<'a>(
    decomp: &mut DecompilerNative,
    binary: &LoadedBinary,
    binary_data: &[u8],
    options: &mut PrepareOptions<'a>,
) -> Result<()> {
    // Load binary image
    let t0 = Instant::now();
    decomp.load_binary(
        binary_data,
        binary.image_base,
        binary.is_64bit,
        Some(&binary.arch_spec),
        options.compiler_id,
    )?;
    if let Some(t) = options.timings.as_deref_mut() {
        t.load_binary_ms = t0.elapsed().as_secs_f64() * 1000.0;
    }

    // Register symbols
    let t0 = Instant::now();
    decomp.add_symbols(&binary.iat_symbols);
    decomp.add_global_symbols(&binary.global_symbols);
    if let Some(t) = options.timings.as_deref_mut() {
        t.symbols_ms = t0.elapsed().as_secs_f64() * 1000.0;
    }

    // Install symbol provider for on-demand lookups
    let t0 = Instant::now();
    decomp.set_symbol_provider(&binary.functions, &binary.global_symbols, &binary.sections);
    if let Some(t) = options.timings.as_deref_mut() {
        t.symbol_provider_ms = t0.elapsed().as_secs_f64() * 1000.0;
    }

    // Register memory sections and known functions
    let t0 = Instant::now();
    register_memory_sections(decomp, binary, options.verbose);
    if let Some(t) = options.timings.as_deref_mut() {
        t.sections_ms = t0.elapsed().as_secs_f64() * 1000.0;
    }

    let t0 = Instant::now();
    register_known_functions(decomp, binary, options.verbose);
    if let Some(t) = options.timings.as_deref_mut() {
        t.known_functions_ms = t0.elapsed().as_secs_f64() * 1000.0;
    }

    // Load FID databases (best-effort)
    let t0 = Instant::now();
    load_fid_databases(decomp, binary, options.verbose)?;
    if let Some(t) = options.timings.as_deref_mut() {
        t.fid_ms = t0.elapsed().as_secs_f64() * 1000.0;
    }

    // GDT (type info): best-effort when path is provided
    let t0 = Instant::now();
    if let Some(path) = options.gdt_path {
        if !path.is_empty() {
            if let Err(e) = decomp.set_gdt(path) {
                if options.verbose {
                    eprintln!("[!] Warning: Failed to set GDT {}: {}", path, e);
                }
            }
        }
    }
    if let Some(t) = options.timings.as_deref_mut() {
        t.gdt_ms = t0.elapsed().as_secs_f64() * 1000.0;
    }

    // timeout_ms is in options for future use when the native decompiler exposes it

    Ok(())
}

