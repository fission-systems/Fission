fn main() {
    // Only add rpath if the native_decomp feature is enabled
    if std::env::var("CARGO_FEATURE_NATIVE_DECOMP").is_ok() {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
            .unwrap_or_else(|e| panic!("CARGO_MANIFEST_DIR should be set: {}", e));
        let manifest_path = std::path::Path::new(&manifest_dir);

        // Find project root (fission/crates/fission-cli -> fission/)
        let root_dir = manifest_path
            .parent() // crates/
            .and_then(|p| p.parent()) // fission/
            .unwrap_or_else(|| panic!("Failed to find project root directory"));

        let lib_path = root_dir.join("ghidra_decompiler").join("build");

        if lib_path.exists() {
            // Tell cargo where to find the library for linking
            println!("cargo:rustc-link-search=native={}", lib_path.display());

            // On macOS and Linux, add the library path to rpath so it can be found at runtime
            let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
            if target_os == "macos" || target_os == "linux" {
                println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib_path.display());
            }

            println!("cargo:rerun-if-changed={}", lib_path.display());
        }
    }

    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_NATIVE_DECOMP");
    println!("cargo:rerun-if-changed=build.rs");
}
