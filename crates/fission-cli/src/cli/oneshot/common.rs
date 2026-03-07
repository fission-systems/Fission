use crate::cli::output::OutputSilencer;
use fission_core::config::Config;
use fission_core::find_sla_dir;
use fission_ffi::DecompilerNative;
use fission_loader::loader::LoadedBinary;

pub(super) fn init_decompiler(verbose: bool) -> DecompilerNative {
    let sla_dir = find_sla_dir();

    if verbose {
        eprintln!("[*] Initializing native decompiler...");
    }

    let _silencer = OutputSilencer::new_if(!verbose);
    let mut decomp = match DecompilerNative::new(&sla_dir) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Error: Failed to create decompiler: {}", e);
            std::process::exit(1);
        }
    };

    let config = Config::default();
    let log_verbose = config.decompiler.log_verbose || verbose;
    decomp.set_log_verbose(log_verbose);
    if !config.decompiler.log_file.is_empty() {
        decomp.set_log_file(&config.decompiler.log_file);
    }

    decomp
}

pub(super) fn resolve_profile(profile: Option<&str>) -> (&'static str, Option<String>) {
    let selected = profile.unwrap_or("balanced").to_ascii_lowercase();
    match selected.as_str() {
        "quality" => ("quality", None),
        "speed" => ("speed", None),
        "balanced" => ("balanced", None),
        _ => ("balanced", Some(selected)),
    }
}

pub(super) fn apply_profile(decomp: &mut DecompilerNative, profile: &str) {
    match profile {
        "quality" => {
            decomp.set_feature("infer_pointers", true);
            decomp.set_feature("analyze_loops", true);
            decomp.set_feature("readonly_propagate", true);
        }
        "speed" => {
            decomp.set_feature("infer_pointers", false);
            decomp.set_feature("analyze_loops", false);
            decomp.set_feature("readonly_propagate", false);
        }
        _ => {
            decomp.set_feature("infer_pointers", true);
            decomp.set_feature("analyze_loops", false);
            decomp.set_feature("readonly_propagate", true);
        }
    }
}

pub(super) fn detect_compiler_id(binary: &LoadedBinary) -> Option<&'static str> {
    let detection = fission_loader::detect(binary);
    let is_pe = binary.format.to_ascii_uppercase().starts_with("PE");
    detection
        .compiler()
        .map(|d| match d.name.to_lowercase().as_str() {
            "microsoft visual c++" | "msvc" => "windows",
            "gcc" | "mingw" => {
                if is_pe {
                    "windows"
                } else {
                    "gcc"
                }
            }
            "clang" => "clang",
            _ => "default",
        })
}

pub(super) fn resolve_compiler_id(
    binary: &LoadedBinary,
    user_override: Option<&str>,
) -> (Option<&'static str>, Option<String>) {
    if let Some(user_compiler) = user_override {
        let resolved = match user_compiler.to_ascii_lowercase().as_str() {
            "windows" => Some("windows"),
            "gcc" => Some("gcc"),
            "clang" => Some("clang"),
            "default" => Some("default"),
            "auto" => detect_compiler_id(binary),
            _ => detect_compiler_id(binary).or(Some("default")),
        };

        let unknown = match user_compiler.to_ascii_lowercase().as_str() {
            "windows" | "gcc" | "clang" | "default" | "auto" => None,
            _ => Some(user_compiler.to_string()),
        };

        return (resolved, unknown);
    }

    (detect_compiler_id(binary), None)
}
